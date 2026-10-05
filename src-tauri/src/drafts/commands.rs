//! The command surface of draft storage, and the readers it answers from
//! (`DRS-draft-storage.md`).

use super::*;

#[tauri::command]
pub fn list_drafts(
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DraftHierarchy, String> {
    let root = project.require_root()?;
    let (mut hierarchy, repaired) = list_drafts_reporting(&root);
    attach_graduation(&app, &mut hierarchy.drafts);
    // DRS-FR-37: a repair changes what the author sees — a folder they never
    // made stops being in their tree — so it is worth an explanation in the one
    // place they would look for one. Silent on the ordinary path, where nothing
    // was misplaced. Ids alone: a draft's id is opaque and a comment log's
    // contents are the author's, so neither the storage nor a line of it is
    // named here.
    if !repaired.is_empty() {
        crate::logging::log_info(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "misplaced draft storage reunited with its draft",
            crate::log_fields! { "drafts" => repaired.len(), "ids" => repaired.join(",") },
        );
    }
    Ok(hierarchy)
}

#[tauri::command]
pub fn create_draft<R: tauri::Runtime>(
    name: Option<String>,
    folder: Option<String>,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<DraftCreated, String>
where
    tauri::AppHandle<R>: crate::logging::LogSink + Clone + Send + 'static,
{
    let root = project.require_root()?;
    let created = create_draft_impl(&root, name.as_deref(), folder.as_deref())?;
    // PST-FR-KGRW: the committer records what it did through the application's
    // one sink, and the creation event is raised from a path that holds none.
    // Offered here, from a command that does hold one, so a session spent
    // entirely in the New Artifact tab still reports why a commit was skipped
    // or dropped.
    crate::storage_floor::commit::offer_sink(&app);
    announce(&app, DraftChange::to(&created.draft.id, vec![created.file.clone()]));
    // DHS-FR-22 is deliberately silent here: creating a draft settles no version
    // (DHS-FR-07), so there is no entry for a surface to hear about and an event
    // saying otherwise would have every open rail re-read for nothing.
    Ok(created)
}

// ---------------------------------------------------------------------------
// The organisation of the drafts root (DRP-FR-22 … DRP-FR-28)
//
// None of these five reaches inside a draft: they address the author's
// organising directories under `.synthesis/drafts/` and are invoked by the
// Drafts panel. No operation anywhere reaches inside one either (DRS-FR-13).
// ---------------------------------------------------------------------------

/// Announce a change to the organisation of the drafts root (DRS-FR-22).
///
/// Deliberately not [`announce`]: no draft's file set and no draft file's
/// contents changed, so there is nothing for the BM25 indexer to reconcile
/// (DRS-FR-32) — a draft carries its whole directory with it, and every path
/// inside it is draft-relative and therefore unmoved.
pub(super) fn announce_organisation<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let _ = app.emit(DRAFTS_CHANGED, ());
}

#[tauri::command]
pub fn create_drafts_folder(
    parent: String,
    name: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DraftFolder, String> {
    let root = project.require_root()?;
    let folder = create_drafts_folder_impl(&root, &parent, &name)?;
    crate::logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "drafts folder created",
        crate::log_fields! { "path" => folder.path.clone() },
    );
    announce_organisation(&app);
    Ok(folder)
}

#[tauri::command]
pub fn rename_drafts_folder(
    path: String,
    name: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DraftFolder, String> {
    let root = project.require_root()?;
    let folder = rename_drafts_folder_impl(&root, &path, &name)?;
    crate::logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "drafts folder renamed",
        crate::log_fields! { "from" => path, "to" => folder.path.clone() },
    );
    announce_organisation(&app);
    Ok(folder)
}

#[tauri::command]
pub fn delete_drafts_folder(
    path: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    // DRS-FR-22: a command that fails emits nothing, so the event never
    // announces a change that did not land. The partial result DRS-FR-31 leaves
    // possible on an I/O failure is not lost by that: the Drafts panel re-lists
    // after a failed operation as surely as after a successful one
    // (`../../specifications/ui/DRP-drafts-panel.md` DRP-FR-32), so the tree it
    // shows is what is on disk rather than what was attempted.
    match delete_drafts_folder_impl(&root, &path) {
        Ok(()) => {
            crate::logging::log_info(
                &app,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "drafts folder deleted, children reparented",
                crate::log_fields! { "path" => path },
            );
            announce_organisation(&app);
            Ok(())
        }
        Err(e) => {
            crate::logging::log_warn(
                &app,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "drafts folder could not be deleted",
                crate::log_fields! { "path" => path, "reason" => e.clone() },
            );
            Err(e)
        }
    }
}

#[tauri::command]
pub fn move_draft_to_folder(
    draft_id: String,
    folder: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DraftSummary, String> {
    let root = project.require_root()?;
    // DRS-FR-QPSC: a GitHub-shadow draft refuses every write with its own
    // reason, whatever its status — so this guard runs before the lock.
    require_not_github_shadow(&root, &draft_id)?;
    // DRS-FR-19 / DRS-FR-20: a draft a graduation holds, or has already
    // graduated, changes in no way — not the record, not the prompt, not a byte
    // under its folder — and nothing is emitted for a call that landed nowhere.
    crate::graduation::require_unlocked_draft(&app, &draft_id)?;
    let summary = move_draft_to_folder_impl(&root, &draft_id, &folder)?;
    crate::logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "draft filed in another folder",
        crate::log_fields! { "draft_id" => draft_id, "folder" => folder },
    );
    announce_organisation(&app);
    Ok(summary)
}

#[tauri::command]
pub fn move_drafts_folder(
    path: String,
    destination: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DraftFolder, String> {
    let root = project.require_root()?;
    let folder = move_drafts_folder_impl(&root, &path, &destination)?;
    crate::logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "drafts folder moved",
        crate::log_fields! { "from" => path, "to" => folder.path.clone() },
    );
    announce_organisation(&app);
    Ok(folder)
}

/// DRS-FR-17: the panel's text filter, which is the one operation it makes that
/// reads a draft's files.
#[tauri::command]
pub fn search_drafts(
    text: String,
    project: State<'_, ProjectState>,
) -> Result<Vec<DraftMatch>, String> {
    let root = project.require_root()?;
    Ok(search_drafts_impl(&root, &text))
}

#[tauri::command]
pub fn open_draft(
    id: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DraftRecord, String> {
    let root = project.require_root()?;
    let mut record = open_draft_impl(&root, &id)?;
    resolve_graduated_status(&app, &mut record);
    Ok(record)
}

/// DRS-FR-18 / DRS-FR-KQTW: give every row the run the queue holds for it, and
/// resolve the status it reports, from **one** queue read for the whole list.
///
/// The relationship is reported rather than owned — nothing about the run is
/// written into the draft, so what a surface shows cannot fall out of step with
/// what the queue holds. The two answers are taken together here so that every
/// reader of a draft's status reaches it by one route: a surface that read the
/// stored status instead would go on filing a released draft as `graduated`
/// long after the panel and the draft's own tab had stopped.
///
/// A project with no queue to read leaves every row exactly as the walk found
/// it, which is the same answer a project with an empty queue gives.
pub(crate) fn attach_graduation<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    drafts: &mut [DraftSummary],
) {
    let Some(queue) = crate::graduation::project_queue(app) else {
        return;
    };
    for summary in drafts.iter_mut() {
        summary.graduation = crate::graduation::draft_graduation(&queue, &summary.id);
        summary.status = resolved_status(
            &queue,
            &summary.id,
            summary.status,
            summary.github_issue.is_some(),
        );
    }
}

/// DRS-FR-KQTW: the status a draft **reports**, resolved against the queue.
///
/// The resolution runs in **both** directions, because the stored record and the
/// runs can disagree either way. It reads `active` in place of a stored
/// `graduated` no run holds any more; and it reads `graduated` in place of a
/// stored `active` or `archived` where a run does hold it, which is the state an
/// implementation-only restart leaves behind — such a run is enqueued past the
/// specification publication boundary without a transition, so nothing calls
/// `set_draft_graduated` for it and the record it was restarted over may have
/// been normalised to `active` in the meantime (DRS-FR-SLFN,
/// per `GSU-graduation-start.md` GSU-FR-IRAC).
///
/// A one-way resolution would leave that draft reporting `active` while every
/// write against it is refused, which is the disagreement DRS-FR-19 exists to
/// prevent.
/// DRS-FR-KQTW: the resolution of [`resolved_status`], reachable from a test
/// that holds a queue rather than a running application.
#[cfg(test)]
pub fn resolved_status_for_test(
    queue: &crate::graduation::GraduationQueue,
    draft_id: &str,
    stored: DraftStatus,
) -> DraftStatus {
    resolved_status(queue, draft_id, stored, false)
}

/// DRS-FR-EZDB: [`resolved_status`] for a GitHub-shadow draft, reachable from
/// a test that holds a queue rather than a running application.
#[cfg(test)]
pub fn resolved_shadow_status_for_test(
    queue: &crate::graduation::GraduationQueue,
    draft_id: &str,
    stored: DraftStatus,
) -> DraftStatus {
    resolved_status(queue, draft_id, stored, true)
}

/// `shadow` says the draft carries a GitHub issue link (DRS-FR-XDWS). Such a
/// draft reports `graduated` while a committed run stands and `github_shadow`
/// otherwise — never `active` (DRS-FR-EZDB).
pub(super) fn resolved_status(
    queue: &crate::graduation::GraduationQueue,
    draft_id: &str,
    stored: DraftStatus,
    shadow: bool,
) -> DraftStatus {
    match crate::graduation::draft_graduation_stands(queue, draft_id) {
        true => DraftStatus::Graduated,
        false if shadow => DraftStatus::GithubShadow,
        false if stored == DraftStatus::Graduated => DraftStatus::Active,
        false => stored,
    }
}

/// DRS-FR-KQTW: report the status the draft's runs make it, not the file's.
///
/// The record on disk is **not** rewritten here: a read is not activity on a
/// draft (DRS-FR-38), and the stored value is normalised by the next
/// `set_draft_status` the released draft accepts. See [`resolved_status`] for
/// why the resolution runs in both directions.
pub(crate) fn resolve_graduated_status<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    record: &mut DraftRecord,
) {
    let Some(queue) = crate::graduation::project_queue(app) else {
        return;
    };
    let shadow = record.github_issue.is_some();
    record.status = resolved_status(&queue, &record.id, record.status, shadow);
}

