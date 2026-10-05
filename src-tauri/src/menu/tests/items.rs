//! The item set, the labels, the accelerators, and the id-to-action routing
//! (SNV-FR-14, SNV-FR-19, SNV-FR-23..31, SWN-FR-02, SWN-FR-14, SWN-FR-16, PPK-FR-15).

use super::*;

#[test]
fn app_menu_builders_are_in_scope() {
    // Compile-time guard for SNV-FR-14: the native-menu builders stay wired
    // with the runtime signature `install_app_menu` calls in `setup`.
    // (Building the real menu requires a GUI runtime, so behavioral
    // role-dispatch — SNV-FR-14, EDT-FR-15, EDT-FR-22 / EDT-FR-15 — is verified manually.)
    let _build = build_app_menu::<tauri::Wry, tauri::AppHandle<tauri::Wry>>;
    let _install = install_app_menu::<tauri::Wry>;
    // SNV-FR-24/25: the menu-event dispatcher stays wired with the runtime
    // signature `on_menu_event` calls in `run()`.
    let _dispatch = handle_menu_event::<tauri::Wry>;
}

// ------------------------------------------------------------------
// File menu (SNV-FR-23..26). The native menu itself needs a GUI runtime to
// build, so SNV-FR-23, SWN-FR-14 (the menu renders with its four items in order) and
// SNV-FR-26 (Exit quits via the native role) are verified manually — but the
// load-bearing logic is pure and pinned here: the item set/order the menu
// is built from (FILE_MENU_ITEMS), the id->action mapping (NFI-FR-06, NFW-FR-06, NTA-FR-07, SNV-FR-24/19),
// and the Close-Project teardown (OVW-FR-02, PST-FR-14, ASC-FR-14 / SNV-FR-25).
// ------------------------------------------------------------------

#[test]
fn file_menu_items_are_the_three_creation_items_then_save_save_all_settings_close_project() {
    // SNV-FR-23 / SWN-FR-14: `build_app_menu` iterates `file_menu_order` to
    // construct the custom items, so pinning that order pins the rendered
    // one (Exit is appended after them, behind a separator). The creation
    // cluster leads — New File, New Artifact, New Folder, matching the
    // Library folder context menu's order (LCM-FR-01) — then Save and Save
    // All carrying the two save accelerators (SNV-FR-28 / SNV-FR-30), then
    // the two settings entries, then Close Project.
    assert_eq!(
        file_menu_order(false),
        vec![
            (MENU_NEW_FILE, "New File", None),
            (MENU_NEW_ARTIFACT, "New Artifact", None),
            (MENU_NEW_FOLDER, "New Folder", None),
            (MENU_SAVE, "Save", Some("CmdOrCtrl+S")),
            (MENU_SAVE_ALL, "Save All", Some("CmdOrCtrl+Shift+S")),
            (MENU_GLOBAL_SETTINGS, "Global settings", None),
            (MENU_PROJECT_SETTINGS, "Project settings", None),
            (MENU_CLOSE_PROJECT, "Close Project", None),
        ]
    );
}

// SWN-FR-14 / SNV-FR-23: on macOS the File menu holds the same items
// WITHOUT the two settings entries, which are in the app-name menu instead,
// Global settings first.
#[test]
fn on_macos_the_settings_entries_are_in_the_app_name_menu_and_not_the_file_menu() {
    let file: Vec<&str> = file_menu_order(true).iter().map(|(id, ..)| *id).collect();
    assert_eq!(
        file,
        [
            MENU_NEW_FILE,
            MENU_NEW_ARTIFACT,
            MENU_NEW_FOLDER,
            MENU_SAVE,
            MENU_SAVE_ALL,
            MENU_CLOSE_PROJECT,
        ]
    );
    let app: Vec<&str> = app_menu_order(true).iter().map(|(id, ..)| *id).collect();
    assert_eq!(
        app,
        [
            MENU_GLOBAL_SETTINGS,
            MENU_PROJECT_SETTINGS,
            MENU_ABOUT,
            MENU_QUIT
        ]
    );
    // And on every other platform the app-name menu carries Quit alone.
    let app_elsewhere: Vec<&str> =
        app_menu_order(false).iter().map(|(id, ..)| *id).collect();
    assert_eq!(app_elsewhere, [MENU_QUIT]);
}

