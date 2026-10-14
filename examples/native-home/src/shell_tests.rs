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
    assert_eq!(focus(&s), RAIL_BASE);
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
