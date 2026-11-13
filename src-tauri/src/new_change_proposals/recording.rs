//! Recording and updating a new-change proposal (NCP-FR-VTPG, NCP-FR-RPGN,
//! NCP-FR-NDHQ, NCP-FR-SNMA).

use super::*;

/// What a caller hands [`record_new_change_proposal`].
pub struct NewChange<'a> {
    pub stream_id: &'a str,
    pub change: &'a str,
    pub name: &'a str,
    pub files: &'a [ProposedFile],
    pub position: u32,
    pub total: u32,
    pub rationale: &'a str,
    pub agent: &'a Participant,
    pub thread_id: &'a str,
}

/// What an update does (NCP-FR-SNMA).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewChangeUpdateAction {
    Change {
        name: Option<String>,
        files: Option<Vec<ProposedFile>>,
        position: Option<u32>,
        total: Option<u32>,
    },
    Withdraw,
}

/// What a caller hands [`update_new_change_proposal`].
pub struct NewChangeUpdate<'a> {
    pub stream_id: &'a str,
    pub change: &'a str,
    pub action: NewChangeUpdateAction,
    pub rationale: &'a str,
    pub agent: &'a Participant,
    pub thread_id: &'a str,
}

/// NCP-FR-RPGN: the name, the files, the position, and the total are valid,
/// and the name names no change of the stream and no live proposal of the
/// split other than `except`.
fn check_fields(
    site: &ChangeSite,
    others: &[NewChangeProposal],
    except: Option<&str>,
    (name, files, position, total): (&str, &[ProposedFile], u32, u32),
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(ERR_BLANK_NAME.to_string());
    }
    if !crate::openspec::change::valid_change_name(name) {
        return Err(ERR_INVALID_CHANGE_NAME.to_string());
    }
    if position < 1 || total < position {
        return Err(ERR_BAD_POSITION.to_string());
    }
    check_files(files)?;
    let taken = folder_exists(site, name)
        || others.iter().any(|p| {
            Some(p.id.as_str()) != except
                && p.name == name
                && matches!(
                    p.state,
                    NewChangeProposalState::Pending | NewChangeProposalState::Accepted
                )
        });
    if taken {
        return Err(ERR_CHANGE_EXISTS.to_string());
    }
    Ok(())
}

/// NCP-FR-VTPG: record a proposal and append its comment, as one act.
pub fn record_new_change_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    new: NewChange<'_>,
) -> Result<NewChangeProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _guard = split_lock(new.stream_id, new.change);
    let outcome = record_locked(app, store, sites, &new);
    match &outcome {
        Ok(record) => log_info(app, "recorded", record),
        Err(code) => log_refusal(app, "record", code, (new.stream_id, new.change)),
    }
    outcome
}

fn record_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    new: &NewChange<'_>,
) -> Result<NewChangeProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let site = sites.site(new.stream_id, new.change)?;
    if read_split(store, new.stream_id, new.change).is_some() {
        return Err(ERR_SPLIT_CONFIRMED.to_string());
    }
    let others = list_proposals_impl(store, new.stream_id, new.change);
    if others.iter().any(|p| p.state == NewChangeProposalState::Pending) {
        return Err(ERR_PROPOSAL_PENDING.to_string());
    }
    check_fields(&site, &others, None, (new.name, new.files, new.position, new.total))?;
    let thread = locate_thread(store, new.stream_id, new.change, new.thread_id)?;
    if thread.resolved {
        return Err(ERR_DISCUSSION_RESOLVED.to_string());
    }
    // NCP-FR-NDHQ: one split per change, its id minted by the first recording.
    let set_id = others.first().map(|p| p.set_id.clone()).unwrap_or_else(new_note_id);
    let dir = proposals_dir(store, new.stream_id, new.change)?;
    let id = new_note_id();
    let comment_id = new_note_id();
    let now = now_rfc3339();
    let record = NewChangeProposal {
        id: id.clone(),
        set_id,
        stream_id: new.stream_id.to_string(),
        change: new.change.to_string(),
        name: new.name.to_string(),
        paths: new.files.iter().map(|f| f.path.clone()).collect(),
        position: new.position,
        total: new.total,
        rationale: new.rationale.to_string(),
        thread_id: new.thread_id.to_string(),
        comment_id: comment_id.clone(),
        state: NewChangeProposalState::Pending,
        revision: 1,
        created_change: None,
        commit_owed: false,
        created_at: now.clone(),
        decided_at: None,
        agent: new.agent.clone(),
    };
    let cleanup = || {
        let _ = store.delete_under(&dir, format!("{id}.{FILES_EXT}"), false);
        let _ = store.delete_under(&dir, format!("{id}.{RECORD_EXT}"), false);
    };
    let files = FilesDocument { files: new.files.to_vec() };
    if store.write_toml_atomic(at(&dir, &id, FILES_EXT), &files).is_err()
        || write_record(store, &dir, &record).is_err()
    {
        cleanup();
        return Err(ERR_WRITE_FAILED.to_string());
    }
    let attachment = comments::Attachment::NewChangeProposal {
        proposal_id: id.clone(),
        stream_id: new.stream_id.to_string(),
        change: new.change.to_string(),
        update: None,
    };
    match comments::append_agent_comment_with(
        store,
        thread.log_ref(),
        new.thread_id,
        comment_id,
        new.rationale.to_string(),
        vec![attachment],
        comments::Stamp { by: new.agent, at: &now },
    ) {
        Ok(appended) => comments::emit_discussion_changed(app, &appended),
        Err(error) => {
            cleanup();
            return Err(if error == comments::ERR_DISCUSSION_RESOLVED {
                ERR_DISCUSSION_RESOLVED.to_string()
            } else {
                ERR_WRITE_FAILED.to_string()
            });
        }
    }
    announce(app, new.stream_id, new.change, Some(&record), NewChangeCause::Recorded);
    Ok(record)
}

