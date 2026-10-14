//! Behaviour checks for the real focus/press machines, without a GL context or TV actions.
use super::*;
fn fixture() -> Snapshot {
    let apps = (0..12)
        .map(|i| Application {
            id: format!("com.example.app{i:02}"),
            title: format!("Application {i:02}"),
            kind: "native".into(),
            visible: true,
            icon: None,
        })
        .collect::<Vec<_>>();
    Snapshot {
        captured_at: "fixture".into(),
        launch_ids: apps.iter().map(|a| a.id.clone()).collect(),
        applications: apps,
        inputs: vec![Input {
            id: "HDMI_1".into(),
            label: "HDMI 1".into(),
            plugged: true,
            signal: false,
            active: false,
        }],
        context: Context {
            input: "HDMI_1".into(),
            dynamic_range: "sdr".into(),
            picture_mode: "cinema".into(),
        },
        settings: BTreeMap::new(),
    }
}
fn focus(shell: &Shell) -> u32 {
    shell
        .input
        .engine
        .current(InputOwner::Entry(entry(shell.page())))
        .unwrap()
        .elem
}
fn settle(shell: &mut Shell, ms: u32) {
    for t in (0..=ms).step_by(16) {
        shell.tick(t, 0.016);
    }
}
#[test]
fn navigation_uses_drawer_identity_and_remembers_each_page() {
    let _serial = plx_base::testlock::serial();
    let mut s = Shell::new(fixture(), Preferences::default());
    settle(&mut s, 100);
    let first = focus(&s);
    s.direction(Dir::Right);
    let second = focus(&s);
    assert_ne!(first, second);
    s.direction(Dir::Left);
    assert_eq!(focus(&s), first);
    s.direction(Dir::Left);
    assert_eq!(focus(&s), RAIL_BASE);
    s.direction(Dir::Down);
    s.activate(
        FocusKey {
            entry: entry(Page::Home),
            elem: focus(&s),
        },
        false,
    );
    assert_eq!(s.page(), Page::Applications);
    s.direction(Dir::Right);
    let app_focus = focus(&s);
    s.open(Page::Inputs);
    s.open(Page::Applications);
    assert_eq!(focus(&s), app_focus);
    s.back();
    assert_eq!(s.page(), Page::Home);
    s.back();
    assert!(s.should_exit());
    assert!(s.take_requests().is_empty());
}
#[test]
fn moving_focus_cancels_an_armed_application_launch() {
    let _serial = plx_base::testlock::serial();
    let mut s = Shell::new(fixture(), Preferences::default());
    s.press(true, PressFrom::Key);
    s.direction(Dir::Right);
    s.press(false, PressFrom::Key);
    settle(&mut s, 400);
    assert!(s.take_requests().is_empty());
}
#[test]
fn hold_toggles_once_and_does_not_launch_on_release() {
    let _serial = plx_base::testlock::serial();
    let mut s = Shell::new(fixture(), Preferences::default());
    s.open(Page::Applications);
    let id = s.surface.cards.items[0].id.clone();
    assert!(s.preferences().favourites.contains(&id));
    s.press(true, PressFrom::Key);
    for ms in (0..=800).step_by(16) {
        s.tick(ms, 0.016);
        s.repeat();
    }
    s.press(false, PressFrom::Key);
    for ms in (816..1200).step_by(16) {
        s.tick(ms, 0.016);
    }
    assert!(!s.preferences().favourites.contains(&id));
    assert!(s.take_requests().is_empty());
    assert!(s.take_preferences_changed());
}
#[test]
fn explicitly_empty_favourites_stay_empty_after_restart() {
    let _serial = plx_base::testlock::serial();
    let prefs = Preferences {
        initialized: true,
        favourites: BTreeSet::new(),
    };
    let s = Shell::new(fixture(), prefs.clone());
    assert!(s.surface.cards.items.is_empty());
    assert_eq!(focus(&s), 500);
    let roundtrip: Preferences =
        serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
    assert!(Shell::new(fixture(), roundtrip)
        .surface
        .cards
        .items
        .is_empty());
}
#[test]
fn inputs_are_typed_separately_from_application_launches() {
    let _serial = plx_base::testlock::serial();
    let mut s = Shell::new(fixture(), Preferences::default());
    s.open(Page::Inputs);
    s.activate(
        FocusKey {
            entry: entry(Page::Inputs),
            elem: 500,
        },
        false,
    );
    assert_eq!(
        s.take_requests(),
        vec![Action::RequestInput("HDMI_1".into())]
    );
    s.activate(
        FocusKey {
            entry: entry(Page::Home),
            elem: 500,
        },
        false,
    );
    assert!(s.take_requests().is_empty());
}
#[test]
fn empty_or_unregistered_catalogue_keeps_navigation_available() {
    let _serial = plx_base::testlock::serial();
    let mut fixture = fixture();
    fixture.launch_ids.clear();
    fixture.inputs.clear();
    let mut s = Shell::new(fixture, Preferences::default());
    assert_eq!(focus(&s), RAIL_BASE);
    s.direction(Dir::Down);
    assert_eq!(focus(&s), RAIL_BASE + 1);
    s.activate(
        FocusKey {
            entry: entry(Page::Home),
            elem: RAIL_BASE + 1,
        },
        false,
    );
    assert_eq!(s.page(), Page::Applications);
    assert_eq!(focus(&s), RAIL_BASE + 1);
}

