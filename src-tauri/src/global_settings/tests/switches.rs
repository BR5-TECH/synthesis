//! The user-global switches: selection follows tab, notifications, and the
//! commit action of the changes panel.
//!
//! One part of `mod.rs`, which holds the imports and the helpers these use.

use super::*;

// ---- Selection follows tab (GSS-FR-33) -------------------------------

#[test]
fn selection_follows_tab_defaults_to_on_including_for_a_record_written_before_the_field() {
    // GSS-FR-20 / GSS-FR-33. The same two defaults that have to agree for
    // the notifications switch, and for the same reason: the requirement is
    // explicit that an existing record with no such field reads back ON, so
    // the behaviour is opt-out for an author mid-upgrade exactly as it is
    // for a fresh install.
    assert!(
        AppPreferences::default().selection_follows_tab,
        "a fresh machine follows the active tab until the author says otherwise"
    );

    let legacy = r#"
        theme = "dark"
        mainWindowFullscreen = false
        searchQueryMode = "regex"
        notificationsEnabled = false
    "#;
    let decoded: AppPreferences = toml::from_str(legacy).unwrap();
    assert!(
        decoded.selection_follows_tab,
        "an upgrade must not silently opt an existing author out"
    );
    assert_eq!(decoded.theme, Theme::Dark, "and the rest still decodes");
    assert!(
        !decoded.notifications_enabled,
        "including a field the record did carry"
    );
}

#[test]
fn the_selection_follows_tab_switch_round_trips_through_the_store() {
    // GSS-FR-33, GSS-FR-20: written off, it reads back off across a reload, because
    // this module persists the flag rather than interpreting it.
    let store = GlobalSettingsStore::in_memory();
    let mut prefs = store.load_app_preferences().unwrap();
    assert!(prefs.selection_follows_tab, "on before anything is written");

    prefs.selection_follows_tab = false;
    store.save_app_preferences(prefs).unwrap();
    assert!(!store.load_app_preferences().unwrap().selection_follows_tab);
}

#[test]
fn the_selection_follows_tab_switch_is_user_global_not_per_project() {
    // GSS-FR-20's other half, and the whole point of GSS-FR-33 putting this
    // in the app-preferences record rather than the per-project slot: the
    // flag describes how the author navigates, so every project reads the
    // one they set. A round-trip in a single project cannot show that.
    let store = GlobalSettingsStore::in_memory();
    let mut prefs = store.load_app_preferences().unwrap();
    prefs.selection_follows_tab = false;
    store.save_app_preferences(prefs).unwrap();

    // Neither project's slot carries the flag: it is written once, in the
    // app-preferences record, whichever project happens to be open.
    assert!(!store.load_app_preferences().unwrap().selection_follows_tab);
}

#[test]
fn the_selection_follows_tab_switch_is_read_by_every_project() {
    // GSS-FR-33, GSS-FR-20's cross-project clause, against a real file rather than an
    // in-memory store: the flag has to be in the app-preferences record and
    // nowhere else, so a relaunch into a *different* project reads it back.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");

    {
        let store = GlobalSettingsStore::with_path(path.clone());
        let mut prefs = store.load_app_preferences().unwrap();
        prefs.selection_follows_tab = false;
        store.save_app_preferences(prefs).unwrap();
        // Two projects opened, each getting a per-project slot of its own.
        store
            .save_project_layout(
                "/dev/acme",
                crate::layout::LayoutPreferences::default(),
            )
            .unwrap();
        store
            .save_project_layout(
                "/dev/beta",
                crate::layout::LayoutPreferences::default(),
            )
            .unwrap();
    }

    // A fresh store over the same file — the relaunch.
    let reloaded = GlobalSettingsStore::with_path(path.clone());
    assert!(
        !reloaded.load_app_preferences().unwrap().selection_follows_tab,
        "the flag is user-global, so it reads back whichever project is open"
    );

    // And it is stored exactly once, rather than copied into either slot.
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        text.matches("selectionFollowsTab").count(),
        1,
        "expected one occurrence, in the app-preferences record: {text}"
    );
}

// ---- Notifications switch (GSS-FR-32) --------------------------------