// ABT-FR-KMVD / SNV-FR-23 / SNV-FR-KXPE: About sits immediately before the quit
// item of the menu that carries the settings entries — the app-name menu on
// macOS, the File menu elsewhere — and it is in no other menu.
#[test]
fn about_sits_immediately_before_the_quit_item_of_the_platform_menu() {
    let tail: Vec<&str> = file_menu_tail(false).iter().map(|(id, ..)| *id).collect();
    assert_eq!(tail, [MENU_ABOUT, MENU_EXIT]);
    assert_eq!(ABOUT_ITEM.1, "About");
    assert_eq!(ABOUT_ITEM.2, None);
    assert!(
        !file_menu_order(false).iter().any(|(id, ..)| *id == MENU_ABOUT),
        "About is not among the items before the separator"
    );

    let mac_tail: Vec<&str> = file_menu_tail(true).iter().map(|(id, ..)| *id).collect();
    assert_eq!(mac_tail, [MENU_EXIT], "the macOS File menu carries no About");
    let app: Vec<&str> = app_menu_order(true).iter().map(|(id, ..)| *id).collect();
    assert_eq!(&app[app.len() - 2..], [MENU_ABOUT, MENU_QUIT]);

    let app_elsewhere: Vec<&str> =
        app_menu_order(false).iter().map(|(id, ..)| *id).collect();
    assert!(!app_elsewhere.contains(&MENU_ABOUT));
}

// ABT-FR-KMVD, ABT-FR-NVQT: the About entry relays its intent to the window
// as the event of the same name, and it does not reach past an open settings
// window, because it acts on the window that settings window blocks.
#[test]
fn the_about_entry_relays_an_event_and_is_blocked_by_a_settings_window() {
    assert_eq!(MENU_ABOUT, "menu:about");
    assert_eq!(menu_event_action(MENU_ABOUT), Some(MenuAction::About));
    assert!(!MenuAction::About.reaches_past_a_settings_window());
}

// SWN-FR-16 / PPK-FR-15: the entry is offered only while a project is open,
// and the presence claim is what makes each transition happen exactly once.
// A double insert would put two Project settings entries in the menu, and a
// double remove would throw away the handle's place in it.
#[test]
fn the_project_settings_entry_changes_presence_at_most_once_per_state() {
    let items = SettingsMenuItems::<tauri::test::MockRuntime>::default();
    // Built present, because the menu is constructed with the entry in it.
    assert!(!items.claim(true), "already present");
    // The application starts on the Project picker, which takes it out.
    assert!(items.claim(false), "taken out");
    assert!(!items.claim(false), "and not taken out twice");
    // Opening a project puts it back, once.
    assert!(items.claim(true), "put back");
    assert!(!items.claim(true), "and not inserted twice");
}

// SWN-FR-16: the entry is taken out of, and put back into, the position the
// order table gives it — so an item added before it cannot leave it landing
// somewhere else in the menu when a project reopens.
#[test]
fn the_project_settings_entry_returns_to_the_index_its_menu_gives_it() {
    assert_eq!(
        project_settings_index(false),
        file_menu_order(false)
            .iter()
            .position(|(id, ..)| *id == MENU_PROJECT_SETTINGS)
            .unwrap()
    );
    assert_eq!(
        project_settings_index(true),
        app_menu_order(true)
            .iter()
            .position(|(id, ..)| *id == MENU_PROJECT_SETTINGS)
            .unwrap()
    );
}

// SWN-FR-02: while a settings window is open the parent takes no
// menu interaction — but the two settings entries and the quit are not the
// parent's, so they still reach through.
#[test]
fn only_the_settings_entries_and_the_quit_reach_past_an_open_settings_window() {
    for action in [
        MenuAction::GlobalSettings,
        MenuAction::ProjectSettings,
        MenuAction::Exit,
    ] {
        assert!(action.reaches_past_a_settings_window(), "{action:?}");
    }
    for action in [
        MenuAction::NewFile,
        MenuAction::NewFolder,
        MenuAction::NewArtifact,
        MenuAction::Save,
        MenuAction::SaveAll,
        MenuAction::CloseProject,
        MenuAction::Find,
        MenuAction::FindReplace,
    ] {
        assert!(!action.reaches_past_a_settings_window(), "{action:?}");
    }
}

