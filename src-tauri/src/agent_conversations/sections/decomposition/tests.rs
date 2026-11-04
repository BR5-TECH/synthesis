//! Tests for the decomposition in `discussion_history`
//! (`AGC-agent-conversations.md` AGC-FR-TIGM, AGC-FR-ZZWP).

use tempfile::TempDir;

use super::*;
use crate::comments::{Participant, Stamp, ThreadRef};
use crate::new_draft_proposals as nd;

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent { agent_id: "a".into(), handle: "arch".into(), model: None, title: None }
}

struct Draft {
    _dir: TempDir,
    root: crate::fs::RootFs,
    draft_id: String,
    thread_id: String,
}

impl Draft {
    fn new() -> Draft {
        let dir = TempDir::new().expect("tempdir");
        let root = crate::fs::RootFs::for_root(crate::changes::canonicalize_lenient(dir.path()));
        let draft_id = crate::drafts::create_draft_at_root(&root, Some("relay")).unwrap().draft.id;
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft { draft_id: draft_id.clone() },
            None,
            "@arch decompose it".into(),
            Vec::new(),
            Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .unwrap();
        Draft { _dir: dir, root, draft_id, thread_id: thread.id }
    }

    fn roots(&self) -> Roots<'_> {
        Roots { worktree: &self.root, store: &self.root }
    }

    fn propose(&self, name: &str, body: &str, position: i64) -> nd::NewDraftProposal {
        nd::record_new_draft_proposal(
            &crate::tools::tests::mock_app().handle().clone(),
            &self.root,
            &self.root,
            nd::NewDraftInput {
                draft_id: &self.draft_id,
                name,
                body,
                position,
                total: 3,
                rationale: "<spoken>Next part.</spoken>",
                agent: &agent(),
                target: ThreadRef::discussion(&self.draft_id),
                thread_id: &self.thread_id,
            },
        )
        .unwrap()
    }

    fn reject(&self, id: &str) {
        nd::decide_new_draft_in_process(
            &crate::tools::tests::mock_app().handle().clone(),
            &self.root,
            &self.root,
            id,
            nd::NewDraftDecision::Reject,
            None,
            &human(),
        )
        .unwrap();
    }

    fn thread(&self) -> comments::Discussion {
        comments::locate_discussion_of(
            &self.root,
            &DiscussionTarget::Draft { draft_id: self.draft_id.clone() },
            &self.thread_id,
        )
        .unwrap()
    }
}

// AGC-FR-TIGM: the history ends with the decomposition, its id, and whether it
// is confirmed, then every proposal oldest first, and the body of the pending
// proposal alone.
#[test]
fn the_history_ends_with_the_decomposition_and_the_pending_body() {
    let d = Draft::new();
    let first = d.propose("relay-auth", "FIRST-BODY", 1);
    d.reject(&first.id);
    let second = d.propose("relay-sync", "SECOND-BODY", 2);
    let body = discussion_history_section(d.roots(), &d.thread(), "").body;
    let named = format!("The decomposition {} of this draft, not confirmed,", first.set_id);
    let at = body.find(&named).expect("the decomposition, by its id");
    let tail = &body[at..];
    let one = tail.find(&first.id).unwrap();
    let two = tail.find(&second.id).unwrap();
    assert!(one < two, "oldest first");
    assert!(tail.contains("part 1 of 3 — “relay-auth” — rejected"));
    assert!(tail.contains("part 2 of 3 — “relay-sync” — pending"));
    assert!(tail.contains("SECOND-BODY"));
    assert!(!body.contains("FIRST-BODY"));
}

// AGC-FR-TIGM: a confirmed decomposition says so.
#[test]
fn a_confirmed_decomposition_says_it_is_confirmed() {
    let d = Draft::new();
    let first = d.propose("relay-auth", "FIRST-BODY", 1);
    let marker = crate::drafts::SupersededMarker {
        set_id: first.set_id.clone(),
        confirmed_at: "2026-01-01T00:00:00Z".into(),
        drafts: Vec::new(),
    };
    crate::drafts::mark_draft_superseded(d.roots().worktree, &d.draft_id, marker).expect("marked");
    let text = decomposition_text(d.roots(), &d.thread().target).expect("decomposition");
    assert!(text.contains(&format!("The decomposition {} of this draft, confirmed,", first.set_id)));
}

// AGC-FR-TIGM: a draft with no new-draft proposal, and a note, have none.
#[test]
fn a_target_with_no_new_draft_proposal_has_no_decomposition() {
    let d = Draft::new();
    assert!(decomposition_text(d.roots(), &d.thread().target).is_none());
    let note = DiscussionTarget::Note { note_id: "n".into() };
    assert!(decomposition_text(d.roots(), &note).is_none());
}

// AGC-FR-ZZWP: a reference names the proposal as it stands and never its body.
#[test]
fn a_reference_names_the_proposal_as_it_stands() {
    let d = Draft::new();
    let p = d.propose("relay-auth", "ZEBRA-BODY", 1);
    let reads = new_draft_reads(d.roots(), &d.thread());
    let line = new_draft_line(
        &comments::Attachment::NewDraftProposal {
            proposal_id: p.id.clone(),
            draft_id: d.draft_id.clone(),
            update: None,
        },
        &reads,
    );
    assert!(line.starts_with("[proposed a new draft: id "), "{line}");
    assert!(line.contains("part 1 of 3 — “relay-auth” — pending"));
    assert!(!line.contains("ZEBRA-BODY"));
    let changed = new_draft_line(
        &comments::Attachment::NewDraftProposal {
            proposal_id: p.id.clone(),
            draft_id: d.draft_id.clone(),
            update: Some(comments::NewDraftUpdateAction::Withdraw),
        },
        &reads,
    );
    assert!(changed.starts_with("[withdrew the proposed new draft"));
    let subject = new_draft_line(
        &comments::Attachment::NewDraftSubject {
            proposal_id: p.id.clone(),
            draft_id: d.draft_id.clone(),
        },
        &reads,
    );
    assert!(subject.starts_with("[about the proposed new draft: id "));
    let unknown = new_draft_line(
        &comments::Attachment::NewDraftSubject {
            proposal_id: "gone".into(),
            draft_id: d.draft_id.clone(),
        },
        &reads,
    );
    assert_eq!(unknown, "[about the proposed new draft gone]");
}
