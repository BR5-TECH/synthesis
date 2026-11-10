//! Updating the open proposal of a change (CEP-FR-AXEV).

use super::*;
use crate::text_hunks::updates::UpdateEntry;

fn update(f: &Fixture, updates: &[ChangeUpdateEntry]) -> Result<ChangeEditProposal, UpdateRefusal> {
    update_change_proposal(
        &f.handle(),
        &f.store,
        &f.sites,
        ChangeProposalUpdate {
            stream_id: STREAM,
            change: CHANGE,
            updates,
            rationale: "Tighter. <spoken>Tighter.</spoken>",
            agent: &agent(),
            thread_id: &f.thread_id,
        },
    )
}

// CEP-FR-AXEV / CMS-FR-BSQB: an update changes a hunk in place, adds a hunk of
// another file by its path, and withdraws one, and its comment carries the
// counts.
#[test]
fn an_update_changes_adds_and_withdraws_across_files() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    let spec = row_id(&record, "specs/auth/spec.md");
    let changed = replace(
        "tasks.md",
        "- [ ] 1.1 Add the form\n",
        "- [ ] 1.1 Add the form\n- [ ] 1.2 Log out\n",
    );
    let added = replace("proposal.md", "- Add login.", "- Add login and logout.");
    let updated = update(
        &f,
        &[
            ChangeUpdateEntry {
                entry: UpdateEntry::Change { id: tasks.clone(), hunk: changed.hunk },
                path: None,
            },
            ChangeUpdateEntry {
                entry: UpdateEntry::Add(added.hunk),
                path: Some("proposal.md".into()),
            },
            ChangeUpdateEntry { entry: UpdateEntry::Withdraw { id: spec.clone() }, path: None },
        ],
    )
    .expect("updated");
    assert_eq!(updated.row(&tasks).expect("row").revision, 1);
    assert_eq!(state_of(&updated, &spec), HunkState::Withdrawn);
    assert_eq!(updated.hunk_count, 3);
    assert_eq!(updated.ledger[2].path, "proposal.md");
    let last = f.thread().comments.last().cloned().expect("comment");
    assert_eq!(
        last.attachments,
        vec![comments::Attachment::ChangeProposal {
            proposal_id: record.id.clone(),
            stream_id: STREAM.into(),
            change: CHANGE.into(),
            update: Some(comments::ProposalUpdateCounts { changed: 1, added: 1, withdrawn: 1 }),
        }]
    );
}

// CEP-FR-AXEV: an added hunk names its path, inside the change, and an
// update with no open proposal is refused.
#[test]
fn an_update_needs_an_open_proposal_and_a_path_for_an_add() {
    let f = Fixture::new();
    let added = replace("proposal.md", "- Add login.", "- Add logout.");
    assert_eq!(
        update(
            &f,
            &[ChangeUpdateEntry {
                entry: UpdateEntry::Add(added.hunk.clone()),
                path: Some("proposal.md".into())
            }]
        ),
        Err(UpdateRefusal::NoOpenProposal)
    );
    f.pending();
    assert_eq!(
        update(
            &f,
            &[ChangeUpdateEntry { entry: UpdateEntry::Add(added.hunk.clone()), path: None }]
        ),
        Err(UpdateRefusal::PathMissing { at: 1, of: 1 })
    );
    assert_eq!(
        update(
            &f,
            &[ChangeUpdateEntry {
                entry: UpdateEntry::Add(added.hunk),
                path: Some("../x.md".into())
            }]
        ),
        Err(UpdateRefusal::PathOutsideChange { at: 1, of: 1 })
    );
    assert_eq!(
        update(
            &f,
            &[ChangeUpdateEntry { entry: UpdateEntry::Withdraw { id: "nope".into() }, path: None }]
        ),
        Err(UpdateRefusal::HunkNotFound { at: 1, of: 1 })
    );
}

// CEP-FR-AXEV (PCP-FR-RVSN): an accepted hunk can not be updated.
#[test]
fn an_accepted_hunk_is_not_updatable() {
    let f = Fixture::new();
    let record = f.pending();
    let tasks = row_id(&record, "tasks.md");
    f.accept(&record.id, &tasks).expect("accepted");
    assert_eq!(
        update(&f, &[ChangeUpdateEntry { entry: UpdateEntry::Withdraw { id: tasks }, path: None }]),
        Err(UpdateRefusal::HunkNotUpdatable { at: 1, of: 1 })
    );
}

// CEP-FR-SQCG: an update emits the event with the cause `updated`.
#[test]
fn an_update_emits_the_event() {
    let f = Fixture::new();
    let record = f.pending();
    let seen = f.watch();
    update(
        &f,
        &[ChangeUpdateEntry {
            entry: UpdateEntry::Withdraw { id: row_id(&record, "tasks.md") },
            path: None,
        }],
    )
    .expect("updated");
    assert!(seen.lock().unwrap().iter().any(|e| e["cause"] == "updated" && e["change"] == CHANGE));
}
