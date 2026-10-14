//! Bounded native shell with ordinary-app live reads and capability-gated launch requests.
use native_home::{
    model::Page,
    preferences::Persistence,
    runtime::{poll, Window},
    shell::{Preferences, Shell, Snapshot},
};
use plx_machine::machine::PressFrom;
use plx_ui::screen::Dir;
use std::{
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = cfg!(target_arch = "arm");
    let args = std::env::args().collect::<Vec<_>>();
    let assets = if device {
        plx_base::paths::app_dir().to_path_buf()
    } else {
        PathBuf::from(args.get(1).ok_or("Specify snapshot asset directory")?)
    };
    let output = if device {
        plx_base::paths::in_runtime_dir(&format!("shell-{}", std::process::id()))
    } else {
        PathBuf::from(args.get(2).ok_or("Specify new capture directory")?)
    };
    let capture_only = !device && !args.iter().any(|a| a == "--interactive");
    let window = Window::new(!capture_only)?;
    let live_origin = Instant::now();
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700).create(&output)?;
    }
    let native_probe = if device && assets.join("native-read-probe.json").exists() {
        Some(
            std::thread::Builder::new()
                .name("home-read-probe".into())
                .spawn(native_home::native_probe::collect)?,
        )
    } else {
        None
    };
    let bytes = std::fs::read(assets.join("snapshot.json"))?;
    if bytes.len() > 1024 * 1024 {
        return Err("Snapshot too large".into());
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
    #[allow(unused_mut)]
    let mut live_worker = None;
    #[cfg(target_arch = "arm")]
    if device {
        live_worker = Some(native_home::live::Worker::native(
            plx_base::paths::app_id().to_owned(),
            live_origin,
        )?);
    }
    let fixture_path = assets.join("live-fixture.json");
    if !device && fixture_path.exists() {
        let bytes = std::fs::read(fixture_path)?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Live fixture too large".into());
        }
        let values: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
        live_worker = Some(native_home::live::Worker::fixture(
            values,
            "com.ash.nativehome.preview".into(),
            live_origin,
            Duration::from_millis(500),
        )?);
    }
    let mut live_history = Vec::new();
    let proof_path = assets.join("launch-proof.json");
    let mut launch_proof = None;
    if proof_path.exists() {
        let bytes = std::fs::read(proof_path)?;
        if bytes.len() > 1024 {
            return Err("Oversized launch proof".into());
        }
        launch_proof = Some(serde_json::from_slice::<native_home::actions::Request>(
            &bytes,
        )?);
    }
    #[allow(unused_mut)]
    let mut action_worker = None;
    #[cfg(target_arch = "arm")]
    if device {
        action_worker = Some(native_home::actions::Worker::native(
            plx_base::paths::app_id().into(),
            native_home::actions::Capabilities {
                apps: true,
                inputs: matches!(
                    launch_proof,
                    Some(native_home::actions::Request::Input { .. })
                ),
                settings: matches!(
                    launch_proof,
                    Some(native_home::actions::Request::LgSettings {})
                ),
            },
        )?);
    }
    let mut action_receipts = Vec::new();
    let state_path = if device {
        PathBuf::from("/media/developer/.com.ash.nativehome.preview-state")
    } else {
        PathBuf::from(
            std::env::var_os("PLXNATIVE_RUNTIME_DIR")
                .ok_or("An isolated host runtime is required")?,
        )
        .join("home-preferences")
    };
    let (mut persistence, loaded_preferences, mut storage_error) =
        match Persistence::start(&state_path) {
            Ok((writer, loaded)) => (Some(writer), loaded, None),
            Err(error) => (None, None, Some(error.to_string())),
        };
    let mut committed_preferences = None;
    let mut shell = Shell::new(snapshot, loaded_preferences.clone().unwrap_or_default());
    queue_preferences(&mut shell, &mut persistence, &mut storage_error);
    let preferences_at_boot = shell.preferences().clone();
    shell.enable_app_launches(action_worker.is_some());
    shell.load_icons(&assets);
    let mut now = 0;
    for (page, name) in [
        (Page::Home, "home"),
        (Page::Applications, "applications"),
        (Page::Inputs, "inputs"),
        (Page::Controls, "controls"),
        (Page::Picture, "picture"),
        (Page::Sound, "sound"),
    ] {
        shell.open(page);
        for _ in 0..45 {
            now += 17;
            poll_live(&live_worker, &mut shell, live_origin, &mut live_history);
            shell.tick(now, 1.0 / 60.0);
            shell.draw();
            window.swap();
        }
        shell.draw();
        window.capture(&output.join(format!("{name}.ppm")))?;
    }
    let mut navigation = Vec::new();
    shell.open(Page::Applications);
    for dir in [
        Dir::Right,
        Dir::Down,
        Dir::Up,
        Dir::Left,
        Dir::Left,
        Dir::Down,
    ] {
        shell.direction(dir);
        now += 17;
        shell.tick(now, 1.0 / 60.0);
        navigation.push(shell.focused_item());
    }
    shell.press(true, PressFrom::Key);
    now += 50;
    shell.tick(now, 0.05);
    shell.press(false, PressFrom::Key);
    for _ in 0..30 {
        now += 17;
        shell.tick(now, 1.0 / 60.0);
    }
    let navigation_page = format!("{:?}", shell.page());
    // This is a deterministic component probe, explicitly separate from physical SDL input.
    let scripted_requests = shell
        .take_requests()
        .into_iter()
        .map(|a| format!("{a:?}"))
        .collect::<Vec<_>>();
    let probe_path = assets.join("preferences-hold-probe.json");
    let mut probe_identity = None;
    let mut probe_changed = false;
    if probe_path.exists() {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Probe {
            application_id: String,
        }
        let bytes = std::fs::read(probe_path)?;
        if bytes.len() > 1024 {
            return Err("Oversized preference probe".into());
        }
        let probe: Probe = serde_json::from_slice(&bytes)?;
        shell.open(Page::Applications);
        if !shell.focus_application(&probe.application_id) {
            return Err("Probe app is not in the drawer".into());
        }
        probe_identity = shell.focused_item();
        let before = shell
            .preferences()
            .favourites
            .contains(&probe.application_id);
        shell.press(true, PressFrom::Key);
        for _ in 0..45 {
            now += 17;
            shell.repeat();
            shell.tick(now, 1.0 / 60.0);
        }
        shell.press(false, PressFrom::Key);
        for _ in 0..30 {
            now += 17;
            shell.tick(now, 1.0 / 60.0);
        }
        probe_changed = before
            != shell
                .preferences()
                .favourites
                .contains(&probe.application_id);
        queue_preferences(&mut shell, &mut persistence, &mut storage_error);
    }
    shell.open(Page::Home);
    let mut requests = Vec::new();
    let mut proof_request = None;
    if let Some(request) = launch_proof {
        match &request {
            native_home::actions::Request::Application { id }
                if id == "com.ash.nativehome.targetproof" =>
            {
                shell.open(Page::Applications);
                if !shell.focus_application(id) {
                    return Err("Launch target is not in live drawer".into());
                }
            }
            native_home::actions::Request::LgSettings {} => {
                shell.open(Page::Picture);
                if !shell.focus_lg_settings() {
                    return Err("LG Settings route unavailable".into());
                }
            }
            native_home::actions::Request::Input { id } if id == "HDMI_4" => {
                shell.open(Page::Inputs);
                if !shell.focus_input(id) {
                    return Err("HDMI 4 unavailable".into());
                }
            }
            _ => return Err("Unapproved isolated proof route".into()),
        }
        shell.press(true, PressFrom::Key);
        now += 50;
        shell.tick(now, 0.05);
        shell.press(false, PressFrom::Key);
        for _ in 0..30 {
            now += 17;
            shell.tick(now, 1.0 / 60.0);
        }
        proof_request = Some(request);
    }
    dispatch_requests(
        &mut shell,
        &action_worker,
        proof_request.as_ref(),
        &mut requests,
    );
    let start = Instant::now();
    let mut last = start;
    let mut events = 0;
    let mut event_log = Vec::new();
    let mut hidden_at: Option<Instant> = None;

    let mut reason = "capture_complete";
    if !capture_only {
        reason = "timeout";
        while start.elapsed() < Duration::from_secs(30) && !shell.should_exit() {
            let current = Instant::now();
            let dt = (current - last).as_secs_f32().clamp(0.001, 0.05);
            last = current;
            now += (dt * 1000.0) as u32;
            poll_live(&live_worker, &mut shell, live_origin, &mut live_history);
            shell.tick(now, dt);
            while let Some(e) = poll() {
                events += 1;
                match e.kind {
                    1 => {
                        reason = "window_closed";
                        shell.cancel_press();
                    }
                    7 | 8 => {
                        shell.cancel_press();
                        hidden_at.get_or_insert(current);
                    }
                    9 => {
                        hidden_at = None;
                    }
                    2 if hidden_at.is_none() || e.key == 6 => match e.key {
                        1 => shell.direction(Dir::Up),
                        2 => shell.direction(Dir::Down),
                        3 => shell.direction(Dir::Left),
                        4 => shell.direction(Dir::Right),
                        5 => {
                            if e.repeat != 0 {
                                shell.repeat();
                            } else {
                                shell.press(true, PressFrom::Key);
                            }
                        }
                        6 if e.repeat == 0 => shell.back(),
                        _ => {}
                    },
                    3 if e.key == 5 => shell.press(false, PressFrom::Key),
                    4 | 5 if hidden_at.is_none() => {
                        shell.pointer(e.x as f32, e.y as f32, e.kind == 5)
                    }
                    6 if hidden_at.is_none() => {
                        for _ in 0..e.wheel.unsigned_abs().min(3) {
                            shell.direction(if e.wheel > 0 { Dir::Up } else { Dir::Down });
                        }
                    }
                    _ => {}
                }
                if event_log.len() < 32 {
                    event_log.push(e);
                }
            }
            if reason == "window_closed" {
                break;
            }
            // LG may queue focus loss before Back. Stop drawing while hidden, but drain the
            // window's own queue briefly so the actual key can be observed before teardown.
            if hidden_at.is_some_and(|t| t.elapsed() >= Duration::from_millis(500)) {
                reason = "window_hidden";
                break;
            }
            queue_preferences(&mut shell, &mut persistence, &mut storage_error);
            if let Some(writer) = &mut persistence {
                accept_replies(
                    writer.poll(),
                    &mut committed_preferences,
                    &mut storage_error,
                );
            }
            if storage_error.is_some() {
                shell.storage_notice("Favourites could not be saved • previous data kept");
            }
            if let Some(worker) = &action_worker {
                action_receipts.extend(worker.poll());
            }
            dispatch_requests(
                &mut shell,
                &action_worker,
                proof_request.as_ref(),
                &mut requests,
            );
            if hidden_at.is_none() {
                shell.draw();
                window.swap();
            }
            // Bounded 60 Hz host cap also handles compositors whose swap does not block.
            let elapsed = current.elapsed();
            if elapsed < Duration::from_micros(16_667) {
                std::thread::sleep(Duration::from_micros(16_667) - elapsed);
            }
        }
        if shell.should_exit() {
            reason = "back_from_home";
        }
    }
    if let Some(worker) = action_worker {
        action_receipts.extend(worker.finish());
    }
    if let Some(worker) = live_worker {
        worker.finish();
    }
    if let Some(writer) = persistence {
        accept_replies(
            writer.finish(),
            &mut committed_preferences,
            &mut storage_error,
        );
    }
    let native_api_probe = match native_probe {
        Some(worker) => Some(
            worker
                .join()
                .map_err(|_| "Read capability worker panicked")?,
        ),
        None => None,
    };
    if let Some(data) = &native_api_probe {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(output.join("native-api-probe.json"))?;
        serde_json::to_writer_pretty(&mut file, data)?;
    }
    let persistent_readback = committed_preferences.as_ref() == Some(shell.preferences())
        || (loaded_preferences.as_ref() == Some(shell.preferences()) && storage_error.is_none());
    let report = serde_json::json!({"device":device,"app_id":plx_base::paths::app_id(),"uid":unsafe{libc::geteuid()},"renderer":window.renderer,
        "drawable":[1920,1080],"events":events,"event_log":event_log,"exit_reason":reason,"requests_not_executed":requests,
        "setting_writes":false,"application_launches":action_receipts.iter().any(|r|r.admitted),"action_receipts":action_receipts,"data":if shell.live_updates()>0{if device{"ordinary app live readings"}else{"simulated live readings"}}else{"captured TV snapshot"},"live_updates":shell.live_updates(),"live_history":live_history,"preferences":shell.preferences(),
        "preferences_at_boot":preferences_at_boot,"preferences_loaded":loaded_preferences.is_some(),"preferences_readback_confirmed":persistent_readback,
        "storage_error":storage_error,"native_read_probe_registered":native_api_probe.as_ref().and_then(|v|v.get("registered")),"storage_class":if device{"developer media"}else{"host instance"},"hold_probe_identity":probe_identity,"hold_probe_changed":probe_changed,
        "performance_validated":false,"scripted_navigation":{"focus":navigation,"page_after_ok":navigation_page,"passed":navigation_page=="Inputs","requests_not_executed":scripted_requests},"elapsed_seconds":start.elapsed().as_secs_f64()});
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o640)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(output.join("shell-receipt.json"))?;
    serde_json::to_writer_pretty(&mut file, &report)?;
    file.write_all(b"\n")?;
    println!("{}", report);
    Ok(())
}

