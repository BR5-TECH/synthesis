//! The plan tools and the plan card in discussions that are not Map threads
//! (`CVL-conversation-loop.md` CVL-FR-RWQB, CVL-FR-JMEP, CVL-FR-KYBE;
//! `MPR-map-plan-proposals.md` MPR-FR-VYPE, MPR-FR-KDTQ, MPR-FR-CPGZ;
//! `AGC-agent-conversations.md` AGC-FR-ZPOI, AGC-FR-UOPJ).
//!
//! One part of `../tests/mod.rs`, which holds the harness.

use super::plan_chaining::plan_project;
use super::*;
use crate::comments::{Attachment, DiscussionTarget};

const PLAN_TOOL: &str = crate::tools::propose_plan_changes::NAME;
const FILE: &str = "spec.md";

/// The whole-file discussion of `spec.md`, which addresses `arch`.
fn file_discussion(h: &Harness) -> Discussion {
    h.root().write_text_atomic(h.root().path().join(FILE), "# Spec\n\nPayments.\n").unwrap();
    crate::comments::open_discussion_unchecked(
        &h.root(),
        &DiscussionTarget::Artifact { artifact_id: FILE.into() },
        None,
        "@arch file an issue for the payments work".into(),
        Vec::new(),
        &human("ada"),
        "2026-03-01T10:00:00Z",
    )
    .expect("a file discussion")
}

fn thread_now(h: &Harness, thread: &Discussion) -> Discussion {
    crate::comments::locate_discussion_of(&h.root(), &thread.target, &thread.id).expect("thread")
}

fn draft_row(key: &str) -> serde_json::Value {
    serde_json::json!({ "key": key, "op": "new_draft", "name": key, "body": "Build it." })
}

fn state_of(sections: &[InputSection]) -> String {
    sections
        .iter()
        .find(|s| s.tag == TAG_DISCUSSION_STATE)
        .map(|s| s.body.clone())
        .unwrap_or_default()
}

// CVL-FR-RWQB, CVL-FR-KYBE, AGC-FR-ZPOI, MPR-FR-KDTQ, PPT-FR-DDSX: a turn of a
// file discussion is offered the plan tools, posts a plan card into that
// discussion, updates it, and ends `awaiting_reply` with a closing comment.
#[test]
fn a_file_discussion_turn_posts_a_plan_card_in_its_own_discussion() {
    let h = plan_project(vec![
        Ok(ScriptedReply::calls(
            PLAN_TOOL,
            serde_json::json!({
                "rationale": said("One draft for the payments work."),
                "rows": [draft_row("a")],
            }),
        )),
        Ok(ScriptedReply::calls(
            PLAN_TOOL,
            serde_json::json!({ "updates": [{ "action": "add", "key": "b", "row": draft_row("b") }] }),
        )),
        Ok(ScriptedReply::answer(said("I added a second draft."))),
    ]);
    let thread = file_discussion(&h);
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let turn = wait_for_terminal(&h, &turn.id);
    assert_eq!(turn.state, AgentTurnState::AwaitingReply, "{:?}", turn.failure);
    let (request, _) = h.seam.requests().into_iter().next().expect("a request");
    let offered: Vec<String> = request.tools.iter().map(|t| t.name.to_string()).collect();
    for name in [PLAN_TOOL, "list_plan_issues", "search_plan_issues", "read_plan_issue"] {
        assert!(offered.iter().any(|t| t == name), "{name} is offered: {offered:?}");
    }
    // MPR-FR-VYPE: the input of the turn holds no Map part.
    let tags: Vec<&str> = request.input.iter().map(|s| s.tag.as_str()).collect();
    assert!(!tags.contains(&TAG_MAP_CONTEXT) && !tags.contains(&"plan_references"), "{tags:?}");
    let proposals = crate::map_plan::proposals_in(&h.root(), &thread.id);
    assert_eq!(proposals.len(), 1, "one card");
    assert_eq!(proposals[0].rows.len(), 2);
    let owner = proposals[0].owner.clone().expect("an owner");
    assert_eq!(owner.target, thread.target);
    let comments = thread_now(&h, &thread).comments;
    assert_eq!(comments.len(), 3, "the opening, the card, and the closing comment");
    for comment in &comments[1..] {
        assert!(comment
            .attachments
            .iter()
            .any(|a| matches!(a, Attachment::MapPlanProposal { proposal_id, .. } if *proposal_id == proposals[0].id)));
    }
    assert!(crate::comments::fold_map_discussions(&h.root()).is_empty(), "no Map thread");
}

