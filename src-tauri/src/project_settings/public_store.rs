//! The project-public store (PSS-FR-02, PSS-FR-10, PSS-FR-17).
//!
//! `.synthesis/project.toml` inside the active worktree, committed to the
//! project's Git repository, read as a whole and written as a whole. Every
//! section of it — the line-ending convention, the draft template, the graduation
//! concurrency limit, the shared loop bounds of [`super::loop_settings`], the
//! Docker image entries — travels through the two functions here, which is what
//! makes PSS-FR-17's "carries every other section through unchanged" one rule
//! rather than one per section.
//!
//! Split out of the module root, which had reached a thousand lines. What the
//! store holds and what a read returns are unchanged.

use tauri::State;

use super::{
    loop_settings, project_toml_path, DRAFT_TEMPLATE_KEY, ERR_MALFORMED_PROJECT_CONFIG,
    GRADUATION_CONCURRENCY_KEY, GraduationConcurrency, LEGACY_STREAM_CONCURRENCY_KEY,
    LINE_ENDINGS_KEY, LineEndings,
    ProjectConfig, ProjectState,
};

// ---------------------------------------------------------------------------
// Project-public config (PSS-FR-02 / PSS-FR-17)
// ---------------------------------------------------------------------------

/// Read `project.toml` as a generic table, distinguishing "not written yet" from
/// "written and damaged".
///
/// A file that exists but does not parse is the typed error of PSS-FR-10. An
/// **absent** file is treated as defaults and created on first save: PSS-FR-02
/// has `PST-project-storage.md::create_project` (PST-FR-02) write it at creation
/// time, which this walking-skeleton build does not yet do, so erroring here
/// would make every existing project unreadable rather than surfacing a real
/// defect. The malformed case — the one that actually signals damage — keeps its
/// typed error.
pub(super) fn load_public(root: &crate::fs::RootFs) -> Result<toml::Table, String> {
    let path = project_toml_path(root);
    if !path.exists() {
        return Ok(toml::Table::new());
    }
    root.read_toml::<toml::Table>(path).map_err(|_| ERR_MALFORMED_PROJECT_CONFIG.to_string())
}

/// Write `project.toml` atomically (PSS-FR-04). Unlike the project-local store
/// this file is *committed*, so no ignore rule is written alongside it.
pub(super) fn save_public(root: &crate::fs::RootFs, value: &toml::Table) -> Result<(), String> {
    // Parents are created by the write itself (FSA-FR-05).
    root.write_toml_atomic(project_toml_path(root), value).map_err(|e| e.to_string())
}

/// PSS-FR-17: the project-public config, with the line-ending convention
/// defaulting to `lf` when the key is absent.
pub fn load_project_config_from(root: &crate::fs::RootFs) -> Result<ProjectConfig, String> {
    let table = load_public(root)?;
    let line_endings = table
        .get(LINE_ENDINGS_KEY)
        .cloned()
        .and_then(|v| v.try_into::<LineEndings>().ok())
        .unwrap_or_default();
    // PSS-FR-21: an absent key is the *unset* state rather than empty text, so
    // the two stay distinct all the way up to the Draft template section
    // (`../ui/SET-project-settings.md` SET-FR-19). A key holding something that
    // is not a string reads as unset for the same reason the convention above
    // falls back: a malformed *file* is the typed error of PSS-FR-10, and one
    // odd value is not a damaged store.
    let draft_template = table
        .get(DRAFT_TEMPLATE_KEY)
        .and_then(|v| v.as_str())
        .map(str::to_string);
    // PSS-FR-KMBT: a value below one, or one that is neither an integer nor
    // `unlimited`, is repaired to one on read. A limit of zero would stop every
    // run from ever starting, which no author can have meant.
    // PSS-FR-LGKY: the earlier stream key stands in only where the project-wide
    // key is absent.
    let graduation_concurrency_limit = Some(GraduationConcurrency::from_stored(
        table
            .get(GRADUATION_CONCURRENCY_KEY)
            .or_else(|| table.get(LEGACY_STREAM_CONCURRENCY_KEY)),
    ));
    Ok(ProjectConfig {
        line_endings,
        draft_template,
        graduation_concurrency_limit,
        // PSS-FR-ZLCF: unset and stored are distinct, and a value outside its
        // bounds is repaired to unset rather than clamped. A clamped value
        // would be a bound nobody chose, where an unset one leaves the loop at
        // the default its own module states.
        execution_timeout_ms: loop_settings::read_execution_timeout(&table),
        provider_call_deadline_ms: loop_settings::read_provider_call_deadline(&table),
        retry_budget: loop_settings::read_retry_budget(&table),
    })
}


