//! Recording a change edit proposal (CEP-FR-QDJH and the rules it applies).

use super::*;

// CEP-FR-HXCC / CEP-FR-IISM / CEP-FR-QDJH: one proposal holds hunks of several
// files in proposal order, each with its change-relative path, and lives in
// the change's own folder of the store, not in the working copy.
#[test]
fn a_recording_holds_hunks_of_several_files_in_the_store() {
    let f = Fixture::new();
    let record = f.pending();
    assert_eq!(record.state, ChangeProposalState::Pending);
    assert_eq!(record.hunk_count, 2);
    let paths: Vec<&str> = record.ledger.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, ["tasks.md", "specs/auth/spec.md"]);
    let dir = f.store.path().join(format!("changes/{STREAM}/{CHANGE}/proposals"));
    assert!(dir.join(format!("{}.toml", record.id)).is_file());
    assert!(dir.join(format!("{}.hunks", record.id)).is_file());
    // Nothing is written into the working copy before a decision.
    assert_eq!(f.file("tasks.md").as_deref(), Some(TASKS_MD));
    assert!(f.file("specs/auth/spec.md").is_none());
    assert!(!f.wc.path().join(".synthesis").exists());
}

// CEP-FR-QDJH / CMS-FR-QLMV: the comment that announces the proposal carries
// the `change_proposal` attachment and the rationale, in the turn's thread.
#[test]
fn the_recording_appends_one_agent_comment_with_the_reference() {
    let f = Fixture::new();
    let record = f.pending();
    let thread = f.thread();
    let last = thread.comments.last().expect("comment");
    assert_eq!(last.id, record.comment_id);
    assert!(last.body.contains("logout"));
    assert_eq!(
        last.attachments,
        vec![comments::Attachment::ChangeProposal {
            proposal_id: record.id.clone(),
            stream_id: STREAM.into(),
            change: CHANGE.into(),
            update: None,
        }]
    );
}

// CEP-FR-ZOIG: a change holds at most one pending proposal.
#[test]
fn a_second_recording_while_one_is_pending_is_refused() {
    let f = Fixture::new();
    f.pending();
    let again = f.record(&[replace("proposal.md", "Users need to log in.", "Users log in.")]);
    assert_eq!(again, Err(RecordRefusal::ProposalPending));
}

// CEP-FR-WJKY: a path that is absolute, holds `..`, or leaves the change
// folder is refused with its place, and nothing is recorded.
#[test]
fn a_path_outside_the_change_is_refused_and_records_nothing() {
    let f = Fixture::new();
    for bad in ["../other/tasks.md", "/etc/passwd", "specs/../../x.md", "", "./tasks.md"] {
        let refused = f.record(&[
            replace("tasks.md", "- [ ] 1.1 Add the form\n", "- [ ] 1.1 Add it\n"),
            replace(bad, "a", "b"),
        ]);
        assert_eq!(refused, Err(RecordRefusal::PathOutsideChange { at: 2, of: 2 }), "{bad}");
    }
    assert!(list_proposals_impl(&f.store, STREAM, CHANGE).is_empty());
}

// CEP-FR-HRCX: a path that names no file is a new file whose one hunk is an
// `add` of the whole text; a second hunk on the same new path overlaps it.
#[test]
fn a_hunk_on_a_missing_path_is_a_new_file() {
    let f = Fixture::new();
    let record = f.pending();
    let row = record.row(&row_id(&record, "specs/auth/spec.md")).expect("row");
    assert!(row.new_file);
    assert_eq!(row.kind, crate::text_hunks::anchors::HunkKind::Add);

    let g = Fixture::new();
    let refused = g.record(&[new_file("design.md", "a"), new_file("design.md", "b")]);
    assert_eq!(refused, Err(RecordRefusal::HunkOverlap { at: 2, of: 2 }));
}

// CEP-FR-AYVS: hunks are placed against the file of the working copy, and a
// hunk whose text is not there is refused with its place.
#[test]
fn a_hunk_whose_text_is_not_in_its_file_is_refused() {
    let f = Fixture::new();
    let refused = f.record(&[
        replace("tasks.md", "- [ ] 1.1 Add the form\n", "x\n"),
        replace("proposal.md", "not in the file", "y"),
    ]);
    assert_eq!(refused, Err(RecordRefusal::HunkAnchorLost { at: 2, of: 2 }));
}