/// DRS-FR-15: the record, or the typed refusal for a draft that is not the
/// single prompt DRS-FR-11 requires.
///
/// Nothing is deleted, nothing is moved, and no file is chosen as the prompt on
/// the author's behalf — the tab renders the inconsistency instead (NAW-FR-41).
pub fn open_draft_impl(root: &fs::RootFs, id: &str) -> Result<DraftRecord, String> {
    let dir = draft_dir(root, id)?;
    let record = read_record_at(root, &dir, id)?;
    if prompt_of(root, &dir, &record).is_none() {
        return Err(ERR_NOT_SINGLE_FILE.to_string());
    }
    Ok(record)
}

// ---------------------------------------------------------------------------
// Resolution for the agent tools (DRS-FR-38)
// ---------------------------------------------------------------------------

/// DRS-FR-41: the prompt activity of the draft whose directory the caller
/// already holds.
///
/// The walk-free half of [`draft_prompt_activity`], for the drafts-root watch
/// (DRS-FR-42): a filesystem event names the path it happened at, so the draft's
/// directory is already known and re-deriving it by walking the whole drafts root
/// would make a burst of writes to one prompt cost a walk per batch. It reads
/// `draft.toml` and lists `files/` — the same two reads [`prompt_of`] performs —
/// and stats the one prompt it finds.
///
/// `None` where the directory holds no readable record, where `files/` is not the
/// single prompt of DRS-FR-11, or where the filesystem reports no modification
/// time. All three are the same answer to the caller: there is no prompt activity
/// to report for this path.
pub fn prompt_activity_in(root: &fs::RootFs, dir: &Path, id: &str) -> Option<String> {
    let record = read_record_at(root, dir, id).ok()?;
    let prompt = prompt_of(root, dir, &record)?;
    prompt_activity_at(root, dir, &prompt)
}

