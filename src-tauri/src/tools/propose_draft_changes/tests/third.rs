//! The last tests for `PDC-propose-draft-changes-tool.md`.
//!
//! One part of `mod.rs`, which holds the fixtures these all run against.

use super::second::{PDC_SEVERAL_LEAK_LOG, PDC_SEVERAL_REFUSAL_LOG};
use super::*;

// ---------------------------------------------------------------------------
// Several changes in one call (PDC-FR-TZKQ)
// ---------------------------------------------------------------------------

/// One well-formed change, for a call that is about something else.
fn one_change() -> Vec<HunkArg> {
    vec![HunkArg {
        kind: Some("replace".into()),
        before: Some(ORIGINAL.into()),
        after: Some(PROPOSED.into()),
        after_text: None,
        lead: None,
        trail: None,
        note: None,
    }]
}

/// Five well-formed changes, so a refusal about one of them has a place, a
/// count, and a total that are three different numbers — the only shape in
/// which "change 2 of 5" can be told from "change 5 of 5".
fn five_changes() -> Vec<HunkArg> {
    // The fixture prompt is short, so the five stand next to each other. They
    // are still five separate changes, which is all the place needs.
    let anchors = ["orig", "# ", "Spec", "The", "inal"];
    for anchor in anchors {
        assert_eq!(ORIGINAL.matches(anchor).count(), 1, "{anchor:?} occurs once");
    }
    anchors
        .iter()
        .zip(["revis", "## ", "Specification", "This", "ised"])
        .map(|(before, after)| HunkArg {
            kind: Some("replace".into()),
            before: Some((*before).into()),
            after: Some(after.into()),
            after_text: None,
            lead: None,
            trail: None,
            note: None,
        })
        .collect()
}

// PDC-FR-TZKQ / PDC-FR-18: a call carrying several changes and refused for one
// of them names **which**, as the model counted its own list.
//
// This is the call a user's log named: eight changes, every one of them naming
// text, refused as text not found. Told only that, the model can send the same
// eight again and no more — and does.
#[test]
fn a_refusal_about_one_of_several_changes_names_which_one() {
    let f = Fixture::new();
    let mut hunks = five_changes();
    // The **second** of five, so the place, the count, and the number of
    // changes sent are three different numbers. Refused at the last of three,
    // all three are `3` and a place reported as the count reads correctly.
    hunks[1] = HunkArg {
        kind: Some("replace".into()),
        before: Some("words the prompt has never held".into()),
        after: Some("something else".into()),
        after_text: None,
        lead: None,
        trail: None,
        note: None,
    };

    PDC_SEVERAL_REFUSAL_LOG.clear();
    let refused =
        block_on(f.tool().with_buffer(&PDC_SEVERAL_REFUSAL_LOG).call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            updates: None,
            hunks,
        }))
        .unwrap_err();

    let message = refused.to_string();
    assert!(message.starts_with("Change 2 of 5: "), "the refusal names the change: {message}",);
    assert!(message.contains(PROPOSAL_TEXT_NOT_FOUND), "{message}");

    // And the record says which, so a reader can tell a model that is one
    // change out from one that invented the whole set.
    let record = records(&PDC_SEVERAL_REFUSAL_LOG, Domain::Ai)
        .into_iter()
        .rev()
        .find(|r| r.message == "tool call refused")
        .expect("the refusal is recorded");
    assert_eq!(record.fields["refusal"], serde_json::json!("text_not_found"));
    assert_eq!(record.fields["refusalAt"], serde_json::json!(2));
    assert_eq!(record.fields["hunks"], serde_json::json!(5));
}

