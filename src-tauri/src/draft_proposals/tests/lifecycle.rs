//! Tests for a proposal across the life of its draft, its thread and its events.

use super::*;

// ---------------------------------------------------------------------------
// DCP-FR-14 — the file went while the proposal stood (DCP-FR-18)
// ---------------------------------------------------------------------------

#[test]
fn accepting_a_proposal_whose_file_is_gone_refuses_and_declining_it_still_works() {
    let f = Fixture::new();
    let proposal = f.pending();
    // DCP-FR-18 / DRS-FR-14: the prompt's path changes in exactly one way —
    // renaming the draft renames the one file it holds.
    drafts::rename_draft_impl(&f.root, &f.draft_id, "overview").expect("rename");

    assert_eq!(f.accept(&proposal.id).unwrap_err(), ERR_PATH_MISSING);
    assert!(
        drafts::load_draft_file_impl(&f.root, &f.draft_id, FILE).is_err(),
        "no file was created at that path (DCP-FR-18)",
    );
    assert_eq!(
        find_proposal(&f.root, &proposal.id).expect("still there").1.state,
        ProposalState::Pending,
        "the proposal stays pending for the author to clear",
    );

    let decided = f.decline_with(&proposal.id, None).expect("declining works");
    assert_eq!(decided.proposal.state, ProposalState::Rejected);
}

// ---------------------------------------------------------------------------
// DCP-FR-19, DRS-FR-20, DRS-FR-21 — a proposal goes with its draft (DCP-FR-02)
// ---------------------------------------------------------------------------

#[test]
fn deleting_the_draft_takes_its_proposals_with_it() {
    let f = Fixture::new();
    let proposal = f.pending();
    let dir = f.proposals_dir();
    assert!(dir.is_dir());

    drafts::delete_draft_impl(&f.root, &f.root, &f.draft_id).expect("delete");
    assert!(!dir.exists(), "the whole folder went with the draft");
    assert!(
        find_proposal(&f.root, &proposal.id).is_none(),
        "and nothing finds it afterwards",
    );
}

#[test]
/// DRS-FR-20: graduating retains everything the draft holds, proposals among
/// them, so a graduated draft is a complete draft on disk rather than a
/// tombstone.
fn graduating_the_draft_leaves_its_proposals_in_place() {
    let f = Fixture::new();
    let pending = f.pending();
    let dir = f.proposals_dir();

    drafts::set_draft_graduated(&f.root, &f.draft_id, "run-1").expect("graduate");
    assert!(dir.exists(), "graduation removes nothing the draft holds");
    assert!(
        find_proposal(&f.root, &pending.id).is_some(),
        "and the undecided proposal is still findable",
    );
}

// ---------------------------------------------------------------------------
// DCP-FR-22 — unknown ids (DCP-FR-22)
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_proposal_is_refused_by_every_operation_that_names_one() {
    let f = Fixture::new();
    assert_eq!(
        load_content_impl(&f.root, "nope").unwrap_err(),
        ERR_PROPOSAL_NOT_FOUND,
    );
    assert_eq!(f.accept("nope").unwrap_err(), ERR_PROPOSAL_NOT_FOUND);
    assert_eq!(f.decline_with("nope", None).unwrap_err(), ERR_PROPOSAL_NOT_FOUND);
    // A traversal dressed as an id is refused before it is joined into a path.
    assert!(!is_valid_proposal_id("../../etc/passwd"));
    assert!(!is_valid_proposal_id(""));
    assert_eq!(
        load_content_impl(&f.root, "../../etc/passwd").unwrap_err(),
        ERR_PROPOSAL_NOT_FOUND,
    );
}

