//! Updating the open proposal of a change (CEP-FR-AXEV), with the semantics
//! and the refusals of PCP-FR-RVSN and PCP-FR-RKJI.

use super::*;
use crate::comments::ProposalUpdateCounts;
use crate::prompt_proposals::HunkState;
use crate::text_hunks::hunks::{ContextSide, HunkDocument, PlanError};
use crate::text_hunks::updates::{self, Standing, UpdateEntry, UpdateError};

/// One update entry, with the change-relative path an `add` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeUpdateEntry {
    pub entry: UpdateEntry,
    /// CEP-FR-AXEV: required for an `add`, ignored otherwise.
    pub path: Option<String>,
}

/// What a caller hands [`update_change_proposal`].
pub struct ChangeProposalUpdate<'a> {
    pub stream_id: &'a str,
    pub change: &'a str,
    pub updates: &'a [ChangeUpdateEntry],
    pub rationale: &'a str,
    pub agent: &'a Participant,
    pub thread_id: &'a str,
}

/// Why [`update_change_proposal`] wrote nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateRefusal {
    NoOpenProposal,
    /// CEP-FR-OIZT, PCE-FR-MVKT.
    ReworkRunning,
    NoUpdates,
    UnknownStream,
    ChangeNotFound,
    PathOutsideChange {
        at: usize,
        of: usize,
    },
    PathMissing {
        at: usize,
        of: usize,
    },
    HunkNotFound {
        at: usize,
        of: usize,
    },
    HunkNotUpdatable {
        at: usize,
        of: usize,
    },
    HunkNamedTwice {
        at: usize,
        of: usize,
    },
    NoChange {
        at: usize,
        of: usize,
    },
    HunkAnchorLost {
        at: usize,
        of: usize,
    },
    HunkAmbiguous {
        at: usize,
        of: usize,
    },
    HunkOverlap {
        at: usize,
        of: usize,
    },
    HunkContextTooLong {
        at: usize,
        of: usize,
        side: ContextSide,
    },
    ThreadResolved,
    NotRecorded,
}

impl UpdateRefusal {
    fn of_plan(error: UpdateError, places: &[usize], of: usize) -> Self {
        let p = |k: usize| places.get(k.saturating_sub(1)).copied().unwrap_or(1);
        match error {
            UpdateError::NoUpdates | UpdateError::Plan(PlanError::NoHunks) => Self::NoUpdates,
            UpdateError::UnknownHunk(k) => Self::HunkNotFound { at: p(k), of },
            UpdateError::NotUpdatable(k) => Self::HunkNotUpdatable { at: p(k), of },
            UpdateError::NamedTwice(k) => Self::HunkNamedTwice { at: p(k), of },
            UpdateError::NoChange(k) => Self::NoChange { at: p(k), of },
            UpdateError::Plan(PlanError::AnchorLost(k)) => Self::HunkAnchorLost { at: p(k), of },
            UpdateError::Plan(PlanError::Ambiguous(k)) => Self::HunkAmbiguous { at: p(k), of },
            UpdateError::Plan(PlanError::Overlap(k)) => Self::HunkOverlap { at: p(k), of },
            UpdateError::Plan(PlanError::ContextTooLong(k, side)) => {
                Self::HunkContextTooLong { at: p(k), of, side }
            }
        }
    }

    /// A stable code for the log record (CEP-FR-MZFF).
    pub fn code(self) -> &'static str {
        match self {
            Self::NoOpenProposal => "no_open_proposal",
            Self::ReworkRunning => ERR_REWORK_RUNNING,
            Self::NoUpdates => "no_updates",
            Self::UnknownStream => ERR_UNKNOWN_STREAM,
            Self::ChangeNotFound => ERR_CHANGE_NOT_FOUND,
            Self::PathOutsideChange { .. } => ERR_PATH_OUTSIDE_CHANGE,
            Self::PathMissing { .. } => "path_missing",
            Self::HunkNotFound { .. } => ERR_HUNK_NOT_FOUND,
            Self::HunkNotUpdatable { .. } => "hunk_not_updatable",
            Self::HunkNamedTwice { .. } => "hunk_named_twice",
            Self::NoChange { .. } => "no_change",
            Self::HunkAnchorLost { .. } => "hunk_anchor_lost",
            Self::HunkAmbiguous { .. } => "hunk_ambiguous",
            Self::HunkOverlap { .. } => "hunk_overlap",
            Self::HunkContextTooLong { .. } => "hunk_context_too_long",
            Self::ThreadResolved => comments::ERR_DISCUSSION_RESOLVED,
            Self::NotRecorded => ERR_WRITE_FAILED,
        }
    }
}

