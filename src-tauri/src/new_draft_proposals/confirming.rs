//! Confirming a decomposition (DCP-FR-VLLP, DCP-FR-FAQQ, DCP-FR-ORTA,
//! DCP-FR-QINH).

use crate::comments::{self, DiscussionTarget, Participant};
use crate::draft_proposals::{draft_lock, thread_ref};
use crate::notes::{new_note_id, now_rfc3339};

use super::*;

/// DCP-FR-VLLP: archive the original draft and write its `superseded`
/// marker. The author reaches this through the command, or through
/// `decide_draft_changes` on an instruction of the author (DCP-FR-IKTU).
///
/// `instruction_line` is the line of DCP-FR-HMUG for a confirmation by tool,
/// and `None` for the command.
pub fn confirm_decomposition_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    draft_id: &str,
    by: &Participant,
    instruction_line: Option<&str>,
) -> Result<drafts::DraftRecord, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let guard = draft_lock(draft_id);
    let outcome = confirm_locked(app, root, store, draft_id, by, instruction_line);
    drop(guard);
    match outcome {
        Ok(record) => {
            announce(app, draft_id, None, NewDraftChangeCause::Confirmed);
            Ok(record)
        }
        Err(reason) => {
            log_refusal(app, "confirm", draft_id, &reason);
            Err(reason)
        }
    }
}

fn confirm_locked<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &fs::RootFs,
    store: &fs::RootFs,
    draft_id: &str,
    by: &Participant,
    instruction_line: Option<&str>,
) -> Result<drafts::DraftRecord, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    require_open_original(app, root, draft_id)?;
    // Most recently created first.
    let all = list_impl(root, draft_id)?;
    if all.iter().any(|p| p.state == NewDraftState::Pending) {
        return Err(ERR_PROPOSAL_PENDING.into());
    }
    // DCP-FR-FAQQ: oldest first, each with the current name of its draft, or
    // the proposed name where that draft no longer exists.
    let accepted: Vec<&NewDraftProposal> =
        all.iter().rev().filter(|p| p.state == NewDraftState::Accepted).collect();
    let Some(first) = accepted.first() else {
        return Err(ERR_NOTHING_ACCEPTED.into());
    };
    let names: Vec<drafts::SupersedingDraft> = accepted
        .iter()
        .filter_map(|p| {
            let id = p.created_draft_id.clone()?;
            let name =
                drafts::read_draft_record(root, &id).map(|r| r.name).unwrap_or(p.name.clone());
            Some(drafts::SupersedingDraft { id, name })
        })
        .collect();
    let marker = drafts::SupersededMarker {
        set_id: first.set_id.clone(),
        confirmed_at: now_rfc3339(),
        drafts: names.clone(),
    };
    // The refusal stays typed: an untyped write error can carry a path.
    let record = drafts::mark_draft_superseded(root, draft_id, marker)
        .map_err(|e| typed_reason(&e).to_string())?;
    drafts::announce_drafts_changed(app, draft_id);

    // DCP-FR-QINH: one human comment into the thread of the newest decided
    // proposal. A failed append leaves the confirmation standing.
    if let Some(newest) = all.first() {
        let quoted: Vec<String> = names.iter().map(|d| format!("“{}”", d.name)).collect();
        let mut body =
            format!("Decomposition confirmed. This draft is superseded by {}.", quoted.join(", "));
        // DCP-FR-HMUG: a confirmation by tool ends with the instruction line.
        if let Some(line) = instruction_line {
            body.push_str("\n\n");
            body.push_str(line);
        }
        let target = DiscussionTarget::Draft { draft_id: draft_id.to_string() };
        let file_rel = comments::locate_discussion_of(store, &target, &newest.thread_id)
            .and_then(|d| d.fragment_target.map(|f| f.path));
        match comments::add_comment_with_id(
            store,
            thread_ref(draft_id, file_rel.as_ref()),
            &newest.thread_id,
            new_note_id(),
            body,
            by,
            &now_rfc3339(),
        ) {
            Ok(thread) => comments::emit_discussion_changed(app, &thread),
            Err(reason) => log_refusal(app, "confirm_comment", draft_id, &reason),
        }
    }
    Ok(record)
}
