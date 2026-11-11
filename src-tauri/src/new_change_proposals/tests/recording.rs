//! Recording and updating a new-change proposal.

use super::*;

// NCP-FR-AZON / NCP-FR-MTCW / NCP-FR-VTPG / NCP-FR-NDHQ / CMS-FR-QYGP: a
// recording writes the record and the files in the change's store folder,
// mints the split id, and appends one agent comment with the reference.
#[test]
fn a_recording_stores_the_proposal_and_announces_it() {
    let f = Fixture::new();
    let seen = f.watch();
    let record = f.record_named("part-one", 1, 2).expect("recorded");
    let dir = f.store.path().join(format!("changes/{STREAM}/{CHANGE}/new-changes"));
    assert!(dir.join(format!("{}.toml", record.id)).is_file());
    assert!(dir.join(format!("{}.files", record.id)).is_file());
    assert_eq!(record.paths, vec!["proposal.md".to_string(), "specs/auth/spec.md".to_string()]);
    assert_eq!(record.state, NewChangeProposalState::Pending);
    assert!(!record.set_id.is_empty());
    let last = f.thread().comments.last().cloned().expect("comment");
    assert_eq!(last.id, record.comment_id);
    assert_eq!(
        last.attachments,
        vec![comments::Attachment::NewChangeProposal {
            proposal_id: record.id.clone(),
            stream_id: STREAM.into(),
            change: CHANGE.into(),
            update: None,
        }]
    );
    assert_eq!(seen.lock().unwrap()[0]["cause"], "recorded");
    assert!(f.wc_file("openspec/changes/part-one/proposal.md").is_none(), "nothing is created");
}

// NCP-FR-RPGN: one pending proposal at a time, and the field and name rules.
#[test]
fn a_recording_refuses_on_the_rules_of_the_split() {
    let f = Fixture::new();
    let first = f.record_named("part-one", 1, 2).expect("recorded");
    assert_eq!(f.record_named("part-two", 2, 2), Err(ERR_PROPOSAL_PENDING.to_string()));
    f.decide(&first.id, false).expect("rejected");
    assert_eq!(f.record_named(CHANGE, 1, 2), Err(ERR_CHANGE_EXISTS.to_string()));
    assert_eq!(f.record_named("Bad Name", 1, 2), Err(ERR_INVALID_CHANGE_NAME.to_string()));
    assert_eq!(f.record_named("part-two", 3, 2), Err(ERR_BAD_POSITION.to_string()));
    assert_eq!(f.record_named("part-two", 0, 2), Err(ERR_BAD_POSITION.to_string()));
    assert!(check_files(&[ProposedFile { path: "tasks.md".into(), text: "x".into() }]).is_err());
    assert_eq!(
        check_files(&[
            ProposedFile { path: "proposal.md".into(), text: "x".into() },
            ProposedFile { path: "../x.md".into(), text: "x".into() },
        ]),
        Err(ERR_PATH_OUTSIDE_CHANGE.to_string())
    );
    assert_eq!(
        check_files(&[ProposedFile { path: "proposal.md".into(), text: " ".into() }]),
        Err(ERR_BLANK_FILE.to_string())
    );
    // NCP-FR-NDHQ: a later recording takes the same split id.
    let second = f.record_named("part-two", 1, 2).expect("recorded");
    assert_eq!(second.set_id, first.set_id);
}

// NCP-FR-RPGN: a name of an accepted proposal is taken.
#[test]
fn an_accepted_name_is_taken() {
    let f = Fixture::new();
    let first = f.record_named("part-one", 1, 2).expect("recorded");
    f.decide(&first.id, true).expect("accepted");
    assert_eq!(f.record_named("part-one", 2, 2), Err(ERR_CHANGE_EXISTS.to_string()));
}

// NCP-FR-QBIF: a pending change edit proposal does not refuse a recording.
#[test]
fn a_pending_edit_proposal_does_not_refuse_a_recording() {
    let f = Fixture::new();
    crate::change_proposals::record_change_proposal(
        &f.handle(),
        &f.store,
        &f.sites,
        crate::change_proposals::NewChangeProposal {
            stream_id: STREAM,
            change: CHANGE,
            hunks: &[crate::change_proposals::PathedHunk {
                path: "proposal.md".into(),
                hunk: crate::text_hunks::hunks::ProposedHunk::from_fields(
                    Some("Too big.".into()),
                    Some("Big.".into()),
                    None,
                    None,
                    None,
                    None,
                ),
            }],
            rationale: "r",
            agent: &agent(),
            thread_id: &f.thread_id,
        },
    )
    .expect("edit proposal");
    assert!(f.record_named("part-one", 1, 2).is_ok());
}

