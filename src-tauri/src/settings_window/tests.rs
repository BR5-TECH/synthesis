//! The tests of the Settings window: which request opens or focuses it, how the
//! window keeps its single instance, and which size it opens at.

use super::*;

fn open(section: Option<&str>) -> Request {
    Request::Open {
        section: section.map(str::to_string),
    }
}

// SWN-FR-01 / SWN-FR-17: the one window has one label and the title Settings.
#[test]
fn the_window_has_one_label_and_the_title_settings() {
    assert_eq!(SETTINGS_WINDOW_LABEL, "settings");
    assert_eq!(SETTINGS_WINDOW_TITLE, "Settings");
}

// SWN-FR-03: the default and the minimum size, stated once.
#[test]
fn the_default_and_minimum_size_are_eight_hundred_by_six_hundred() {
    assert_eq!(DEFAULT_WINDOW_SIZE, (800, 600));
    assert_eq!(MIN_WINDOW_SIZE, (800, 600));
}

// SWN-FR-05: nothing is open, so the requested window is simply created.
#[test]
fn a_request_with_nothing_open_creates_the_window() {
    assert_eq!(
        decide(open(None), false, false),
        Action::Create { section: None }
    );
}

// SWN-FR-05, SWN-FR-06: the window is already open, so it is focused and no
// second one opens.
#[test]
fn a_request_for_the_open_window_focuses_it_and_opens_no_second() {
    assert_eq!(
        decide(open(None), true, false),
        Action::Focus { section: None }
    );
}

// SWN-FR-06, SWN-FR-HZWD: a request naming a section presents that section
// whether the window was closed or already open.
#[test]
fn a_sectioned_request_carries_its_section_to_a_new_or_an_open_window() {
    assert_eq!(
        decide(open(Some("global/agents")), true, false),
        Action::Focus {
            section: Some("global/agents".into())
        }
    );
    assert_eq!(
        decide(open(Some("project/graduation")), false, false),
        Action::Create {
            section: Some("project/graduation".into())
        }
    );
}

// SWN-FR-08: a close is a sweep whose transition is the close itself.
#[test]
fn closing_the_open_window_sweeps_it_first() {
    assert_eq!(
        decide(Request::Close, true, false),
        Action::Sweep(PendingSweep {
            label: "settings".into(),
            next: Transition::Close,
        })
    );
}

// A close with no window open belongs to nobody.
#[test]
fn closing_a_window_that_is_not_open_does_nothing() {
    assert_eq!(decide(Request::Close, false, false), Action::Inert);
}

// SWN-FR-HZWD: a section address names its scope before the first slash.
#[test]
fn a_section_address_names_its_scope() {
    assert_eq!(address_scope("global/agents"), Some(SettingsScope::Global));
    assert_eq!(address_scope("project/agents"), Some(SettingsScope::Project));
    assert_eq!(
        address_scope("global/agents:new"),
        Some(SettingsScope::Global)
    );
    // An item that carries a slash of its own cannot change the scope.
    assert_eq!(
        address_scope("global/agents:project/x"),
        Some(SettingsScope::Global)
    );
    // A section with no scope, or an unknown scope, names none.
    assert_eq!(address_scope("agents"), None);
    assert_eq!(address_scope("team/agents"), None);
    assert_eq!(address_scope(""), None);
}

// SWN-FR-15 / SWN-FR-16: the window is reachable with no project open, and a
// Project section is reachable from nowhere in that state.
#[test]
fn no_route_presents_a_project_section_while_no_project_is_open() {
    assert!(!route_allowed(&open(Some("project/template")), false));
    assert!(route_allowed(&open(Some("project/template")), true));
    // Everything else is unaffected: the Global group belongs to the user and
    // the machine, and a close or a quit is about a window that exists.
    assert!(route_allowed(&open(None), false));
    assert!(route_allowed(&open(Some("global/docker")), false));
    assert!(route_allowed(&Request::Close, false));
    assert!(route_allowed(&Request::Quit, false));
}

// SWN-FR-12: while a sweep runs, a second close and a request for the window
// each start nothing. The quit is in the list because
// `defer_quit_for_settings` upgrades the sweep in flight BEFORE it dispatches —
// if that order ever inverted, the quit would fall in here and be dropped
// rather than deferred.
#[test]
fn every_request_made_during_a_sweep_is_inert() {
    for request in [open(None), open(Some("global/docker")), Request::Close, Request::Quit] {
        assert_eq!(decide(request, true, true), Action::Inert);
    }
}

// SWN-FR-19: a quit with the window open sweeps it first.
#[test]
fn a_quit_sweeps_the_open_settings_window_first() {
    assert_eq!(
        decide(Request::Quit, true, false),
        Action::Sweep(PendingSweep {
            label: "settings".into(),
            next: Transition::Quit,
        })
    );
    // With nothing open the quit is the ordinary exit path's.
    assert_eq!(decide(Request::Quit, false, false), Action::Inert);
}

// SWN-FR-11 / SWN-FR-19: only a failed quit-sweep has an exit hold to give
// back; a failed close never claimed one.
#[test]
fn only_a_failed_quit_releases_the_exit_hold() {
    assert!(releases_exit_gate(&Transition::Quit));
    assert!(!releases_exit_gate(&Transition::Close));
}