// PDC-FR-18: the place is a count of what the model sent, and the sentence
// around it is this application's — no part of what it wrote reaches a record.
#[test]
fn a_refusal_that_names_a_change_still_records_no_text() {
    let f = Fixture::new();
    let secret = "TEXT NOBODY SHOULD SEE IN A LOG";
    PDC_SEVERAL_LEAK_LOG.clear();
    let _ = block_on(f.tool().with_buffer(&PDC_SEVERAL_LEAK_LOG).call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        updates: None,
        hunks: vec![
            one_change().remove(0),
            HunkArg {
                kind: Some("replace".into()),
                before: Some(secret.into()),
                after: Some("x".into()),
                after_text: None,
                lead: None,
                trail: None,
                note: None,
            },
        ],
    }));
    let rendered = serde_json::to_string(
        &records(&PDC_SEVERAL_LEAK_LOG, Domain::Ai)
            .into_iter()
            .map(|r| (r.message, r.fields))
            .collect::<Vec<_>>(),
    )
    .expect("records render");
    assert!(!rendered.contains(secret), "a record carried the text");
    assert!(rendered.contains("refusalAt"), "and it did say which change");
}

// PDC-FR-TZKQ: the other two placement refusals carry the place on the same
// terms. Reached through the same mapping, and each is a different correction,
// so a caller told one for another fixes the wrong thing.
#[test]
fn every_placement_refusal_names_the_change_it_is_about() {
    // Text the prompt holds twice, so the change that names it names no one
    // place. The fixture's prompt holds `i` twice inside `original` alone.
    let f = Fixture::new();
    let mut hunks = five_changes();
    hunks[2] = HunkArg {
        kind: Some("replace".into()),
        before: Some("i".into()),
        after: Some("y".into()),
        after_text: None,
        lead: None,
        trail: None,
        note: None,
    };
    let refused = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            updates: None,
            hunks,
        })
        .unwrap_err();
    assert_eq!(refused.to_string(), format!("Change 3 of 5: {PROPOSAL_TEXT_NOT_UNIQUE}"),);

    // And two changes covering the same text, which names the later of them.
    let f = Fixture::new();
    let refused = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            updates: None,
            hunks: vec![
                HunkArg {
                    kind: Some("replace".into()),
                    before: Some("The original".into()),
                    after: Some("A rewritten".into()),
                    after_text: None,
                    lead: None,
                    trail: None,
                    note: None,
                },
                HunkArg {
                    kind: Some("replace".into()),
                    before: Some("original.".into()),
                    after: Some("revised.".into()),
                    after_text: None,
                    lead: None,
                    trail: None,
                    note: None,
                },
            ],
        })
        .unwrap_err();
    assert_eq!(refused.to_string(), format!("Change 2 of 2: {PROPOSAL_HUNKS_OVERLAP}"),);
}

// TLC-FR-10 / TLC-FR-11: a refusal that names a change is still an argument
// the model can correct on its next call, classified as one and marked as one.
// Listing the variant is not classifying it.
#[test]
fn a_refusal_that_names_a_change_is_a_retryable_argument_refusal() {
    let refusal =
        ToolRefusal::InvalidArgumentsAt { message: PROPOSAL_TEXT_NOT_FOUND, at: 2, of: 5 };
    assert_eq!(refusal.reason(), "invalid_arguments");
    assert!(refusal.retryable());
    assert_eq!(refusal_code(&refusal), Some("text_not_found"));
}

// ---------------------------------------------------------------------------
// The kind a change carries (PDC-FR-VKMR)
// ---------------------------------------------------------------------------

fn named(kind: Option<&str>, before: Option<&str>, after: Option<&str>) -> HunkArg {
    HunkArg {
        kind: kind.map(str::to_string),
        before: before.map(str::to_string),
        after: after.map(str::to_string),
        after_text: Some("the text it follows".into()),
        lead: None,
        trail: None,
        note: None,
    }
}

/// Every name a model can put on one change, including none and a wrong one.
const NAMES: [Option<&str>; 5] = [None, Some("add"), Some("del"), Some("replace"), Some("")];

// PDC-FR-VKMR: a change carrying both texts is a replacement, whatever the
// model called it. This is the case that made a rewrite draw as an insertion
// and leave the old text in the prompt.
#[test]
fn a_change_carrying_both_texts_is_a_replacement() {
    use crate::draft_proposals::anchors::HunkKind;
    for name in NAMES {
        let kind = named(name, Some("old"), Some("new")).to_proposed().kind;
        assert_eq!(kind, HunkKind::Replace, "named {name:?}");
    }
}

