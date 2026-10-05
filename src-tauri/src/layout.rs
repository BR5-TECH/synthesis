//! Layout preferences (SNV-shell-navigation.md SNV-FR-08/11/12/13).
//!
//! Panel sizing/side/collapse/hide state plus the main window's outer
//! dimensions and maximized state, persisted across sessions.
//!
//! They live in the **user-global per-project slot**
//! (`GSS-global-settings-storage.md` GSS-FR-17 /
//! `PSS-project-settings-storage.md` PSS-FR-06), not in either project store.
//! A project's `.synthesis/` is committed content, so a repository holds one
//! copy per worktree — persisting the layout there would reshuffle panel widths
//! and window geometry every time the active worktree changed. Holding it
//! against the project's identity anchor instead is what keeps the shell's
//! shape stable across a switch (SNV-FR-32). With no project open the pair
//! falls back to a global default slot, so the first project opened inherits a
//! sensible layout.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::global_settings::GlobalSettingsStore;
use crate::project::ProjectState;

/// Layout preferences persisted across sessions.
///
/// Per `specifications/ui/SNV-shell-navigation.md` (SNV-FR-08, SNV-FR-11, SNV-FR-12, SNV-FR-13)
/// the payload carries panel sizing, panel side, collapse/hide state, AND the
/// main window's outer dimensions and maximized state. Field names are
/// camelCase on the wire so the frontend can send them verbatim.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct LayoutPreferences {
    /// Width of the vertical panel as a **fraction of the shell's inner width**
    /// (the window's content width less the activity bar) — SNV-FR-34.
    ///
    /// A fraction rather than a pixel count so a project restored on a display
    /// of a different size keeps its proportions (SNV-FR-35). This module stores
    /// whatever value it is given; the clamping — a 180 px floor and a 50%
    /// ceiling, floor winning when the shell is too narrow for both — is owned
    /// by the shell (SNV-FR-34), because only it knows the live shell width.
    ///
    /// `0.0` (the default) means "never persisted", and the shell substitutes
    /// its own default fraction.
    pub vertical_panel_fraction: f64,
    /// Height of the bottom panel in CSS pixels.
    pub bottom_panel_height: f64,
    /// Which edge the vertical panel + activity bar live on.
    pub vertical_panel_side: VerticalPanelSide,
    /// Whether the vertical panel is collapsed to icon-only.
    pub vertical_panel_collapsed: bool,
    /// Whether the vertical panel is fully hidden.
    pub vertical_panel_hidden: bool,
    /// Whether the bottom panel is collapsed.
    pub bottom_panel_collapsed: bool,
    /// Whether the bottom panel is fully hidden.
    ///
    /// Optional because, unlike the vertical panel, the bottom panel's default
    /// is *closed*: a plain `bool` defaults to `false`, so a record written for
    /// some unrelated field (a splitter drag, a window resize) would read back
    /// as "explicitly shown" and open the panel uninvited on the next launch.
    /// `None` means the author has never decided, and the shell keeps it shut.
    pub bottom_panel_hidden: Option<bool>,
    /// Which surface the bottom panel renders — SNV-FR-08 / GSS-FR-17.
    ///
    /// The bottom panel shows exactly one of Runs / Git / History at a time,
    /// chosen from the activity bar's bottom-panel cluster (SNV-FR-44), and the
    /// choice is a property of the project rather than of the session: reopening
    /// a project returns the author to the surface they were last working in.
    /// Defaults to `Runs`, the surface a project with no persisted layout opens
    /// on (RUN-FR-01).
    pub bottom_panel_surface: BottomSurface,
    /// Persisted outer width of the main window in CSS pixels (SNV-FR-08/SNV-FR-12).
    pub main_window_outer_width: f64,
    /// Persisted outer height of the main window in CSS pixels (SNV-FR-08/SNV-FR-12).
    pub main_window_outer_height: f64,
    /// Persisted main-window maximized state (SNV-FR-08/SNV-FR-12).
    pub main_window_maximized: bool,
    /// SNV-FR-08: the split of each draft's New Artifact tab between its
    /// document column and its discussion column
    /// (`DDS-draft-discussion.md` DDS-FR-PNXR), keyed by draft id.
    ///
    /// Here rather than on the draft's own record because `draft.toml` is
    /// committed content that travels with the draft, and how one author has
    /// arranged their columns is a view preference rather than part of what the
    /// draft says. The value is a preset name or a fraction; this module stores
    /// whatever it is given and the shell decides what is valid.
    pub draft_discussion_ratios: std::collections::BTreeMap<String, String>,
    /// SNV-FR-08: whether each draft's discussion column is hidden
    /// (`DDS-draft-discussion.md` DDS-FR-XQMF), keyed by draft id.
    ///
    /// Beside the ratio rather than inside it, because hiding a column is not
    /// narrowing it: the narrowest either column is dragged to is one that can
    /// still be read, and a draft that is read rather than discussed takes the
    /// whole tab. A draft with no entry has never been hidden.
    pub draft_discussion_hidden: std::collections::BTreeMap<String, bool>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VerticalPanelSide {
    Left,
    Right,
}

