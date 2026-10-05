//! The app-preferences record and the enums and font settings it holds
//! (`specifications/core/GSS-global-settings-storage.md` GSS-FR-04,
//! GSS-FR-19, GSS-FR-21, GSS-FR-24, GSS-FR-25, GSS-FR-29, GSS-FR-32,
//! GSS-FR-33, GSS-FR-34, GSS-FR-QDNV).
//!
//! One part of `global_settings.rs`, which re-exports every item here, so a
//! caller names them through that module as before.

use serde::{Deserialize, Serialize};

/// App theme preference (GSS-FR-04). `system` is the default when unset; the
/// app-wide application of the value and the behaviour of `system` are owned
/// at the overview level (`OVW-overview.md` OVW-FR-09 / OVW-FR-10) — this
/// module only persists the chosen value.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

/// The interpretation the universal search bar applies to a query (GSS-FR-21).
///
/// User-global rather than project-scoped because it describes how the user
/// searches rather than anything about a project. This module persists the value
/// and does not interpret it: the modes themselves are defined by
/// `SCC-search.md` SCC-FR-05, and their wire vocabulary is the one
/// `search::SearchMode` decodes.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchQueryMode {
    #[default]
    LiteralInsensitive,
    SmartCase,
    Regex,
}

/// How a Diff tab lays a comparison out (GSS-FR-24).
///
/// User-global rather than project-scoped or per-tab because it describes how
/// the author reads a diff rather than anything about a project or a file. This
/// module persists the value and does not interpret it: what each mode renders
/// is owned by `../ui/DFV-diff-viewer.md`.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffVisualizationMode {
    #[default]
    Unified,
    SideBySide,
    Final,
}

/// Whether a Diff tab renders a file's source text or, for Markdown, its
/// formatted document (GSS-FR-24).
///
/// A stored `rich` is persisted unchanged even while the open file cannot be
/// rendered rich: the disablement a non-Markdown file causes is display-only
/// (`../ui/DFV-diff-viewer.md` DFV-FR-18).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffRenderingMode {
    #[default]
    Source,
    Rich,
}

/// Which of the Changes panel's three footer actions is selected (GSS-FR-25).
///
/// Stored whatever the repository can do at this moment: the selection
/// describes the author's habit, and its availability is decided by the panel
/// (`../ui/CHG-changes.md` CHG-FR-35).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangesCommitAction {
    #[default]
    Commit,
    CommitAndPush,
    Push,
}

/// One typographic role's font setting (GSS-FR-29).
///
/// Each of the three fields is independently unset until chosen, and `None`
/// means "the application's built-in default for this role" rather than any
/// value this module could name. That is what lets a user set a family without
/// also committing to a size.
///
/// This module persists and interprets nothing here: it never checks that
/// `family` is installed on this machine (resolving an absent family is
/// `GLS-global-settings.md` GLS-FR-22's job, and `FNT-font-enumeration.md`
/// FNT-FR-09 never validates one either), and it never bounds `size_px` or
/// `line_height` (bounding is GLS-FR-19's). A family since uninstalled reads
/// back exactly as stored.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct FontRole {
    /// Family name as chosen from the list `crate::fonts` reports. `None` means
    /// the built-in default face for the role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// `None` means the built-in default size for the role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_px: Option<f64>,
    /// A multiplier of the role's font size, not a length. `None` means the
    /// built-in default for the role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
}

/// The three typographic roles' font settings (GSS-FR-29).
///
/// One per role of `../ui/OVW-overview.md` OVW-FR-13. User-global rather than
/// project-scoped because they describe how the author wants to read rather
/// than anything about a project.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct FontSettings {
    /// Panels, tabs, chrome, modals, the status bar, comment and note bodies —
    /// everything the other two roles do not name.
    pub ui: FontRole,
    /// The prose of a rendered Markdown document: the Editor's WYSIWYG surface
    /// and a Diff tab's Rich rendering.
    pub rich: FontRole,
    /// Literal source text and code: the Editor's raw-text surface, code and
    /// frontmatter inside a rendered document, and a Diff tab's Source
    /// rendering with its line-number gutter.
    pub source: FontRole,
}