#[test]
fn the_state_driven_items_render_disabled_and_every_other_item_enabled() {
    // SNV-FR-28 / SNV-FR-30 / SNV-FR-43: at startup nothing is open, nothing
    // is unsaved, and no Editor is active, so all four state-driven items
    // start greyed out — which is also what makes their accelerators inert
    // until the frontend enables them. A regression that started them
    // enabled would bind a live ⌘S to a write of whatever the (nonexistent)
    // active tab held, and a live ⌘F to a panel with no Editor to open in.
    for id in INITIALLY_DISABLED {
        assert!(!starts_enabled(id), "{id} must start disabled");
    }
    for (id, label, _) in all_menu_item_specs() {
        if INITIALLY_DISABLED.contains(&id) {
            continue;
        }
        assert!(
            starts_enabled(id),
            "{label:?} ({id}) must render enabled — only the state-driven items are not"
        );
    }
}

#[test]
fn edit_menu_carries_find_and_find_replace_with_their_accelerators() {
    // SNV-FR-43: `build_app_menu` iterates EDIT_MENU_ITEMS to construct the
    // two custom Edit-menu items, so pinning this list pins their labels,
    // their order below Select All, and the chords they carry. The native
    // menu needs a GUI runtime to build, so this is the structural proxy for
    // the visible half of SNV-FR-43, EFR-FR-AYNZ.
    assert_eq!(
        EDIT_MENU_ITEMS,
        &[
            (MENU_FIND, "Find", Some("CmdOrCtrl+F")),
            (MENU_FIND_REPLACE, "Find && Replace", Some("CmdOrCtrl+R")),
        ]
    );
}

/// What the menu layer renders from a label: a lone `&` marks a mnemonic and
/// is stripped, `&&` collapses to a literal `&`. Mirrors muda's own
/// `strip_mnemonic`, so the assertions below are about what the user reads
/// rather than about how the label happens to be spelled.
fn rendered(label: &str) -> String {
    label.replace("&&", "\u{0}").replace('&', "").replace('\u{0}', "&")
}

#[test]
fn the_find_replace_item_reads_as_find_and_replace() {
    // SNV-FR-43 names the item "Find & Replace". Spelled with a single `&`
    // the ampersand is eaten as a mnemonic and the user sees "Find  Replace"
    // — the label still builds, so only this catches it.
    let (_, label, _) = EDIT_MENU_ITEMS[1];
    assert_eq!(rendered(label), "Find & Replace");
}

#[test]
fn every_menu_label_reads_with_each_word_capitalised() {
    // SNV-FR-23, SWN-FR-14: menu labels are title-cased — every word's first letter is
    // capital — and consistently so across every menu, present and future.
    // "Save All" and "New Folder" are the shape; a lowercase "New folder"
    // still builds and still routes, so only this catches the drift.
    //
    // The two settings entries are the stated exception: SWN-FR-14 names
    // them "Global settings" and "Project settings" in sentence case, which
    // is also how every surface in the application refers to the two
    // windows. Spelling them differently in the menu alone would be the
    // drift this test exists to catch, in the other direction.
    let sentence_case = [MENU_GLOBAL_SETTINGS, MENU_PROJECT_SETTINGS];
    for (id, label, _) in all_menu_item_specs() {
        if sentence_case.contains(&id) {
            continue;
        }
        for word in rendered(label).split_whitespace() {
            let Some(first) = word.chars().next() else {
                continue;
            };
            // Skip words that do not start with a letter at all (the
            // ampersand in "Find & Replace", an ellipsis, a digit).
            if !first.is_alphabetic() {
                continue;
            }
            assert!(
                first.is_uppercase(),
                "{label:?} ({id}): word {word:?} must start with a capital"
            );
        }
    }
}

// SWN-FR-14: the two entries read exactly as the spec names them, and in
// that order — Global settings first.
#[test]
fn the_settings_entries_read_as_the_spec_names_them() {
    assert_eq!(
        SETTINGS_MENU_ITEMS,
        &[
            (MENU_GLOBAL_SETTINGS, "Global settings", None),
            (MENU_PROJECT_SETTINGS, "Project settings", None),
        ]
    );
}

