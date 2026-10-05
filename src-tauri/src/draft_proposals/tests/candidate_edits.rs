//! Tests for the author's own rewrite of a candidate, the stored participant and the statistics line.

use super::*;

// ---------------------------------------------------------------------------
// DCP-FR-25, DCP-FR-26, DCP-FR-10 .. DCP-FR-19, DCP-FR-24 — the author's own rewrite of a candidate
// (DCP-FR-10, DCP-FR-15, DCP-FR-25, DCP-FR-26, DCP-FR-27)
// ---------------------------------------------------------------------------

const REWRITTEN: &str = "# Spec\n\nA better opening, in my own words.\n";

impl Fixture {
    /// The candidate as it currently stands, with the baseline its next save is
    /// checked against.
    fn candidate(&self, id: &str) -> ProposalContent {
        load_content_impl(&self.root, id).expect("candidate")
    }

    fn save_candidate(
        &self,
        id: &str,
        content: &str,
        baseline: &str,
    ) -> Result<CandidateSaved, String> {
        save_candidate_impl(&self.root, id, content, baseline).map(|(_, saved)| saved)
    }

    fn record_of(&self, id: &str) -> DraftChangeProposal {
        find_proposal(&self.root, id).expect("record").1
    }

    fn draft_updated_at(&self) -> String {
        drafts::list_drafts_impl(&self.root)
            .drafts
            .into_iter()
            .find(|d| d.id == self.draft_id)
            .expect("draft")
            .updated_at
    }
}