/// PSS-FR-17: persist the project-public config.
///
/// The write **merges** the keys this payload declares into the file rather than
/// replacing it. `save_project_config` is a whole-store write in the contract's
/// sense — one call persists the store — but the typed payload above is only a
/// partial view of what `project.toml` holds (PSS-FR-07 / PSS-FR-08 / PSS-FR-09
/// put plugin enablement, adapter configuration and MCP connections in the same
/// file). Serialising the struct over the file would silently delete every one
/// of those sections, which is precisely what PSS-FR-17's "carries every other
/// project-public section through unchanged" forbids.
pub fn save_project_config_to(root: &crate::fs::RootFs, config: ProjectConfig) -> Result<(), String> {
    let mut table = load_public(root)?;
    let value = toml::Value::try_from(config.line_endings)
        .map_err(|e| format!("failed to encode line endings: {e}"))?;
    table.insert(LINE_ENDINGS_KEY.to_string(), value);
    // PSS-FR-21, the three ways a write can speak about the template. The
    // absent case is what lets the status bar persist a line-ending convention
    // without disturbing a configured template, and the empty case is what
    // makes deleting every character in the Draft template section return the
    // project to the state it had before a template was ever written — rather
    // than recording a configured empty one, which nothing would ever read as
    // "no template".
    // Read before the template is consumed below, the shared bounds being a
    // separate section of the same whole-store write (PSS-FR-17).
    loop_settings::write_into(&mut table, &config);
    match config.draft_template {
        None => {}
        Some(text) if text.is_empty() => {
            table.remove(DRAFT_TEMPLATE_KEY);
        }
        Some(text) => {
            table.insert(DRAFT_TEMPLATE_KEY.to_string(), toml::Value::String(text));
        }
    }
    // PSS-FR-ZVSD: a payload that names no limit leaves the stored one alone.
    // PSS-FR-FHQU: a named limit is stored as an integer of one or more, or as
    // the text `unlimited`.
    // PSS-FR-LGKY: the earlier stream key is removed once a limit is named.
    if let Some(limit) = config.graduation_concurrency_limit {
        table.insert(GRADUATION_CONCURRENCY_KEY.to_string(), limit.to_stored());
        table.remove(LEGACY_STREAM_CONCURRENCY_KEY);
    }
    save_public(root, &table)
}

/// PST-FR-23: the convention every artifact write normalises to.
///
/// Best-effort by design — a damaged `project.toml` is surfaced by
/// `load_project_config` where the user can act on it, but it must never make a
/// *save* fail and cost the user their edit, so the default stands in here.
pub fn line_endings_for(root: &crate::fs::RootFs) -> LineEndings {
    load_project_config_from(root)
        .map(|c| c.line_endings)
        .unwrap_or_default()
}

#[tauri::command]
pub fn load_project_config(project: State<'_, ProjectState>) -> Result<ProjectConfig, String> {
    let root = project.require_root()?;
    load_project_config_from(&root)
}

#[tauri::command]
pub fn save_project_config<R: tauri::Runtime>(
    config: ProjectConfig,
    project: State<'_, ProjectState>,
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    let root = project.require_root()?;
    let before = load_project_config_from(&root)
        .ok()
        .and_then(|stored| stored.graduation_concurrency_limit);
    let named = config.graduation_concurrency_limit;
    save_project_config_to(&root, config)?;
    // PSS-FR-ZVSD, GRD-FR-IJKV: a limit that grew frees slots at once, so every
    // queue is offered a dispatch without waiting for a run to end.
    if let (Some(before), Some(after)) = (before, named) {
        if after != before {
            crate::logging::log_info(
                &app,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "graduation concurrency limit changed",
                crate::log_fields! {
                    "unlimited" => matches!(after, GraduationConcurrency::Unlimited),
                    "limit" => match after {
                        GraduationConcurrency::Limited(n) => i64::from(n),
                        GraduationConcurrency::Unlimited => -1,
                    },
                },
            );
            crate::graduation::advance_all_queues(&app);
        }
    }
    Ok(())
}