#[test]
fn no_menu_label_carries_an_unescaped_ampersand() {
    // The same trap for every other item, present and future: an ampersand
    // that is not doubled silently vanishes from the rendered menu.
    for (id, label, _) in all_menu_item_specs() {
        assert!(
            !label.replace("&&", "").contains('&'),
            "{label:?} ({id}) carries an unescaped `&`: it renders as {:?}",
            rendered(label)
        );
    }
}

#[test]
fn each_find_accelerator_is_bound_exactly_once() {
    // SNV-FR-43: ⌘F and ⌘R are each bound, and to one item. An unbound chord
    // is a shortcut that does nothing; a doubly-bound one is undefined
    // behaviour in the native menu.
    for (chord, expected) in [
        ("CmdOrCtrl+F", MENU_FIND),
        ("CmdOrCtrl+R", MENU_FIND_REPLACE),
    ] {
        let bound: Vec<&str> = all_menu_item_specs()
            .iter()
            .filter(|(.., accel)| *accel == Some(chord))
            .map(|(id, ..)| *id)
            .collect();
        assert_eq!(bound, [expected], "{chord} must be bound to {expected} alone");
    }
}

#[test]
fn the_find_items_the_enablement_handles_need_are_declared_in_the_menu() {
    // `build_app_menu` captures the Find / Find & Replace handles by matching
    // on these ids while iterating EDIT_MENU_ITEMS. If an id drifted out of
    // that list the capture would silently yield `None`, the menu would still
    // build, and `set_find_menu_state` would go inert — ⌘F stuck greyed out
    // forever with no error anywhere.
    let ids: Vec<&str> = EDIT_MENU_ITEMS.iter().map(|(id, ..)| *id).collect();
    assert!(ids.contains(&MENU_FIND), "Find must be an Edit-menu item");
    assert!(
        ids.contains(&MENU_FIND_REPLACE),
        "Find & Replace must be an Edit-menu item"
    );
}

#[test]
fn each_find_item_captures_its_own_enablement_slot() {
    // Swapping the arms is invisible in every other test — the handles need a
    // GUI runtime to build — but would grey out Find when Find & Replace
    // should be greyed and vice versa.
    assert_eq!(find_slot(MENU_FIND), Some(FindSlot::Find));
    assert_eq!(find_slot(MENU_FIND_REPLACE), Some(FindSlot::FindReplace));
    for (id, label, _) in all_menu_item_specs() {
        if id == MENU_FIND || id == MENU_FIND_REPLACE {
            continue;
        }
        assert_eq!(find_slot(id), None, "{label:?} ({id}) owns no find slot");
    }
}

#[test]
fn activating_find_and_find_replace_emits_their_own_channel() {
    // SNV-FR-43 / EFR-FR-AYNZ / EFR-FR-BJUY: the two items must reach the
    // frontend on distinct channels. A swap is a one-character change that
    // makes ⌘F open Find & Replace — the id->action mapping alone cannot
    // catch it, since both ids map to *some* valid action either way.
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tauri::Listener;

    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());

    for (activated, expected, other) in [
        (MENU_FIND, MENU_FIND, MENU_FIND_REPLACE),
        (MENU_FIND_REPLACE, MENU_FIND_REPLACE, MENU_FIND),
    ] {
        let hit = Arc::new(AtomicBool::new(false));
        let wrong = Arc::new(AtomicBool::new(false));
        let (h, w) = (hit.clone(), wrong.clone());
        app.listen(expected, move |_| h.store(true, Ordering::SeqCst));
        app.listen(other, move |_| w.store(true, Ordering::SeqCst));

        handle_menu_event(&app.handle().clone(), activated);

        assert!(hit.load(Ordering::SeqCst), "{activated} must emit {expected}");
        assert!(
            !wrong.load(Ordering::SeqCst),
            "{activated} must not emit {other}"
        );
    }
}

// ABT-FR-KMVD: activating About reaches the window on the `menu:about` channel.
#[test]
fn activating_about_emits_the_about_channel() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tauri::Listener;

    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());
    let hit = Arc::new(AtomicBool::new(false));
    let h = hit.clone();
    app.listen(MENU_ABOUT, move |_| h.store(true, Ordering::SeqCst));

    handle_menu_event(&app.handle().clone(), MENU_ABOUT);

    assert!(hit.load(Ordering::SeqCst), "About must emit {MENU_ABOUT}");
}