#[test]
fn app_picker_restores_registered_identity_from_a_remembered_rail_without_accepting_arbitrary_targets(
) {
    let _serial = plx_base::testlock::serial();
    let mut s = Shell::new(fixture(), Preferences::default());
    s.open(Page::Applications);
    s.direction(Dir::Left);
    s.direction(Dir::Down);
    s.open(Page::Inputs);
    s.open(Page::Applications);
    assert_eq!(focus(&s), RAIL_BASE + 2);
    assert!(!s.focus_application("com.example.gone"));
    assert_eq!(focus(&s), RAIL_BASE + 2);
    assert!(s.focus_application("com.example.app01"));
    assert_eq!(s.focused_item().as_deref(), Some("com.example.app01"));
    s.open(Page::Inputs);
    assert!(!s.focus_application("HDMI_1"));
    assert!(s.take_requests().is_empty());
}

fn live_update(ms: u32) -> crate::live::Update {
    use crate::live::{CatalogueReading, PictureReading, Reading, Update};
    let data = fixture();
    Update {
        observed_ms: ms,
        simulated: true,
        catalogue: Reading::Value(CatalogueReading {
            apps: data.applications,
            launch_ids: data.launch_ids,
        }),
        inputs: Reading::Value(data.inputs),
        picture: Reading::Value(PictureReading {
            context: data.context,
            settings: Vec::new(),
        }),
        sound: Reading::Failed("Unavailable".into()),
    }
}
#[test]
fn unchanged_live_refresh_preserves_an_armed_gesture_and_app_identity() {
    let _serial = plx_base::testlock::serial();
    let mut s = Shell::new(fixture(), Preferences::default());
    s.open(Page::Applications);
    s.tick(100, 0.016);
    s.apply_live(&live_update(100), 100);
    s.press(true, PressFrom::Key);
    s.apply_live(&live_update(110), 110);
    assert!(s.input.arm.is_some());
    s.tick(150, 0.016);
    s.press(false, PressFrom::Key);
    for ms in (160..560).step_by(16) {
        s.tick(ms, 0.016);
    }
    assert_eq!(
        s.take_requests(),
        vec![Action::RequestApplication("com.example.app00".into())]
    );
}
#[test]
fn rename_reorder_keeps_focus_and_favourites_but_removal_cancels_the_old_press() {
    let _serial = plx_base::testlock::serial();
    let mut s = Shell::new(fixture(), Preferences::default());
    s.open(Page::Applications);
    s.focus_application("com.example.app01");
    let prefs = s.preferences().clone();
    let mut update = live_update(10);
    let crate::live::Reading::Value(ref mut c) = update.catalogue else {
        unreachable!()
    };
    c.apps.reverse();
    c.apps
        .iter_mut()
        .find(|a| a.id == "com.example.app01")
        .unwrap()
        .title = "Zulu".into();
    s.apply_live(&update, 10);
    assert_eq!(s.focused_item().as_deref(), Some("com.example.app01"));
    assert_eq!(s.preferences(), &prefs);
    s.press(true, PressFrom::Key);
    let crate::live::Reading::Value(ref mut c) = update.catalogue else {
        unreachable!()
    };
    c.apps.retain(|a| a.id != "com.example.app01");
    c.launch_ids.remove("com.example.app01");
    s.apply_live(&update, 20);
    s.press(false, PressFrom::Key);
    settle(&mut s, 400);
    assert_ne!(s.focused_item().as_deref(), Some("com.example.app01"));
    assert!(s.take_requests().is_empty());
}
#[test]
fn failed_or_expired_live_catalogue_keeps_cards_but_cannot_request_launch() {
    let _serial = plx_base::testlock::serial();
    let mut s = Shell::new(fixture(), Preferences::default());
    s.open(Page::Applications);
    let mut update = live_update(0);
    s.apply_live(&update, 0);
    s.age_live(15001);
    let key = focus(&s);
    s.activate(
        FocusKey {
            entry: entry(Page::Applications),
            elem: key,
        },
        false,
    );
    assert!(s.take_requests().is_empty());
    update.catalogue = crate::live::Reading::Failed("No data".into());
    s.apply_live(&update, 15002);
    assert!(s.focused_item().is_some());
    s.activate(
        FocusKey {
            entry: entry(Page::Applications),
            elem: key,
        },
        false,
    );
    assert!(s.take_requests().is_empty());
}

