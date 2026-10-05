//! The last tests for `PDC-propose-draft-changes-tool.md`.
//!
//! One part of `mod.rs`, which holds the fixtures these all run against.

use super::*;
use super::second::{PDC_SEVERAL_LEAK_LOG, PDC_SEVERAL_REFUSAL_LOG};

// ---------------------------------------------------------------------------
// What `revises` can mean on the draft as it stands (PDC-FR-GMWR)
// ---------------------------------------------------------------------------

/// One well-formed change, for a call that is about something else.
fn one_change() -> Vec<HunkArg> {
    vec![HunkArg {
        kind: Some("replace".into()),
        before: Some(ORIGINAL.into()),
        after: Some(PROPOSED.into()),
        after_text: None,
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
            note: None,
        })
        .collect()
}

/// Several well-formed changes, each naming text the prompt holds once, and
/// **given out of document order** so that a claim about a change keeping its
/// position says something: written in document order, argument order and
/// document order are the same list and neither can be told from the other.
fn three_changes() -> Vec<HunkArg> {
    let anchors = ["original", "# Spec", "The"];
    for anchor in anchors {
        assert_eq!(
            ORIGINAL.matches(anchor).count(),
            1,
            "the fixture prompt must hold {anchor:?} once, or these changes are refused as text naming more than one place",
        );
    }
    anchors
        .iter()
        .zip(["revised", "# Specification", "This"])
        .map(|(before, after)| HunkArg {
            kind: Some("replace".into()),
            before: Some((*before).into()),
            after: Some(after.into()),
            after_text: None,
            note: None,
        })
        .collect()
}

// PDC-FR-GMWR / TLC-FR-07: a draft holding no standing proposal holds no change
// a name could have meant, so the field is ignored and the call is the ordinary
// new proposal the model composed.
//
// This is the state a model cannot otherwise leave: a call naming a change never
// reaches the recording path, so refusing it would answer every call the model
// made with a refusal about a change that is not there, and the model could
// propose nothing at all.
#[test]
fn a_name_on_a_draft_holding_no_proposal_is_ignored_and_the_changes_are_recorded() {
    let f = Fixture::new();
    let out = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: Some("a-change-that-was-never-there".into()),
            hunks: three_changes(),
        })
        .expect("the field is ignored, not refused");
    assert!(out.proposed);
    assert_eq!(f.proposals().len(), 1, "the proposal was recorded");
    assert_eq!(f.hunks_of_the_one_proposal().len(), 3, "all three changes");
}

// PDC-FR-GMWR: a name that resolves to a change awaiting the author replaces
// that one change in place, keeping its id and its position.
#[test]
fn a_name_that_resolves_replaces_that_one_change_in_place() {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: three_changes(),
    })
    .expect("a proposal stands");
    let before = f.hunks_of_the_one_proposal();
    let second = before[1].id.clone();

    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: Some(second.clone()),
        hunks: vec![HunkArg {
            kind: Some("replace".into()),
            before: Some("The".into()),
            after: Some("That".into()),
            after_text: None,
            note: None,
        }],
    })
    .expect("the revision is taken");

    let after = f.hunks_of_the_one_proposal();
    assert_eq!(after.len(), before.len(), "a revision adds no change");
    // Its id and its place, which is what "in place" means. The changes are
    // given out of document order, so a revision whose new text sits elsewhere
    // in the prompt staying at this index is the position claim rather than a
    // coincidence of the two orders being one list.
    assert_eq!(after[1].id, second, "the change keeps its id and its place");
    assert_eq!(after[1].after_text(), "That");
    // And the changes either side of it are untouched — the same changes, not
    // merely the same number of them.
    for index in [0, 2] {
        assert_eq!(after[index].id, before[index].id);
        assert_eq!(after[index].before_text(), before[index].before_text());
        assert_eq!(after[index].after_text(), before[index].after_text());
        assert_eq!(after[index].revision, before[index].revision);
    }
}

// PDC-FR-GMWR: a name that resolves, carrying more than one change, is the
// refusal that names the correction — which of several changes replaces the one
// named is a question only the model can answer.
//
// This is the call a user's log named: `revising: true` beside eight changes.
#[test]
fn a_name_that_resolves_beside_several_changes_asks_for_the_one_that_replaces_it() {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: one_change(),
    })
    .expect("a proposal stands");
    let standing = f.hunks_of_the_one_proposal()[0].id.clone();

    let refused = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: Some(standing),
            hunks: three_changes(),
        })
        .unwrap_err();
    assert_eq!(refused.to_string(), PROPOSAL_REVISE_ONE);
    // The correction, and what to do with the changes it did not take.
    assert!(refused.to_string().contains("only the one change"));
    assert!(refused.to_string().contains("Wait for the author"));
}