// NCP-FR-SNMA: an update changes the fields it carries and bumps the
// revision, or withdraws; no open proposal and no change are refused.
#[test]
fn an_update_changes_or_withdraws_the_pending_proposal() {
    let f = Fixture::new();
    let update = |action| {
        update_new_change_proposal(
            &f.handle(),
            &f.store,
            &f.sites,
            NewChangeUpdate {
                stream_id: STREAM,
                change: CHANGE,
                action,
                rationale: "r",
                agent: &agent(),
                thread_id: &f.thread_id,
            },
        )
    };
    assert_eq!(update(NewChangeUpdateAction::Withdraw), Err(ERR_NO_OPEN_PROPOSAL.to_string()));
    f.record_named("part-one", 1, 2).expect("recorded");
    let same = NewChangeUpdateAction::Change {
        name: Some("part-one".into()),
        files: None,
        position: None,
        total: None,
    };
    assert_eq!(update(same), Err(ERR_NO_CHANGE.to_string()));
    let renamed = update(NewChangeUpdateAction::Change {
        name: Some("part-a".into()),
        files: None,
        position: None,
        total: Some(3),
    })
    .expect("changed");
    assert_eq!((renamed.name.as_str(), renamed.total, renamed.revision), ("part-a", 3, 2));
    let last = f.thread().comments.last().cloned().expect("comment");
    assert!(matches!(
        &last.attachments[0],
        comments::Attachment::NewChangeProposal {
            update: Some(comments::NewDraftUpdateAction::Change),
            ..
        }
    ));
    let withdrawn = update(NewChangeUpdateAction::Withdraw).expect("withdrawn");
    assert_eq!(withdrawn.state, NewChangeProposalState::Withdrawn);
}

// NCP-FR-EZTC / AGC-FR-JNOS: the read of a turn lists the proposals oldest
// first and the files of the pending one.
#[test]
fn the_turn_read_lists_the_split() {
    let f = Fixture::new();
    assert!(read_new_change_proposals(&f.store, STREAM, CHANGE).is_none());
    let first = f.record_named("part-one", 1, 2).expect("recorded");
    f.decide(&first.id, false).expect("rejected");
    f.record_named("part-two", 1, 2).expect("recorded");
    let split = read_new_change_proposals(&f.store, STREAM, CHANGE).expect("split");
    assert_eq!(
        split.proposals.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["part-one", "part-two"]
    );
    assert_eq!(split.pending_files.len(), 2);
    assert!(!split.confirmed);
}

fn update_of(f: &Fixture, action: NewChangeUpdateAction) -> Result<NewChangeProposal, String> {
    update_new_change_proposal(
        &f.handle(),
        &f.store,
        &f.sites,
        NewChangeUpdate {
            stream_id: STREAM,
            change: CHANGE,
            action,
            rationale: "r",
            agent: &agent(),
            thread_id: &f.thread_id,
        },
    )
}

fn resolve(f: &Fixture) {
    let thread = f.thread();
    comments::set_resolution_to(
        &f.store,
        thread.log_ref(),
        &f.thread_id,
        true,
        &human(),
        "2026-01-02T00:00:00Z",
    )
    .expect("resolved");
}

// NCP-FR-RPGN / NCP-FR-SNMA: a resolved discussion refuses a recording and an
// update, and the update leaves both files of the pending proposal as they
// were.
#[test]
fn a_resolved_discussion_refuses_a_recording_and_an_update() {
    let f = Fixture::new();
    let record = f.record_named("part-one", 1, 2).expect("recorded");
    let dir = f.store.path().join(format!("changes/{STREAM}/{CHANGE}/new-changes"));
    let before_record = std::fs::read_to_string(dir.join(format!("{}.toml", record.id))).unwrap();
    let before_files = std::fs::read_to_string(dir.join(format!("{}.files", record.id))).unwrap();
    resolve(&f);
    assert_eq!(
        update_of(&f, NewChangeUpdateAction::Withdraw),
        Err(ERR_DISCUSSION_RESOLVED.to_string())
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(format!("{}.toml", record.id))).unwrap(),
        before_record
    );
    assert_eq!(
        std::fs::read_to_string(dir.join(format!("{}.files", record.id))).unwrap(),
        before_files
    );
    let fresh = Fixture::new();
    resolve(&fresh);
    assert_eq!(fresh.record_named("part-one", 1, 2), Err(ERR_DISCUSSION_RESOLVED.to_string()));
    assert!(read_new_change_proposals(&fresh.store, STREAM, CHANGE).is_none());
}

// NCP-FR-SNMA / NCP-FR-MQCS: after the split is confirmed, an update is
// refused as `split_confirmed`, not as `no_open_proposal`.
#[test]
fn an_update_after_a_confirmation_is_refused() {
    let f = Fixture::new();
    let first = f.record_named("part-one", 1, 2).expect("recorded");
    f.decide(&first.id, true).expect("accepted");
    confirm_split(&f.handle(), &f.store, STREAM, CHANGE, &human()).expect("confirmed");
    assert_eq!(
        update_of(&f, NewChangeUpdateAction::Withdraw),
        Err(ERR_SPLIT_CONFIRMED.to_string())
    );
}