/// App preferences persisted in user-global config (GSS-FR-04).
///
/// Six facts share this one record, edited from five different places, which is
/// why GSS-FR-20 makes `save_app_preferences` a **whole-record** write: the
/// caller supplies every field, changed or not. The Appearance section
/// (`GLS-global-settings.md` GLS-FR-14) edits `theme` and `fonts` and carries
/// the rest through untouched; the shell, the search input, a Diff tab, and the
/// Changes panel each do the same for the fields they do not own.
// Not `Eq`: `FontRole` carries a size and a line height as floats, and a font
// size is a measurement rather than a token — a value the user typed, not one
// of a fixed set. `PartialEq` is all this record is ever compared with. They are
// `f64` rather than `f32` because that is what a JSON number decodes to on both
// sides of the bridge, so a value round-trips through the wire bit-identical
// rather than picking up a widening artifact.
/// GSS-FR-32 / GSS-FR-33: `Default` is written out rather than derived, because
/// two fields' defaults are not their type's. `notifications_enabled` and
/// `selection_follows_tab` both default to `true` where `bool::default()` is
/// `false`, and a derived impl would make "no record stored yet" mean
/// notifications off and the panel not following — the opposite of what the
/// contract says in both cases, and invisible until someone wondered why a fresh
/// install never notified.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct AppPreferences {
    pub theme: Theme,
    /// GSS-FR-19: the main window's OS full-screen state, `false` when unset.
    ///
    /// User-global rather than a field of the per-project layout slot
    /// (GSS-FR-17): it describes how the user wants the main window presented,
    /// whichever project is opened. Written by the shell whenever the window
    /// enters or leaves full-screen and read when the main window mounts
    /// (`SNV-shell-navigation.md` SNV-FR-38 / SNV-FR-39). This module persists
    /// the flag and never interprets it — nothing here consults it when serving
    /// the Project picker, which is never full-screen
    /// (`PPK-project-picker.md` PPK-FR-14).
    pub main_window_fullscreen: bool,
    /// GSS-FR-21: the search bar's active query mode, `literal_insensitive`
    /// when unset. The search input reads it on mount and writes it whenever the
    /// user changes the active toggle (`../ui/SCH-search.md` SCH-FR-13).
    pub search_query_mode: SearchQueryMode,
    /// GSS-FR-24: how a Diff tab lays a comparison out, `unified` when unset. A
    /// Diff tab reads it when it mounts and writes it whenever the user
    /// activates a different toggle (`../ui/DFV-diff-viewer.md` DFV-FR-23).
    pub diff_visualization_mode: DiffVisualizationMode,
    /// GSS-FR-24: whether a Diff tab renders source or rich, `source` when
    /// unset.
    pub diff_rendering_mode: DiffRenderingMode,
    /// GSS-FR-25: the action the Changes panel's footer control performs,
    /// `commit` when unset. User-global rather than project-scoped because it
    /// describes the author's committing habit rather than anything about a
    /// project; the panel reads it on mount and writes it whenever the author
    /// picks a different action (`../ui/CHG-changes.md` CHG-FR-34). Persisted
    /// and not interpreted: which actions are available at a given moment is
    /// that panel's decision, so a stored `push` survives unchanged even while
    /// nothing is pushable.
    pub changes_commit_action: ChangesCommitAction,
    /// GSS-FR-32: whether the application posts OS notifications at all, `true`
    /// when unset. User-global rather than project-scoped because it describes
    /// how the author wants to be interrupted rather than anything about a
    /// project; the Notifications section reads it on mount and writes it
    /// whenever the switch changes (`../ui/GLS-global-settings.md` GLS-FR-25).
    ///
    /// Persisted and not interpreted: whether any given raise becomes a posted
    /// notification is `../ui/NTF-notifications.md` NTF-FR-08's decision. The
    /// operating system's own permission is deliberately **not** stored here —
    /// it is read from the platform each time it is needed
    /// (`NTD-notification-delivery.md` NTD-FR-02), so a store copied to another
    /// machine carries no stale claim about what that machine allows.
    ///
    /// `#[serde(default = ...)]` rather than the struct's blanket `default`,
    /// because `bool`'s default is `false` and this field's is `true`: a record
    /// written before the field existed must read back as notifications on, not
    /// silently off.
    #[serde(default = "notifications_enabled_default")]
    pub notifications_enabled: bool,
    /// GSS-FR-33: whether a change of the main viewport's active tab moves the
    /// vertical panel's selection to the item that tab is a view onto, `true`
    /// when unset. User-global rather than project-scoped because it describes
    /// how the author navigates rather than anything about a project; the
    /// Navigation section reads it on mount and writes it whenever the switch
    /// changes (`../ui/GLS-global-settings.md` GLS-FR-28), and the shell reads
    /// it at every qualifying tab activation
    /// (`../ui/SNV-shell-navigation.md` SNV-FR-64).
    ///
    /// Persisted and not interpreted: which panel a given tab selects in, and
    /// what happens when its target cannot be resolved, are SNV-FR-66's and
    /// SNV-FR-67's decisions rather than this module's.
    ///
    /// `#[serde(default = ...)]` rather than the struct's blanket `default`, for
    /// the same reason `notifications_enabled` carries one: `bool`'s default is
    /// `false` and this field's is `true`, so a record written before the field
    /// existed must read back as the behaviour ON. The requirement is explicit
    /// that the default holds for an existing record as much as for a fresh
    /// machine — this attribute is the whole of what makes that true.
    #[serde(default = "selection_follows_tab_default")]
    pub selection_follows_tab: bool,
    /// GSS-FR-34: the width of the Runs panel's graduation history rail as a
    /// fraction of that panel's usable content width, `0.20` when unset.
    ///
    /// User-global rather than a field of the per-project layout slot
    /// (GSS-FR-17), and the two are separate facts: the layout slot holds the
    /// shape of one project's shell, while this describes how wide the author
    /// wants a run history beside a run wherever they read one. The graduation
    /// section reads it when it mounts and writes it when a drag completes or a
    /// keyboard adjustment is made (`../ui/GRH-graduation-history.md` GRH-FR-MCHQ).
    ///
    /// Persisted and not interpreted: the fraction's bounds, what it is
    /// measured against, and the steps a key press moves it by are GRH-FR-MCHQ's,
    /// so a stored value outside those bounds is returned exactly as stored for
    /// that surface to clamp.
    ///
    /// `#[serde(default = ...)]` rather than the struct's blanket `default`,
    /// for the reason `notifications_enabled` carries one: `f64`'s default is
    /// `0.0` and this field's is `0.20`, so a record written before the field
    /// existed must read back as the default width rather than as a rail of no
    /// width at all.
    #[serde(default = "graduation_rail_width_default")]
    pub graduation_rail_width_fraction: f64,
    /// GSS-FR-QDNV: the width of the graduation run region's paths column as a
    /// fraction of the width of its two columns, `0.30` when unset.
    ///
    /// User-global, and persisted without interpretation: its bounds and its
    /// use are `../ui/GRU-graduation-runs.md` GRU-FR-KWRB's. The serde default
    /// is the field's own for the reason the rail width carries one: `f64`'s
    /// default is `0.0`, and a record written before the field existed must
    /// read back as the default width.
    #[serde(default = "graduation_paths_width_default")]
    pub graduation_paths_width_fraction: f64,
    /// GSS-FR-MSPQ: the width of the Git panel's files column as a fraction of
    /// the width of its large view, `0.30` when unset.
    ///
    /// User-global, and persisted without interpretation: its bounds and its
    /// use are `../ui/GIT-git.md` GIT-FR-FATV's. The serde default is the
    /// field's own, so a record written before the field existed reads back as
    /// the default width rather than as a column of no width.
    #[serde(default = "git_files_width_default")]
    pub git_files_width_fraction: f64,
    /// GSS-FR-29: the three typographic roles' font settings, every field
    /// independently unset until chosen. The Appearance section reads them on
    /// mount and writes them whenever the user changes a control
    /// (`../ui/GLS-global-settings.md` GLS-FR-17 / GLS-FR-20).
    ///
    /// Declared **last** deliberately: this is the record's only sub-table, and
    /// TOML requires a table's scalar values to be emitted before its
    /// sub-tables. Serde serialises fields in declaration order, so a scalar
    /// declared after this one would fail to serialise at all.
    pub fonts: FontSettings,
}