// ---------------------------------------------------------------------------
// A proposal in an anchored thread, not only a discussion (DCP-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn a_proposal_reaches_an_anchored_draft_thread_too() {
    let f = Fixture::new();
    // Built through the writer directly: the anchored-draft-thread path has no
    // pure helper of its own, the surface reaching it through a Tauri command.
    let thread_id = crate::notes::new_note_id();
    comments::append_as_scoped(
        &f.root,
        comments::LogScope::Draft {
            draft_id: &f.draft_id,
        },
        FILE,
        &human(),
        "2026-01-01T00:02:00Z",
        vec![
            (
                thread_id.clone(),
                comments::EventBody::ThreadOpened {
                    artifact_path: FILE.to_string(),
                    anchor: comments::LegacyAnchor {
                        start: 0,
                        end: 6,
                        quote: "# Spec".into(),
                    },
                },
            ),
            (
                thread_id.clone(),
                comments::EventBody::CommentAdded {
                    comment_id: crate::notes::new_note_id(),
                    body: "@arch this heading".into(),
                    quotes: Vec::new(),
                    attachments: Vec::new(),
                },
            ),
        ],
    )
    .expect("anchored thread");
    let thread = comments::list_draft_fragment_discussions_in(&f.root, &f.draft_id, FILE)
        .into_iter()
        .find(|t| t.id == thread_id)
        .expect("anchored thread");

    let proposal = record_proposal(
        &f.handle(),
        &f.root,
        &f.root,
        NewProposal {
            draft_id: &f.draft_id,
            path: FILE,
            hunks: &hunks::whole_document(ORIGINAL, PROPOSED),
            rationale: "Heading first.",
            agent: &agent(),
            target: ThreadRef::draft_file(&f.draft_id, FILE),
            thread_id: &thread.id,
        },
    )
    .expect("recorded");

    let folded = comments::list_draft_fragment_discussions_in(&f.root, &f.draft_id, FILE)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    assert_eq!(folded.comments.len(), 2);
    assert_eq!(folded.comments[1].attachments.len(), 1);

    // And the decision finds its way back to that same anchored thread rather
    // than to the draft's discussion log.
    let decided = f.accept(&proposal.id).expect("accepted");
    let folded = comments::list_draft_fragment_discussions_in(&f.root, &f.draft_id, FILE)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    assert_eq!(folded.comments.len(), 3, "the decision landed here");
    assert!(folded.comments[2].body.contains("Accepted"));
    // DCR-FR-15: the origin kind the surface builds the fresh turn's origin
    // from. An anchored thread is a `draft_comment`, and a turn dispatched
    // against the wrong kind resolves the wrong conversation.
    assert_eq!(decided.origin_kind, ORIGIN_DRAFT_COMMENT);
    assert_eq!(decided.comment_id.as_deref(), Some(folded.comments[2].id.as_str()));
}

// ---------------------------------------------------------------------------
// A decision the conversation cannot take still decides (DCP-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn a_locked_conversation_refuses_the_decision_before_anything_is_written() {
    // DCP-FR-15 (DCP-FR-15): one of the two conditions that would make the
    // decision comment impossible, checked up front — so the prompt is
    // untouched, the history gained nothing, and the proposal is still pending
    // rather than accepted-with-the-comment-owed.
    let f = Fixture::new();
    let proposal = f.pending();
    let before = f.file_body();
    f.lock();

    assert_eq!(
        f.accept(&proposal.id).unwrap_err(),
        crate::comments::ERR_DISCUSSION_LOCKED,
    );
    assert_eq!(f.file_body(), before, "the prompt is byte-for-byte what it was");
    assert!(
        crate::draft_history::list_impl(&f.root, &f.draft_id)
            .expect("history")
            .entries
            .is_empty(),
        "no version was recorded",
    );
    assert_eq!(
        find_proposal(&f.root, &proposal.id).expect("still there").1.state,
        ProposalState::Pending,
    );
    assert_eq!(
        f.discussion().comments.len(),
        2,
        "the locked log gained no line",
    );
}

// ---------------------------------------------------------------------------
// DCP-FR-17 — the event, on recording as well as on deciding (DCP-FR-16)
// ---------------------------------------------------------------------------

