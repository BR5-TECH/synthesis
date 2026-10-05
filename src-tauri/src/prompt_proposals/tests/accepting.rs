//! Tests for the acceptance of a prompt change proposal (PCP-FR-12 … PCP-FR-14).

use super::*;


// ---------------------------------------------------------------------------
// Accepting (PCP-FR-12 … PCP-FR-14)
// ---------------------------------------------------------------------------

#[test]
fn an_acceptance_writes_through_the_projects_own_save_path() {
    // PST-FR-09, PST-FR-16, PST-FR-17 / PCP-FR-12, PCP-FR-17.
    let f = Fixture::new();
    let events = f.watch();
    let proposal = f.pending();

    let outcome = f.accept(&proposal.id).expect("accepted");
    assert_eq!(f.artifact_body(), PROPOSED);
    assert_eq!(outcome.proposal.state, PromptProposalState::Accepted);
    assert!(outcome.proposal.decided_at.is_some());
    assert!(!outcome.proposal.comment_owed);
    assert_eq!(outcome.origin_kind, ORIGIN_ARTIFACT_DISCUSSION);
    // PST-FR-17 / PST-FR-31: the acceptance leaves no record of the save. What
    // moves the artifact to the head of Recently edited is the modification time
    // this write gave its file, exactly as the author's own typing does.
    // PST-FR-16: the acceptance's own write is suppressed as an external change,
    // the checksum having been recorded before the bytes became visible.
    assert_eq!(
        f.tracker.current(PROMPT).as_deref(),
        Some(fs::sha256_bytes(PROPOSED.as_bytes()).as_str()),
    );

    let seen = events.lock().unwrap();
    assert_eq!(seen.len(), 2, "one for the recording, one for the decision");
    assert_eq!(seen[1]["proposal"]["state"], "accepted");
    assert_eq!(seen[1]["artifactId"], PROMPT);
}

#[test]
fn an_acceptance_replaces_the_file_as_it_stands_without_comparing_a_baseline() {
    // PCP-FR-13.
    let f = Fixture::new();
    let proposal = f.pending();
    let author_edit = "# Review\n\nOne.\n\nTwo.\n\nThree.\n\nFour.\n\nFive.\n";
    write_file(&f.root, PROMPT, author_edit);

    f.accept(&proposal.id).expect("accepted");
    assert_eq!(
        f.artifact_body(),
        PROPOSED,
        "the candidate replaces whatever the file held",
    );
}

#[test]
fn an_acceptance_that_cannot_write_leaves_the_file_and_the_proposal_alone() {
    // PCP-FR-28 / PCP-FR-14, PCP-FR-17: the artifact write is scripted to fail
    // by removing the file's parent directory's write permission.
    let f = Fixture::new();
    let events = f.watch();
    let proposal = f.pending();
    let before = f.discussion().comments.len();

    let dir = f.root.path().join("prompts");
    let mut perms = std::fs::metadata(&dir).unwrap().permissions();
    let restore = perms.clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o500);
        std::fs::set_permissions(&dir, perms).unwrap();
    }
    #[cfg(not(unix))]
    {
        perms.set_readonly(true);
        std::fs::set_permissions(&dir, perms).unwrap();
    }

    let refused = f.accept(&proposal.id).expect_err("the write cannot land");
    std::fs::set_permissions(&dir, restore).unwrap();

    assert_eq!(refused, ERR_WRITE_FAILED);
    assert_eq!(f.artifact_body(), ORIGINAL, "never written rather than restored");
    // PCR-FR-11: the session's baseline is untouched, so the watcher's next pass
    // reads the file's own unchanged bytes as its own rather than raising a
    // divergence over an edit the author never lost (PST-FR-16).
    assert_eq!(
        f.tracker.current(PROMPT),
        None,
        "a failed acceptance leaves no baseline naming bytes no file holds",
    );
    let record = read_proposal(&f.root, &proposal.id).expect("record");
    assert_eq!(record.state, PromptProposalState::Pending);
    assert!(record.decided_at.is_none());
    assert!(!record.comment_owed);
    assert_eq!(f.discussion().comments.len(), before, "no comment appended");
    assert_eq!(
        f.folder_files(),
        vec![
            format!("{}.content", proposal.id),
            format!("{}.toml", proposal.id),
        ],
        "neither the journal nor the prior remains",
    );
    assert_eq!(
        events.lock().unwrap().len(),
        1,
        "only the recording announced anything",
    );
}

