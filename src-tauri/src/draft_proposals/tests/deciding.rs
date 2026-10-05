//! Tests for the decision on a proposal: accept, decline and the state it leaves.

use super::*;

// ---------------------------------------------------------------------------
// DHS-FR-09 — the bytes survive the round trip (DCP-FR-08, DCP-FR-12)
// ---------------------------------------------------------------------------

#[test]
fn the_proposed_text_is_stored_and_applied_byte_for_byte() {
    let f = Fixture::new();
    // Trailing whitespace, an unusual indentation, and no final newline — the
    // three things a well-meaning normaliser would quietly fix.
    let awkward = "# Spec   \n\n\t  indented oddly\n\nno trailing newline";
    let proposal = f.propose(FILE, awkward).expect("recorded");

    assert_eq!(
        load_content_impl(&f.root, &proposal.id).expect("content").content,
        awkward,
    );
    f.accept(&proposal.id).expect("accepted");
    assert_eq!(f.file_body(), awkward, "what was read is what landed");
}

// ---------------------------------------------------------------------------
// DCP-FR-09 — listing (DCP-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn proposals_list_most_recently_created_first_whatever_their_state() {
    let f = Fixture::new();
    let first = f.propose(FILE, "one").expect("first");
    f.decline_with(&first.id, None).expect("declined");
    let second = f.propose(FILE, "two").expect("second");

    let listed = list_proposals_impl(&f.root, &f.draft_id).expect("list");
    assert_eq!(listed.len(), 2, "decided proposals are listed too");
    // Both were created inside one millisecond, so the tie-break by id is what
    // actually orders them — which is the point: the order is total either way.
    let ids: Vec<&str> = listed.iter().map(|p| p.id.as_str()).collect();
    assert!(ids.contains(&first.id.as_str()) && ids.contains(&second.id.as_str()));
    assert_eq!(
        listed.iter().filter(|p| p.state == ProposalState::Pending).count(),
        1,
    );
    assert_eq!(
        list_proposals_impl(&f.root, "no-such-draft").unwrap_err(),
        ERR_DRAFT_NOT_FOUND,
    );
}

// ---------------------------------------------------------------------------
// DCP-FR-16, DRS-FR-12, DRS-FR-22, DRS-FR-28, DHS-FR-15, DHS-FR-22 — an acceptance writes through the draft's own save path (DCP-FR-11)
// ---------------------------------------------------------------------------

#[test]
fn accepting_writes_the_file_and_moves_the_record() {
    let f = Fixture::new();
    let before = drafts::list_drafts_impl(&f.root).drafts
        .into_iter()
        .find(|d| d.id == f.draft_id)
        .expect("draft")
        .updated_at;
    let proposal = f.pending();

    let decided = f.accept(&proposal.id).expect("accepted").proposal;
    assert_eq!(f.file_body(), PROPOSED);
    assert_eq!(decided.state, ProposalState::Accepted);
    assert!(decided.decided_at.is_some());

    let after = drafts::list_drafts_impl(&f.root).drafts
        .into_iter()
        .find(|d| d.id == f.draft_id)
        .expect("draft");
    assert!(
        after.updated_at >= before,
        "the write went through the draft's save path, so it restamped the draft \
         (DCP-FR-11)",
    );
    assert!(
        !after.inconsistent,
        "a proposal replaces the prompt and adds no file (DCP-FR-11)",
    );
}

// ---------------------------------------------------------------------------
// DCP-FR-12 — a change applies where it resolves, and nowhere else
// ---------------------------------------------------------------------------

// DCP-FR-12: an acceptance replaces the resolved range of that change alone and
// leaves every other byte of the prompt as it was — the author's own edits
// elsewhere included.
#[test]
fn accepting_applies_one_change_and_keeps_the_authors_other_edits() {
    let f = Fixture::new();
    let proposal = f
        .propose_hunks(
            FILE,
            &[hunks::ProposedHunk {
                kind: crate::draft_proposals::anchors::HunkKind::Replace,
                before: Some("original".into()),
                after: Some("revised".into()),
                after_text: None,
                note: None,
            }],
        )
        .expect("recorded");

    // The author kept typing elsewhere in the prompt while the proposal stood.
    let edited = format!("A NEW OPENING LINE\n{ORIGINAL}");
    drafts::save_draft_file_impl(&f.root, &f.draft_id, FILE, &edited).expect("edit");

    f.accept(&proposal.id).expect("accepted");
    assert_eq!(
        f.file_body(),
        edited.replace("original", "revised"),
        "the author's own line survives; only the named text changed (DCP-FR-12)",
    );
}

