//! The per-project slot: the active worktree and the saved layout.
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// -- Per-project slot (GSS-FR-16 / GSS-FR-17 / GSS-FR-18) --------------
//
// These use `with_path` rather than `in_memory`, deliberately: the slot is
// the first table-of-tables in the file, and the TOML serialiser rejects a
// struct that emits a sub-table before its scalar values. An in-memory
// store never calls `persist`, so it would prove nothing about the shape
// that actually has to reach disk — and `save_active_worktree`'s only
// caller logs its error and carries on, so a serialisation break would be
// completely silent in production.

#[test]
fn a_per_project_slot_survives_a_round_trip_through_disk() {
    // GSS-FR-16: reopening the project returns the worktree it was last
    // working in.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");

    {
        let store = GlobalSettingsStore::with_path(path.clone());
        store
            .save_active_worktree("/dev/acme", "/dev/acme-main")
            .unwrap();
        store
            .save_project_layout(
                "/dev/acme",
                crate::layout::LayoutPreferences {
                    vertical_panel_fraction: 0.28,
                    main_window_maximized: true,
                    ..Default::default()
                },
            )
            .unwrap();
        // A second project, and the no-project-open default slot.
        store.save_active_worktree("/dev/other", "/dev/other").unwrap();
        store
            .save_project_layout(
                "",
                crate::layout::LayoutPreferences {
                    vertical_panel_fraction: 0.45,
                    ..Default::default()
                },
            )
            .unwrap();
    }

    let reloaded = GlobalSettingsStore::with_path(path.clone());
    assert_eq!(
        reloaded.load_active_worktree("/dev/acme").unwrap(),
        Some("/dev/acme-main".to_string()),
    );
    assert_eq!(
        reloaded
            .load_project_layout("/dev/acme")
            .unwrap()
            .unwrap()
            .vertical_panel_fraction,
        0.28,
    );
    assert!(
        reloaded
            .load_project_layout("/dev/acme")
            .unwrap()
            .unwrap()
            .main_window_maximized
    );
    assert_eq!(
        reloaded.load_active_worktree("/dev/other").unwrap(),
        Some("/dev/other".to_string()),
        "one project's slot does not overwrite another's"
    );
    assert_eq!(
        reloaded.load_project_layout("").unwrap().unwrap().vertical_panel_fraction,
        0.45,
        "the no-project-open default slot round-trips too"
    );
    // And the write really happened, rather than failing into a log line.
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("acme-main"), "{text}");
}

#[test]
fn saving_a_slot_leaves_the_rest_of_the_store_intact() {
    // The slot lives alongside recents and the registries in one file; a
    // slot write must not drop them.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    let store = GlobalSettingsStore::with_path(path.clone());
    store.record_recent_project("acme", "/dev/acme").unwrap();
    store
        .save_app_preferences(AppPreferences { theme: Theme::Dark, ..Default::default() })
        .unwrap();

    store
        .save_active_worktree("/dev/acme", "/dev/acme-main")
        .unwrap();

    let reloaded = GlobalSettingsStore::with_path(path);
    assert_eq!(reloaded.list_recent_projects().unwrap().len(), 1);
    assert_eq!(reloaded.load_app_preferences().unwrap().theme, Theme::Dark);
    assert_eq!(
        reloaded.load_active_worktree("/dev/acme").unwrap(),
        Some("/dev/acme-main".to_string()),
    );
}

#[test]
fn a_layout_slot_written_before_the_fraction_rename_still_loads() {
    // The panel width moved from a pixel count (`verticalPanelWidth`) to a
    // fraction (`verticalPanelFraction`). The rename is safe only because
    // serde ignores the unknown key — nothing here uses
    // `deny_unknown_fields`. If that ever changed, the WHOLE
    // `synthesis.toml` would fail to parse and GSS-FR-13 would "repair" it
    // to defaults, wiping recent projects, pins, and both registries. This
    // test fails loudly if that day comes.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    std::fs::write(
        &path,
        "[appPreferences]\ntheme = \"dark\"\n\n\
         [[recentProjects]]\nname = \"acme\"\npath = \"/dev/acme\"\n\
         lastOpenedAt = \"2026-01-01T00:00:00Z\"\npinned = true\nmissing = false\n\n\
         [projects.\"/dev/acme\".layout]\n\
         verticalPanelWidth = 280.0\n\
         mainWindowOuterWidth = 1600.0\n\
         mainWindowOuterHeight = 1000.0\n",
    )
    .unwrap();

    let store = GlobalSettingsStore::with_path(path);

    // The store did NOT fall back to repaired defaults: everything else in
    // the file survived the unknown key.
    assert_eq!(store.load_app_preferences().unwrap().theme, Theme::Dark);
    let recents = store.list_recent_projects().unwrap();
    assert_eq!(recents.len(), 1, "recents must survive the legacy key");
    assert!(recents[0].pinned);

    let layout = store.load_project_layout("/dev/acme").unwrap().unwrap();
    // The stale pixel value is dropped rather than reinterpreted as a
    // fraction — 280.0 as a fraction would be nonsense. Zero means "never
    // persisted", and the shell substitutes its own default (SNV-FR-34).
    assert_eq!(layout.vertical_panel_fraction, 0.0);
    // And the fields that did not change are still there.
    assert_eq!(layout.main_window_outer_width, 1600.0);
    assert_eq!(layout.main_window_outer_height, 1000.0);
}

#[test]
fn a_project_with_no_slot_reads_back_as_unset() {
    // GSS-FR-16: a project that has never activated a worktree, and one
    // whose file predates the slot entirely, both read as unset so the
    // caller falls back to the primary worktree.
    let store = GlobalSettingsStore::in_memory();
    assert_eq!(store.load_active_worktree("/dev/acme").unwrap(), None);
    assert_eq!(store.load_project_layout("/dev/acme").unwrap(), None);
}

#[test]
fn a_file_written_before_the_slot_existed_still_loads() {
    // GSS-FR-13: `#[serde(default)]` is what makes the new tables optional.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    std::fs::write(
        &path,
        "[appPreferences]\ntheme = \"dark\"\n",
    )
    .unwrap();

    let store = GlobalSettingsStore::with_path(path);
    assert_eq!(store.load_app_preferences().unwrap().theme, Theme::Dark);
    assert_eq!(store.load_active_worktree("/dev/acme").unwrap(), None);
}