/// DRS-FR-41: the draft's `prompt_activity_at` alone, for a caller holding an id
/// that needs the activity time without the whole hierarchy.
///
/// Returns the same typed "not found" and [`ERR_NOT_SINGLE_FILE`]
/// [`draft_record`] returns, and refreshes nothing: reading a prompt's
/// modification time is not activity on the draft, so no record is rewritten,
/// no `updated_at` moves, and no `"drafts changed"` follows.
///
/// A draft that resolves and holds its one prompt whose modification time the
/// filesystem does not report answers `None` rather than an error — the
/// filesystem's silence is not the draft's fault, and a caller ordering a list
/// by activity wants to skip it rather than to fail.
pub fn draft_prompt_activity(root: &fs::RootFs, id: &str) -> Result<Option<String>, String> {
    let dir = draft_dir(root, id)?;
    let record = read_record_at(root, &dir, id)?;
    let Some(prompt) = prompt_of(root, &dir, &record) else {
        return Err(ERR_NOT_SINGLE_FILE.to_string());
    };
    Ok(prompt_activity_at(root, &dir, &prompt))
}

/// DRS-FR-38: a draft's record together with the whole text of its live prompt.
///
/// `prompt_path` is carried beside the record rather than read back off it
/// because it is the *held* file — what [`prompt_of`] found in `files/` and
/// checked against the record — so a caller cannot end up reporting a pointer
/// that no file answers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftPrompt {
    pub draft: DraftRecord,
    /// The draft-relative path of the one prompt (DRS-FR-11).
    pub prompt_path: String,
    /// The complete live prompt. Never a history snapshot, a proposal
    /// candidate, or any other version.
    pub content: String,
}