// DCP-FR-18 / DCP-FR-BMLX: an acceptance whose change no longer resolves is a
// typed refusal that writes nothing. Rejecting is how such a change is cleared.
#[test]
fn accepting_a_change_whose_text_the_author_rewrote_is_refused() {
    let f = Fixture::new();
    let proposal = f
        .propose_hunks(
            FILE,
            &[hunks::ProposedHunk {
                kind: crate::draft_proposals::anchors::HunkKind::Replace,
                before: Some("original".into()),
                after: Some("revised".into()),
                after_text: None,
                note: None,
            }],
        )
        .expect("recorded");

    let rewritten = "# Spec\n\nCompletely different prose now.\n";
    drafts::save_draft_file_impl(&f.root, &f.draft_id, FILE, rewritten).expect("edit");

    assert_eq!(
        f.accept(&proposal.id).expect_err("refused"),
        ERR_ANCHOR_LOST,
        "the text it changes is no longer in the prompt (DCP-FR-18)",
    );
    assert_eq!(f.file_body(), rewritten, "nothing was written");
    f.decline(&proposal.id).expect("declining clears it");
}

// ---------------------------------------------------------------------------
// DCP-FR-14 — declining writes nothing (DCP-FR-14, DCP-FR-15, DCP-FR-10)
// ---------------------------------------------------------------------------

