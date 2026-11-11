//! Recording a change edit proposal — the only path by which one comes into
//! existence (CEP-FR-QDJH).

use super::*;
use crate::text_hunks::hunks::{self as text, PlanError, ProposedHunk};

/// One proposed hunk with the change-relative path of its file (PCE-FR-PQUD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathedHunk {
    pub path: String,
    pub hunk: ProposedHunk,
}

/// What a caller hands [`record_change_proposal`].
pub struct NewChangeProposal<'a> {
    pub stream_id: &'a str,
    pub change: &'a str,
    pub hunks: &'a [PathedHunk],
    pub rationale: &'a str,
    pub agent: &'a Participant,
    /// The conversation of the turn (CEP-FR-QDJH's `origin`).
    pub thread_id: &'a str,
}

/// The hunks of one path, with the one-based places the caller gave them.
pub(super) struct PathGroup<'a> {
    pub(super) path: &'a str,
    pub(super) places: Vec<usize>,
    pub(super) hunks: Vec<ProposedHunk>,
}

/// Group hunks by path, in the order each path first appears.
pub(super) fn group_by_path(hunks: &[PathedHunk]) -> Vec<PathGroup<'_>> {
    let mut groups: Vec<PathGroup<'_>> = Vec::new();
    for (index, entry) in hunks.iter().enumerate() {
        match groups.iter_mut().find(|g| g.path == entry.path) {
            Some(group) => {
                group.places.push(index + 1);
                group.hunks.push(entry.hunk.clone());
            }
            None => groups.push(PathGroup {
                path: &entry.path,
                places: vec![index + 1],
                hunks: vec![entry.hunk.clone()],
            }),
        }
    }
    groups
}

/// A plan error of one group, at the place the caller counted.
pub(super) fn refusal_at(error: PlanError, places: &[usize], of: usize) -> RecordRefusal {
    let place = |k: usize| places.get(k.saturating_sub(1)).copied().unwrap_or(1);
    match error {
        PlanError::NoHunks => RecordRefusal::NoHunks,
        PlanError::AnchorLost(k) => RecordRefusal::HunkAnchorLost { at: place(k), of },
        PlanError::Ambiguous(k) => RecordRefusal::HunkAmbiguous { at: place(k), of },
        PlanError::Overlap(k) => RecordRefusal::HunkOverlap { at: place(k), of },
        PlanError::ContextTooLong(k, side) => {
            RecordRefusal::HunkContextTooLong { at: place(k), of, side }
        }
    }
}

/// Plan the hunks of one path against its file in the working copy.
///
/// CEP-FR-HRCX: a path that names no file of the change is a new file. Its one
/// hunk is an `add` placed against the empty text, and a second hunk on it
/// overlaps the first.
pub(super) fn plan_group(
    site: &ChangeSite,
    group: &PathGroup<'_>,
    of: usize,
) -> Result<HunkSetFile, RecordRefusal> {
    let first = group.places[0];
    site.file_rel(group.path).map_err(|_| RecordRefusal::PathOutsideChange { at: first, of })?;
    match site.read_file(group.path) {
        Some(current) => {
            let document = text::plan(&current, &group.hunks, new_note_id)
                .map_err(|e| refusal_at(e, &group.places, of))?;
            Ok(HunkSetFile { path: group.path.to_string(), new_file: false, document })
        }
        // Something stands at the path and it is not a file.
        None if site.exists(group.path) => Err(RecordRefusal::PathOutsideChange { at: first, of }),
        None => {
            if group.hunks.len() > 1 {
                return Err(RecordRefusal::HunkOverlap { at: group.places[1], of });
            }
            let hunk = &group.hunks[0];
            let whole = hunk.after.clone().or_else(|| hunk.after_text.clone()).unwrap_or_default();
            if whole.is_empty() || hunk.before.as_deref().is_some_and(|b| !b.is_empty()) {
                return Err(RecordRefusal::HunkAnchorLost { at: first, of });
            }
            let document = text::plan("", &text::whole_document("", &whole), new_note_id)
                .map_err(|e| refusal_at(e, &group.places, of))?;
            Ok(HunkSetFile { path: group.path.to_string(), new_file: true, document })
        }
    }
}