impl Default for VerticalPanelSide {
    fn default() -> Self {
        VerticalPanelSide::Left
    }
}

/// The bottom panel's active surface (SNV-FR-44 / SNV-FR-46).
///
/// Mirrors the frontend's `BottomSurface` union. Lowercase on the wire so the
/// two sides exchange the same three tokens.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BottomSurface {
    Runs,
    Git,
    History,
}

impl Default for BottomSurface {
    fn default() -> Self {
        BottomSurface::Runs
    }
}

/// PSS-FR-06: the open project's layout, read from the user-global per-project
/// slot. An empty slot key (no project open) selects the global default slot.
#[tauri::command]
pub fn load_layout_preferences(
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<Option<LayoutPreferences>, String> {
    store.load_project_layout(&project.slot_key())
}

/// PSS-FR-06 / GSS-FR-17: persist the open project's layout. The value is
/// keyed by project, never by worktree, so a switch of active worktree reads
/// back exactly what was saved.
#[tauri::command]
pub fn save_layout_preferences(
    preferences: LayoutPreferences,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    store.save_project_layout(&project.slot_key(), preferences)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_prefs() -> LayoutPreferences {
        LayoutPreferences {
            vertical_panel_fraction: 0.28,
            bottom_panel_height: 180.0,
            vertical_panel_side: VerticalPanelSide::Right,
            vertical_panel_collapsed: false,
            vertical_panel_hidden: false,
            bottom_panel_collapsed: true,
            bottom_panel_hidden: Some(false),
            bottom_panel_surface: BottomSurface::Git,
            main_window_outer_width: 1440.0,
            main_window_outer_height: 900.0,
            main_window_maximized: false,
            draft_discussion_ratios: std::collections::BTreeMap::from([(
                "d1".to_string(),
                "chat70".to_string(),
            )]),
            draft_discussion_hidden: std::collections::BTreeMap::from([("d1".to_string(), true)]),
        }
    }

    #[test]
    fn layout_preferences_default_is_all_zero_and_left() {
        let p = LayoutPreferences::default();
        assert_eq!(p.vertical_panel_fraction, 0.0);
        assert_eq!(p.bottom_panel_height, 0.0);
        assert_eq!(p.vertical_panel_side, VerticalPanelSide::Left);
        assert!(!p.vertical_panel_collapsed);
        assert!(!p.vertical_panel_hidden);
        assert!(!p.bottom_panel_collapsed);
        assert_eq!(p.bottom_panel_hidden, None);
        assert_eq!(p.bottom_panel_surface, BottomSurface::Runs);
        assert_eq!(p.main_window_outer_width, 0.0);
        assert_eq!(p.main_window_outer_height, 0.0);
        assert!(!p.main_window_maximized);
        // SNV-FR-08 / DDS-FR-PNXR: a project nobody has split a draft in holds
        // no ratio, and every draft opens on the default split.
        assert!(p.draft_discussion_ratios.is_empty());
        // SNV-FR-08 / DDS-FR-XQMF: and no draft has had its discussion column
        // hidden, so every draft opens on two columns.
        assert!(p.draft_discussion_hidden.is_empty());
    }

    #[test]
    fn layout_preferences_serializes_camelcase() {
        let json = serde_json::to_value(sample_prefs()).unwrap();
        // The frontend speaks camelCase — pin the wire contract.
        for key in [
            "verticalPanelFraction",
            "bottomPanelHeight",
            "verticalPanelSide",
            "verticalPanelCollapsed",
            "verticalPanelHidden",
            "bottomPanelCollapsed",
            "bottomPanelHidden",
            "bottomPanelSurface",
            "mainWindowOuterWidth",
            "mainWindowOuterHeight",
            "mainWindowMaximized",
            // SNV-FR-08 / DDS-FR-PNXR / DDS-FR-XQMF: the two per-draft maps.
            // The names must match `src/types/worktree.ts` byte for byte — a
            // mismatch is dropped on save and absent on load, with no error.
            "draftDiscussionRatios",
            "draftDiscussionHidden",
        ] {
            assert!(
                json.get(key).is_some(),
                "expected key {key:?} in serialized LayoutPreferences; got {json}"
            );
        }
    }

    #[test]
    fn layout_preferences_deserializes_from_js_shape() {
        let json = r#"{
            "verticalPanelFraction": 0.32,
            "bottomPanelHeight": 200.0,
            "verticalPanelSide": "left",
            "verticalPanelCollapsed": true,
            "verticalPanelHidden": false,
            "bottomPanelCollapsed": false,
            "bottomPanelHidden": true,
            "bottomPanelSurface": "history",
            "mainWindowOuterWidth": 1920.0,
            "mainWindowOuterHeight": 1080.0,
            "mainWindowMaximized": true,
            "draftDiscussionRatios": { "d-1": "doc70" },
            "draftDiscussionHidden": { "d-1": true, "d-4": false }
        }"#;
        let p: LayoutPreferences = serde_json::from_str(json).unwrap();
        assert_eq!(p.vertical_panel_fraction, 0.32);
        assert_eq!(p.bottom_panel_height, 200.0);
        assert_eq!(p.vertical_panel_side, VerticalPanelSide::Left);
        assert!(p.vertical_panel_collapsed);
        assert!(!p.vertical_panel_hidden);
        assert!(!p.bottom_panel_collapsed);
        assert_eq!(p.bottom_panel_hidden, Some(true));
        assert_eq!(p.bottom_panel_surface, BottomSurface::History);
        assert_eq!(p.main_window_outer_width, 1920.0);
        assert_eq!(p.main_window_outer_height, 1080.0);
        assert!(p.main_window_maximized);
        // SNV-FR-08 / DDS-FR-XQMF: whether each draft's discussion column is
        // hidden arrives keyed by draft id, beside the split ratio and not
        // inside it.
        assert_eq!(p.draft_discussion_ratios.get("d-1").map(String::as_str), Some("doc70"));
        assert_eq!(p.draft_discussion_hidden.get("d-1"), Some(&true));
        assert_eq!(p.draft_discussion_hidden.get("d-4"), Some(&false));
    }

    #[test]
    fn layout_preferences_roundtrips_through_json() {
        let original = sample_prefs();
        let json = serde_json::to_string(&original).unwrap();
        let decoded: LayoutPreferences = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn vertical_panel_side_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&VerticalPanelSide::Left).unwrap(),
            "\"left\""
        );
        assert_eq!(
            serde_json::to_string(&VerticalPanelSide::Right).unwrap(),
            "\"right\""
        );
    }

    #[test]
    fn vertical_panel_side_rejects_unknown() {
        assert!(serde_json::from_str::<VerticalPanelSide>("\"middle\"").is_err());
    }

    #[test]
    fn bottom_surface_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&BottomSurface::Runs).unwrap(),
            "\"runs\""
        );
        assert_eq!(
            serde_json::to_string(&BottomSurface::Git).unwrap(),
            "\"git\""
        );
        assert_eq!(
            serde_json::to_string(&BottomSurface::History).unwrap(),
            "\"history\""
        );
    }

    #[test]
    fn bottom_surface_rejects_unknown() {
        assert!(serde_json::from_str::<BottomSurface>("\"diagnostics\"").is_err());
    }

    /// A slot written before the bottom panel's surface was part of the record
    /// still loads, and reports the Runs default (RUN-FR-01) rather than
    /// failing the whole read and costing the author their panel width too.
    #[test]
    fn layout_preferences_defaults_bottom_surface_when_absent() {
        let json = r#"{ "verticalPanelFraction": 0.4 }"#;
        let p: LayoutPreferences = serde_json::from_str(json).unwrap();
        assert_eq!(p.vertical_panel_fraction, 0.4);
        assert_eq!(p.bottom_panel_surface, BottomSurface::Runs);
        assert_eq!(p.bottom_panel_hidden, None);
        // SNV-FR-08 / DDS-FR-XQMF: a record written before this field existed
        // reads back clean, and every draft opens on two columns.
        assert!(p.draft_discussion_hidden.is_empty());
        assert!(p.draft_discussion_ratios.is_empty());
    }

    /// The distinction the `Option` exists for: a record saved for an unrelated
    /// field must not read back as a decision to show the bottom panel.
    #[test]
    fn bottom_panel_hidden_distinguishes_unset_from_shown() {
        let unset: LayoutPreferences =
            serde_json::from_str(r#"{ "mainWindowMaximized": true }"#).unwrap();
        assert_eq!(unset.bottom_panel_hidden, None);
        let shown: LayoutPreferences =
            serde_json::from_str(r#"{ "bottomPanelHidden": false }"#).unwrap();
        assert_eq!(shown.bottom_panel_hidden, Some(false));
    }

    #[test]
    fn store_load_returns_none_when_empty() {
        let store = GlobalSettingsStore::in_memory();
        assert_eq!(store.load_project_layout("/p").unwrap(), None);
    }

    #[test]
    fn store_save_then_load_returns_payload() {
        let store = GlobalSettingsStore::in_memory();
        let prefs = sample_prefs();
        store.save_project_layout("/p", prefs.clone()).unwrap();
        assert_eq!(store.load_project_layout("/p").unwrap(), Some(prefs));
    }

    #[test]
    fn store_save_overwrites_previous() {
        let store = GlobalSettingsStore::in_memory();
        store
            .save_project_layout("/p", LayoutPreferences::default())
            .unwrap();
        let updated = sample_prefs();
        store.save_project_layout("/p", updated.clone()).unwrap();
        assert_eq!(store.load_project_layout("/p").unwrap(), Some(updated));
    }

    #[test]
    fn store_persists_new_main_window_fields_specifically() {
        // Regression guard for the SNV-FR-08 extension: the new main-window
        // fields must survive save/load round-trip.
        let store = GlobalSettingsStore::in_memory();
        let mut prefs = LayoutPreferences::default();
        prefs.main_window_outer_width = 1366.0;
        prefs.main_window_outer_height = 768.0;
        prefs.main_window_maximized = true;
        store.save_project_layout("/p", prefs).unwrap();
        let got = store.load_project_layout("/p").unwrap().unwrap();
        assert_eq!(got.main_window_outer_width, 1366.0);
        assert_eq!(got.main_window_outer_height, 768.0);
        assert!(got.main_window_maximized);
    }

    #[test]
    fn one_project_keeps_one_layout_across_every_worktree_of_its_repository() {
        // PSS-FR-06 / SNV-FR-08 / GSS-FR-17: the slot is keyed by the project's
        // identity anchor, and a worktree switch does not change that anchor —
        // so the shell's shape survives the switch untouched. Two distinct
        // projects still keep distinct layouts.
        let store = GlobalSettingsStore::in_memory();
        let prefs = sample_prefs();
        store.save_project_layout("/dev/acme", prefs.clone()).unwrap();

        // Whichever worktree is active, the key is the anchor, so the same
        // preferences read back.
        assert_eq!(store.load_project_layout("/dev/acme").unwrap(), Some(prefs));
        assert_eq!(
            store.load_project_layout("/dev/other").unwrap(),
            None,
            "a different project has its own slot"
        );
    }

    #[test]
    fn with_no_project_open_the_default_slot_is_used() {
        // PSS-FR-06: an empty slot key is the global default, so
        // the first project opened inherits a sensible layout.
        let store = GlobalSettingsStore::in_memory();
        let prefs = sample_prefs();
        store.save_project_layout("", prefs.clone()).unwrap();
        assert_eq!(store.load_project_layout("").unwrap(), Some(prefs));
        assert_eq!(
            store.load_project_layout("/dev/acme").unwrap(),
            None,
            "the default slot is not silently adopted as a project's own"
        );
    }

    #[test]
    fn save_layout_preferences_accepts_partial_main_window_only_payload() {
        // The walking-skeleton frontend may persist only the main-window
        // dimensions on first save (panel fields absent). `#[serde(default)]`
        // on `LayoutPreferences` lets the missing fields fall back to their
        // `Default::default()` values rather than failing deserialization.
        let json = r#"{
            "mainWindowOuterWidth": 1600,
            "mainWindowOuterHeight": 1000,
            "mainWindowMaximized": false
        }"#;
        let p: LayoutPreferences = serde_json::from_str(json).expect(
            "partial main-window-only payload must deserialize without missing-field errors",
        );
        assert_eq!(p.main_window_outer_width, 1600.0);
        assert_eq!(p.main_window_outer_height, 1000.0);
        assert!(!p.main_window_maximized);
        // Absent fields should default.
        assert_eq!(p.vertical_panel_fraction, 0.0);
        assert_eq!(p.bottom_panel_height, 0.0);
        assert_eq!(p.vertical_panel_side, VerticalPanelSide::Left);
        assert!(!p.vertical_panel_collapsed);
        assert!(!p.vertical_panel_hidden);
        assert!(!p.bottom_panel_collapsed);
        assert_eq!(p.bottom_panel_hidden, None);
    }

    #[test]
    fn save_layout_preferences_accepts_partial_panel_only_payload() {
        // Mirror of the above for the panel-only direction.
        let json = r#"{
            "verticalPanelFraction": 0.28,
            "bottomPanelHeight": 180,
            "verticalPanelSide": "right",
            "verticalPanelCollapsed": true,
            "verticalPanelHidden": false,
            "bottomPanelCollapsed": false,
            "bottomPanelHidden": true
        }"#;
        let p: LayoutPreferences = serde_json::from_str(json)
            .expect("partial panel-only payload must deserialize without missing-field errors");
        assert_eq!(p.vertical_panel_fraction, 0.28);
        assert_eq!(p.bottom_panel_height, 180.0);
        assert_eq!(p.vertical_panel_side, VerticalPanelSide::Right);
        assert!(p.vertical_panel_collapsed);
        assert_eq!(p.bottom_panel_hidden, Some(true));
        // Main window fields should default.
        assert_eq!(p.main_window_outer_width, 0.0);
        assert_eq!(p.main_window_outer_height, 0.0);
        assert!(!p.main_window_maximized);
    }

    #[test]
    fn layout_preference_command_functions_are_in_scope() {
        // Belt-and-braces compile-time reference: if either function is
        // renamed or removed, this test fails to compile (caught locally
        // before it can silently regress at runtime).
        let _load = load_layout_preferences;
        let _save = save_layout_preferences;
        // PPK-FR-12 re-show centering command (registered in generate_handler!).
        let _center = crate::window::center_picker;
    }
}
