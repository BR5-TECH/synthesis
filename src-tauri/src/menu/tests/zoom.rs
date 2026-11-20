//! The Zoom submenu of the Window menu (SNV-FR-LVDP, SNV-FR-QHMA, SNV-FR-IPCN).

use super::*;

// SNV-FR-LVDP: the three command items, their labels, and their accelerators.
#[test]
fn the_zoom_commands_carry_their_labels_and_accelerators() {
    assert_eq!(
        ZOOM_MENU_ITEMS,
        &[
            (MENU_ZOOM_IN, "Zoom In", Some("CmdOrCtrl+=")),
            (MENU_ZOOM_OUT, "Zoom Out", Some("CmdOrCtrl+-")),
            (MENU_ZOOM_RESET, "Actual Size", Some("CmdOrCtrl+0")),
        ]
    );
    assert_eq!(ZOOM_SUBMENU_LABEL, "Zoom");
}

// SNV-FR-XRTA, SNV-FR-NZJR: the window zoom item has a name of its own.
#[test]
fn the_window_zoom_item_reads_zoom_window() {
    assert_eq!(WINDOW_ZOOM_LABEL, "Zoom Window");
}

// SNV-FR-LVDP, EZM-FR-COYK: one step item for each zoom step, named `N%`.
#[test]
fn the_step_items_are_the_zoom_steps() {
    assert_eq!(EDITOR_ZOOM_STEPS, [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300]);
    assert_eq!(zoom_step_label(125), "125%");
    assert_eq!(zoom_step_id(125), "menu:zoom-step:125");
}

// SNV-FR-LVDP: each step id maps back to its step. An id that is not a step is
// not owned.
#[test]
fn a_step_id_maps_to_its_step_and_only_steps_are_owned() {
    for step in EDITOR_ZOOM_STEPS {
        assert_eq!(zoom_step_of(&zoom_step_id(step)), Some(step));
        assert_eq!(menu_event_action(&zoom_step_id(step)), Some(MenuAction::ZoomTo(step)));
    }
    for id in ["menu:zoom-step:33", "menu:zoom-step:", "menu:zoom-step:abc", "menu:zoom-step:-50"] {
        assert_eq!(zoom_step_of(id), None, "{id}");
        assert_eq!(menu_event_action(id), None, "{id}");
    }
}

// SNV-FR-LVDP: the command ids map to their actions.
#[test]
fn the_zoom_command_ids_map_to_their_actions() {
    assert_eq!(menu_event_action(MENU_ZOOM_IN), Some(MenuAction::ZoomIn));
    assert_eq!(menu_event_action(MENU_ZOOM_OUT), Some(MenuAction::ZoomOut));
    assert_eq!(menu_event_action(MENU_ZOOM_RESET), Some(MenuAction::ZoomReset));
}

// SNV-FR-IPCN, SWN-FR-02: no zoom item reaches past an open Settings window.
#[test]
fn no_zoom_action_reaches_past_a_settings_window() {
    for action in
        [MenuAction::ZoomIn, MenuAction::ZoomOut, MenuAction::ZoomReset, MenuAction::ZoomTo(100)]
    {
        assert!(!action.reaches_past_a_settings_window(), "{action:?}");
    }
}

// SNV-FR-QHMA: at a step, the item of that step has the check mark.
#[test]
fn at_a_step_its_item_has_the_check_mark() {
    for step in EDITOR_ZOOM_STEPS {
        assert_eq!(zoom_menu_plan(step, true).checked, Some(step));
        assert_eq!(zoom_menu_plan(step, false).checked, Some(step));
    }
}

// SNV-FR-QHMA: between two steps, no item has the check mark.
#[test]
fn between_two_steps_no_item_has_the_check_mark() {
    for level in [51, 99, 101, 137, 299] {
        assert_eq!(zoom_menu_plan(level, true).checked, None, "{level}");
    }
}

// SNV-FR-IPCN: on a zoomable tab every item is enabled, except Zoom In at 300%
// and Zoom Out at 50%.
#[test]
fn the_items_are_enabled_on_a_zoomable_tab_within_the_limits() {
    let mid = zoom_menu_plan(100, true);
    assert!(mid.zoom_in && mid.zoom_out && mid.reset && mid.steps);
    let top = zoom_menu_plan(300, true);
    assert!(!top.zoom_in && top.zoom_out && top.reset && top.steps);
    let bottom = zoom_menu_plan(50, true);
    assert!(bottom.zoom_in && !bottom.zoom_out && bottom.reset && bottom.steps);
}

// SNV-FR-IPCN: on a tab that is not zoomable every item is disabled.
#[test]
fn every_item_is_disabled_on_a_tab_that_is_not_zoomable() {
    let plan = zoom_menu_plan(100, false);
    assert!(!plan.zoom_in && !plan.zoom_out && !plan.reset && !plan.steps);
}

// SNV-FR-LVDP: every zoom id is distinct and distinct from every other menu id.
#[test]
fn zoom_ids_are_distinct_from_each_other_and_from_other_menu_items() {
    let mut ids: Vec<String> = ZOOM_MENU_ITEMS.iter().map(|(id, ..)| id.to_string()).collect();
    ids.extend(EDITOR_ZOOM_STEPS.iter().map(|step| zoom_step_id(*step)));
    let macos = cfg!(target_os = "macos");
    let mut others = app_menu_order(macos);
    others.extend(file_menu_order(macos));
    others.extend(file_menu_tail(macos));
    others.extend_from_slice(EDIT_MENU_ITEMS);
    others.extend_from_slice(WINDOW_MENU_ITEMS);
    ids.extend(others.iter().map(|(id, ..)| id.to_string()));
    let unique: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len());
}