#[test]
fn recording_a_proposal_announces_it_as_pending() {
    // The regression this exists for: without an emission here, every surface
    // that follows the event — the tab's marker (NAW-FR-35), the rail's control
    // (CMT-FR-67), the Drafts panel's row (DRP-FR-19) and the shell's
    // notification (DCR-FR-18) — learns of a proposal only when the project is
    // reopened, and the notification never fires at all.
    let f = Fixture::new();
    let seen = f.watch_proposals();
    let proposal = f.pending();

    let events = seen.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(events.len(), 1, "one event, on the recording");
    assert_eq!(events[0]["draftId"], f.draft_id.as_str());
    assert_eq!(events[0]["proposal"]["id"], proposal.id.as_str());
    assert_eq!(
        events[0]["proposal"]["state"], "pending",
        "and it carries `pending`, which is what the raise turns on (DCR-FR-18)",
    );
}

#[test]
fn a_refused_recording_announces_nothing() {
    let f = Fixture::new();
    f.pending();
    let seen = f.watch_proposals();

    f.propose(FILE, "different again").unwrap_err();
    f.propose("missing.md", PROPOSED).unwrap_err();
    f.propose(FILE, ORIGINAL).unwrap_err();

    assert!(
        seen.lock().unwrap_or_else(|e| e.into_inner()).is_empty(),
        "no consumer redraws for a proposal that was not recorded",
    );
}

// ---------------------------------------------------------------------------
// DCP-FR-06 — all or nothing, from the record side too (DCP-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn a_failed_record_write_leaves_no_comment_behind() {
    // The half the lock refusal cannot reach: the record write is the *last*
    // pre-append step, so a failure there is the one that could leave a comment
    // naming a proposal that does not exist — in a log that is append-only and
    // cannot be corrected (CMS-FR-04).
    //
    // Forced by making the proposals folder read-only, which is what fails the
    // record write without touching anything else — and which forces nothing at
    // all for root, who may write into it regardless.
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_failed_record_write_leaves_no_comment_behind",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let f = Fixture::new();
    let dir = f.proposals_dir();
    let mut perms = std::fs::metadata(&dir).expect("meta").permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&dir, perms).expect("chmod");

    let outcome = f.propose(FILE, PROPOSED);

    // Restore before asserting, so a failure here does not leave the temp dir
    // undeletable.
    let mut perms = std::fs::metadata(&dir).expect("meta").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&dir, perms).expect("chmod");

    assert_eq!(outcome.unwrap_err(), RecordRefusal::NotRecorded);
    assert_eq!(
        f.discussion().comments.len(),
        1,
        "the conversation gained no line (DCP-FR-06)",
    );
    assert!(
        f.proposals_folder_files().is_empty(),
        "and no candidate was left behind either: {:?}",
        f.proposals_folder_files(),
    );
    // The draft's one slot is free, so the agent can simply propose again.
    assert!(pending_for(&f.root, &f.draft_id).is_none());
}

