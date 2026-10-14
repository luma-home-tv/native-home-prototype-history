//! Bounded interactive native shell. Snapshot-only: requests are recorded, never executed.
use native_home::{
    model::Page,
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
        plx_base::paths::in_runtime_dir("shell")
    } else {
        PathBuf::from(args.get(2).ok_or("Specify new capture directory")?)
    };
    let capture_only = !device && !args.iter().any(|a| a == "--interactive");
    let window = Window::new(!capture_only)?;
    std::fs::create_dir(&output)?;
    let bytes = std::fs::read(assets.join("snapshot.json"))?;
    if bytes.len() > 1024 * 1024 {
        return Err("Snapshot too large".into());
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
    let mut shell = Shell::new(snapshot, Preferences::default());
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
    shell.open(Page::Home);
    let start = Instant::now();
    let mut last = start;
    let mut events = 0;
    let mut event_log = Vec::new();
    let mut hidden_at: Option<Instant> = None;
    let mut requests = Vec::new();
    let mut reason = "capture_complete";
    if !capture_only {
        reason = "timeout";
        while start.elapsed() < Duration::from_secs(30) && !shell.should_exit() {
            let current = Instant::now();
            let dt = (current - last).as_secs_f32().clamp(0.001, 0.05);
            last = current;
            now += (dt * 1000.0) as u32;
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
            requests.extend(shell.take_requests().into_iter().map(|a| format!("{a:?}")));
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
    let report = serde_json::json!({"device":device,"app_id":plx_base::paths::app_id(),"uid":unsafe{libc::geteuid()},"renderer":window.renderer,
        "drawable":[1920,1080],"events":events,"event_log":event_log,"exit_reason":reason,"requests_not_executed":requests,
        "setting_writes":false,"application_launches":false,"data":"captured TV snapshot","preferences":shell.preferences(),
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