// SNV-FR-LVDP: no other menu item holds a zoom accelerator.
#[test]
fn each_zoom_accelerator_is_bound_exactly_once() {
    let macos = cfg!(target_os = "macos");
    let mut specs = app_menu_order(macos);
    specs.extend(file_menu_order(macos));
    specs.extend(file_menu_tail(macos));
    specs.extend_from_slice(EDIT_MENU_ITEMS);
    specs.extend_from_slice(WINDOW_MENU_ITEMS);
    specs.extend_from_slice(ZOOM_MENU_ITEMS);
    for accel in ["CmdOrCtrl+=", "CmdOrCtrl+-", "CmdOrCtrl+0"] {
        assert_eq!(specs.iter().filter(|(.., a)| *a == Some(accel)).count(), 1, "{accel}");
    }
}

// SNV-FR-LVDP, PWN-FR-AINZ: a step item emits the shared event with its step,
// and a command item emits its own event.
#[test]
fn activating_a_zoom_item_emits_its_event_and_a_step_carries_its_percent() {
    use std::sync::{Arc, Mutex};
    use tauri::{Listener, Manager};

    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());
    app.manage(ZoomMenuItems::<tauri::test::MockRuntime>::default());
    let seen: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    for event in [MENU_ZOOM_IN, MENU_ZOOM_OUT, MENU_ZOOM_RESET, MENU_ZOOM_SET] {
        let seen = seen.clone();
        app.listen(event, move |e| {
            seen.lock().unwrap().push((event.to_string(), e.payload().to_string()));
        });
    }

    handle_menu_event(&app.handle().clone(), MENU_ZOOM_IN);
    handle_menu_event(&app.handle().clone(), MENU_ZOOM_OUT);
    handle_menu_event(&app.handle().clone(), MENU_ZOOM_RESET);
    handle_menu_event(&app.handle().clone(), &zoom_step_id(175));

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 4, "{seen:?}");
    assert_eq!(seen[0].0, MENU_ZOOM_IN);
    assert_eq!(seen[1].0, MENU_ZOOM_OUT);
    assert_eq!(seen[2].0, MENU_ZOOM_RESET);
    assert_eq!(seen[3].0, MENU_ZOOM_SET);
    let payload: serde_json::Value = serde_json::from_str(&seen[3].1).unwrap();
    assert_eq!(payload["percent"], 175);
}

// SNV-FR-QHMA: a push before the menu is installed only keeps the state.
#[test]
fn setting_the_zoom_menu_state_before_the_menu_is_installed_is_inert() {
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());
    app.manage(ZoomMenuItems::<tauri::test::MockRuntime>::default());

    set_zoom_menu_state(125, true, app.handle().clone(), app.state(), Default::default());
    set_zoom_menu_state(137, false, app.handle().clone(), app.state(), Default::default());
}

#[test]
fn zoom_menu_state_command_compiles_with_a_generic_runtime() {
    let _cmd = set_zoom_menu_state::<tauri::Wry>;
}

// SNV-FR-QHMA: a push keeps the state that a later click shows again.
#[test]
fn a_push_is_remembered_for_the_next_reapply() {
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());
    app.manage(ZoomMenuItems::<tauri::test::MockRuntime>::default());
    assert_eq!(app.state::<ZoomMenuItems<tauri::test::MockRuntime>>().last(), (100, false));

    set_zoom_menu_state(137, true, app.handle().clone(), app.state(), Default::default());

    assert_eq!(app.state::<ZoomMenuItems<tauri::test::MockRuntime>>().last(), (137, true));
}

// SNV-FR-IPCN: the limits hold at the steps next to them and past them.
#[test]
fn the_zoom_limits_hold_next_to_and_past_the_ends() {
    assert!(zoom_menu_plan(299, true).zoom_in);
    assert!(!zoom_menu_plan(301, true).zoom_in);
    assert!(zoom_menu_plan(51, true).zoom_out);
    assert!(!zoom_menu_plan(49, true).zoom_out);
    assert!(!zoom_menu_plan(0, true).zoom_out);
    assert!(!zoom_menu_plan(1000, true).zoom_in);
}

// SNV-FR-NZJR, SNV-FR-XRTA, SNV-FR-LVDP: `build_app_menu` builds the Window menu
// from this table, so its order is the order the author sees. (A menu cannot be
// built in a test: muda needs the main thread of a GUI runtime.)
#[test]
fn the_window_menu_holds_the_zoom_items_in_order() {
    assert_eq!(
        WINDOW_MENU_ORDER,
        &[
            WindowEntry::Minimize,
            WindowEntry::ZoomWindow,
            WindowEntry::ZoomSubmenu,
            WindowEntry::Separator,
            WindowEntry::TabItems,
        ]
    );
    let tabs: Vec<&str> = WINDOW_MENU_ITEMS.iter().map(|(_, label, _)| *label).collect();
    assert_eq!(tabs, ["Next Tab", "Previous Tab"]);
}

// SNV-FR-QHMA: a step click shows the pushed state again before the Settings
// window can swallow it. The command items are plain items and need nothing.
#[test]
fn only_a_step_click_puts_the_check_marks_back() {
    assert!(MenuAction::ZoomTo(175).resyncs_zoom_check_marks());
    for action in [MenuAction::ZoomIn, MenuAction::ZoomOut, MenuAction::ZoomReset, MenuAction::Save]
    {
        assert!(!action.resyncs_zoom_check_marks(), "{action:?}");
    }
}

// SNV-FR-QHMA: with the Settings window open, a step click still runs the
// resync and emits nothing.
#[test]
fn a_step_click_behind_a_settings_window_emits_nothing() {
    assert!(!MenuAction::ZoomTo(175).reaches_past_a_settings_window());
    assert!(MenuAction::ZoomTo(175).resyncs_zoom_check_marks());
}