// SWN-FR-12 / SWN-FR-19: a quit arriving mid-sweep upgrades the sweep in
// flight rather than starting a second one.
#[test]
fn a_quit_during_a_sweep_upgrades_it_rather_than_starting_another() {
    let state = SettingsWindows::default();
    assert!(!state.upgrade_to_quit(), "nothing to upgrade with no sweep");
    state.set(Some(PendingSweep {
        label: "settings".into(),
        next: Transition::Close,
    }));
    assert!(state.upgrade_to_quit());
    assert_eq!(state.peek().map(|s| s.next), Some(Transition::Quit));

    // …and the answer then performs the QUIT, not the close it started as.
    assert_eq!(
        state.take_for("settings").map(|s| s.next),
        Some(Transition::Quit)
    );
}

// A sweep answer from a window that is not the one under sweep performs
// nobody's transition.
#[test]
fn a_sweep_answer_is_taken_only_from_the_window_it_was_asked_of() {
    let state = SettingsWindows::default();
    state.set(Some(PendingSweep {
        label: "settings".into(),
        next: Transition::Close,
    }));
    assert_eq!(state.take_for("main"), None);
    assert!(state.peek().is_some(), "the sweep is still outstanding");
    assert!(state.take_for("settings").is_some());
    assert_eq!(state.take_for("settings"), None, "taken exactly once");
}

// SWN-FR-MRBK: the window remembers the size it was placed at, so a close can
// tell whether the author resized it.
#[test]
fn the_placed_size_is_remembered_for_the_close() {
    let state = SettingsWindows::default();
    assert_eq!(state.placed(), None);
    state.set_placed(Some((800, 600)));
    assert_eq!(state.placed(), Some((800, 600)));
}

// SWN-FR-13 / AGT-FR-06: the section travels in the boot URL, and a section
// address can carry an id this side does not choose the shape of. Anything
// that could truncate the query or invent a parameter is escaped.
#[test]
fn a_section_address_is_escaped_on_its_way_into_the_url() {
    assert_eq!(encode_query_value("global"), "global");
    assert_eq!(encode_query_value("global/agents:new"), "global%2Fagents%3Anew");
    assert_eq!(
        encode_query_value("global/agents:a&settings=1"),
        "global%2Fagents%3Aa%26settings%3D1"
    );
    assert_eq!(encode_query_value("a b#c"), "a%20b%23c");
    // Unreserved characters are left alone, so a plain id stays readable.
    assert_eq!(encode_query_value("a-b_c.d~e9"), "a-b_c.d~e9");
}

// SWN-FR-03: the built window's inner size is the outer target less whatever
// chrome the platform draws.
#[test]
fn the_inner_size_is_the_outer_target_less_the_decoration() {
    assert_eq!(inner_size_for_outer((800, 600), (0, 28)), (800, 572));
    assert_eq!(inner_size_for_outer((1600, 1200), (16, 60)), (1584, 1140));
    // A decoration at least as large as the target would otherwise ask for
    // a window with no content at all.
    assert_eq!(inner_size_for_outer((800, 600), (900, 700)), (1, 1));
}

// SWN-FR-03, SWN-FR-CLMP: with nothing stored the window opens at the default.
#[test]
fn an_unresized_window_opens_at_the_default_size() {
    assert_eq!(opening_size(None, Some((1920, 1080))), (800, 600));
    assert_eq!(opening_size(None, None), (800, 600));
}

// SWN-FR-MRBK, SWN-FR-CLMP: a stored size that fits the work area is used as
// it is.
#[test]
fn a_stored_size_that_fits_is_used() {
    assert_eq!(opening_size(Some((1000, 700)), Some((1920, 1080))), (1000, 700));
    assert_eq!(opening_size(Some((1920, 1080)), Some((1920, 1080))), (1920, 1080));
}

// SWN-FR-CLMP: a stored size larger than the work area is clamped to it.
#[test]
fn a_stored_size_larger_than_the_work_area_is_clamped() {
    assert_eq!(opening_size(Some((2400, 1500)), Some((1440, 900))), (1440, 900));
    assert_eq!(opening_size(Some((2400, 700)), Some((1440, 900))), (1440, 700));
}

// SWN-FR-03, SWN-FR-CLMP: a stored size below the minimum is raised to it.
#[test]
fn a_stored_size_below_the_minimum_is_raised_to_it() {
    assert_eq!(opening_size(Some((300, 200)), Some((1920, 1080))), (800, 600));
}

// SWN-FR-CLMP: when the work area cannot hold the minimum, the window falls
// back to the default size.
#[test]
fn a_work_area_smaller_than_the_minimum_falls_back_to_the_default() {
    assert_eq!(opening_size(Some((1000, 700)), Some((700, 500))), (800, 600));
    assert_eq!(opening_size(Some((1000, 700)), Some((1920, 500))), (800, 600));
}

// With the work area unknown there is nothing to clamp against; the minimum
// still holds.
#[test]
fn an_unknown_work_area_clamps_nothing_but_the_minimum_holds() {
    assert_eq!(opening_size(Some((1000, 700)), None), (1000, 700));
    assert_eq!(opening_size(Some((10, 10)), None), (800, 600));
}

// The commands must stay in scope under their registered names, so a rename
// fails to compile here rather than at runtime in the frontend.
#[test]
fn the_settings_window_commands_are_in_scope() {
    let _ = open_settings_window::<tauri::test::MockRuntime>;
    let _ = finish_settings_close::<tauri::test::MockRuntime>;
    let _ = get_settings_window_context::<tauri::test::MockRuntime>;
}