#[test]
fn notifications_default_to_on_including_for_a_record_written_before_the_field() {
    // GSS-FR-04, GSS-FR-19, GSS-FR-21, GSS-FR-24, GSS-FR-25, GSS-FR-29, GSS-FR-33, GSS-FR-34 / GSS-FR-32. Two different defaults have to agree, and
    // `bool::default()` is the wrong answer for both:
    //   - `AppPreferences::default()`, for a machine with no stored record;
    //   - serde's field default, for a record stored before the field
    //     existed, which is every record already on an author's disk.
    assert!(
        AppPreferences::default().notifications_enabled,
        "a fresh machine notifies until the author says otherwise"
    );

    let legacy = r#"
        theme = "dark"
        mainWindowFullscreen = false
        searchQueryMode = "regex"
    "#;
    let decoded: AppPreferences = toml::from_str(legacy).unwrap();
    assert!(
        decoded.notifications_enabled,
        "an upgrade must not silently turn notifications off"
    );
    assert_eq!(decoded.theme, Theme::Dark, "and the rest still decodes");
}

#[test]
fn the_notifications_switch_round_trips_through_the_store() {
    // GSS-FR-32, GSS-FR-20: written off, it reads back off across a reload, because
    // this module persists the flag rather than interpreting it.
    let store = GlobalSettingsStore::in_memory();
    let mut prefs = store.load_app_preferences().unwrap();
    assert!(prefs.notifications_enabled, "on before anything is written");

    prefs.notifications_enabled = false;
    store.save_app_preferences(prefs).unwrap();
    assert!(!store.load_app_preferences().unwrap().notifications_enabled);
}

// ---- Changes panel commit action (GSS-FR-25) -------------------------

#[test]
fn the_commit_action_defaults_to_commit_and_survives_every_other_write() {
    // GSS-FR-25, GSS-FR-20 / GSS-FR-04, GSS-FR-19, GSS-FR-21, GSS-FR-24, GSS-FR-29, GSS-FR-32, GSS-FR-33, GSS-FR-34.
    assert_eq!(
        AppPreferences::default().changes_commit_action,
        ChangesCommitAction::Commit
    );

    let store = GlobalSettingsStore::in_memory();
    assert_eq!(
        store.load_app_preferences().unwrap().changes_commit_action,
        ChangesCommitAction::Commit,
        "a user who has never chosen reads back the default"
    );

    // The Changes panel's own write.
    let mut editing = store.load_app_preferences().unwrap();
    editing.changes_commit_action = ChangesCommitAction::CommitAndPush;
    store.save_app_preferences(editing).unwrap();
    assert_eq!(
        store.load_app_preferences().unwrap().changes_commit_action,
        ChangesCommitAction::CommitAndPush
    );

    // A Diff tab then edits its own field, carrying the rest through.
    let mut editing = store.load_app_preferences().unwrap();
    editing.diff_visualization_mode = DiffVisualizationMode::Final;
    store.save_app_preferences(editing).unwrap();
    let after = store.load_app_preferences().unwrap();
    assert_eq!(after.diff_visualization_mode, DiffVisualizationMode::Final);
    assert_eq!(
        after.changes_commit_action,
        ChangesCommitAction::CommitAndPush
    );

    // GSS-FR-25: a stored `push` is persisted unchanged; this module does
    // not interpret whether the action is currently available.
    let mut editing = store.load_app_preferences().unwrap();
    editing.changes_commit_action = ChangesCommitAction::Push;
    store.save_app_preferences(editing).unwrap();
    assert_eq!(
        store.load_app_preferences().unwrap().changes_commit_action,
        ChangesCommitAction::Push
    );
}

#[test]
fn the_commit_action_round_trips_through_the_persisted_file() {
    // GSS-FR-25, GSS-FR-20: the value survives a relaunch, which is what makes the
    // choice one habit across every project.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("synthesis.toml");
    let store = GlobalSettingsStore::with_path(path.clone());
    let mut editing = store.load_app_preferences().unwrap();
    editing.changes_commit_action = ChangesCommitAction::CommitAndPush;
    store.save_app_preferences(editing).unwrap();

    let reopened = GlobalSettingsStore::with_path(path);
    assert_eq!(
        reopened.load_app_preferences().unwrap().changes_commit_action,
        ChangesCommitAction::CommitAndPush
    );
}