#[test]
fn changed_picture_context_and_failed_audio_never_reuse_fresh_control_values() {
    let _serial = plx_base::testlock::serial();
    let mut data = fixture();
    let ctx = data.context.clone();
    data.settings.insert(
        "sound".into(),
        vec![Setting {
            id: "volume".into(),
            focus_key: 20,
            label: "Volume".into(),
            value: "10".into(),
            context: ctx,
            protected: false,
            available: true,
        }],
    );
    let mut s = Shell::new(data, Preferences::default());
    s.open(Page::Sound);
    s.apply_live(&live_update(100), 100);
    assert_eq!(s.surface.table.activate(0), None);
    assert_eq!(
        s.surface.table.table.sections[0].rows[0].value.as_deref(),
        Some("Unavailable")
    );
    let mut update = live_update(110);
    let crate::live::Reading::Value(ref mut picture) = update.picture else {
        unreachable!()
    };
    picture.context.dynamic_range = "hdr".into();
    picture.settings = vec![Setting {
        id: "brightness".into(),
        focus_key: 1,
        label: "Brightness".into(),
        value: "70".into(),
        context: picture.context.clone(),
        protected: false,
        available: true,
    }];
    s.open(Page::Picture);
    s.apply_live(&update, 110);
    assert_eq!(s.snapshot.context.dynamic_range, "hdr");
    assert_eq!(
        s.surface.table.table.sections[0].rows[0].value.as_deref(),
        Some("70")
    );
    s.age_live(15111);
    assert_eq!(s.surface.table.activate(0), None);
    assert_eq!(
        s.surface.table.table.sections[0].rows[0].value.as_deref(),
        Some("Unavailable")
    );
}

#[test]
fn explicit_system_routes_use_registered_surface_keys() {
    let _serial = plx_base::testlock::serial();
    let mut s = Shell::new(fixture(), Preferences::default());
    assert!(s.focus_input("HDMI_1"));
    assert!(!s.focus_lg_settings());
    s.open(Page::Inputs);
    assert!(!s.focus_input("HDMI_99"));
    assert!(s.focus_input("HDMI_1"));
    assert_eq!(focus(&s), 500);
    s.open(Page::Picture);
    assert!(s.focus_lg_settings());
    assert_eq!(focus(&s), TABLE_BASE + 1000);
    s.activate(
        FocusKey {
            entry: entry(Page::Picture),
            elem: focus(&s),
        },
        false,
    );
    assert_eq!(s.take_requests(), vec![Action::OpenLgSettings]);
}