#[test]
fn a_journal_left_by_an_interrupted_acceptance_is_rolled_forward_or_back() {
    // PCP-FR-14.
    // The commit point passed: the artifact holds the candidate.
    let f = Fixture::new();
    let proposal = f.pending();
    let journal = stage_for_test(&f, &proposal, /* committed */ true);
    assert!(f.dir().join(format!("{}.journal", proposal.id)).is_file());

    let listed = f.list();
    assert_eq!(listed[0].state, PromptProposalState::Accepted);
    assert!(!listed[0].comment_owed, "the owed comment was appended");
    let thread = f.discussion();
    let decision = thread.comments.last().expect("the decision");
    assert_eq!(decision.id, journal.comment_id);
    assert!(decision.body.starts_with("Accepted the proposed change"));
    assert!(
        !f.dir().join(format!("{}.journal", proposal.id)).exists()
            && !f.dir().join(format!("{}.prior", proposal.id)).exists(),
        "the journal and the prior are gone, the journal last",
    );

    // A second reconciliation changes nothing and duplicates no comment.
    let count = f.discussion().comments.len();
    f.list();
    assert_eq!(f.discussion().comments.len(), count);

    // The commit point was not passed: the artifact holds the prior bytes.
    let g = Fixture::new();
    let other = g.pending();
    stage_for_test(&g, &other, /* committed */ false);
    let listed = g.list();
    assert_eq!(listed[0].state, PromptProposalState::Pending);
    assert_eq!(g.artifact_body(), ORIGINAL);
    assert!(
        !g.dir().join(format!("{}.journal", other.id)).exists(),
        "the journal is cleared",
    );
}

#[test]
fn a_file_matching_neither_checksum_is_left_exactly_as_it_stands() {
    // PCP-FR-13 / PCP-FR-14, PCP-FR-28.
    let f = Fixture::new();
    let proposal = f.pending();
    stage_for_test(&f, &proposal, /* committed */ true);
    let third_party = "# Review\n\nSomeone else wrote this.\n";
    write_file(&f.root, PROMPT, third_party);

    let listed = f.list();
    assert_eq!(f.artifact_body(), third_party, "left exactly as it stands");
    assert_eq!(
        listed[0].state,
        PromptProposalState::Pending,
        "settled the safe way, so it is offered again",
    );

    // …and accepting it afterwards replaces that text whole, on the ordinary
    // terms.
    f.accept(&proposal.id).expect("accepted");
    assert_eq!(f.artifact_body(), PROPOSED);
}

#[test]
fn completing_an_owed_decision_appends_exactly_once() {
    // PCP-FR-25 / PCP-FR-14.
    let f = Fixture::new();
    let proposal = f.pending();
    let journal = stage_for_test(&f, &proposal, /* committed */ true);

    let reconciled = reconcile(&f.handle(), &f.root, &f.root, &proposal.id).expect("reconciled");
    assert_eq!(
        reconciled,
        Reconciliation::Settled {
            proposal_id: proposal.id.clone(),
            comment_id: Some(journal.comment_id.clone()),
        },
    );
    let comments = f.discussion().comments.len();

    // A second run is a no-op that duplicates nothing.
    assert_eq!(
        reconcile(&f.handle(), &f.root, &f.root, &proposal.id).expect("reconciled"),
        Reconciliation::Nothing,
    );
    assert_eq!(f.discussion().comments.len(), comments);
}

/// Make the committed comment log unwritable, so the decision comment's append
/// fails while every read before it still succeeds.
///
/// This is how a test reaches the one thing that can fail **after** the commit
/// point (PCP-FR-14): the pre-checks read the thread and pass, the artifact
/// write lands, and only the append is refused.
fn discussion_log(f: &Fixture) -> std::path::PathBuf {
    let dir = f.root.path().join("comments");
    std::fs::read_dir(&dir)
        .expect("comments dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".discussion.jsonl"))
        })
        .expect("the discussion's log")
}

fn seal_comment_log(f: &Fixture) -> std::fs::Permissions {
    let path = discussion_log(f);
    let restore = std::fs::metadata(&path).expect("log").permissions();
    let mut sealed = restore.clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        sealed.set_mode(0o444);
    }
    #[cfg(not(unix))]
    sealed.set_readonly(true);
    std::fs::set_permissions(&path, sealed).expect("seal");
    restore
}

fn unseal_comment_log(f: &Fixture, restore: std::fs::Permissions) {
    std::fs::set_permissions(discussion_log(f), restore).expect("unseal");
}