// PDC-FR-GMWR: a name that does not resolve while a proposal stands is the
// no-such-change refusal, whatever the call carries beside it — the model named
// a change wrongly, and being asked to send one change instead would ask it to
// correct the thing that is not wrong.
#[test]
fn a_name_that_does_not_resolve_while_a_proposal_stands_names_the_change_as_the_fault() {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: one_change(),
    })
    .expect("a proposal stands");

    for hunks in [one_change(), three_changes()] {
        let refused = f
            .call(ProposeDraftChangesArgs {
                path: FILE.into(),
                rationale: RATIONALE.into(),
                revises: Some("a-change-that-was-never-there".into()),
                hunks,
            })
            .unwrap_err();
        assert_eq!(refused.to_string(), PROPOSAL_NO_SUCH_CHANGE);
    }
}

// PDC-FR-GMWR / TLC-FR-07: a blank name is no name at all, so the call is an
// ordinary new proposal rather than a revision of nothing.
#[test]
fn a_blank_name_is_no_name_at_all() {
    let f = Fixture::new();
    let out = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: Some("   ".into()),
            hunks: one_change(),
        })
        .expect("a blank name is ignored");
    assert!(out.proposed);
    // A revision answers `proposed` too, so the flag alone tells the two
    // branches apart no better than the name does.
    assert_eq!(f.proposals().len(), 1, "it recorded rather than revised");
    assert_eq!(f.hunks_of_the_one_proposal().len(), 1);
}

// PDC-FR-02: the model is told the one-change rule **before** it composes, in
// the description it reads and in the field's own description. A rule that only
// a refusal states is one the model breaks first and reads afterwards.
#[test]
fn the_one_change_rule_is_stated_where_the_model_reads_it() {
    let f = Fixture::new();
    let tool = f.tool();
    let schema = tool.parameters();
    let description = tool.description();
    assert!(
        description.contains("`revises`"),
        "the description names the field: {description}",
    );
    for text in [
        description.clone(),
        schema["properties"]["revises"]["description"]
            .as_str()
            .expect("description")
            .to_string(),
    ] {
        assert!(
            text.contains("one change"),
            "the text states the one-change rule: {text}",
        );
    }
}

// PDC-FR-GMWR / PDC-FR-TZKQ: a revision is placed against the prompt as a
// recording is, so a replacement naming text the prompt does not hold reads the
// refusal for **that** mistake. Reported as a name that resolved to nothing, it
// would send the model looking for a change it had named correctly.
#[test]
fn a_revision_naming_text_the_prompt_does_not_hold_is_told_that_and_not_that_the_change_is_gone() {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: one_change(),
    })
    .expect("a proposal stands");
    let standing = f.hunks_of_the_one_proposal()[0].id.clone();

    let refused = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: Some(standing),
            hunks: vec![HunkArg {
                kind: Some("replace".into()),
                before: Some("words the prompt has never held".into()),
                after: Some("something else".into()),
                after_text: None,
                note: None,
            }],
        })
        .unwrap_err();
    assert_eq!(refused.to_string(), PROPOSAL_TEXT_NOT_FOUND);
}

/// Put one change of the standing proposal into a state, without going through
/// the surface that usually does.
fn set_state(f: &Fixture, hunk_id: &str, state: crate::draft_proposals::hunks::HunkState) {
    let proposal = f.proposals()[0].id.clone();
    crate::draft_proposals::set_hunk_state(&f.root(), &f.draft_id, &proposal, hunk_id, state)
        .expect("the state is set");
}

