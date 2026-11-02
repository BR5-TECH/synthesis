//! Tests for the entry loop that `DDC-decide-draft-changes-tool.md` and
//! `DPC-decide-prompt-changes-tool.md` share, against a backend held in
//! memory. The stores themselves are exercised in the tests of each tool.

use std::cell::RefCell;

use super::*;

/// A proposal in memory, with a typed error to answer each decision with.
struct Fake {
    view: RefCell<ProposalView>,
    lost: Vec<&'static str>,
    fail_with: Option<&'static str>,
    calls: RefCell<Vec<String>>,
    discussions_read: RefCell<usize>,
}

impl Fake {
    fn new(hunks: &[(&str, HunkStateView)]) -> Fake {
        Fake {
            view: RefCell::new(ProposalView {
                id: "p1".into(),
                state: ProposalStateView::Pending,
                hunks: hunks.iter().map(|(id, state)| (id.to_string(), *state)).collect(),
            }),
            lost: Vec::new(),
            fail_with: None,
            calls: RefCell::new(Vec::new()),
            discussions_read: RefCell::new(0),
        }
    }

    fn set(&self, hunk_id: &str, state: HunkStateView) {
        let mut view = self.view.borrow_mut();
        for (id, s) in view.hunks.iter_mut() {
            if id == hunk_id {
                *s = state;
            }
        }
        let undecided = view
            .hunks
            .iter()
            .any(|(_, s)| matches!(s, HunkStateView::Pending | HunkStateView::Discussing));
        if !undecided {
            let accepted = view.hunks.iter().any(|(_, s)| *s == HunkStateView::Accepted);
            view.state =
                if accepted { ProposalStateView::Accepted } else { ProposalStateView::Rejected };
        }
    }

    fn act(&self, what: &str, hunk_id: &str) -> Result<(), String> {
        self.calls.borrow_mut().push(format!("{what}:{hunk_id}"));
        match self.fail_with {
            Some(error) => Err(error.to_string()),
            None => Ok(()),
        }
    }
}

impl ProposalBackend for Fake {
    fn file_word(&self) -> &'static str {
        "the file"
    }
    fn no_open_message(&self) -> &'static str {
        NO_OPEN_FILE_PROPOSAL
    }
    fn open_proposal(&self) -> Option<ProposalView> {
        let view = self.view.borrow().clone();
        (view.state == ProposalStateView::Pending).then_some(view)
    }
    fn proposal(&self, _: &str) -> Option<ProposalView> {
        Some(self.view.borrow().clone())
    }
    fn is_lost(&self, _: &str, hunk_id: &str) -> bool {
        self.lost.contains(&hunk_id)
    }
    fn accept(&self, _: &str, hunk_id: &str) -> Result<Accepted, String> {
        self.act("accept", hunk_id)?;
        self.set(hunk_id, HunkStateView::Accepted);
        Ok(Accepted { comment_owed: Some(false) })
    }
    fn reject(&self, _: &str, hunk_id: &str) -> Result<(), String> {
        self.act("reject", hunk_id)?;
        self.set(hunk_id, HunkStateView::Rejected);
        Ok(())
    }
    fn discussions(&self) -> Vec<Discussion> {
        *self.discussions_read.borrow_mut() += 1;
        Vec::new()
    }
    fn open_discussion(&self, _: &str, hunk_id: &str) -> Result<String, String> {
        self.act("discuss", hunk_id)?;
        Ok(format!("d-{hunk_id}"))
    }
    fn hold(&self, _: &str, hunk_id: &str) -> Result<(), String> {
        self.calls.borrow_mut().push(format!("hold:{hunk_id}"));
        self.set(hunk_id, HunkStateView::Discussing);
        Ok(())
    }
    fn retryable_error(&self, error: &str) -> bool {
        error == "write_failed"
    }
}

fn args(decisions: &[(&str, &str)]) -> DecideChangesArgs {
    DecideChangesArgs {
        decisions: Some(
            decisions
                .iter()
                .map(|(id, action)| DecisionArg {
                    id: Some(id.to_string()),
                    action: Some(action.to_string()),
                })
                .collect(),
        ),
    }
}

// DDC-FR-DSTU, DDC-FR-WSIA, DPC-FR-RGGP
#[test]
fn entries_run_one_at_a_time_in_call_order() {
    let fake = Fake::new(&[("a", HunkStateView::Pending), ("b", HunkStateView::Pending)]);
    let output =
        run_entries(&fake, &args(&[("b", "reject"), ("x", "accept"), ("a", "accept")])).unwrap();
    assert_eq!(*fake.calls.borrow(), vec!["reject:b", "accept:a"]);
    let places: Vec<usize> = output.decisions.iter().map(|row| row.place).collect();
    assert_eq!(places, vec![1, 2, 3]);
    assert_eq!(output.decisions[1].refusal.as_ref().unwrap().code, RefusalCode::UnknownHunk);
    assert!(output.closed);
    assert_eq!(output.proposal_state, ProposalStateView::Accepted);
}