#[test]
fn setting_the_find_menu_state_before_the_menu_is_installed_is_inert() {
    // The frontend pushes its first enablement as the shell mounts, which can
    // precede `install_app_menu` storing the handles. With none stored this
    // must return quietly rather than unwrapping — a panic inside a command
    // aborts the IPC call and the frontend's render path with it.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());
    app.manage(FindMenuItems::<tauri::test::MockRuntime>::default());

    set_find_menu_state(true, app.handle().clone(), app.state());
    set_find_menu_state(false, app.handle().clone(), app.state());
}

#[test]
fn find_menu_state_command_compiles_with_a_generic_runtime() {
    let _cmd = set_find_menu_state::<tauri::Wry>;
}

#[test]
fn the_save_items_the_enablement_handles_need_are_declared_in_the_menu() {
    // `build_app_menu` captures the Save / Save All handles by matching on
    // these ids while iterating FILE_MENU_ITEMS. If an id ever drifted out
    // of that list the capture would silently yield `None`, the menu would
    // still build, and `set_save_menu_state` would go inert — Save would be
    // stuck greyed out forever with no error anywhere.
    let ids: Vec<&str> = file_menu_order(cfg!(target_os = "macos"))
        .iter()
        .map(|(id, ..)| *id)
        .collect();
    assert!(ids.contains(&MENU_SAVE), "Save must be a File-menu item");
    assert!(
        ids.contains(&MENU_SAVE_ALL),
        "Save All must be a File-menu item"
    );
}

#[test]
fn each_save_accelerator_is_bound_exactly_once() {
    // SNV-FR-28 / SNV-FR-30: ⌘S and ⇧⌘S are each bound, and to one item —
    // an unbound chord is a shortcut that does nothing, and a doubly-bound
    // one is undefined behaviour in the native menu.
    for (chord, expected) in [("CmdOrCtrl+S", MENU_SAVE), ("CmdOrCtrl+Shift+S", MENU_SAVE_ALL)] {
        let bound: Vec<&str> = all_menu_item_specs()
            .iter()
            .filter(|(.., accel)| *accel == Some(chord))
            .map(|(id, ..)| *id)
            .collect();
        assert_eq!(bound, [expected], "{chord} must be bound to {expected} alone");
    }
}

/// Every custom item `build_app_menu` renders, in the order it renders them.
fn all_menu_item_specs() -> Vec<MenuItemSpec> {
    let macos = cfg!(target_os = "macos");
    let mut specs = app_menu_order(macos);
    specs.extend(file_menu_order(macos));
    specs.extend(file_menu_tail(macos));
    specs.extend_from_slice(EDIT_MENU_ITEMS);
    specs
}

#[test]
fn menu_item_ids_are_distinct_and_nonempty() {
    // The ids double as event names; a collision or an empty id would make a
    // menu item silently un-routable (the silent-failure class). Every
    // custom item is included — the two quit entry points as much as the
    // three File items.
    let ids: Vec<&str> = all_menu_item_specs().iter().map(|(id, ..)| *id).collect();
    for id in &ids {
        assert!(!id.is_empty(), "menu id must be non-empty");
    }
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        ids.len(),
        "menu ids must be globally distinct"
    );
}

#[test]
fn every_menu_item_id_is_routable() {
    // A menu item whose id `menu_event_action` does not own is inert: it
    // renders, the user clicks it, and nothing happens.
    for (id, label, _) in all_menu_item_specs() {
        assert!(
            menu_event_action(id).is_some(),
            "{label:?} ({id}) renders but is not routed to an action"
        );
    }
}

#[test]
fn exactly_one_menu_item_carries_the_quit_accelerator() {
    // SNV-FR-26: the platform-standard quit chord is bound, and bound once.
    // `build_app_menu` builds every item from these specs, so an accelerator
    // missing here is an accelerator missing from the menu — ⌘Q would do
    // nothing at all. Two items carrying it would be an undefined binding.
    let bound: Vec<&str> = all_menu_item_specs()
        .iter()
        .filter(|(.., accel)| *accel == Some("CmdOrCtrl+Q"))
        .map(|(id, ..)| *id)
        .collect();
    assert_eq!(bound, [MENU_EXIT], "⌘Q must be bound to Exit, and only Exit");
}

