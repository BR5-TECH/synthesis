//! A `proposal_hunk` attachment naming a hunk of a prompt change proposal, in
//! the input a turn carries (AGC-FR-DKHT, CMS-FR-AHDQ).
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

const PROMPT_PATH: &str = "prompts/review.md";
const PROMPT_BODY: &str = "# Review\n\nRead the file, summarise it and file a note.\n";

/// A prompt artifact with a discussion on it, typed by a file-scope assignment.
fn seed_prompt(h: &Harness) -> Discussion {
    let root = h.root();
    std::fs::create_dir_all(root.path().join("prompts")).expect("dirs");
    std::fs::write(root.path().join(PROMPT_PATH), PROMPT_BODY).expect("prompt");
    std::fs::create_dir_all(root.path().join(".synthesis")).expect("dirs");
    std::fs::write(
        root.path().join(".synthesis/library.toml"),
        format!("[assignments.\"{PROMPT_PATH}\"]\ntype = \"prompt\"\nscope = \"file\"\n"),
    )
    .expect("assignment");
    crate::comments::open_discussion_in(
        &root,
        &root,
        &crate::comments::DiscussionTarget::Artifact { artifact_id: PROMPT_PATH.into() },
        None,
        "@arch what would you change?".into(),
        Vec::new(),
        crate::comments::Stamp { by: &human("ada"), at: "2099-01-01T00:00:00Z" },
    )
    .expect("discussion")
}

/// One replacement recorded against the prompt and left standing.
fn record_one(h: &Harness, thread_id: &str) -> crate::prompt_proposals::PromptChangeProposal {
    let hunks = [crate::text_hunks::hunks::ProposedHunk::from_fields(
        Some("file a note".into()),
        Some("file two notes".into()),
        None,
        None,
        None,
        None,
    )];
    crate::prompt_proposals::record_prompt_proposal(
        &h.app.handle().clone(),
        &h.root(),
        &h.root(),
        crate::prompt_proposals::NewPromptProposal {
            artifact_id: PROMPT_PATH,
            change: crate::prompt_proposals::ProposedChange::Hunks(&hunks),
            rationale: "Two notes are clearer.",
            agent: &crate::comments::Participant::Agent {
                agent_id: "a1".into(),
                handle: "arch".into(),
                model: None,
                title: None,
            },
            thread_id,
        },
    )
    .expect("recorded")
}

/// CMS-FR-AHDQ: a hunk discussion of the artifact — a new discussion record
/// whose opening comment carries one `proposal_hunk` naming the artifact.
fn open_hunk_discussion(h: &Harness, proposal_id: &str, hunk_id: &str) -> Discussion {
    crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &crate::comments::DiscussionTarget::Artifact { artifact_id: PROMPT_PATH.into() },
        None,
        "@arch why two?".into(),
        vec![crate::comments::AttachmentInput::ProposalHunk {
            proposal_id: proposal_id.into(),
            hunk_id: hunk_id.into(),
            draft_id: None,
            artifact_id: Some(PROMPT_PATH.into()),
            path: PROMPT_PATH.into(),
        }],
        crate::comments::Stamp { by: &human("ada"), at: "2099-01-02T00:00:00Z" },
    )
    .expect("hunk discussion")
}

// AGC-FR-DKHT / CMS-FR-AHDQ: a hunk discussion's opening comment names its
// hunk in the turn's input — id, kind, state, and bounded excerpts — read from
// the prompt proposal store, exactly as a draft's does.
#[test]
fn an_artifact_proposal_hunk_names_its_change_on_the_current_comment() {
    let h = Harness::new(vec![]);
    let thread = seed_prompt(&h);
    let proposal = record_one(&h, &thread.id);
    let hunk_id = proposal.ledger[0].id.clone();
    let discussion = open_hunk_discussion(&h, &proposal.id, &hunk_id);
    let trigger = discussion.comments[0].id.clone();

    let sections =
        build_input(Roots::same(&h.root()), &ConversationOrigin::of(&discussion), &trigger);
    let expected = format!(
        "[about proposed change {hunk_id} to {PROMPT_PATH}: replace — undecided — changes: \"file a note\" — offers: \"file two notes\"]"
    );
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    assert!(current.body.contains(&expected), "{}", current.body);
}

// AGC-FR-DKHT: the state named is the hunk's state as it stands now.
#[test]
fn an_artifact_proposal_hunk_names_the_state_its_hunk_holds_now() {
    let h = Harness::new(vec![]);
    let thread = seed_prompt(&h);
    let proposal = record_one(&h, &thread.id);
    let hunk_id = proposal.ledger[0].id.clone();
    crate::prompt_proposals::hold_hunk_impl(&h.root(), &proposal.id, &hunk_id, true).expect("held");
    let discussion = open_hunk_discussion(&h, &proposal.id, &hunk_id);
    let trigger = discussion.comments[0].id.clone();
    let sections =
        build_input(Roots::same(&h.root()), &ConversationOrigin::of(&discussion), &trigger);
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    assert!(
        current.body.contains(&format!("change {hunk_id} to {PROMPT_PATH}: replace — undecided")),
        "{}",
        current.body
    );
}

// AGC-FR-DKHT: a reference that cannot be read degrades to the bare line that
// names it, so the agent can still pass its id as `revises`.
#[test]
fn an_artifact_proposal_hunk_that_cannot_be_read_keeps_a_bare_line() {
    let h = Harness::new(vec![]);
    seed_prompt(&h);
    let discussion = open_hunk_discussion(&h, "p-nonesuch", "h-nonesuch");
    let trigger = discussion.comments[0].id.clone();
    let sections =
        build_input(Roots::same(&h.root()), &ConversationOrigin::of(&discussion), &trigger);
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    assert!(
        current.body.contains(&format!("[about proposed change h-nonesuch to {PROMPT_PATH}]")),
        "{}",
        current.body
    );
}

// AGC-FR-09 / CMS-FR-66: the proposal's own `prompt_proposal` reference still
// renders in the history of the conversation it was posted into.
#[test]
fn a_prompt_proposal_reference_still_renders_in_the_history() {
    let h = Harness::new(vec![]);
    let thread = seed_prompt(&h);
    record_one(&h, &thread.id);
    let thread = crate::comments::add_comment_to(
        &h.root(),
        crate::comments::ThreadRef::artifact_discussion(PROMPT_PATH),
        &thread.id,
        "@arch why?".into(),
        Vec::new(),
        Vec::new(),
        crate::comments::Stamp { by: &human("ada"), at: "2099-01-03T00:00:00Z" },
    )
    .expect("reply");
    let trigger = thread.comments.last().expect("a comment").id.clone();
    let sections = build_input(Roots::same(&h.root()), &ConversationOrigin::of(&thread), &trigger);
    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);
    assert!(
        history.body.contains(&format!("[proposed a new version of {PROMPT_PATH}]")),
        "{}",
        history.body
    );
}
