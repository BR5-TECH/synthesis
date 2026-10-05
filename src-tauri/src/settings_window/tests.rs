//! The tests of the settings window: which request opens which surface, and
//! how the window keeps its single instance.

use super::*;

fn open_global() -> Request {
    Request::Open {
        kind: SettingsWindowKind::Global,
        section: None,
    }
}

fn open_project() -> Request {
    Request::Open {
        kind: SettingsWindowKind::Project,
        section: None,
    }
}

// SWN-FR-01 / SWN-FR-17: each window has its own label, title, and boot slug.
#[test]
fn each_window_has_its_own_label_title_and_slug() {
    assert_eq!(SettingsWindowKind::Global.label(), "settings-global");
    assert_eq!(SettingsWindowKind::Project.label(), "settings-project");
    assert_eq!(SettingsWindowKind::Global.title(), "Global settings");
    assert_eq!(SettingsWindowKind::Project.title(), "Project settings");
    assert_eq!(SettingsWindowKind::Global.slug(), "global");
    assert_eq!(SettingsWindowKind::Project.slug(), "project");
    assert_eq!(
        SettingsWindowKind::from_label("settings-global"),
        Some(SettingsWindowKind::Global)
    );
    assert_eq!(
        SettingsWindowKind::from_label("settings-project"),
        Some(SettingsWindowKind::Project)
    );
    assert_eq!(SettingsWindowKind::from_label("main"), None);
}

// SWN-FR-03: the fixed outer size, stated once.
#[test]
fn the_fixed_size_is_eight_hundred_by_six_hundred() {
    assert_eq!(SETTINGS_WINDOW_SIZE, (800, 600));
}

// SWN-FR-05: nothing is open, so the requested window is simply created.
#[test]
fn a_request_with_nothing_open_creates_the_window() {
    assert_eq!(
        decide(open_global(), None, false),
        Action::Create {
            kind: SettingsWindowKind::Global,
            section: None
        }
    );
}

// SWN-FR-06: the window asked for is the one already open.
#[test]
fn a_request_for_the_open_window_focuses_it_and_opens_no_second() {
    assert_eq!(
        decide(open_global(), Some(SettingsWindowKind::Global), false),
        Action::Focus {
            kind: SettingsWindowKind::Global,
            section: None
        }
    );
}

// SWN-FR-06 / SWN-FR-13: a request naming a section presents that section
// in the window it focuses.
#[test]
fn a_sectioned_request_for_the_open_window_carries_the_section() {
    let request = Request::Open {
        kind: SettingsWindowKind::Global,
        section: Some("agents".into()),
    };
    assert_eq!(
        decide(request, Some(SettingsWindowKind::Global), false),
        Action::Focus {
            kind: SettingsWindowKind::Global,
            section: Some("agents".into())
        }
    );
}

// SWN-FR-05 / SWN-FR-07: the open window saves and closes first, and the
// requested one opens only after it.
#[test]
fn a_request_for_the_other_window_sweeps_the_open_one_first() {
    assert_eq!(
        decide(open_global(), Some(SettingsWindowKind::Project), false),
        Action::Sweep(PendingSweep {
            label: "settings-project".into(),
            next: Transition::Open {
                kind: SettingsWindowKind::Global,
                section: None
            },
        })
    );
    assert_eq!(
        decide(open_project(), Some(SettingsWindowKind::Global), false),
        Action::Sweep(PendingSweep {
            label: "settings-global".into(),
            next: Transition::Open {
                kind: SettingsWindowKind::Project,
                section: None
            },
        })
    );
}

// SWN-FR-08: a close is a sweep whose transition is the close itself.
#[test]
fn closing_the_open_window_sweeps_it_first() {
    assert_eq!(
        decide(
            Request::Close {
                kind: SettingsWindowKind::Global
            },
            Some(SettingsWindowKind::Global),
            false
        ),
        Action::Sweep(PendingSweep {
            label: "settings-global".into(),
            next: Transition::Close,
        })
    );
}

// A close naming a window that is not the open one belongs to nobody.
#[test]
fn closing_a_window_that_is_not_open_does_nothing() {
    assert_eq!(
        decide(
            Request::Close {
                kind: SettingsWindowKind::Global
            },
            Some(SettingsWindowKind::Project),
            false
        ),
        Action::Inert
    );
    assert_eq!(
        decide(
            Request::Close {
                kind: SettingsWindowKind::Global
            },
            None,
            false
        ),
        Action::Inert
    );
}

// SWN-FR-15 / SWN-FR-16: Global settings is reachable with no project open;
// Project settings is reachable from nowhere in that state.
#[test]
fn no_route_opens_project_settings_while_no_project_is_open() {
    assert!(!route_allowed(&open_project(), false));
    assert!(route_allowed(&open_project(), true));
    // Everything else is unaffected: Global settings belongs to the user
    // and the machine, and a close or a quit is about a window that exists.
    assert!(route_allowed(&open_global(), false));
    assert!(route_allowed(
        &Request::Close {
            kind: SettingsWindowKind::Project
        },
        false
    ));
    assert!(route_allowed(&Request::Quit, false));
}