#[test]
fn menu_event_action_maps_each_file_item() {
    // NFI-FR-06, NFW-FR-06, NTA-FR-07, SNV-FR-24 / OVW-FR-02, PST-FR-14, ASC-FR-14, SNV-FR-25: each owned id resolves to its action.
    assert_eq!(menu_event_action(MENU_NEW_FOLDER), Some(MenuAction::NewFolder));
    assert_eq!(
        menu_event_action(MENU_NEW_ARTIFACT),
        Some(MenuAction::NewArtifact)
    );
    assert_eq!(menu_event_action(MENU_SAVE), Some(MenuAction::Save));
    assert_eq!(menu_event_action(MENU_SAVE_ALL), Some(MenuAction::SaveAll));
    assert_eq!(
        menu_event_action(MENU_CLOSE_PROJECT),
        Some(MenuAction::CloseProject)
    );
}

#[test]
fn each_save_item_captures_its_own_enablement_slot() {
    // `build_app_menu` captures the two handles by this mapping. Swapping
    // the arms would grey out Save when Save All should be greyed and vice
    // versa — invisible in every other test, because the handles need a GUI
    // runtime to build and the symptom only exists in a rendered menu.
    assert_eq!(save_slot(MENU_SAVE), Some(SaveSlot::Save));
    assert_eq!(save_slot(MENU_SAVE_ALL), Some(SaveSlot::SaveAll));
    // No other item has live enablement; claiming a slot for one would make
    // it fight the save items for the same handle.
    for (id, label, _) in all_menu_item_specs() {
        if id == MENU_SAVE || id == MENU_SAVE_ALL {
            continue;
        }
        assert_eq!(save_slot(id), None, "{label:?} ({id}) owns no save slot");
    }
}

#[test]
fn activating_save_and_save_all_emits_their_own_channel() {
    // The dispatch half of the contract, driven through the real
    // `handle_menu_event`: Save must reach the frontend as `"menu:save"` and
    // Save All as `"menu:save-all"`. A swap here is a one-character change
    // that makes ⌘S write every dirty artifact in the project instead of the
    // active tab — the id->action mapping alone cannot catch it, since both
    // ids map to *some* valid action either way.
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tauri::Listener;

    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());

    for (activated, expected, other) in [
        (MENU_SAVE, MENU_SAVE, MENU_SAVE_ALL),
        (MENU_SAVE_ALL, MENU_SAVE_ALL, MENU_SAVE),
    ] {
        let hit = Arc::new(AtomicBool::new(false));
        let wrong = Arc::new(AtomicBool::new(false));
        let (h, w) = (hit.clone(), wrong.clone());
        app.listen(expected, move |_| h.store(true, Ordering::SeqCst));
        app.listen(other, move |_| w.store(true, Ordering::SeqCst));

        handle_menu_event(&app.handle().clone(), activated);

        assert!(hit.load(Ordering::SeqCst), "{activated} must emit {expected}");
        assert!(
            !wrong.load(Ordering::SeqCst),
            "{activated} must not emit {other}"
        );
    }
}

#[test]
fn both_quit_entry_points_are_owned_and_map_to_exit() {
    // SNV-FR-26 / EDT-FR-33: Exit must NOT be the predefined quit role. That
    // role hands the quit to the platform (`NSApp terminate:` on macOS),
    // which never reaches this side — the process dies with the pending
    // Editor writes still in the buffer. Owning both ids is what routes them
    // through the flush. A regression here is silent data loss on ⌘Q.
    assert_eq!(menu_event_action(MENU_EXIT), Some(MenuAction::Exit));
    assert_eq!(menu_event_action(MENU_QUIT), Some(MenuAction::Exit));
    assert_ne!(MENU_EXIT, MENU_QUIT, "the two items need distinct ids");
}

#[test]
fn menu_event_action_ignores_unowned_ids() {
    // Predefined roles (Edit/Window) and any stray id are not ours; the
    // dispatcher must no-op on them rather than misfire a menu action.
    for id in ["", "undo", "quit", "Exit", "menu:unknown", "menu:new-folderX"] {
        assert_eq!(menu_event_action(id), None, "id {id:?} must be unowned");
    }
}