#[test]
fn a_candidate_save_replaces_the_candidate_and_changes_nothing_of_the_draft() {
    // DCP-FR-26, DCP-FR-10 / DCP-FR-25: the whole reason the review surface can promise
    // that a candidate under revision has cost the draft nothing.
    let f = Fixture::new();
    let proposal = f.pending();
    let before_files = f.proposals_folder_files();
    let before_updated = f.draft_updated_at();
    let events = f.watch_proposals();

    let baseline = f.candidate(&proposal.id).checksum;
    let saved = f
        .save_candidate(&proposal.id, REWRITTEN, &baseline)
        .expect("saved");

    // The candidate is the author's text, byte for byte, and the checksum
    // reported is the one their next save will name as its baseline.
    let after = f.candidate(&proposal.id);
    assert_eq!(after.content, REWRITTEN);
    assert_eq!(after.checksum, saved.checksum);
    assert_ne!(saved.checksum, baseline);

    // And not one thing about the draft moved.
    assert_eq!(f.file_body(), ORIGINAL, "no file under files/ was written");
    assert_eq!(f.draft_updated_at(), before_updated);
    assert_eq!(
        f.proposals_folder_files(),
        before_files,
        "exactly one file was written, and it already existed",
    );
    assert_eq!(f.record_of(&proposal.id).state, ProposalState::Pending);
    // DCP-FR-26: the proposal's state is exactly what it was, so nothing is
    // announced.
    assert!(events.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
}

#[test]
fn the_edited_flag_describes_the_text_rather_than_the_history() {
    // DCP-FR-12, DCP-FR-14 / DCP-FR-25: an author who puts the agent's words back exactly
    // has an unedited candidate again.
    let f = Fixture::new();
    let proposal = f.pending();
    assert!(!proposal.candidate_edited);
    let origin = proposal.origin_checksum.clone().expect("stamped");
    assert_eq!(origin, f.candidate(&proposal.id).checksum);

    let saved = f
        .save_candidate(&proposal.id, REWRITTEN, &origin)
        .expect("saved");
    assert!(f.record_of(&proposal.id).candidate_edited);

    f.save_candidate(&proposal.id, PROPOSED, &saved.checksum)
        .expect("restored");
    assert!(
        !f.record_of(&proposal.id).candidate_edited,
        "restoring the agent's own bytes clears the flag",
    );
}

#[test]
fn a_candidate_save_against_a_stale_baseline_writes_nothing() {
    // DCP-FR-26 / DCP-FR-27: a review editing a candidate another window has
    // since rewritten cannot overwrite it unknowingly.
    let f = Fixture::new();
    let proposal = f.pending();
    let stale = f.candidate(&proposal.id).checksum;

    // Another window gets there first.
    let fresh = f
        .save_candidate(&proposal.id, REWRITTEN, &stale)
        .expect("the first save lands");

    let err = f
        .save_candidate(&proposal.id, "something else entirely\n", &stale)
        .expect_err("the second is refused");
    assert_eq!(err, ERR_CANDIDATE_STALE);
    let after = f.candidate(&proposal.id);
    assert_eq!(after.content, REWRITTEN, "byte-for-byte as it was");
    assert_eq!(after.checksum, fresh.checksum);
    assert_eq!(f.record_of(&proposal.id).state, ProposalState::Pending);
}

#[test]
fn a_candidate_save_against_a_decided_proposal_is_refused() {
    // DCP-FR-27, DCP-FR-13 / DCP-FR-26: the text a decided proposal holds is the record of
    // what was decided.
    let f = Fixture::new();
    let proposal = f.pending();
    let baseline = f.candidate(&proposal.id).checksum;
    f.decline_with(&proposal.id, None).expect("declined");

    let err = f
        .save_candidate(&proposal.id, REWRITTEN, &baseline)
        .expect_err("refused");
    assert_eq!(err, ERR_ALREADY_DECIDED);
    assert_eq!(f.candidate(&proposal.id).content, PROPOSED);

    assert_eq!(
        save_candidate_impl(&f.root, "no-such-proposal", REWRITTEN, &baseline).unwrap_err(),
        ERR_PROPOSAL_NOT_FOUND,
    );
}

#[test]
fn accepting_an_edited_candidate_lands_the_authors_text_and_says_so() {
    // DCP-FR-25, DCP-FR-19, DCP-FR-24 / DCP-FR-12, DCP-FR-15: the candidate as it stands becomes the
    // file, and the agent is told its text was rewritten rather than that its
    // text landed.
    let f = Fixture::new();
    let proposal = f.pending();
    let baseline = f.candidate(&proposal.id).checksum;
    f.save_candidate(&proposal.id, REWRITTEN, &baseline)
        .expect("saved");

    f.accept(&proposal.id).expect("accepted");
    assert_eq!(f.file_body(), REWRITTEN);
    let thread = f.discussion();
    let body = &thread.comments.last().expect("decision").body;
    assert!(
        body.contains("edited version"),
        "the decision comment tells the agent its text was rewritten: {body}",
    );

    // Declining an edited candidate still costs the draft nothing, and says so
    // in the same terms (DCP-FR-14).
    let f2 = Fixture::new();
    let p2 = f2.pending();
    let baseline = f2.candidate(&p2.id).checksum;
    f2.save_candidate(&p2.id, REWRITTEN, &baseline).expect("saved");
    f2.decline_with(&p2.id, None).expect("declined");
    assert_eq!(f2.file_body(), ORIGINAL);
    let thread = f2.discussion();
    let body = &thread.comments.last().expect("decision").body;
    assert!(body.contains("having edited it first"), "{body}");
}

#[test]
fn a_candidate_save_refuses_a_proposal_id_that_would_leave_the_folder() {
    // DCP-FR-21 / FSA-FR-10: every path this module resolves is checked, and a
    // proposal id is a bare token because it is joined into one (DRS-FR-16). An
    // id carrying a separator or a dot segment names a file elsewhere inside
    // `.synthesis/` even though `resolve_under` refuses to leave the root.
    let f = Fixture::new();
    f.pending();

    for id in ["../secrets", "a/b", "..", "with space", ""] {
        assert!(!is_valid_proposal_id(id), "{id} must not be a valid id");
        assert_eq!(
            save_candidate_impl(&f.root, id, REWRITTEN, "whatever").unwrap_err(),
            ERR_PROPOSAL_NOT_FOUND,
            "{id}",
        );
    }
}

#[test]
fn a_candidate_save_that_cannot_write_leaves_the_candidate_as_it_was() {
    // DCP-FR-27: the write is atomic, so a failure leaves the candidate
    // byte-for-byte as it was and the proposal pending with whatever
    // `candidate_edited` already said.
    let f = Fixture::new();
    let proposal = f.pending();
    let baseline = f.candidate(&proposal.id).checksum;

    // A directory where the atomic write expects to be able to place a file.
    let content = f.proposals_dir().join(format!("{}.hunks", proposal.id));
    std::fs::remove_file(&content).expect("clear the way");
    std::fs::create_dir(&content).expect("occupy the path");

    let err = f
        .save_candidate(&proposal.id, REWRITTEN, &baseline)
        .expect_err("refused");
    // Either the checksum could not be taken over a directory or the write
    // itself failed; both are typed refusals rather than a panic, and neither
    // leaves a candidate behind.
    assert!(
        err == ERR_WRITE_FAILED || err == ERR_PROPOSAL_NOT_FOUND,
        "unexpected refusal: {err}",
    );
    assert_eq!(f.record_of(&proposal.id).state, ProposalState::Pending);
    assert!(!f.record_of(&proposal.id).candidate_edited);
}

#[test]
fn a_candidate_save_emits_no_drafts_changed_and_restamps_nothing() {
    // DCP-FR-25, the half `a_candidate_save_replaces_the_candidate…` cannot
    // reach: the DRAFT's own event channel must carry nothing either, or every
    // surface following it would redraw a draft that did not change — and the
    // indexer would re-read a file nobody wrote.
    let f = Fixture::new();
    let proposal = f.pending();
    let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    f.app.listen(drafts::DRAFTS_CHANGED, move |event| {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            sink.lock().unwrap_or_else(|e| e.into_inner()).push(value);
        }
    });
    let before = f.draft_updated_at();

    let baseline = f.candidate(&proposal.id).checksum;
    f.save_candidate(&proposal.id, REWRITTEN, &baseline)
        .expect("saved");

    assert!(seen.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
    assert_eq!(f.draft_updated_at(), before);
}

// ---------------------------------------------------------------------------
// The agent participant a proposal stores (CMS-FR-65, read by DCR-FR-04)
// ---------------------------------------------------------------------------

#[test]
fn a_proposals_stored_participant_round_trips_its_title_snapshot() {
    // DCR-FR-04 reads the *stored* participant rather than the comment's, so the
    // snapshot has to survive this module's own TOML round trip. Asserted after
    // a list rather than on the returned record: `record_proposal` could carry a
    // field it never wrote and the modal would still read nothing.
    let f = Fixture::new();
    let recorded = f.pending();
    assert_eq!(recorded.agent, agent());

    let listed = f.list_through_command();
    assert_eq!(listed.len(), 1);
    match &listed[0].agent {
        Participant::Agent {
            agent_id,
            handle,
            model,
            title,
        } => {
            assert_eq!(agent_id, "agent-1");
            assert_eq!(handle, "arch");
            assert_eq!(model.as_deref(), Some("m"));
            assert_eq!(title.as_deref(), Some("Developer"));
        }
        other => panic!("expected the proposing agent's participant, got {other:?}"),
    }
}

#[test]
fn a_stored_participant_predating_the_snapshot_reads_back_carrying_none() {
    // The older shape on disk: a `.toml` written before the field existed. It
    // has to load carrying no title rather than failing the read or acquiring
    // one, on the same terms CMS-FR-65 gives an older log line.
    let f = Fixture::new();
    let older = Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        title: None,
    };
    let recorded = record_proposal(
        &f.handle(),
        &f.root,
        &f.root,
        NewProposal {
            draft_id: &f.draft_id,
            path: FILE,
            hunks: &hunks::whole_document(ORIGINAL, PROPOSED),
            rationale: "The opening buries the point.",
            agent: &older,
            target: f.target(),
            thread_id: &f.thread_id,
        },
    )
    .expect("recorded");
    assert_eq!(recorded.agent, older);

    let listed = f.list_through_command();
    assert_eq!(listed[0].agent, older);
    match &listed[0].agent {
        Participant::Agent { title, .. } => assert_eq!(*title, None),
        other => panic!("expected an agent participant, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// The statistics line a decided proposal contributes (DCP-FR-WTKA)
// ---------------------------------------------------------------------------

/// Give the fixture's app a project state pointing at its root, which is what
/// the statistics recorder resolves the draft's log against.
fn with_project(f: &Fixture) {
    use tauri::Manager;
    f.app.manage(crate::project::ProjectState::default());
    f.app
        .state::<crate::project::ProjectState>()
        .set_root(f.root.path().to_path_buf());
}

/// The draft's statistics, after the asynchronous writer has caught up.
///
/// Production never waits for it — that is the whole of
/// `DSS-draft-statistics-storage.md` DSS-FR-TUMX — so a test that wants to read
/// what was written is the one thing that does.
fn decision_statistics(f: &Fixture) -> crate::statistics::DraftStatistics {
    crate::statistics::wait_for_writer();
    crate::statistics::read_statistics_impl(&f.root, &f.draft_id).expect("statistics")
}

#[test]
fn dcp_ts_yevn_a_decided_proposal_contributes_one_line_and_a_pending_one_none() {
    // DCP-FR-16, DCP-FR-25 / DCP-FR-WTKA. Decided through `decide` and then announced,
    // which is what the two decision commands do at their one emit point.
    let f = Fixture::new();
    with_project(&f);

    let accepted = f.pending();
    let decided = f.accept(&accepted.id).expect("accepted");
    announce(&f.handle(), &BUFFER, &decided.proposal);

    let declined = f.propose(FILE, "# Spec\n\nAnother opening.\n").expect("recorded");
    let refused = f.decline_with(&declined.id, None).expect("declined");
    announce(&f.handle(), &BUFFER, &refused.proposal);

    // A third, recorded and left pending, and a candidate saved against it.
    let pending = f.propose(FILE, "# Spec\n\nA third opening.\n").expect("recorded");
    announce(&f.handle(), &BUFFER, &pending);

    let stats = decision_statistics(&f);
    assert_eq!(stats.totals.accepted_proposals.value, Some(1));
    assert_eq!(stats.totals.rejected_proposals.value, Some(1));
}

#[test]
fn dcp_ts_lwda_a_duplicated_decision_contributes_once_and_carries_no_content() {
    // DSS-FR-TUMX / DCP-FR-WTKA, per DSS-FR-PNUE.
    let f = Fixture::new();
    with_project(&f);
    let proposal = f.pending();
    let decided = f.accept(&proposal.id).expect("accepted");

    // The line a resumed operation appended twice.
    announce(&f.handle(), &BUFFER, &decided.proposal);
    announce(&f.handle(), &BUFFER, &decided.proposal);

    assert_eq!(decision_statistics(&f).totals.accepted_proposals.value, Some(1));

    let path = crate::statistics::log_path(&f.root, &f.draft_id).expect("path");
    let text = f.root.read_text(&path).expect("log");
    // The line carries the proposal's id and the decision and nothing else: no
    // candidate byte, no rationale, no feedback, no path, no participant.
    for forbidden in ["Spec", "opening", "buries", FILE, "raver119", "arch"] {
        assert!(
            !text.contains(forbidden),
            "a proposal statistics line must not carry {forbidden:?}",
        );
    }
}