// PDC-FR-VKMR: a change carrying the old text alone is a deletion, whatever the
// model called it.
#[test]
fn a_change_carrying_the_old_text_alone_is_a_deletion() {
    use crate::draft_proposals::anchors::HunkKind;
    for name in NAMES {
        let kind = named(name, Some("old"), None).to_proposed().kind;
        assert_eq!(kind, HunkKind::Del, "named {name:?}");
    }
}

// PDC-FR-VKMR: a change carrying the new text alone is an insertion, whatever
// the model called it. An empty `before` is no text at all.
#[test]
fn a_change_carrying_the_new_text_alone_is_an_insertion() {
    use crate::draft_proposals::anchors::HunkKind;
    for name in NAMES {
        let kind = named(name, None, Some("new")).to_proposed().kind;
        assert_eq!(kind, HunkKind::Add, "named {name:?}");
        let kind = named(name, Some(""), Some("new")).to_proposed().kind;
        assert_eq!(kind, HunkKind::Add, "named {name:?} with an empty before");
    }
}

// PDC-FR-VKMR: a change carrying neither text names nothing to do. It is read
// as a replacement, which the placement then refuses on the `before` it does
// not hold. Read as an insertion it would be placed on its `after_text` alone
// and recorded as a change with no text in it, which DCP-FR-HRQN forbids and
// which accepting writes nothing for.
#[test]
fn a_change_carrying_no_text_at_all_is_refused_rather_than_recorded() {
    use crate::draft_proposals::anchors::HunkKind;
    use crate::draft_proposals::hunks::{plan, PlanError};
    for name in NAMES {
        let empty = named(name, None, None).to_proposed();
        assert_eq!(empty.kind, HunkKind::Replace, "named {name:?}");
        assert_eq!(
            plan("the text it follows and more", &[empty], || "id".to_string()),
            Err(PlanError::AnchorLost(1)),
            "named {name:?}",
        );
    }
}

// PDC-FR-02 / PDC-FR-VKMR: the rule reaches the caller as well as the record —
// the parameter description tells the model that its fields decide.
#[test]
fn the_kind_description_says_the_fields_decide() {
    assert!(KIND_DESCRIPTION.contains("The fields you send are what decide"));
}

// PDC-FR-ZQOH, CVL-FR-UCTB
#[test]
fn a_rationale_with_no_spoken_part_is_refused_and_records_nothing() {
    let f = Fixture::new();
    let before = f.discussion().comments.len();

    let refusal = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: "The opening buries the point.".into(),
            updates: None,
            hunks: one_change(),
        })
        .expect_err("refused");

    assert_eq!(refusal, ToolRefusal::InvalidArguments(RATIONALE_NO_SPOKEN_PART));
    assert!(refusal.retryable(), "the model can add the tag");
    assert!(f.proposals().is_empty());
    assert_eq!(f.discussion().comments.len(), before);
}

// CVL-FR-HREC: the model is told the rule in the text it reads.
#[test]
fn the_description_states_the_rule_of_the_spoken_part() {
    assert!(DESCRIPTION.contains("one `<spoken>…</spoken>` tag in `rationale`"));
    assert!(DESCRIPTION.contains("A rationale with no tag is refused."));
    assert!(RATIONALE_DESCRIPTION.contains("`<spoken>…</spoken>`"));
}

// PDC-FR-ZQOH, PDC-FR-18: the refusal is found by its code, with no text in it.
#[test]
fn a_refusal_for_no_spoken_part_logs_its_code_and_no_text() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    block_on(f.tool().with_buffer(&BUFFER).call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: "Zanzibarreason.".into(),
        updates: None,
        hunks: one_change(),
    }))
    .expect_err("refused");
    let records = records(&BUFFER, Domain::Ai);
    let warn = records.iter().find(|r| r.level == LogLevel::Warn).expect("the refusal");
    let fields = serde_json::to_value(&warn.fields).expect("fields");
    assert_eq!(fields["refusal"], "no_spoken_part");
    assert!(!fields.to_string().contains("Zanzibarreason"), "{fields}");
}