/// DRS-FR-38: a draft's current record, for a caller holding an id and nothing
/// else.
///
/// Read from `draft.toml` on every call rather than from a cache, so a draft
/// renamed or archived since some earlier walk is reported as it now stands —
/// which is the whole reason `../tools/DST-draft-search-tool.md` resolves
/// through here (DST-FR-12) instead of carrying name and status on an indexed
/// hit that may predate both.
///
/// Read-only in the strict sense the tools need: no record is rewritten, no
/// `updated_at` is refreshed, no `"drafts changed"` is emitted, and nothing is
/// surfaced on the indexing channel of DRS-FR-28. Reading a draft is not
/// activity on it.
pub fn draft_record(root: &fs::RootFs, id: &str) -> Result<DraftRecord, String> {
    // The single-prompt invariant is checked here rather than left to the
    // caller (DRS-FR-38): an inconsistent draft has no prompt to report a path
    // for, and a caller handed a record it cannot then read is a caller that
    // will offer a model something it cannot load.
    open_draft_impl(root, id)
}

/// DRS-FR-38: the record and the complete live prompt, together.
///
/// Reads the one file at the record's `prompt_path` inside `files/`, on exactly
/// the terms [`load_draft_file_impl`] reads it, and **never** from `history/`,
/// `proposals/`, `comments/`, or `conversation.jsonl` — none of which this call
/// opens or reports. That is what makes `RDT-read-draft-tool.md`'s promise
/// (RDT-FR-04) a property of the resolution rather than of the tool's restraint.
///
/// The directory comes from the walk of DRS-FR-36 rather than from the id, for
/// the reason [`draft_dir`] gives at length.
pub fn read_draft_prompt(root: &fs::RootFs, id: &str) -> Result<DraftPrompt, String> {
    let dir = draft_dir(root, id)?;
    let record = read_record_at(root, &dir, id)?;
    // DRS-FR-15: the typed refusal for a draft that is not the single prompt
    // DRS-FR-11 requires. No file is chosen as the prompt on the author's
    // behalf, and nothing is repaired.
    let prompt_path = prompt_of(root, &dir, &record).ok_or(ERR_NOT_SINGLE_FILE)?;
    let abs = draft_file_path_in(root, &dir, &prompt_path)?;
    // DRS-FR-38: a prompt that is present and structurally sound but cannot be
    // decoded as text — or that a transient I/O failure keeps shut — is its own
    // outcome. Folding it into the "not found" of a missing draft would have a
    // caller tell its user the draft does not exist, which is false and sends
    // them looking for an id they already had right.
    let content = root
        .read_text(&abs)
        .map_err(|_| ERR_PROMPT_UNREADABLE.to_string())?;
    Ok(DraftPrompt {
        draft: record,
        prompt_path,
        content,
    })
}