// CEP-FR-AYVS / CEP-FR-LYSS: an unknown stream and a missing change folder.
#[test]
fn an_unknown_stream_and_a_missing_change_are_refused() {
    let f = Fixture::new();
    let unknown = record_change_proposal(
        &f.handle(),
        &f.store,
        &f.sites,
        NewChangeProposal {
            stream_id: "gone",
            change: CHANGE,
            hunks: &[replace("tasks.md", "a", "b")],
            rationale: "r",
            agent: &agent(),
            thread_id: &f.thread_id,
        },
    );
    assert_eq!(unknown, Err(RecordRefusal::UnknownStream));
    let missing = record_change_proposal(
        &f.handle(),
        &f.store,
        &f.sites,
        NewChangeProposal {
            stream_id: STREAM,
            change: "no-such-change",
            hunks: &[replace("tasks.md", "a", "b")],
            rationale: "r",
            agent: &agent(),
            thread_id: &f.thread_id,
        },
    );
    assert_eq!(missing, Err(RecordRefusal::ChangeNotFound));
}

// CEP-FR-QDJH: a resolved conversation takes no proposal, and nothing is
// recorded.
#[test]
fn a_resolved_conversation_records_nothing() {
    let f = Fixture::new();
    let thread = f.thread();
    comments::set_resolution_to(
        &f.store,
        thread.log_ref(),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .expect("resolve");
    let refused = f.record(&[replace("tasks.md", "- [ ] 1.1 Add the form\n", "x\n")]);
    assert_eq!(refused, Err(RecordRefusal::ThreadResolved));
    assert!(list_proposals_impl(&f.store, STREAM, CHANGE).is_empty());
}

// CEP-FR-SQCG: a recording emits the event with the stream, the change, the
// proposal, and the cause.
#[test]
fn a_recording_emits_the_event() {
    let f = Fixture::new();
    let seen = f.watch();
    let record = f.pending();
    let events = seen.lock().unwrap().clone();
    let event = events.first().expect("event");
    assert_eq!(event["streamId"], STREAM);
    assert_eq!(event["change"], CHANGE);
    assert_eq!(event["proposal"]["id"], record.id.as_str());
    assert_eq!(event["cause"], "recorded");
}

// CEP-FR-HUFE / CEP-FR-XXMW: the hunks read back in proposal order, each with
// its path, its new-file flag, its state, and its placement.
#[test]
fn the_hunks_and_the_turn_read_carry_each_path() {
    let f = Fixture::new();
    let record = f.pending();
    let read = load_hunks_impl(&f.store, &f.sites, &record.id).expect("hunks");
    let paths: Vec<(&str, bool)> =
        read.hunks.iter().map(|h| (h.path.as_str(), h.new_file)).collect();
    assert_eq!(paths, [("tasks.md", false), ("specs/auth/spec.md", true)]);
    assert!(read.resolutions.iter().all(|r| matches!(r, HunkPlacement::Resolved { .. })));
    let changes = read_change_proposal_changes(&f.store, &record.id).expect("changes");
    assert_eq!(changes.change, CHANGE);
    assert_eq!(changes.changes.len(), 2);
    assert!(changes.changes[1].after.contains("ADDED"));
    assert_eq!(list_proposals_impl(&f.store, STREAM, CHANGE).len(), 1);
}

// CEP-FR-TFOY: an id that names no proposal is `proposal_not_found`.
#[test]
fn an_unknown_proposal_id_is_not_found() {
    let f = Fixture::new();
    assert_eq!(
        load_hunks_impl(&f.store, &f.sites, "nope").map(|_| ()),
        Err(ERR_PROPOSAL_NOT_FOUND.to_string())
    );
    assert_eq!(f.accept("../x", "h").map(|_| ()), Err(ERR_PROPOSAL_NOT_FOUND.to_string()));
}

// CEP-FR-HUFE: the proposals of a change list most recently created first.
#[test]
fn the_proposals_list_newest_first() {
    let f = Fixture::new();
    let first = f.pending();
    f.decide(&first.id, Decision::Decline, None).expect("declined");
    let second = f
        .record(&[replace("proposal.md", "Users need to log in.", "Users log in.")])
        .expect("recorded");
    let ids: Vec<String> =
        list_proposals_impl(&f.store, STREAM, CHANGE).into_iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![second.id, first.id]);
}

// CEP-FR-WJKY / CEP-FR-HRCX / PCP-FR-07: a path that names a folder is
// outside the change; a new-file hunk with a `before` names no file; a
// proposal that changes nothing is refused.
#[test]
fn a_folder_path_a_new_file_with_before_and_no_change_are_refused() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.wc.path().join(format!("openspec/changes/{CHANGE}/specs")))
        .expect("dir");
    assert_eq!(
        f.record(&[new_file("specs", "x\n")]),
        Err(RecordRefusal::PathOutsideChange { at: 1, of: 1 })
    );
    assert_eq!(
        f.record(&[replace("design.md", "a", "b")]),
        Err(RecordRefusal::HunkAnchorLost { at: 1, of: 1 })
    );
    assert_eq!(f.record(&[replace("tasks.md", "Login", "Login")]), Err(RecordRefusal::NoChange));
}