#[test]
fn every_recorded_proposal_is_named_by_the_comment_that_announced_it() {
    // DCP-FR-06's positive half: the record reaches disk carrying the id of a
    // comment that does not exist yet, and the append then writes exactly that
    // id — so the two always agree, whichever order a reader folds them in.
    let f = Fixture::new();
    let proposal = f.pending();
    let thread = f.discussion();
    let announced = thread.comments.last().expect("the proposal's comment");

    assert_eq!(proposal.comment_id, announced.id);
    match &announced.attachments[0] {
        comments::Attachment::Proposal { proposal_id, .. } => {
            assert_eq!(proposal_id, &proposal.id);
        }
        other => panic!("expected a proposal reference, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// DCP-FR-04 under concurrency — the check is a read followed by a write
// ---------------------------------------------------------------------------

#[test]
fn two_agents_proposing_at_once_record_exactly_one() {
    // PDC-FR-17, PDC-FR-08 / DCP-FR-04. AGC-FR-03 puts several turns in flight at once, so
    // two agents asked about one draft genuinely can reach the pending check
    // together. Without serialisation both pass it and the draft ends up holding
    // two undecided proposals, of which `pending_for` surfaces one — the other
    // occupying the slot while being unreachable from every surface.
    let f = Fixture::new();
    let recorded = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(std::sync::Barrier::new(4));
    // `tauri::App` is not `Sync`, so the threads take a handle and the plain
    // values rather than the fixture itself.
    let handle = f.handle();
    let root = f.root.clone();
    let draft_id = f.draft_id.clone();
    let thread_id = f.thread_id.clone();

    std::thread::scope(|scope| {
        for n in 0..4 {
            let recorded = Arc::clone(&recorded);
            let barrier = Arc::clone(&barrier);
            let (handle, root) = (handle.clone(), root.clone());
            let (draft_id, thread_id) = (draft_id.clone(), thread_id.clone());
            scope.spawn(move || {
                let content = format!("# Spec\n\nRewrite {n}.\n");
                barrier.wait();
                let outcome = record_proposal(
                    &handle,
                    &root,
                    &root,
                    NewProposal {
                        draft_id: &draft_id,
                        path: FILE,
                        hunks: &hunks::whole_document(ORIGINAL, &content),
                        rationale: "Leads with the point.",
                        agent: &agent(),
                        target: ThreadRef::discussion(&draft_id),
                        thread_id: &thread_id,
                    },
                );
                if outcome.is_ok() {
                    recorded.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });

    assert_eq!(
        recorded.load(Ordering::SeqCst),
        1,
        "exactly one call may record while the others find the slot taken",
    );
    let all = list_proposals_impl(&f.root, &f.draft_id).expect("list");
    assert_eq!(all.len(), 1, "one candidate on disk: {all:?}");
    assert_eq!(
        f.discussion().comments.len(),
        2,
        "and one comment announcing it",
    );
}

// ---------------------------------------------------------------------------
// DCP-FR-16, DCP-FR-17, DHS-FR-19 — a failed draft-file write (DCP-FR-13)
// ---------------------------------------------------------------------------

#[test]
fn a_failed_draft_write_leaves_the_proposal_pending_and_the_file_untouched() {
    // The read-only `files/` below binds every process except root.
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_failed_draft_write_leaves_the_proposal_pending_and_the_file_untouched",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let f = Fixture::new();
    let proposal = f.pending();

    // The draft's `files/` made read-only is what fails the save without
    // touching the record or the conversation.
    let files_dir = f.root.path().join(format!(
        ".synthesis/drafts/{}/files",
        f.draft_id
    ));
    let mut perms = std::fs::metadata(&files_dir).expect("meta").permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&files_dir, perms).expect("chmod");

    let outcome = f.accept(&proposal.id);

    let mut perms = std::fs::metadata(&files_dir).expect("meta").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&files_dir, perms).expect("chmod");

    assert_eq!(outcome.unwrap_err(), ERR_WRITE_FAILED);
    assert_eq!(f.file_body(), ORIGINAL, "byte-for-byte what it was");
    assert_eq!(
        find_proposal(&f.root, &proposal.id).expect("still there").1.state,
        ProposalState::Pending,
        "so the author can decide it again (DCP-FR-13)",
    );
    assert_eq!(
        f.discussion().comments.len(),
        2,
        "and no decision was announced to the conversation",
    );
}

// ---------------------------------------------------------------------------
// DCR-FR-15 — what a decision hands the surface back
// ---------------------------------------------------------------------------

#[test]
fn a_decision_reports_the_comment_and_the_origin_kind_it_landed_in() {
    // The surface dispatches one fresh turn to the proposing agent naming this
    // comment as its trigger (DCR-FR-15, AGC-FR-29); without both values it
    // would have to re-fold the thread to guess which comment it had just made.
    let f = Fixture::new();
    let proposal = f.pending();

    let decided = f.accept(&proposal.id).expect("accepted");
    let comment_id = decided.comment_id.expect("the decision's comment");
    assert_eq!(decided.origin_kind, ORIGIN_DRAFT_DISCUSSION);
    assert_eq!(
        f.discussion().comments.last().expect("last").id,
        comment_id,
        "and it names the comment actually appended",
    );

    // A locked conversation refuses the decision up front, so there is nothing
    // for a turn to answer and nothing was decided (DCP-FR-15).
    let f2 = Fixture::new();
    let p2 = f2.pending();
    f2.lock();
    assert_eq!(
        f2.accept(&p2.id).unwrap_err(),
        crate::comments::ERR_DISCUSSION_LOCKED,
    );
}