#[tauri::command]
pub fn rename_draft(
    id: String,
    name: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DraftRecord, String> {
    let root = project.require_root()?;
    // DRS-FR-QPSC: a GitHub-shadow draft refuses every write with its own
    // reason, whatever its status — so this guard runs before the lock.
    require_not_github_shadow(&root, &id)?;
    // DRS-FR-19 / DRS-FR-20: a draft a graduation holds, or has already
    // graduated, changes in no way — not the record, not the prompt, not a byte
    // under its folder — and nothing is emitted for a call that landed nowhere.
    crate::graduation::require_unlocked_draft(&app, &id)?;
    let record = rename_draft_impl(&root, &id, &name)?;
    // DRS-FR-09: a rename moves the primary file, so the file the draft is now
    // bound to is what changed on disk.
    announce(
        &app,
        DraftChange::to(&id, record.prompt_path.clone().into_iter().collect()),
    );
    Ok(record)
}

#[tauri::command]
pub fn set_draft_status(
    id: String,
    status: DraftStatus,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DraftRecord, String> {
    let root = project.require_root()?;
    // DRS-FR-QPSC: a GitHub-shadow draft refuses every write with its own
    // reason, whatever its status — so this guard runs before the lock.
    require_not_github_shadow(&root, &id)?;
    // DRS-FR-19 / DRS-FR-20: a draft a graduation holds, or has already
    // graduated, changes in no way — not the record, not the prompt, not a byte
    // under its folder — and nothing is emitted for a call that landed nowhere.
    crate::graduation::require_unlocked_draft(&app, &id)?;
    let record = set_draft_status_impl(&root, &id, status)?;
    // DRS-FR-10: archiving moves not one file, so the change is to the record
    // alone and names no path.
    announce(&app, DraftChange::record_of(&id));
    Ok(record)
}

#[tauri::command]
pub fn delete_draft(
    id: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    // DRS-FR-QPSC: a GitHub-shadow draft refuses every write with its own
    // reason, whatever its status — so this guard runs before the lock.
    require_not_github_shadow(&root, &id)?;
    // DRS-FR-19 / DRS-FR-20: a draft a graduation holds, or has already
    // graduated, changes in no way — not the record, not the prompt, not a byte
    // under its folder — and nothing is emitted for a call that landed nowhere.
    crate::graduation::require_unlocked_draft(&app, &id)?;
    let store = project.require_store()?;
    delete_draft_impl(&root, &store, &id)?;
    // PST-FR-KGRW: `delete_draft_impl` raised the deletion event, and is reached
    // by callers that hold no sink, so the sink is offered here. The statistics
    // log and the conversations went too, and neither is in a commit: both
    // stand outside every worktree (`RMS-repository-machine-storage.md`
    // RMS-FR-HDNZ).
    crate::storage_floor::commit::offer_sink(&app);
    // The whole draft went, files and all, so there is no surviving path to
    // name.
    announce(&app, DraftChange::record_of(&id));
    Ok(())
}

#[tauri::command]
pub fn load_draft_file_contents(
    id: String,
    path: String,
    project: State<'_, ProjectState>,
) -> Result<DraftContents, String> {
    let root = project.require_root()?;
    load_draft_file_impl(&root, &id, &path)
}

#[tauri::command]
pub fn save_draft_file_contents(
    id: String,
    path: String,
    body: String,
    app: tauri::AppHandle,
    project: State<'_, ProjectState>,
) -> Result<DraftSaveResult, String> {
    let root = project.require_root()?;
    // DRS-FR-QPSC: a GitHub-shadow draft refuses every write with its own
    // reason, whatever its status — so this guard runs before the lock.
    require_not_github_shadow(&root, &id)?;
    // DRS-FR-19 / DRS-FR-20: a draft a graduation holds, or has already
    // graduated, changes in no way — not the record, not the prompt, not a byte
    // under its folder — and nothing is emitted for a call that landed nowhere.
    crate::graduation::require_unlocked_draft(&app, &id)?;
    let saved = save_draft_file_impl(&root, &id, &path, &body)?;
    // PST-FR-DQZT: a save is no draft event, so it commits nothing.
    // `DAS-draft-assets.md` DAS-FR-14: an asset this session stored is held
    // until the insertion that asked for it settles, and a write of the prompt
    // is what settles it — from here on the saved prompt is what protects an
    // asset from housekeeping (DAS-FR-13). Released after the write has landed,
    // so a pass running in the instant before it still finds the hold.
    if let Some(sweeper) = app.try_state::<crate::draft_assets::DraftAssetSweeper>() {
        sweeper.release_draft(&id);
    }
    // DRS-FR-22: a write refreshes the draft's last-activity instant, so the
    // panel's relative timestamps and its ordering follow the author's typing.
    announce(&app, DraftChange::to(&id, vec![path]));
    Ok(saved)
}