// PDC-FR-GMWR: a change the author held for discussion is the case this field
// exists for — it is how the agent answers about the change it was asked to
// reconsider — and a change held for discussion is undecided.
#[test]
fn a_change_the_author_held_for_discussion_is_revisable() {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: three_changes(),
    })
    .expect("a proposal stands");
    let held = f.hunks_of_the_one_proposal()[1].id.clone();
    set_state(&f, &held, crate::draft_proposals::hunks::HunkState::Discussing);

    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: Some(held.clone()),
        hunks: vec![HunkArg {
            kind: Some("replace".into()),
            before: Some("The".into()),
            after: Some("That".into()),
            after_text: None,
            note: None,
        }],
    })
    .expect("the change held for discussion is revised");

    let after = f.hunks_of_the_one_proposal();
    assert_eq!(after[1].id, held, "it keeps its id and its place");
    assert_eq!(after[1].after_text(), "That");
}

// PDC-FR-GMWR: a change already accepted or rejected, while its siblings are
// still undecided, is not one the author is waiting on. The proposal still
// stands, so this is a name the model can correct.
#[test]
fn a_change_already_decided_is_not_one_the_author_awaits() {
    for state in [
        crate::draft_proposals::hunks::HunkState::Accepted,
        crate::draft_proposals::hunks::HunkState::Rejected,
    ] {
        let f = Fixture::new();
        f.call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: None,
            hunks: three_changes(),
        })
        .expect("a proposal stands");
        let settled = f.hunks_of_the_one_proposal()[0].id.clone();
        set_state(&f, &settled, state);

        let refused = f
            .call(ProposeDraftChangesArgs {
                path: FILE.into(),
                rationale: RATIONALE.into(),
                revises: Some(settled),
                hunks: one_change(),
            })
            .unwrap_err();
        assert_eq!(refused.to_string(), PROPOSAL_NO_SUCH_CHANGE, "{state:?}");
    }
}

// PDC-FR-GMWR / PDC-FR-TZKQ: a revision whose text the prompt holds twice reads
// the refusal for **that** mistake. A revision is placed exactly as a recording
// is, so it must be refused in the same words: told the text was not found, the
// model would copy it again rather than copy more of the words around it.
#[test]
fn a_revision_naming_text_the_prompt_holds_twice_is_asked_for_more_of_it() {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: one_change(),
    })
    .expect("a proposal stands");
    let standing = f.hunks_of_the_one_proposal()[0].id.clone();

    let refused = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: Some(standing),
            hunks: vec![HunkArg {
                kind: Some("replace".into()),
                // The prompt is `# Spec\n\nThe original.\n`, which holds this
                // twice inside `original` alone.
                before: Some("i".into()),
                after: Some("y".into()),
                after_text: None,
                note: None,
            }],
        })
        .unwrap_err();
    assert_eq!(refused.to_string(), PROPOSAL_TEXT_NOT_UNIQUE);
}

// PDC-FR-GMWR: a name that resolved when the proposal stood means nothing once
// every change of it is decided — the draft holds no standing proposal, so the
// call is the ordinary new proposal the model composed.
#[test]
fn a_real_name_on_a_proposal_since_fully_decided_is_ignored() {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: one_change(),
    })
    .expect("a proposal stands");
    let settled = f.hunks_of_the_one_proposal()[0].id.clone();
    set_state(
        &f,
        &settled,
        crate::draft_proposals::hunks::HunkState::Rejected,
    );

    let out = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: Some(settled),
            hunks: three_changes(),
        })
        .expect("the name is ignored and the changes are recorded");
    assert!(out.proposed);
    assert_eq!(f.proposals().len(), 2, "a second proposal was recorded");
}

// PDC-FR-07 / PDC-FR-GMWR: a call carrying no changes is asked for changes,
// whatever it names in `revises`. Asked instead to "send only the one change
// that replaces it", a model that sent none would have nothing to correct.
#[test]
fn a_name_beside_no_changes_at_all_is_asked_for_changes() {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: one_change(),
    })
    .expect("a proposal stands");
    let standing = f.hunks_of_the_one_proposal()[0].id.clone();

    let refused = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: Some(standing),
            hunks: vec![],
        })
        .unwrap_err();
    assert_eq!(refused.to_string(), PROPOSAL_NO_HUNKS);
}

// PDC-FR-GMWR: a name with space around it is the name. A model that padded it
// is naming the change it was asked about.
#[test]
fn a_name_with_space_around_it_still_names_the_change() {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: one_change(),
    })
    .expect("a proposal stands");
    let standing = f.hunks_of_the_one_proposal()[0].id.clone();

    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: Some(format!("  {standing}  ")),
        hunks: vec![HunkArg {
            kind: Some("replace".into()),
            before: Some("The".into()),
            after: Some("That".into()),
            after_text: None,
            note: None,
        }],
    })
    .expect("the padded name resolves");
    assert_eq!(f.proposals().len(), 1, "it revised rather than recorded");
    assert_eq!(f.hunks_of_the_one_proposal()[0].id, standing);
}