#[test]
fn home_shelves_cross_by_geometry_and_keep_input_requests_out_of_favourites() {
    let _serial = plx_base::testlock::serial();
    let mut data = fixture();
    data.inputs = (1..=4)
        .map(|i| Input {
            id: format!("HDMI_{i}"),
            label: format!("HDMI {i}"),
            plugged: false,
            signal: false,
            active: false,
        })
        .collect();
    let mut s = Shell::new(data, Preferences::default());
    let first = s.focused_item();
    s.direction(Dir::Right);
    s.direction(Dir::Right);
    let chosen = s.focused_item();
    assert_ne!(chosen, first);
    s.direction(Dir::Down);
    assert_eq!(s.focused_item().as_deref(), Some("HDMI_3"));
    let favourites = s.preferences().clone();
    s.press(true, PressFrom::Key);
    for ms in (0..800).step_by(16) {
        s.tick(ms, 0.016);
        s.repeat();
    }
    assert_eq!(s.preferences(), &favourites);
    s.press(false, PressFrom::Key);
    for ms in (816..1100).step_by(16) {
        s.tick(ms, 0.016);
    }
    assert_eq!(
        s.take_requests(),
        vec![Action::RequestInput("HDMI_3".into())]
    );
    s.direction(Dir::Up);
    assert_eq!(s.focused_item(), chosen);
    s.direction(Dir::Left);
    s.direction(Dir::Left);
    assert_eq!(s.focused_item(), first);
}

#[test]
fn home_updates_inputs_by_identity_without_turning_old_input_readings_into_actions() {
    let _serial = plx_base::testlock::serial();
    let mut s = Shell::new(fixture(), Preferences::default());
    assert!(s.focus_input("HDMI_1"));
    let mut update = Update {
        observed_ms: 100,
        inputs: crate::live::Reading::Value(vec![Input {
            id: "HDMI_1".into(),
            label: "Console".into(),
            plugged: true,
            signal: true,
            active: false,
        }]),
        ..live_update(100)
    };
    s.apply_live(&update, 100);
    assert_eq!(s.focused_item().as_deref(), Some("HDMI_1"));
    assert_eq!(
        s.surface.input_cards.items[0].title.to_str().unwrap(),
        "Console"
    );
    s.press(true, PressFrom::Key);
    update.observed_ms = 200;
    update.inputs = crate::live::Reading::Failed("Unavailable".into());
    s.apply_live(&update, 200);
    s.press(false, PressFrom::Key);
    settle(&mut s, 500);
    assert!(s.take_requests().is_empty());
    s.press(true, PressFrom::Key);
    s.press(false, PressFrom::Key);
    settle(&mut s, 500);
    assert!(s.take_requests().is_empty());
}

#[test]
fn home_focus_springs_report_motion_then_go_quiet_at_rest() {
    let _serial = plx_base::testlock::serial();
    let previous = plx_machine::idle::enabled();
    plx_machine::idle::set_enabled(true);
    let mut s = Shell::new(fixture(), Preferences::default());
    settle(&mut s, 2000);
    s.direction(Dir::Right);
    let (_, moving) = plx_machine::idle::scoped_motion(|| s.tick(2016, 0.016));
    assert!(moving, "deliberate shelf focus must keep animating");
    for ms in (2032..6000).step_by(16) {
        s.tick(ms, 0.016);
    }
    let (_, moving) = plx_machine::idle::scoped_motion(|| s.tick(6016, 0.016));
    assert!(!moving, "settled shelves must stop reporting spring motion");
    plx_machine::idle::set_enabled(previous);
}