// SWN-FR-12: while a sweep runs, a second close, a request for the same
// window, a request for the other one, and a quit each start nothing. The
// quit is in the list because `defer_quit_for_settings` upgrades the sweep
// in flight BEFORE it dispatches — if that order ever inverted, the quit
// would fall in here and be dropped rather than deferred.
#[test]
fn every_request_made_during_a_sweep_is_inert() {
    for request in [
        open_global(),
        open_project(),
        Request::Close {
            kind: SettingsWindowKind::Global,
        },
        Request::Quit,
    ] {
        assert_eq!(
            decide(request, Some(SettingsWindowKind::Global), true),
            Action::Inert
        );
    }
}

// SWN-FR-19: a quit with a settings window open sweeps it first.
#[test]
fn a_quit_sweeps_the_open_settings_window_first() {
    assert_eq!(
        decide(Request::Quit, Some(SettingsWindowKind::Project), false),
        Action::Sweep(PendingSweep {
            label: "settings-project".into(),
            next: Transition::Quit,
        })
    );
    // With nothing open the quit is the ordinary exit path's.
    assert_eq!(decide(Request::Quit, None, false), Action::Inert);
}

// SWN-FR-11 / SWN-FR-19: only a failed quit-sweep has an exit hold to give
// back; a failed close or switch never claimed one.
#[test]
fn only_a_failed_quit_releases_the_exit_hold() {
    assert!(releases_exit_gate(&Transition::Quit));
    assert!(!releases_exit_gate(&Transition::Close));
    assert!(!releases_exit_gate(&Transition::Open {
        kind: SettingsWindowKind::Global,
        section: None
    }));
}

// SWN-FR-12 / SWN-FR-19: a quit arriving mid-sweep upgrades the sweep in
// flight rather than starting a second one.
#[test]
fn a_quit_during_a_sweep_upgrades_it_rather_than_starting_another() {
    let state = SettingsWindows::default();
    assert!(!state.upgrade_to_quit(), "nothing to upgrade with no sweep");
    state.set(Some(PendingSweep {
        label: "settings-global".into(),
        next: Transition::Close,
    }));
    assert!(state.upgrade_to_quit());
    assert_eq!(state.peek().map(|s| s.next), Some(Transition::Quit));

    // …and the answer then performs the QUIT, not the close it started as.
    assert_eq!(
        state.take_for("settings-global").map(|s| s.next),
        Some(Transition::Quit)
    );
}

// A sweep answer from a window that is not the one under sweep performs
// nobody's transition.
#[test]
fn a_sweep_answer_is_taken_only_from_the_window_it_was_asked_of() {
    let state = SettingsWindows::default();
    state.set(Some(PendingSweep {
        label: "settings-global".into(),
        next: Transition::Close,
    }));
    assert_eq!(state.take_for("settings-project"), None);
    assert!(state.peek().is_some(), "the sweep is still outstanding");
    assert!(state.take_for("settings-global").is_some());
    assert_eq!(state.take_for("settings-global"), None, "taken exactly once");
}

// SWN-FR-13 / AGT-FR-06: the section travels in the boot URL, and a section
// address can carry an id this side does not choose the shape of. Anything
// that could truncate the query or invent a parameter is escaped.
#[test]
fn a_section_address_is_escaped_on_its_way_into_the_url() {
    assert_eq!(encode_query_value("agents"), "agents");
    assert_eq!(encode_query_value("agents:new"), "agents%3Anew");
    assert_eq!(
        encode_query_value("agents:a&settings=project"),
        "agents%3Aa%26settings%3Dproject"
    );
    assert_eq!(encode_query_value("a b#c"), "a%20b%23c");
    // Unreserved characters are left alone, so a plain id stays readable.
    assert_eq!(encode_query_value("a-b_c.d~e9"), "a-b_c.d~e9");
}

// SWN-FR-03: the built window's inner size is the fixed outer size less
// whatever chrome the platform draws.
#[test]
fn the_inner_size_is_the_outer_target_less_the_decoration() {
    assert_eq!(inner_size_for_outer((800, 600), (0, 28)), (800, 572));
    assert_eq!(inner_size_for_outer((1600, 1200), (16, 60)), (1584, 1140));
    // A decoration at least as large as the target would otherwise ask for
    // a window with no content at all.
    assert_eq!(inner_size_for_outer((800, 600), (900, 700)), (1, 1));
}

// The kind crosses the IPC boundary as the same lowercase word the frontend
// sends; a rename on either side must not silently pass.
#[test]
fn the_kind_serialises_as_the_word_the_frontend_sends() {
    assert_eq!(
        serde_json::to_string(&SettingsWindowKind::Global).unwrap(),
        "\"global\""
    );
    assert_eq!(
        serde_json::to_string(&SettingsWindowKind::Project).unwrap(),
        "\"project\""
    );
    assert_eq!(
        serde_json::from_str::<SettingsWindowKind>("\"project\"").unwrap(),
        SettingsWindowKind::Project
    );
}

// Both commands must stay in scope under their registered names, so a
// rename fails to compile here rather than at runtime in the frontend.
#[test]
fn the_settings_window_commands_are_in_scope() {
    let _ = open_settings_window::<tauri::test::MockRuntime>;
    let _ = finish_settings_close::<tauri::test::MockRuntime>;
    let _ = get_settings_window_context::<tauri::test::MockRuntime>;
}