#[test]
fn declining_leaves_the_file_alone_keeps_the_candidate_and_records_the_feedback() {
    let f = Fixture::new();
    let proposal = f.pending();

    let decided = f
        .decline_with(&proposal.id, Some("Too terse — keep the second paragraph."))
        .expect("declined")
        .proposal;

    assert_eq!(decided.state, ProposalState::Rejected);
    assert_eq!(f.file_body(), ORIGINAL, "no file of the draft changed");
    assert_eq!(
        load_content_impl(&f.root, &proposal.id).expect("content").content,
        PROPOSED,
        "a declined proposal is still readable (DCP-FR-14)",
    );

    let thread = f.discussion();
    let last = thread.comments.last().expect("decision comment");
    assert!(last.body.contains("Declined"), "body: {:?}", last.body);
    assert!(last.body.contains(FILE));
    assert!(
        last.body.contains("Too terse"),
        "the feedback rides with it (DCP-FR-15)",
    );
    assert!(last.attachments.is_empty(), "a decision carries none");
    match &last.author {
        Participant::Human { login, .. } => assert_eq!(login, "raver119"),
        other => panic!("the decision is the author's (DCP-FR-15), got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// DCP-FR-25 — accepting with no feedback (DCP-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn accepting_with_no_feedback_appends_the_decision_alone_in_order() {
    let f = Fixture::new();
    let proposal = f.pending();
    f.accept(&proposal.id).expect("accepted");

    let thread = f.discussion();
    assert_eq!(thread.comments.len(), 3, "seed, proposal, decision");
    let decision = &thread.comments[2];
    assert_eq!(
        decision.body,
        format!("Accepted the proposed change to `{FILE}`. 1 accepted, 0 rejected."),
        "no feedback means the sentence and its tally alone (DCP-FR-15)",
    );
    assert!(
        thread.comments[1].attachments.len() == 1,
        "the offer and the answer read in sequence (DCP-FR-15)",
    );
}

// ---------------------------------------------------------------------------
// DCP-FR-17 — decided once (DCP-FR-17)
// ---------------------------------------------------------------------------

#[test]
fn a_decided_proposal_refuses_a_second_decision_either_way() {
    let f = Fixture::new();
    let proposal = f.pending();
    f.accept(&proposal.id).expect("accepted");
    let comments_after = f.discussion().comments.len();

    assert_eq!(f.accept(&proposal.id).unwrap_err(), ERR_ALREADY_DECIDED);
    assert_eq!(
        f.decline_with(&proposal.id, None).unwrap_err(),
        ERR_ALREADY_DECIDED,
    );
    assert_eq!(
        f.discussion().comments.len(),
        comments_after,
        "a refused decision appends nothing",
    );

    // And the same holds for one that was declined.
    let second = f.propose(FILE, "another").expect("second");
    f.decline_with(&second.id, None).expect("declined");
    assert_eq!(f.accept(&second.id).unwrap_err(), ERR_ALREADY_DECIDED);
}

// ---------------------------------------------------------------------------
// DHS-FR-19, DHS-FR-20, DHS-FR-21 / DCP-FR-28 — reconciled before any state is reported (DCP-FR-28)
// ---------------------------------------------------------------------------

/// The state a process killed between the commit point and the decision
/// comment's append leaves: the acceptance landed whole, and the conversation is
/// owed one line (DHS-FR-17).
fn owing_a_comment(f: &Fixture, proposal: &DraftChangeProposal) {
    let hist = drafts::draft_history_dir(&f.root, &f.draft_id).expect("history");
    let prior = f.file_body();
    // Every write the transaction makes before its commit point, then the
    // journal at `committed`.
    let _ = crate::draft_history::stage_committed_for_test(
        &f.root,
        &f.draft_id,
        proposal,
        &prior,
        &human(),
    )
    .expect("staged");
    assert!(hist.join("journal.toml").is_file());
}

#[test]
fn a_proposal_whose_acceptance_never_committed_reads_pending_again() {
    // DHS-FR-20, DHS-FR-21 (DCP-FR-28, per DHS-FR-19).
    let f = Fixture::new();
    let proposal = f.pending();
    let before = f.file_body();
    let _ = crate::draft_history::stage_prepared_for_test(
        &f.root,
        &f.draft_id,
        &proposal,
        &before,
        &human(),
    )
    .expect("staged");

    let listed = list_proposals_impl(&f.root, &f.draft_id).expect("list");
    // The list itself does not reconcile — the *command* does (DCP-FR-28), and
    // that is what this asserts through.
    assert_eq!(listed.len(), 1);
    let after = f.list_through_command();

    assert_eq!(after[0].state, ProposalState::Pending);
    assert_eq!(f.file_body(), before, "the prompt holds its prior bytes");
    assert!(
        crate::draft_history::list_impl(&f.root, &f.draft_id)
            .expect("history")
            .entries
            .is_empty(),
        "the draft's history holds no entry for it",
    );
    assert_eq!(f.discussion().comments.len(), 2, "the conversation gained no line");
}

#[test]
fn an_acceptance_owing_its_comment_completes_on_the_authors_retry() {
    // DHS-FR-19, DHS-FR-21 (DCP-FR-28, per DHS-FR-20) and DCR-FR-16, DCR-FR-15, DHS-FR-17's retry: deciding again
    // completes what was owed rather than refusing as already decided, so the
    // surface has a comment to name as the fresh turn's trigger (DCR-FR-15).
    let f = Fixture::new();
    let proposal = f.pending();
    owing_a_comment(&f, &proposal);
    let before = f.discussion().comments.len();

    let decided = f.accept(&proposal.id).expect("the retry completes it");

    assert_eq!(decided.proposal.state, ProposalState::Accepted);
    let comment_id = decided.comment_id.expect("the comment that was owed");
    let folded = f.discussion();
    assert_eq!(folded.comments.len(), before + 1, "appended exactly once");
    assert_eq!(folded.comments.last().expect("last").id, comment_id);
    assert_eq!(f.file_body(), PROPOSED, "the acceptance had already landed");

    // And a second retry finds nothing owed and refuses on the ordinary terms.
    assert_eq!(f.accept(&proposal.id).unwrap_err(), ERR_ALREADY_DECIDED);
    assert_eq!(f.discussion().comments.len(), before + 1);
}

#[test]
fn a_draft_whose_history_cannot_be_reconciled_answers_nothing_about_its_proposals() {
    // DCP-FR-28 (DCP-FR-28, per DHS-FR-21): each of the three returns the typed
    // `history_recovery_failed` rather than a proposal list or a decision, and
    // nothing was written by any of them.
    let f = Fixture::new();
    let proposal = f.pending();
    let prior = f.file_body();
    let _ = crate::draft_history::stage_prepared_for_test(
        &f.root,
        &f.draft_id,
        &proposal,
        &prior,
        &human(),
    )
    .expect("staged");
    // The prior payload is what a rollback restores; without it the
    // reconciliation cannot complete and leaves the journal standing.
    let hist = drafts::draft_history_dir(&f.root, &f.draft_id).expect("history");
    std::fs::remove_file(hist.join("journal.prior")).expect("block recovery");

    let before = f.file_body();
    let comments_before = f.discussion().comments.len();

    // The **commands**, not stand-ins for them: DCP-FR-28's reconciliation is one
    // line inside each, and a test that called the impls directly would stay
    // green if that line were deleted from any of them.
    use tauri::Manager;
    f.app.manage(crate::project::ProjectState::default());
    let project = f.app.state::<crate::project::ProjectState>();
    project.set_root(f.root.path().to_path_buf());

    for refusal in [
        list_draft_change_proposals(
            f.handle(),
            f.draft_id.clone(),
            f.app.state::<crate::project::ProjectState>(),
        )
        .err(),
        f.accept(&proposal.id).err(),
        f.decline_with(&proposal.id, None).err(),
    ] {
        assert_eq!(
            refusal.as_deref(),
            Some(crate::draft_history::ERR_HISTORY_RECOVERY_FAILED),
        );
    }

    assert_eq!(f.file_body(), before, "no write landed");
    assert_eq!(f.discussion().comments.len(), comments_before, "nothing appended");
    assert!(
        hist.join("journal.toml").is_file(),
        "the journal stands so the next run tries again",
    );
}

// ---------------------------------------------------------------------------
// DCP-FR-17 / DHS-FR-23 — one draft's operations are serialised (DCP-FR-04,
// DCP-FR-17, DHS-FR-23)
// ---------------------------------------------------------------------------

#[test]
fn two_windows_deciding_one_proposal_land_exactly_one_decision() {
    // DCP-FR-17: a second decision arriving from a second window finds it
    // settled rather than overwriting what the first decided — including the
    // case where the two *disagree*, which is the one that corrupts state:
    // without the lock both pass the already-decided check, and the decline's
    // record write lands over the acceptance's, leaving a record that reads
    // `rejected` beside a prompt and a history entry that say otherwise.
    let f = Fixture::new();
    let proposal = f.pending();

    // The `App` itself is not `Sync`, so the two threads take the handle and the
    // root — which is all `decide` needs, and is exactly what two windows share.
    let app = f.handle();
    let root = &f.root;
    let id = proposal.id.as_str();
    let (accepted, declined) = std::thread::scope(|scope| {
        let accept =
            scope.spawn(|| decide(&app, root, root, id, Decision::Accept, None, None, &human()));
        let decline =
            scope.spawn(|| decide(&app, root, root, id, Decision::Decline, None, None, &human()));
        (accept.join().expect("accept thread"), decline.join().expect("decline thread"))
    });

    // Exactly one won, and the other found it settled.
    let outcomes = [accepted.is_ok(), declined.is_ok()];
    assert_eq!(
        outcomes.iter().filter(|ok| **ok).count(),
        1,
        "exactly one decision may land: accept={accepted:?} decline={declined:?}",
    );
    for refused in [&accepted, &declined] {
        if let Err(reason) = refused {
            assert_eq!(reason, ERR_ALREADY_DECIDED);
        }
    }

    // And the record, the prompt and the history all say the same thing.
    let record = find_proposal(&f.root, &proposal.id).expect("still there").1;
    let entries = crate::draft_history::list_impl(&f.root, &f.draft_id)
        .expect("history")
        .entries;
    if accepted.is_ok() {
        assert_eq!(record.state, ProposalState::Accepted);
        assert_eq!(f.file_body(), PROPOSED);
        // DHS-FR-07: the version accepted, and the `Original` it superseded —
        // this being the draft's first acceptance.
        assert_eq!(entries.len(), 2, "one version for the acceptance");
    } else {
        assert_eq!(record.state, ProposalState::Rejected);
        assert_eq!(f.file_body(), ORIGINAL);
        assert!(entries.is_empty(), "a decline records no version");
    }
    // Exactly one decision comment, whichever way it went.
    assert_eq!(f.discussion().comments.len(), 3);
}

#[test]
fn a_reader_cannot_roll_back_an_acceptance_that_is_still_running() {
    // DHS-FR-23: a journal on disk says where an operation got to and nothing
    // about whether the process that wrote it is still alive. Every read path
    // reconciles (DCP-FR-28), so a second window merely *listing* the draft
    // reaches `reconcile` — and a `prepared` journal is exactly what a live
    // acceptance has standing while it writes.
    //
    // The state below is that window frozen: the pre-commit writes have landed
    // and the journal reads `prepared`, which is indistinguishable on disk from
    // a crash. What tells the two apart is the draft's lock, held for the whole
    // of the deciding path — so this asserts that a reader **waits** rather than
    // rolling the operation back, and that nothing it could have undone moved
    // while it waited. Without the lock the reader returns at once having
    // deleted the entry and restored the prompt underneath the writer.
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    let f = Fixture::new();
    let proposal = f.pending();
    let prior = f.file_body();
    let journal = crate::draft_history::stage_prepared_for_test(
        &f.root,
        &f.draft_id,
        &proposal,
        &prior,
        &human(),
    )
    .expect("staged");
    let entry_id = crate::draft_history::journal_entry_id(&journal)
        .expect("the staged operation created an entry");
    // DHS-FR-07: a first acceptance stages the `Original` it supersedes as well,
    // and both are what a reader must leave alone.
    let original_id = crate::draft_history::journal_original_entry_id(&journal)
        .expect("the staged first acceptance settles an Original");
    let hist = drafts::draft_history_dir(&f.root, &f.draft_id).expect("history");
    let manifest = hist.join(format!("{entry_id}.toml"));
    let original = hist.join(format!("{original_id}.toml"));
    assert!(manifest.is_file(), "the operation's entry is on disk");
    assert!(original.is_file(), "and the Original it settled");

    let app = f.handle();
    let root = &f.root;
    let draft_id = f.draft_id.as_str();
    let reconciled = AtomicBool::new(false);
    let flag = &reconciled;

    std::thread::scope(|scope| {
        // Stand in for the writer: hold the draft's lock the way the deciding
        // path holds it across its own pre-commit window.
        let guard = draft_lock(draft_id);

        let reader = scope.spawn(move || {
            crate::draft_history::reconcile(&app, root, root, draft_id).expect("reconciled");
            flag.store(true, Ordering::Release);
        });

        // The reader is blocked, and the operation it would have undone is
        // untouched — which is the whole of what the lock is for.
        std::thread::sleep(Duration::from_millis(120));
        assert!(
            !reconciled.load(Ordering::Acquire),
            "a reader rolled back an operation that was still running",
        );
        assert!(manifest.is_file(), "the entry was removed under the writer");
        assert!(original.is_file(), "the Original was removed under the writer");

        drop(guard);
        reader.join().expect("reader thread");
    });

    // Once the writer is done the reader reconciles on the ordinary terms — the
    // journal never committed, so the operation is rolled back whole.
    assert!(reconciled.load(Ordering::Acquire));
    assert!(!manifest.exists());
    assert!(!original.exists(), "the rollback took both entries");
    assert_eq!(f.file_body(), prior);
    assert_eq!(
        find_proposal(&f.root, &proposal.id).expect("there").1.state,
        ProposalState::Pending,
    );
}


// ---------------------------------------------------------------------------
// DRS-FR-QPSC — a GitHub-shadow draft takes no accepted change
// ---------------------------------------------------------------------------

/// DRS-FR-QPSC: accepting a change to a GitHub-shadow draft is refused with
/// `draft_github_shadow`, the prompt is unchanged, and the proposal stays
/// undecided. A decline writes no prompt, so it is not refused.
#[test]
fn a_shadow_draft_refuses_an_accepted_change() {
    let f = Fixture::new();
    let proposal = f.pending();
    drafts::mark_github_shadow_for_test(
        &f.root,
        &f.draft_id,
        drafts::GithubIssueLink {
            repository_owner: "acme".into(),
            repository_name: "widgets".into(),
            issue_number: 4,
            issue_url: "https://github.com/acme/widgets/issues/4".into(),
            project_node_id: "PVT_1".into(),
            claim_state: drafts::GithubClaimState::Claimed,
        },
    );
    let refused = f.accept(&proposal.id).err();
    assert_eq!(refused, Some(drafts::ERR_GITHUB_SHADOW.to_string()));
    assert_eq!(f.file_body(), ORIGINAL, "the prompt is unchanged");
    assert_eq!(f.reread(&proposal.id).state, proposal.state, "still undecided");
    assert!(f.decline(&proposal.id).is_ok());
}