/// GSS-FR-32: notifications are on until the author turns them off.
fn notifications_enabled_default() -> bool {
    true
}

/// GSS-FR-33: the panel follows the active tab until the author turns it off.
fn selection_follows_tab_default() -> bool {
    true
}

/// GSS-FR-34: a rail the author has never sized takes a fifth of the panel.
fn graduation_rail_width_default() -> f64 {
    0.20
}

/// GSS-FR-QDNV: a paths column the author has never sized takes 30 % of the
/// run region's two columns.
fn graduation_paths_width_default() -> f64 {
    0.30
}

/// GSS-FR-MSPQ: a files column the author has never sized takes 30 % of the
/// Git panel's large view.
fn git_files_width_default() -> f64 {
    0.30
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            main_window_fullscreen: false,
            search_query_mode: SearchQueryMode::default(),
            diff_visualization_mode: DiffVisualizationMode::default(),
            diff_rendering_mode: DiffRenderingMode::default(),
            changes_commit_action: ChangesCommitAction::default(),
            notifications_enabled: notifications_enabled_default(),
            selection_follows_tab: selection_follows_tab_default(),
            graduation_rail_width_fraction: graduation_rail_width_default(),
            graduation_paths_width_fraction: graduation_paths_width_default(),
            git_files_width_fraction: git_files_width_default(),
            fonts: FontSettings::default(),
        }
    }
}