// MPR-FR-VYPE, MPR-FR-CPGZ, AGC-FR-UOPJ: the state of a turn of a file
// discussion says nothing about proposals while it holds none, and holds each
// row with its key and state once it holds one, with no focus and no plan
// summary. An accepted draft row says that its draft was created.
#[test]
fn the_state_of_a_file_discussion_holds_the_rows_of_its_proposals() {
    let h = plan_project(Vec::new());
    // MPR-FR-LDJH: the accept below asks for a follow-up turn, and this test
    // runs no model.
    h.app.manage(crate::map_plan::DispatchSeam(Arc::new(|_| Ok(()))));
    let thread = file_discussion(&h);
    let roots = Roots { worktree: &h.root(), store: &h.root() };
    let origin = ConversationOrigin::of(&thread);
    let trigger = thread.comments[0].id.clone();
    let before = state_of(&build_input(roots, &origin, &trigger));
    assert!(!before.contains("Plan proposals"), "{before}");

    let row = crate::map_plan::MapPlanRow {
        key: "a".into(),
        fields: crate::map_plan::RowFields::NewDraft(crate::map_plan::NewDraftFields {
            name: "Payments".into(),
            body: "Build it.".into(),
        }),
        state: crate::map_plan::RowState::Waiting,
        blocked_by: None,
        error: None,
        result: None,
    };
    let comment = crate::map_plan::ProposalComment {
        body: said("One draft."),
        author: Participant::Agent {
            agent_id: "a1".into(),
            handle: "arch".into(),
            model: None,
            title: None,
        },
        spoken_original: None,
    };
    let proposal = crate::map_plan::record_map_plan_proposal(
        &h.app.handle().clone(),
        &origin,
        vec![row],
        comment,
    )
    .expect("a proposal");
    let state = state_of(&build_input(roots, &origin, &trigger));
    assert!(state.contains("Plan proposals of this discussion:"), "{state}");
    assert!(state.contains("- proposal 1 row \"a\": new change \"Payments\": waiting"), "{state}");
    assert!(!state.contains("Focus:"), "no Map focus: {state}");

    crate::map_plan::accept_row(&h.app.handle().clone(), &thread.id, &proposal.id, "a")
        .expect("an accept");
    let state = state_of(&build_input(roots, &origin, &trigger));
    assert!(state.contains("\"Payments\": accepted (change created)"), "{state}");
}

// CVL-FR-JMEP: the prompts of the file, draft, note, and change discussions and
// of a passage comment describe the plan tools: present only while the source
// allows them, the reads change nothing, and a proposal is reviewed in the tab.
#[test]
fn the_prompts_of_every_discussion_describe_the_plan_tools() {
    for kind in [
        OriginKind::ArtifactDiscussion,
        OriginKind::DraftDiscussion,
        OriginKind::NoteDiscussion,
        OriginKind::ChangeDiscussion,
        OriginKind::ArtifactComment,
        OriginKind::DraftComment,
        OriginKind::ChangeComment,
    ] {
        let template = prompt_template(kind);
        for needle in [
            "## Plan tools",
            "`list_plan_issues`, `search_plan_issues`, and `read_plan_issue` read the issues of the delivery plan. They change nothing.",
            "present only when the delivery source of the project is a GitHub Project with a valid configuration",
            "The author reviews each row alone, in this tab.",
            "search the plan, so that you do not propose an issue that the plan holds already",
            "say that you could not read or change the plan",
            "Nothing reaches GitHub, and no change is created, before the author accepts the row that does it.",
        ] {
            assert!(template.contains(needle), "{kind:?} lacks {needle:?}");
        }
    }
}

// MPR-FR-VYPE: the estimate of a file discussion counts the rows of its
// proposals as a turn carries them, so it grows once the discussion holds one.
#[test]
fn the_estimate_of_a_file_discussion_counts_the_rows_of_its_proposals() {
    let h = plan_project(Vec::new());
    let thread = file_discussion(&h);
    let handle = h.app.handle().clone();
    let estimate = || {
        estimate_impl(
            &handle,
            Roots::same(&h.root()),
            PROJECT_KEY,
            thread.target.clone(),
            Some(&thread.id),
            None,
        )
    };
    let before = estimate();
    let origin = ConversationOrigin::of(&thread);
    let comment = crate::map_plan::ProposalComment {
        body: said("One draft."),
        author: Participant::Agent {
            agent_id: "a1".into(),
            handle: "arch".into(),
            model: None,
            title: None,
        },
        spoken_original: None,
    };
    let row = crate::map_plan::MapPlanRow {
        key: "a".into(),
        fields: crate::map_plan::RowFields::NewDraft(crate::map_plan::NewDraftFields {
            name: "Payments for every region of the plan".into(),
            body: "Build it.".into(),
        }),
        state: crate::map_plan::RowState::Waiting,
        blocked_by: None,
        error: None,
        result: None,
    };
    crate::map_plan::record_map_plan_proposal(&handle, &origin, vec![row], comment).unwrap();
    let after = estimate();
    assert!(after.estimated_tokens > before.estimated_tokens, "{before:?} {after:?}");
}

// MPR-FR-KDTQ, AGC-FR-05: a re-prepare or follow-up turn names its discussion
// by the id and the owner target alone; the dispatch reads the fragment target
// back from the store, so a passage thread keeps its comment kind.
#[test]
fn a_turn_named_by_owner_and_id_gets_the_fragment_of_its_passage_thread() {
    let h = plan_project(Vec::new());
    let position = crate::comments::FragmentTarget::in_artifact(FILE, 0, 4, "Spec");
    let passage = crate::comments::open_artifact_fragment_in(
        &h.root(),
        FILE,
        position,
        "@arch file an issue".into(),
        Vec::new(),
        &human("ada"),
        "2026-03-01T10:00:00Z",
    )
    .unwrap();
    let named = ConversationOrigin {
        discussion_id: passage.id.clone(),
        target: passage.target.clone(),
        fragment_target: None,
    };
    let origin = normalise_origin(Roots::same(&h.root()), named);
    assert_eq!(origin.kind(), OriginKind::ArtifactComment);
    assert_eq!(origin.fragment_target, passage.fragment_target);
}
