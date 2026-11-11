//! Deciding a new-change proposal and confirming a split (NCP-FR-UWOR through
//! NCP-FR-AFZP).

use super::*;

/// The commit message of an accepted new change (NCP-FR-UWOR).
pub fn commit_message(name: &str) -> String {
    format!("Add OpenSpec change {name}")
}

/// NCP-FR-EBSA: the decision comment's body.
fn decision_body(record: &NewChangeProposal, accepted: bool, feedback: Option<&str>) -> String {
    let verb = if accepted { "Accepted" } else { "Rejected" };
    let sentence = format!(
        "{verb} the proposed new OpenSpec change `{}` ({} of {}).",
        record.name, record.position, record.total
    );
    match feedback.map(str::trim).filter(|f| !f.is_empty()) {
        Some(feedback) => format!("{sentence}\n\n{feedback}"),
        None => sentence,
    }
}

/// The paths of a created change, relative to the working copy.
fn created_paths(record: &NewChangeProposal, name: &str) -> Vec<String> {
    record
        .paths
        .iter()
        .map(|p| format!("{}/{name}/{p}", crate::change_proposals::CHANGES_REL))
        .collect()
}

/// One commit of the created change folder, or the reason it did not land.
/// `Ok(false)` where Git found nothing to commit.
fn commit_created(
    site: &ChangeSite,
    record: &NewChangeProposal,
    name: &str,
) -> Result<bool, String> {
    match crate::git::commit_exact_paths(
        &site.working_copy,
        &commit_message(name),
        &created_paths(record, name),
    ) {
        Ok(_) => Ok(true),
        Err(e) if e == crate::git::ERR_NOTHING_TO_COMMIT => Ok(false),
        Err(e) => Err(e),
    }
}

/// One decision: the proposal, the direction, the feedback, and who decided.
pub struct NewChangeDecision<'a> {
    pub proposal_id: &'a str,
    pub accept: bool,
    pub feedback: Option<&'a str>,
    pub by: &'a Participant,
}

/// NCP-FR-UWOR / NCP-FR-TWDN: accept or reject a pending proposal.
pub fn decide_new_change<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    decision: NewChangeDecision<'_>,
) -> Result<NewChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (held, _) = read_proposal(store, decision.proposal_id)?;
    let _guard = split_lock(&held.stream_id, &held.change);
    let outcome = decide_locked(app, store, sites, &decision);
    match &outcome {
        Ok(done) => log_info(app, "decided", &done.proposal),
        Err(code) => log_refusal(app, "decide", code, (&held.stream_id, &held.change)),
    }
    outcome
}

fn decide_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    decision: &NewChangeDecision<'_>,
) -> Result<NewChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (mut record, dir) = read_proposal(store, decision.proposal_id)?;
    if record.state != NewChangeProposalState::Pending {
        return Err(ERR_ALREADY_DECIDED.to_string());
    }
    if read_split(store, &record.stream_id, &record.change).is_some() {
        return Err(ERR_SPLIT_CONFIRMED.to_string());
    }
    let thread = locate_thread(store, &record.stream_id, &record.change, &record.thread_id)?;
    if thread.resolved {
        return Err(ERR_DISCUSSION_RESOLVED.to_string());
    }
    let mut committed = false;
    if decision.accept {
        // NCP-FR-BOVN: refused before anything is written.
        if sites.busy_run(&record.stream_id).is_some() {
            return Err(ERR_STREAM_BUSY.to_string());
        }
        let site = sites.site(&record.stream_id, &record.change)?;
        if folder_exists(&site, &record.name) {
            return Err(ERR_CHANGE_EXISTS.to_string());
        }
        let files = read_files(store, &dir, &record.id)?;
        // NCP-FR-UWOR / NCP-FR-KRSA: the files, then the commit. A file that
        // will not write removes the files written before it.
        let mut written: Vec<PathBuf> = Vec::new();
        for file in &files {
            let rel =
                format!("{}/{}/{}", crate::change_proposals::CHANGES_REL, record.name, file.path);
            let target = fs::resolve_under(site.working_copy.path(), rel)
                .map_err(|_| ERR_PATH_OUTSIDE_CHANGE.to_string());
            let landed = target.and_then(|path| {
                site.working_copy
                    .write_text_atomic(&path, &site.normalise(&file.text))
                    .map(|()| path)
                    .map_err(|_| ERR_WRITE_FAILED.to_string())
            });
            match landed {
                Ok(path) => written.push(path),
                Err(code) => {
                    for path in &written {
                        let _ =
                            site.working_copy.delete_under(site.working_copy.path(), path, false);
                    }
                    let folder =
                        format!("{}/{}", crate::change_proposals::CHANGES_REL, record.name);
                    let _ = site.working_copy.delete_under(site.working_copy.path(), folder, true);
                    return Err(code);
                }
            }
        }
        // NCP-FR-KRSA: the record reads `accepted` and owes its commit before
        // the commit runs. A record that will not write removes the folder,
        // so no committed change stands beside a pending proposal.
        record.state = NewChangeProposalState::Accepted;
        record.created_change = Some(record.name.clone());
        record.decided_at = Some(now_rfc3339());
        record.commit_owed = true;
        if let Err(reason) = write_record(store, &dir, &record) {
            let folder = format!("{}/{}", crate::change_proposals::CHANGES_REL, record.name);
            let _ = site.working_copy.delete_under(site.working_copy.path(), folder, true);
            log_refusal(app, "record", &reason, (&record.stream_id, &record.change));
            return Err(ERR_WRITE_FAILED.to_string());
        }
        match commit_created(&site, &record, &record.name.clone()) {
            Ok(landed) => {
                committed = landed;
                record.commit_owed = false;
                // A record that does not write keeps the mark, and a
                // completion then finds nothing to commit and clears it.
                let _ = write_record(store, &dir, &record);
            }
            Err(reason) => {
                log_refusal(app, "commit", &reason, (&record.stream_id, &record.change));
            }
        }
    } else {
        record.state = NewChangeProposalState::Rejected;
        record.decided_at = Some(now_rfc3339());
        write_record(store, &dir, &record)?;
    }

    // NCP-FR-EBSA: one human comment into the proposal's own thread.
    let comment_id = new_note_id();
    let appended = comments::add_comment_with_id(
        store,
        thread.log_ref(),
        &record.thread_id,
        comment_id.clone(),
        decision_body(&record, decision.accept, decision.feedback),
        decision.by,
        &now_rfc3339(),
    );
    let comment_id = match appended {
        Ok(thread) => {
            crate::agent_conversations::retire_recoverable_for_human_comment(
                app,
                &record.thread_id,
            );
            comments::emit_discussion_changed(app, &thread);
            Some(comment_id)
        }
        Err(reason) => {
            log_refusal(app, "decision_comment", &reason, (&record.stream_id, &record.change));
            None
        }
    };
    announce(app, &record.stream_id, &record.change, Some(&record), NewChangeCause::Decided);
    Ok(NewChangeDecisionOutcome {
        proposal: record,
        committed,
        comment_id,
        origin_kind: origin_kind_of(&thread),
    })
}