// PDC-FR-GMWR: a revision replaces a change of the proposal already standing,
// so it records no proposal and appends no comment. The author reads it in the
// change itself, which is where they asked about it.
#[test]
fn a_revision_records_no_proposal_and_appends_no_comment() {
    let f = Fixture::new();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: None,
        hunks: one_change(),
    })
    .expect("a proposal stands");
    let standing = f.hunks_of_the_one_proposal()[0].id.clone();
    let comments = f.discussion().comments.len();

    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: Some(standing),
        hunks: vec![HunkArg {
            kind: Some("replace".into()),
            before: Some("The".into()),
            after: Some("That".into()),
            after_text: None,
            note: None,
        }],
    })
    .expect("the revision is taken");

    assert_eq!(f.proposals().len(), 1, "no second proposal");
    assert_eq!(f.discussion().comments.len(), comments, "no second comment");
}

// TLC-FR-07 / PDC-FR-GMWR: an ignored name leaves an **ordinary** call — the
// proposal is announced in the conversation as any other is, rather than
// recorded somewhere the author never sees it.
#[test]
fn a_call_whose_name_was_ignored_is_an_ordinary_proposal() {
    let f = Fixture::new();
    let before = f.discussion().comments.len();
    f.call(ProposeDraftChangesArgs {
        path: FILE.into(),
        rationale: RATIONALE.into(),
        revises: Some("a-change-that-was-never-there".into()),
        hunks: three_changes(),
    })
    .expect("the name is ignored");
    assert_eq!(
        f.discussion().comments.len(),
        before + 1,
        "the proposal was announced as any other is",
    );
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
        note: None,
    };

    PDC_SEVERAL_REFUSAL_LOG.clear();
    let refused = block_on(
        f.tool()
            .with_buffer(&PDC_SEVERAL_REFUSAL_LOG)
            .call(ProposeDraftChangesArgs {
                path: FILE.into(),
                rationale: RATIONALE.into(),
                revises: None,
                hunks,
            }),
    )
    .unwrap_err();

    let message = refused.to_string();
    assert!(
        message.starts_with("Change 2 of 5: "),
        "the refusal names the change: {message}",
    );
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
    let _ = block_on(
        f.tool()
            .with_buffer(&PDC_SEVERAL_LEAK_LOG)
            .call(ProposeDraftChangesArgs {
                path: FILE.into(),
                rationale: RATIONALE.into(),
                revises: None,
                hunks: vec![
                    one_change().remove(0),
                    HunkArg {
                        kind: Some("replace".into()),
                        before: Some(secret.into()),
                        after: Some("x".into()),
                        after_text: None,
                        note: None,
                    },
                ],
            }),
    );
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
        note: None,
    };
    let refused = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: None,
            hunks,
        })
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        format!("Change 3 of 5: {PROPOSAL_TEXT_NOT_UNIQUE}"),
    );

    // And two changes covering the same text, which names the later of them.
    let f = Fixture::new();
    let refused = f
        .call(ProposeDraftChangesArgs {
            path: FILE.into(),
            rationale: RATIONALE.into(),
            revises: None,
            hunks: vec![
                HunkArg {
                    kind: Some("replace".into()),
                    before: Some("The original".into()),
                    after: Some("A rewritten".into()),
                    after_text: None,
                    note: None,
                },
                HunkArg {
                    kind: Some("replace".into()),
                    before: Some("original.".into()),
                    after: Some("revised.".into()),
                    after_text: None,
                    note: None,
                },
            ],
        })
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        format!("Change 2 of 2: {PROPOSAL_HUNKS_OVERLAP}"),
    );
}

// TLC-FR-10 / TLC-FR-11: a refusal that names a change is still an argument
// the model can correct on its next call, classified as one and marked as one.
// Listing the variant is not classifying it.
#[test]
fn a_refusal_that_names_a_change_is_a_retryable_argument_refusal() {
    let refusal = ToolRefusal::InvalidArgumentsAt {
        message: PROPOSAL_TEXT_NOT_FOUND,
        at: 2,
        of: 5,
    };
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