/// CEP-FR-AXEV: change, add, and withdraw hunks of the change's pending
/// proposal, and append one comment, as one act.
pub fn update_change_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    update: ChangeProposalUpdate<'_>,
) -> Result<ChangeEditProposal, UpdateRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _guard = change_lock(update.stream_id, update.change);
    let outcome = update_locked(app, store, sites, &update);
    if let Err(refusal) = &outcome {
        log_refusal(
            app,
            "update",
            refusal.code(),
            Subject::Change { stream_id: update.stream_id, change: update.change },
        );
    }
    outcome
}

/// The entries of one file, with their places in the caller's list.
struct FileEntries {
    /// Index into the set's files, or `None` for a path the set does not hold.
    file: Option<usize>,
    path: String,
    places: Vec<usize>,
    entries: Vec<UpdateEntry>,
}

fn update_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    update: &ChangeProposalUpdate<'_>,
) -> Result<ChangeEditProposal, UpdateRefusal>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let site = sites.site(update.stream_id, update.change).map_err(|e| match e.as_str() {
        ERR_UNKNOWN_STREAM => UpdateRefusal::UnknownStream,
        _ => UpdateRefusal::ChangeNotFound,
    })?;
    // CEP-FR-OIZT / PCE-FR-MVKT: a rework run of the change gives its result
    // to the author as its own proposal, so no other write reaches the change.
    if sites.rework_run(update.stream_id, update.change).is_some() {
        return Err(UpdateRefusal::ReworkRunning);
    }
    if let Some(open) = pending_for(store, update.stream_id, update.change) {
        reconcile_locked(app, store, sites, &open.id).map_err(|_| UpdateRefusal::NotRecorded)?;
    }
    let mut record =
        pending_for(store, update.stream_id, update.change).ok_or(UpdateRefusal::NoOpenProposal)?;
    if update.updates.is_empty() {
        return Err(UpdateRefusal::NoUpdates);
    }
    let dir = proposals_dir(store, update.stream_id, update.change)
        .map_err(|_| UpdateRefusal::NotRecorded)?;
    let mut set = read_hunks(store, &dir, &record.id).map_err(|_| UpdateRefusal::NotRecorded)?;
    let of = update.updates.len();

    // Each entry goes to the file it is about, in the order the files appear.
    let mut groups: Vec<FileEntries> = Vec::new();
    for (index, item) in update.updates.iter().enumerate() {
        let place = index + 1;
        let (file, path) = match (&item.entry, item.entry.id()) {
            (_, Some(id)) => {
                let file = set
                    .files
                    .iter()
                    .position(|f| f.document.find(id).is_some())
                    .ok_or(UpdateRefusal::HunkNotFound { at: place, of })?;
                (Some(file), set.files[file].path.clone())
            }
            (UpdateEntry::Add(_), None) => {
                let path = item
                    .path
                    .clone()
                    .filter(|p| !p.trim().is_empty())
                    .ok_or(UpdateRefusal::PathMissing { at: place, of })?;
                site.file_rel(&path)
                    .map_err(|_| UpdateRefusal::PathOutsideChange { at: place, of })?;
                (set.files.iter().position(|f| f.path == path), path)
            }
            _ => return Err(UpdateRefusal::NoUpdates),
        };
        match groups.iter_mut().find(|g| g.path == path) {
            Some(group) => {
                group.places.push(place);
                group.entries.push(item.entry.clone());
            }
            None => groups.push(FileEntries {
                file,
                path,
                places: vec![place],
                entries: vec![item.entry.clone()],
            }),
        }
    }

    let standing = |id: &str| {
        record.row(id).map(|row| {
            if row.state.is_updatable() {
                Standing::Live
            } else {
                Standing::Settled
            }
        })
    };
    let mut changed: Vec<String> = Vec::new();
    let mut added: Vec<(String, bool, String)> = Vec::new();
    let mut withdrawn: Vec<String> = Vec::new();
    for group in &groups {
        let (current, doc, new_file) = match group.file {
            Some(i) => {
                let file = &set.files[i];
                let current = if file.new_file {
                    String::new()
                } else {
                    site.read_file(&file.path).unwrap_or_default()
                };
                (current, file.document.clone(), file.new_file)
            }
            None => match site.read_file(&group.path) {
                Some(current) => (current, HunkDocument::new(Vec::new()), false),
                None if site.exists(&group.path) => {
                    return Err(UpdateRefusal::PathOutsideChange { at: group.places[0], of });
                }
                None => (String::new(), HunkDocument::new(Vec::new()), true),
            },
        };
        // CEP-FR-HRCX: a new file holds one hunk, its whole text.
        let adds = group.entries.iter().filter(|e| matches!(e, UpdateEntry::Add(_))).count();
        if new_file && (adds > 1 || (adds == 1 && !doc.hunks.is_empty())) {
            let at = group
                .entries
                .iter()
                .zip(&group.places)
                .rfind(|(e, _)| matches!(e, UpdateEntry::Add(_)))
                .map(|(_, p)| *p)
                .unwrap_or(group.places[0]);
            return Err(UpdateRefusal::HunkOverlap { at, of });
        }
        let plan = updates::plan_update(&current, &doc, standing, &group.entries, new_note_id)
            .map_err(|error| UpdateRefusal::of_plan(error, &group.places, of))?;
        changed.extend(plan.changed.iter().cloned());
        withdrawn.extend(plan.withdrawn.iter().cloned());
        for id in &plan.added {
            added.push((id.clone(), new_file, group.path.clone()));
        }
        match group.file {
            Some(i) => set.files[i].document = plan.doc,
            None => set.files.push(HunkSetFile {
                path: group.path.clone(),
                new_file,
                document: plan.doc,
            }),
        }
    }

    let thread = locate_thread(store, update.stream_id, update.change, update.thread_id)
        .map_err(|_| UpdateRefusal::NotRecorded)?;
    if thread.resolved {
        return Err(UpdateRefusal::ThreadResolved);
    }

    // PCP-FR-RVSN: the ledger follows the plan.
    for row in record.ledger.iter_mut() {
        if changed.contains(&row.id) {
            row.edited = false;
            row.revision += 1;
            row.state = HunkState::Pending;
            row.decided_at = None;
        } else if withdrawn.contains(&row.id) {
            row.state = HunkState::Withdrawn;
            row.decided_at = None;
            row.edited = false;
        }
    }
    for id in changed.iter().chain(added.iter().map(|(id, _, _)| id)) {
        if let Some((_, hunk)) = set.find(id) {
            record
                .origin_checksums
                .insert(id.clone(), fs::sha256_bytes(hunk.after_text().as_bytes()));
        }
    }
    for (id, new_file, path) in &added {
        if let Some((_, hunk)) = set.find(id) {
            record.ledger.push(ChangeHunkRow::new(path, *new_file, hunk));
        }
    }
    resettle(&mut record);

    let proposal_id = record.id.clone();
    let prior_hunks = store.read_text(at(&dir, &proposal_id, HUNKS_EXT)).ok();
    let prior_record = store.read_text(at(&dir, &proposal_id, RECORD_EXT)).ok();
    let restore = || {
        if let Some(text) = &prior_hunks {
            let _ = store.write_text_atomic(at(&dir, &proposal_id, HUNKS_EXT), text);
        }
        if let Some(text) = &prior_record {
            let _ = store.write_text_atomic(at(&dir, &proposal_id, RECORD_EXT), text);
        }
    };
    let hunks_written = set.to_toml().and_then(|text| {
        store.write_text_atomic(at(&dir, &proposal_id, HUNKS_EXT), &text).map_err(|e| e.to_string())
    });
    if hunks_written.is_err() || write_record(store, &dir, &record).is_err() {
        restore();
        return Err(UpdateRefusal::NotRecorded);
    }

    // CMS-FR-BSQB: one agent comment, with the counts of this update.
    let attachment = comments::Attachment::ChangeProposal {
        proposal_id: proposal_id.clone(),
        stream_id: record.stream_id.clone(),
        change: record.change.clone(),
        update: Some(ProposalUpdateCounts {
            changed: changed.len() as u32,
            added: added.len() as u32,
            withdrawn: withdrawn.len() as u32,
        }),
    };
    let appended = match comments::append_agent_comment_with(
        store,
        thread.log_ref(),
        update.thread_id,
        new_note_id(),
        update.rationale.to_string(),
        vec![attachment],
        comments::Stamp { by: update.agent, at: &now_rfc3339() },
    ) {
        Ok(thread) => thread,
        Err(error) => {
            restore();
            return Err(if error == comments::ERR_DISCUSSION_RESOLVED {
                UpdateRefusal::ThreadResolved
            } else {
                UpdateRefusal::NotRecorded
            });
        }
    };
    comments::emit_discussion_changed(app, &appended);
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "change edit proposal updated",
        log_fields! {
            "streamId" => &record.stream_id,
            "change" => &record.change,
            "proposalId" => &proposal_id,
            "changed" => changed.len(),
            "added" => added.len(),
            "withdrawn" => withdrawn.len(),
        },
    );
    announce(app, &record, ChangeProposalCause::Updated);
    Ok(record)
}