/// NCP-FR-KRSA: make the commit an acceptance owes, and clear the mark.
pub fn complete_new_change<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    sites: &dyn SiteResolver,
    proposal_id: &str,
) -> Result<NewChangeDecisionOutcome, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let (held, _) = read_proposal(store, proposal_id)?;
    let _guard = split_lock(&held.stream_id, &held.change);
    let (mut record, dir) = read_proposal(store, proposal_id)?;
    let thread = locate_thread(store, &record.stream_id, &record.change, &record.thread_id)?;
    let mut committed = false;
    if record.commit_owed {
        let site = sites.site(&record.stream_id, &record.change)?;
        let name = record.created_change.clone().unwrap_or_else(|| record.name.clone());
        committed = commit_created(&site, &record, &name).inspect_err(|reason| {
            log_refusal(app, "commit", reason, (&record.stream_id, &record.change));
        })?;
        record.commit_owed = false;
        write_record(store, &dir, &record)?;
        log_info(app, "committed", &record);
        announce(app, &record.stream_id, &record.change, Some(&record), NewChangeCause::Committed);
    }
    Ok(NewChangeDecisionOutcome {
        proposal: record,
        committed,
        comment_id: None,
        origin_kind: origin_kind_of(&thread),
    })
}

/// NCP-FR-MQCS / NCP-FR-MHBS: confirm the split of a change.
pub fn confirm_split<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
    by: &Participant,
) -> Result<ChangeSplit, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let _guard = split_lock(stream_id, change);
    let outcome = confirm_locked(app, store, stream_id, change, by);
    if let Err(code) = &outcome {
        log_refusal(app, "confirm", code, (stream_id, change));
    }
    outcome
}

fn confirm_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &fs::RootFs,
    stream_id: &str,
    change: &str,
    by: &Participant,
) -> Result<ChangeSplit, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    if read_split(store, stream_id, change).is_some() {
        return Err(ERR_SPLIT_CONFIRMED.to_string());
    }
    let mut proposals = list_proposals_impl(store, stream_id, change);
    if proposals.iter().any(|p| p.state == NewChangeProposalState::Pending) {
        return Err(ERR_PROPOSAL_PENDING.to_string());
    }
    proposals.reverse();
    let accepted: Vec<String> = proposals
        .iter()
        .filter(|p| p.state == NewChangeProposalState::Accepted)
        .filter_map(|p| p.created_change.clone())
        .collect();
    let Some(first) = proposals.first() else {
        return Err(ERR_NOTHING_ACCEPTED.to_string());
    };
    if accepted.is_empty() {
        return Err(ERR_NOTHING_ACCEPTED.to_string());
    }
    let split = ChangeSplit {
        set_id: first.set_id.clone(),
        accepted: accepted.clone(),
        confirmed_at: Some(now_rfc3339()),
    };
    let root = change_root(store, stream_id, change)?;
    store
        .write_toml_atomic(root.join("split.toml"), &split)
        .map_err(|_| ERR_WRITE_FAILED.to_string())?;
    // NCP-FR-MHBS: one human comment into the thread of the newest decided
    // proposal.
    if let Some(newest) = proposals
        .iter()
        .filter(|p| p.decided_at.is_some())
        .max_by(|a, b| a.decided_at.cmp(&b.decided_at))
    {
        if let Ok(thread) = locate_thread(store, stream_id, change, &newest.thread_id) {
            let body = format!(
                "Confirmed the split of OpenSpec change `{change}` into {}.",
                accepted.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
            );
            if let Ok(appended) = comments::add_comment_with_id(
                store,
                thread.log_ref(),
                &newest.thread_id,
                new_note_id(),
                body,
                by,
                &now_rfc3339(),
            ) {
                comments::emit_discussion_changed(app, &appended);
            }
        }
    }
    logging::log_info(
        app,
        &BUFFER,
        &[Domain::Backend],
        "change split confirmed",
        log_fields! {
            "streamId" => stream_id,
            "change" => change,
            "setId" => &split.set_id,
            "accepted" => accepted.len(),
        },
    );
    announce(app, stream_id, change, None, NewChangeCause::Confirmed);
    Ok(split)
}