fn queue_preferences(
    shell: &mut Shell,
    writer: &mut Option<Persistence>,
    error: &mut Option<String>,
) {
    if !shell.take_preferences_changed() {
        return;
    }
    if let Some(writer) = writer {
        if let Err(failure) = writer.queue(shell.preferences()) {
            *error = Some(failure);
        }
    }
}
fn accept_replies(
    replies: Vec<Result<Preferences, String>>,
    committed: &mut Option<Preferences>,
    error: &mut Option<String>,
) {
    for reply in replies {
        match reply {
            Ok(prefs) => *committed = Some(prefs),
            Err(failure) => *error = Some(failure),
        }
    }
}

fn poll_live(
    worker: &Option<native_home::live::Worker>,
    shell: &mut Shell,
    origin: Instant,
    history: &mut Vec<serde_json::Value>,
) {
    if let Some(update) = worker.as_ref().and_then(|worker| worker.poll()) {
        let status = |ok: bool| if ok { "read" } else { "unavailable" };
        if history.len() < 32 {
            history.push(
                serde_json::json!({"observed_ms":update.observed_ms,"simulated":update.simulated,
            "catalogue":status(matches!(update.catalogue,native_home::live::Reading::Value(_))),
            "inputs":status(matches!(update.inputs,native_home::live::Reading::Value(_))),
            "picture":status(matches!(update.picture,native_home::live::Reading::Value(_))),
            "sound":status(matches!(update.sound,native_home::live::Reading::Value(_)))}),
            );
        }
        shell.apply_live(&update, origin.elapsed().as_millis() as u32);
    }
    shell.age_live(origin.elapsed().as_millis() as u32);
}

fn dispatch_requests(
    shell: &mut Shell,
    worker: &Option<native_home::actions::Worker>,
    proof_request: Option<&native_home::actions::Request>,
    unexecuted: &mut Vec<String>,
) {
    for action in shell.take_requests() {
        let Some(request) = native_home::actions::Request::from_action(action.clone()) else {
            unexecuted.push(format!("{action:?}"));
            continue;
        };
        let allowed = match proof_request {
            Some(proof) => &request == proof,
            None => matches!(request, native_home::actions::Request::Application { .. }),
        };
        if allowed {
            if let Some(worker) = worker {
                match worker.queue(request) {
                    Ok(()) => {
                        shell.storage_notice("Opening application…");
                        continue;
                    }
                    Err(error) => shell.storage_notice(&error),
                }
            }
        }
        unexecuted.push(format!("{action:?}"));
    }
}