/// NCP-FR-SNMA: change or withdraw the pending proposal, and append one
/// comment, as one act. A refused update leaves both files as they were.
pub fn update_new_change_proposal<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    update: NewChangeUpdate<'_>,
) -> Result<NewChangeProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _guard = split_lock(update.stream_id, update.change);
    let outcome = update_locked(app, store, sites, &update);
    match &outcome {
        Ok(record) => log_info(app, "updated", record),
        Err(code) => log_refusal(app, "update", code, (update.stream_id, update.change)),
    }
    outcome
}

fn update_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    update: &NewChangeUpdate<'_>,
) -> Result<NewChangeProposal, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let site = sites.site(update.stream_id, update.change)?;
    if read_split(store, update.stream_id, update.change).is_some() {
        return Err(ERR_SPLIT_CONFIRMED.to_string());
    }
    let others = list_proposals_impl(store, update.stream_id, update.change);
    let mut record = others
        .iter()
        .find(|p| p.state == NewChangeProposalState::Pending)
        .cloned()
        .ok_or_else(|| ERR_NO_OPEN_PROPOSAL.to_string())?;
    let dir = proposals_dir(store, update.stream_id, update.change)?;
    let mut files = read_files(store, &dir, &record.id)?;
    let action_name;
    match &update.action {
        NewChangeUpdateAction::Withdraw => {
            action_name = crate::comments::NewDraftUpdateAction::Withdraw;
            record.state = NewChangeProposalState::Withdrawn;
            record.decided_at = Some(now_rfc3339());
        }
        NewChangeUpdateAction::Change { name, files: new_files, position, total } => {
            action_name = crate::comments::NewDraftUpdateAction::Change;
            let next_name = name.clone().unwrap_or_else(|| record.name.clone());
            let next_files = new_files.clone().unwrap_or_else(|| files.clone());
            let next_position = position.unwrap_or(record.position);
            let next_total = total.unwrap_or(record.total);
            if next_name == record.name
                && next_files == files
                && next_position == record.position
                && next_total == record.total
            {
                return Err(ERR_NO_CHANGE.to_string());
            }
            check_fields(
                &site,
                &others,
                Some(&record.id),
                (&next_name, &next_files, next_position, next_total),
            )?;
            record.name = next_name;
            record.paths = next_files.iter().map(|f| f.path.clone()).collect();
            record.position = next_position;
            record.total = next_total;
            record.revision += 1;
            files = next_files;
        }
    }
    let thread = locate_thread(store, update.stream_id, update.change, update.thread_id)?;
    if thread.resolved {
        return Err(ERR_DISCUSSION_RESOLVED.to_string());
    }
    let prior_files = store.read_text(at(&dir, &record.id, FILES_EXT)).ok();
    let prior_record = store.read_text(at(&dir, &record.id, RECORD_EXT)).ok();
    let restore = || {
        if let Some(text) = &prior_files {
            let _ = store.write_text_atomic(at(&dir, &record.id, FILES_EXT), text);
        }
        if let Some(text) = &prior_record {
            let _ = store.write_text_atomic(at(&dir, &record.id, RECORD_EXT), text);
        }
    };
    if store.write_toml_atomic(at(&dir, &record.id, FILES_EXT), &FilesDocument { files }).is_err()
        || write_record(store, &dir, &record).is_err()
    {
        restore();
        return Err(ERR_WRITE_FAILED.to_string());
    }
    let attachment = comments::Attachment::NewChangeProposal {
        proposal_id: record.id.clone(),
        stream_id: record.stream_id.clone(),
        change: record.change.clone(),
        update: Some(action_name),
    };
    match comments::append_agent_comment_with(
        store,
        thread.log_ref(),
        update.thread_id,
        new_note_id(),
        update.rationale.to_string(),
        vec![attachment],
        comments::Stamp { by: update.agent, at: &now_rfc3339() },
    ) {
        Ok(appended) => comments::emit_discussion_changed(app, &appended),
        Err(error) => {
            restore();
            return Err(if error == comments::ERR_DISCUSSION_RESOLVED {
                ERR_DISCUSSION_RESOLVED.to_string()
            } else {
                ERR_WRITE_FAILED.to_string()
            });
        }
    }
    announce(app, &record.stream_id, &record.change, Some(&record), NewChangeCause::Updated);
    Ok(record)
}

/// NCP-FR-EZTC: the split of a change for a turn's input. It reads and
/// writes nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitForTurn {
    pub set_id: String,
    pub confirmed: bool,
    /// Oldest first.
    pub proposals: Vec<NewChangeProposal>,
    /// The files of the pending proposal, where one is pending.
    pub pending_files: Vec<ProposedFile>,
}

/// NCP-FR-EZTC: `None` where the change holds no new-change proposal.
pub fn read_new_change_proposals(
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
) -> Option<SplitForTurn> {
    let mut proposals = list_proposals_impl(store, stream_id, change);
    if proposals.is_empty() {
        return None;
    }
    proposals.reverse();
    let dir = proposals_dir(store, stream_id, change).ok()?;
    let pending_files = proposals
        .iter()
        .find(|p| p.state == NewChangeProposalState::Pending)
        .and_then(|p| read_files(store, &dir, &p.id).ok())
        .unwrap_or_default();
    Some(SplitForTurn {
        set_id: proposals[0].set_id.clone(),
        confirmed: read_split(store, stream_id, change).is_some(),
        proposals,
        pending_files,
    })
}