#[test]
fn an_acceptance_whose_comment_cannot_be_appended_is_landed_and_incomplete() {
    // PCP-FR-14, PCP-FR-16.
    let f = Fixture::new();
    let events = f.watch();
    let proposal = f.pending();
    let restore = seal_comment_log(&f);

    let refused = f.accept(&proposal.id).expect_err("the comment cannot land");
    assert_eq!(
        refused, ERR_ACCEPTANCE_INCOMPLETE,
        "and never a `write_failed`, the commit point having been passed",
    );

    // The record and the artifact are both settled; only the conversation is
    // owed a line.
    let record = read_proposal(&f.root, &proposal.id).expect("record");
    assert_eq!(record.state, PromptProposalState::Accepted);
    assert!(record.decided_at.is_some());
    assert!(record.comment_owed, "the comment is owed");
    assert_eq!(f.artifact_body(), PROPOSED, "the artifact holds the candidate");
    assert_eq!(
        events.lock().unwrap().len(),
        2,
        "the event was emitted at the commit point",
    );
    // …and the journal and the prior are still on disk, the journal carrying the
    // minted identity the recovery needs.
    let files = f.folder_files();
    assert!(files.contains(&format!("{}.journal", proposal.id)), "{files:?}");
    assert!(files.contains(&format!("{}.prior", proposal.id)), "{files:?}");

    // A second attempt while the append still fails finds the same debt and the
    // same means of paying it.
    assert_eq!(
        f.complete(&proposal.id).expect("no-op").comment_id,
        None,
        "nothing was appended, so no turn has anything to answer",
    );
    assert!(read_proposal(&f.root, &proposal.id).unwrap().comment_owed);
    assert!(f
        .folder_files()
        .contains(&format!("{}.journal", proposal.id)));

    // Allowed to succeed, the reconciliation appends exactly once, clears the
    // flag, and removes the journal and the prior.
    unseal_comment_log(&f, restore);
    let before = f.discussion().comments.len();
    let outcome = f.complete(&proposal.id).expect("completed");
    assert_eq!(outcome.proposal.state, PromptProposalState::Accepted);
    assert!(!outcome.proposal.comment_owed);
    assert!(outcome.comment_id.is_some(), "the caller has a comment to dispatch against");
    assert_eq!(outcome.origin_kind, ORIGIN_ARTIFACT_DISCUSSION);
    assert_eq!(f.discussion().comments.len(), before + 1);
    assert_eq!(
        f.folder_files(),
        vec![
            format!("{}.content", proposal.id),
            format!("{}.toml", proposal.id),
        ],
    );
    assert_eq!(f.artifact_body(), PROPOSED, "and nothing was written twice");

    // A second call is a no-op returning the proposal as it stands.
    let again = f.complete(&proposal.id).expect("no-op");
    assert_eq!(again.proposal.state, PromptProposalState::Accepted);
    assert_eq!(again.comment_id, None);
    assert_eq!(f.discussion().comments.len(), before + 1, "no duplicate");
}

#[test]
fn neither_decision_answers_from_an_acceptance_the_reconciliation_settled() {
    // PCP-FR-18: a decision against a proposal a reconciliation has just settled
    // to `accepted` is `already_decided` — of **either** kind.
    //
    // The defect this pins: a decline that answered with the reconciled
    // acceptance's own outcome would report success carrying the *accept*
    // comment, so the author would believe they had declined while the agent was
    // told the change was accepted and the artifact kept it.
    for decline in [true, false] {
        let f = Fixture::new();
        let proposal = f.pending();
        stage_for_test(&f, &proposal, /* committed */ true);

        let refused = if decline {
            f.decline(&proposal.id).expect_err("already decided")
        } else {
            f.accept(&proposal.id).expect_err("already decided")
        };
        assert_eq!(refused, ERR_ALREADY_DECIDED, "decline={decline}");
        assert_eq!(
            read_proposal(&f.root, &proposal.id).unwrap().state,
            PromptProposalState::Accepted,
            "and the first decision stands",
        );
        // The reconciliation still ran, so the owed comment was paid — but it is
        // the acceptance's own, and the refusal says so rather than reporting it
        // as this call's outcome.
        let decision = f.discussion().comments.last().cloned().expect("decision");
        assert!(decision.body.starts_with("Accepted the proposed change"));
    }
}

#[test]
fn a_rolled_back_transaction_completes_to_a_pending_proposal() {
    // PCP-FR-14, PCP-FR-25: `complete_prompt_change_decision` decides nothing itself.
    let f = Fixture::new();
    let proposal = f.pending();
    stage_for_test(&f, &proposal, /* committed */ false);

    let outcome = f.complete(&proposal.id).expect("completed");
    assert_eq!(outcome.proposal.state, PromptProposalState::Pending);
    assert_eq!(outcome.comment_id, None, "nothing was decided");
    assert_eq!(f.artifact_body(), ORIGINAL);
    assert_eq!(f.discussion().comments.len(), 2, "the proposal's own comment alone");

    // …and against a proposal that owes no comment it is a no-op returning that
    // proposal as it stands.
    let untouched = f.complete(&proposal.id).expect("no-op");
    assert_eq!(untouched.proposal.state, PromptProposalState::Pending);
}