/// PCP-FR-07: whether a set of files changes nothing, as the write path would
/// persist each file.
fn changes_nothing(site: &ChangeSite, files: &[HunkSetFile]) -> bool {
    files.iter().all(|file| {
        if file.new_file {
            return false;
        }
        let current = site.read_file(&file.path).unwrap_or_default();
        text::apply_all(&current, &file.document)
            .is_some_and(|built| site.normalise(&built) == current)
    })
}

/// CEP-FR-QDJH: record a proposal and append its comment, as one act.
///
/// The hunk set and the record reach the store before the comment is appended,
/// and a failure to append removes both (PCP-FR-06).
pub fn record_change_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    new: NewChangeProposal<'_>,
) -> Result<ChangeEditProposal, RecordRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    record_logged(app, store, sites, &new, None)
}

/// CEP-FR-QDJH, CEP-FR-OIZT: the recording of the finish of the rework run
/// `run_id` (`OCR-openspec-change-runs.md` OCR-FR-WBZX). The `rework_running`
/// refusal does not stop the run it names.
pub fn record_rework_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    new: NewChangeProposal<'_>,
    run_id: &str,
) -> Result<ChangeEditProposal, RecordRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    record_logged(app, store, sites, &new, Some(run_id))
}

/// CEP-FR-QDJH, CEP-FR-YYZZ: the recording of the plan proposal of the apply
/// run `run_id` (`OPU-openspec-plan-updates.md` OPU-FR-OQRA). The
/// `apply_running` refusal does not stop the run it names.
pub fn record_plan_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    new: NewChangeProposal<'_>,
    run_id: &str,
) -> Result<ChangeEditProposal, RecordRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    record_logged(app, store, sites, &new, Some(run_id))
}

fn record_logged<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    new: &NewChangeProposal<'_>,
    own_run_id: Option<&str>,
) -> Result<ChangeEditProposal, RecordRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let mut admitted = None;
    let outcome = record_locked(app, store, sites, new, own_run_id, &mut admitted);
    // OPU-FR-WOKJ: the run writes the id of an admitted plan proposal on its
    // rest. The change lock is released first.
    if let (Ok(record), Some(run_id)) = (&outcome, admitted) {
        sites.admit_plan_proposal(&run_id, &record.id);
    }
    match &outcome {
        Ok(record) => log_recorded(app, record),
        Err(refusal) => log_refusal(
            app,
            "record",
            refusal.code(),
            Subject::Change { stream_id: new.stream_id, change: new.change },
        ),
    }
    outcome
}

