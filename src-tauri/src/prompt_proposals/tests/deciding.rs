//! Tests for declining, deciding twice, and the decision comment (PCP-FR-15 … PCP-FR-19).

use super::*;


// ---------------------------------------------------------------------------
// Declining, deciding twice, and the decision comment (PCP-FR-15 … PCP-FR-19)
// ---------------------------------------------------------------------------

#[test]
fn a_decline_writes_nothing_into_the_project_and_keeps_the_candidate() {
    // PCP-FR-15, PCP-FR-16, PCP-FR-11.
    let f = Fixture::new();
    let proposal = f.pending();
    let outcome = f
        .decline_with(&proposal.id, Some("Too terse — keep the second sentence."))
        .expect("declined");

    assert_eq!(outcome.proposal.state, PromptProposalState::Rejected);
    assert_eq!(f.artifact_body(), ORIGINAL);
    assert_eq!(
        load_content_impl(&f.root, &proposal.id).unwrap().content,
        PROPOSED,
        "the candidate is still readable afterwards",
    );
    let decision = f.discussion().comments.last().cloned().expect("decision");
    assert_eq!(Some(decision.id), outcome.comment_id);
    assert!(decision.body.contains("Declined the proposed change"));
    assert!(decision.body.contains("Too terse"));
    assert_eq!(decision.author, human(), "stamped with the acting author");
    assert!(decision.attachments.is_empty(), "and carries no attachment");
}

#[test]
fn the_decision_comment_says_whose_candidate_was_decided() {
    // PCP-FR-16, PCP-FR-22.
    let f = Fixture::new();
    let proposal = f.pending();
    f.accept(&proposal.id).expect("accepted");
    let decision = f.discussion().comments.last().cloned().expect("decision");
    assert_eq!(
        decision.body,
        format!("Accepted the proposed change to `{PROMPT}`."),
    );

    // …and a candidate the author rewrote says so instead.
    let g = Fixture::new();
    let other = g.pending();
    let loaded = load_content_impl(&g.root, &other.id).unwrap();
    save_candidate_impl(&g.root, &other.id, "# Review\n\nMine.\n", &loaded.checksum)
        .expect("saved");
    g.accept(&other.id).expect("accepted");
    let decision = g.discussion().comments.last().cloned().expect("decision");
    assert!(
        decision.body.starts_with("Accepted an edited version"),
        "{}",
        decision.body,
    );

    // …and the offer and the answer stand in the order they were made.
    let thread = g.discussion();
    let bodies: Vec<&str> = thread.comments.iter().map(|c| c.body.as_str()).collect();
    assert!(bodies[bodies.len() - 2].contains("bury the important step"));
}

#[test]
fn a_locked_conversation_refuses_both_decisions_before_anything_is_written() {
    // PCP-FR-16.
    let f = Fixture::new();
    let proposal = f.pending();
    f.lock();

    assert_eq!(
        f.accept(&proposal.id).unwrap_err(),
        comments::ERR_DISCUSSION_LOCKED,
    );
    assert_eq!(f.artifact_body(), ORIGINAL);
    assert!(
        !f.dir().join(format!("{}.journal", proposal.id)).exists(),
        "no journal was written",
    );
    assert_eq!(
        read_proposal(&f.root, &proposal.id).unwrap().state,
        PromptProposalState::Pending,
    );
    assert_eq!(
        f.decline(&proposal.id).unwrap_err(),
        comments::ERR_DISCUSSION_LOCKED,
    );
    assert_eq!(
        read_proposal(&f.root, &proposal.id).unwrap().state,
        PromptProposalState::Pending,
    );
}

#[test]
fn a_decided_proposal_is_decided_once() {
    // PCP-FR-18.
    let f = Fixture::new();
    let accepted = f.pending();
    f.accept(&accepted.id).expect("accepted");
    let events = f.watch();
    let comments = f.discussion().comments.len();

    assert_eq!(f.accept(&accepted.id).unwrap_err(), ERR_ALREADY_DECIDED);
    assert_eq!(f.decline(&accepted.id).unwrap_err(), ERR_ALREADY_DECIDED);

    let rejected = f.propose("# Review\n\nAnother.\n").expect("recorded");
    f.decline(&rejected.id).expect("declined");
    assert_eq!(f.accept(&rejected.id).unwrap_err(), ERR_ALREADY_DECIDED);
    assert_eq!(f.decline(&rejected.id).unwrap_err(), ERR_ALREADY_DECIDED);

    assert_eq!(f.artifact_body(), PROPOSED, "unchanged by every refusal");
    assert_eq!(f.discussion().comments.len(), comments + 2, "the second proposal and its decline alone");
    // Two events for the second proposal's own life, and none for a refusal.
    assert_eq!(events.lock().unwrap().len(), 2);
}

#[test]
fn an_acceptance_refuses_a_target_that_has_gone_or_stopped_being_a_prompt() {
    // PCP-FR-19, PCP-FR-15, PCP-FR-08.
    let f = Fixture::new();
    let proposal = f.pending();
    std::fs::remove_file(f.root.path().join(PROMPT)).expect("delete");

    assert_eq!(f.accept(&proposal.id).unwrap_err(), ERR_ARTIFACT_NOT_FOUND);
    assert!(!f.root.path().join(PROMPT).exists(), "no file was created");
    assert_eq!(
        read_proposal(&f.root, &proposal.id).unwrap().state,
        PromptProposalState::Pending,
    );
    // Declining is how such a one is cleared.
    f.decline(&proposal.id).expect("declined");
    assert!(!f.root.path().join(PROMPT).exists());

    // …and a target that is no longer a prompt refuses on the same terms.
    let g = Fixture::new();
    let other = g.pending();
    assign(&g.root, &[(PROMPT, ArtifactType::Spec)]);
    assert_eq!(g.accept(&other.id).unwrap_err(), ERR_NOT_A_PROMPT_ARTIFACT);
    assert_eq!(g.artifact_body(), ORIGINAL);
    g.decline(&other.id).expect("declined");
}

#[test]
fn nothing_expires_and_a_decided_proposal_whose_artifact_is_gone_is_swept() {
    // PCP-FR-20.
    let f = Fixture::new();
    let standing = f.pending();
    let decided = {
        // A decided proposal against a second artifact, which then goes.
        let other = "prompts/other.md";
        write_file(&f.root, other, "# Other\n");
        assign(
            &f.root,
            &[(PROMPT, ArtifactType::Prompt), (other, ArtifactType::Prompt)],
        );
        let proposal = f
            .propose_against(other, "# Other\n\nBetter.\n")
            .expect("recorded");
        f.decline(&proposal.id).expect("declined");
        std::fs::remove_file(f.root.path().join(other)).expect("delete");
        proposal
    };

    on_content_root_mounted(&f.handle(), &f.root, &f.root);

    assert!(
        read_proposal(&f.root, &decided.id).is_err(),
        "a decided proposal about a file that is gone is swept",
    );
    let still = read_proposal(&f.root, &standing.id).expect("the pending one stands");
    assert_eq!(still.state, PromptProposalState::Pending);
    assert_eq!(still, standing, "and reads exactly as it did");
}