// DDC-FR-ZGXF, DPC-FR-RGGP
#[test]
fn no_open_proposal_refuses_the_whole_call_and_names_no_entry() {
    let fake = Fake::new(&[("a", HunkStateView::Rejected)]);
    fake.view.borrow_mut().state = ProposalStateView::Rejected;
    let refused = run_entries(&fake, &args(&[("a", "accept")])).unwrap_err();
    assert_eq!(refused, CallRefusal::NoOpenProposal(NO_OPEN_FILE_PROPOSAL));
    assert!(fake.calls.borrow().is_empty());
}

// DDC-FR-LYTA, DDC-FR-SLBB
#[test]
fn a_backend_error_is_decision_failed_and_retryable_by_the_backend_list() {
    for (error, retryable) in [("write_failed", true), ("hunk_already_decided", false)] {
        let mut fake = Fake::new(&[("a", HunkStateView::Pending), ("b", HunkStateView::Pending)]);
        fake.fail_with = Some(error);
        let output = run_entries(&fake, &args(&[("a", "accept"), ("b", "reject")])).unwrap();
        for (index, row) in output.decisions.iter().enumerate() {
            let refusal = row.refusal.as_ref().expect("refused");
            assert_eq!(refusal.code, RefusalCode::DecisionFailed);
            assert_eq!(refusal.error.as_deref(), Some(error));
            assert_eq!(refusal.retryable, retryable);
            assert!(refusal.message.starts_with(&format!("Decision {} of 2: ", index + 1)));
            assert!(refusal.message.contains(error));
            assert_eq!(row.outcome, Outcome::Refused);
            assert_eq!(row.comment_owed, None);
        }
    }
}

// DDC-FR-JCSV — a lost hunk is refused before any action runs.
#[test]
fn a_lost_hunk_is_refused_for_each_action_before_the_backend_runs() {
    let mut fake = Fake::new(&[("a", HunkStateView::Rejected)]);
    fake.lost = vec!["a"];
    let output =
        run_entries(&fake, &args(&[("a", "accept"), ("a", "reject"), ("a", "discuss")])).unwrap();
    for row in &output.decisions {
        assert_eq!(row.refusal.as_ref().unwrap().code, RefusalCode::HunkLost);
        assert!(row.refusal.as_ref().unwrap().message.contains("no longer in the file"));
    }
    assert!(fake.calls.borrow().is_empty());
    // DDC non-functional: the discussions are read for a discuss alone, and a
    // refused one reads none.
    assert_eq!(*fake.discussions_read.borrow(), 0);
}

// DDC-FR-NKIA, DPC-FR-QPNB
#[test]
fn discuss_holds_a_pending_hunk_after_the_discussion_opens_and_no_other() {
    let fake = Fake::new(&[
        ("a", HunkStateView::Pending),
        ("b", HunkStateView::Rejected),
        ("c", HunkStateView::Discussing),
    ]);
    let output =
        run_entries(&fake, &args(&[("a", "discuss"), ("b", "discuss"), ("c", "discuss")])).unwrap();
    assert_eq!(
        *fake.calls.borrow(),
        vec!["discuss:a", "hold:a", "discuss:b", "discuss:c"],
        "the hold follows the opening, for a pending hunk alone",
    );
    assert_eq!(output.decisions[0].discussion_id.as_deref(), Some("d-a"));
    assert_eq!(output.decisions[1].hunk_state, Some(HunkStateView::Rejected));
}

// DDC-FR-WSIA, DPC-FR-UEXT
#[test]
fn the_output_serializes_as_the_contract_surface_names_it() {
    let fake = Fake::new(&[("a", HunkStateView::Pending), ("b", HunkStateView::Pending)]);
    let output = run_entries(&fake, &args(&[("a", "accept"), ("q", "reject")])).unwrap();
    let json = serde_json::to_value(&output).unwrap();
    assert_eq!(json["proposal_state"], "pending");
    assert_eq!(json["closed"], false);
    assert_eq!(json["decisions"][0]["outcome"], "accepted");
    assert_eq!(json["decisions"][0]["hunk_state"], "accepted");
    assert_eq!(json["decisions"][0]["comment_owed"], false);
    assert_eq!(json["decisions"][1]["refusal"]["code"], "unknown_hunk");
    assert!(json["decisions"][1].get("hunk_state").is_none());
    assert_eq!(json["decisions"][1]["refusal"]["retryable"], true);
}