fn record_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    new: &NewChangeProposal<'_>,
    own_run_id: Option<&str>,
    admitted: &mut Option<String>,
) -> Result<ChangeEditProposal, RecordRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _guard = change_lock(new.stream_id, new.change);
    let site = sites.site(new.stream_id, new.change).map_err(|e| match e.as_str() {
        ERR_UNKNOWN_STREAM => RecordRefusal::UnknownStream,
        _ => RecordRefusal::ChangeNotFound,
    })?;
    // CEP-FR-OIZT: while a rework run of the change is not terminal, only the
    // finish of that run records a proposal on it. `own_run_id` names the run
    // whose own recording this is.
    if let Some(running) = sites.rework_run(new.stream_id, new.change) {
        if own_run_id != Some(running.as_str()) {
            return Err(RecordRefusal::ReworkRunning);
        }
    }
    // CEP-FR-YYZZ / OPU-FR-DEEB: while an apply run of the change is not
    // terminal, only its own plan proposal and the plan proposal it admits at
    // a `plan_findings` rest (OPU-FR-WOKJ) are recorded.
    let gate = sites.apply_run(new.stream_id, new.change);
    if let Some(gate) = gate.filter(|gate| own_run_id != Some(gate.run_id.as_str())) {
        if gate.admits_thread.as_deref() != Some(new.thread_id) {
            return Err(RecordRefusal::ApplyRunning);
        }
        *admitted = Some(gate.run_id);
    }
    let of = new.hunks.len();
    if of == 0 {
        return Err(RecordRefusal::NoHunks);
    }
    // CEP-FR-WJKY: every path is checked before anything else, in the order
    // given, so the first refusal names the first path outside the change.
    for (index, entry) in new.hunks.iter().enumerate() {
        if site.file_rel(&entry.path).is_err() {
            return Err(RecordRefusal::PathOutsideChange { at: index + 1, of });
        }
    }
    if pending_for(store, new.stream_id, new.change).is_some() {
        return Err(RecordRefusal::ProposalPending);
    }
    let files = group_by_path(new.hunks)
        .iter()
        .map(|group| plan_group(&site, group, of))
        .collect::<Result<Vec<_>, _>>()?;
    if changes_nothing(&site, &files) {
        return Err(RecordRefusal::NoChange);
    }

    let thread = locate_thread(store, new.stream_id, new.change, new.thread_id)
        .map_err(|_| RecordRefusal::NotRecorded)?;
    if thread.resolved {
        return Err(RecordRefusal::ThreadResolved);
    }

    let dir =
        proposals_dir(store, new.stream_id, new.change).map_err(|_| RecordRefusal::NotRecorded)?;
    let set = HunkSet { files };
    let id = new_note_id();
    let comment_id = new_note_id();
    let now = now_rfc3339();
    let ledger: Vec<ChangeHunkRow> = set
        .hunks()
        .map(|(file, hunk)| ChangeHunkRow::new(&file.path, file.new_file, hunk))
        .collect();
    let origin_checksums = set
        .hunks()
        .map(|(_, hunk)| (hunk.id.clone(), fs::sha256_bytes(hunk.after_text().as_bytes())))
        .collect();
    let mut record = ChangeEditProposal {
        id: id.clone(),
        stream_id: new.stream_id.to_string(),
        change: new.change.to_string(),
        rationale: new.rationale.to_string(),
        thread_id: new.thread_id.to_string(),
        comment_id: comment_id.clone(),
        state: ChangeProposalState::Pending,
        candidate_edited: false,
        comment_owed: false,
        commit_owed: false,
        commit: None,
        hunk_count: 0,
        created_at: now.clone(),
        decided_at: None,
        counts: Default::default(),
        written_paths: Vec::new(),
        ledger,
        origin_checksums,
        agent: new.agent.clone(),
    };
    resettle(&mut record);

    let cleanup = || {
        let _ = store.delete_under(&dir, format!("{id}.{HUNKS_EXT}"), false);
        let _ = store.delete_under(&dir, format!("{id}.{RECORD_EXT}"), false);
    };
    let written = set.to_toml().and_then(|text| {
        store.write_text_atomic(at(&dir, &id, HUNKS_EXT), &text).map_err(|e| e.to_string())
    });
    if written.is_err() {
        cleanup();
        return Err(RecordRefusal::NotRecorded);
    }
    if write_record(store, &dir, &record).is_err() {
        cleanup();
        return Err(RecordRefusal::NotRecorded);
    }

    // CMS-FR-QLMV: the comment carries the reference to the proposal.
    let attachment = comments::Attachment::ChangeProposal {
        proposal_id: id.clone(),
        stream_id: new.stream_id.to_string(),
        change: new.change.to_string(),
        update: None,
    };
    let appended = match comments::append_agent_comment_with(
        store,
        thread.log_ref(),
        new.thread_id,
        comment_id,
        new.rationale.to_string(),
        vec![attachment],
        comments::Stamp { by: new.agent, at: &now },
    ) {
        Ok(thread) => thread,
        Err(error) => {
            cleanup();
            return Err(if error == comments::ERR_DISCUSSION_RESOLVED {
                RecordRefusal::ThreadResolved
            } else {
                RecordRefusal::NotRecorded
            });
        }
    };
    comments::emit_discussion_changed(app, &appended);
    announce(app, &record, ChangeProposalCause::Recorded);
    Ok(record)
}
