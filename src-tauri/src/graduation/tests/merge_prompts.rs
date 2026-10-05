//! What the two merge instructions say (`GRL-graduation-loop.md` GRL-FR-GADT,
//! GRL-FR-SLQF, GRL-FR-MRVK, GRL-FR-YIJG, GRL-FR-TXEB).
//!
//! These read the prompt files themselves. The failure they catch is silent: an
//! instruction that names a field the turn is not given, or that leaves out the
//! rule that makes a merge review a merge review, still gets an answer.

use super::*;

static MERGE_WORK: &str = include_str!("../../../../resources/prompts/graduation/merge-work.md");
static MERGE_REVIEW: &str = include_str!("../../../../resources/prompts/graduation/merge-review.md");

/// Every `input.merge.<name>` a document names.
fn context_fields_named(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (index, _) in text.match_indices("input.merge.") {
        let rest = &text[index + "input.merge.".len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() {
            out.push(name);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The fields of the merge context one part is given.
fn context_fields(part: &str) -> Vec<String> {
    let run = super::merge_input::bare_merge_run();
    let input = driver::compose_input(&run, part, driver::PURPOSE_GENERATE);
    input
        .to_map()
        .get("merge")
        .and_then(|value| value.as_object())
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

fn instruction(source: &str) -> String {
    crate::prompts::instruction_text(source)
}

// GRL-FR-SLQF, GRL-FR-MRVK, GRL-FR-TXEB: every field of the merge context an
// instruction names is one its turn is given, and the review is the one turn
// that is told of `reconciled_paths`.
#[test]
fn every_merge_context_field_a_merge_prompt_names_is_given_to_its_turn() {
    let work = context_fields(driver::PART_WORK);
    let review = context_fields(driver::PART_REVIEW);

    for (name, source, carried) in [
        ("merge-work.md", MERGE_WORK, &work),
        ("merge-review.md", MERGE_REVIEW, &review),
    ] {
        let named = context_fields_named(source);
        assert!(!named.is_empty(), "{name} names no merge context field");
        for field in named {
            assert!(
                carried.iter().any(|known| known == &field),
                "{name} names `input.merge.{field}`, which its turn is not given: {carried:?}"
            );
        }
    }
    assert!(
        context_fields_named(MERGE_REVIEW).contains(&"reconciled_paths".to_string()),
        "the review reads the reconciled paths"
    );
    assert!(
        !context_fields_named(MERGE_WORK).contains(&"reconciled_paths".to_string()),
        "the work turn is not told of a field it is not given"
    );
}

// GRL-FR-TXEB: an instruction names every field of the context its turn is
// given, so a field the turn receives and no instruction reads is caught.
#[test]
fn every_merge_context_field_a_turn_is_given_is_named_by_its_prompt() {
    for (name, source, part) in [
        ("merge-work.md", MERGE_WORK, driver::PART_WORK),
        ("merge-review.md", MERGE_REVIEW, driver::PART_REVIEW),
    ] {
        let named = context_fields_named(source);
        for field in context_fields(part) {
            assert!(
                named.contains(&field),
                "{name} never tells its turn to read `input.merge.{field}`"
            );
        }
    }
}

// GRL-FR-GADT, GRL-FR-SLQF, GRL-FR-MRVK: the two instructions are distinct
// files, compiled in whole, and each is the same text for every merge run.
#[test]
fn the_two_merge_instructions_are_distinct_and_identical_for_every_run() {
    assert_ne!(MERGE_WORK, MERGE_REVIEW);
    assert_ne!(instruction(MERGE_WORK), instruction(MERGE_REVIEW));
    assert_eq!(*driver::MERGE_WORK_PROMPT, instruction(MERGE_WORK));
    assert_eq!(*driver::MERGE_REVIEW_PROMPT, instruction(MERGE_REVIEW));

    let first = super::merge_input::bare_merge_run();
    let mut second = super::merge_input::bare_merge_run();
    second.id = "gmerge2".into();
    second.stream_name = "another".into();
    second.merge.as_mut().unwrap().unresolved_paths = vec!["other.rs".into()];
    for part in [driver::PART_WORK, driver::PART_REVIEW] {
        assert_eq!(
            driver::instruction_for_run(&first, part),
            driver::instruction_for_run(&second, part),
            "{part}: the instruction does not depend on the run"
        );
    }
}

// GRL-FR-GADT, GRL-FR-YIJG: neither merge instruction carries an authoring
// comment or the name of a requirement into the text an agent reads.
#[test]
fn a_merge_instruction_holds_no_authoring_comment_and_no_requirement_name() {
    for (name, source) in [("merge-work.md", MERGE_WORK), ("merge-review.md", MERGE_REVIEW)] {
        let text = instruction(source);
        assert!(!text.contains("<!--") && !text.contains("-->"), "{name}");
        assert!(!text.contains("-FR-"), "{name}");
        assert!(source.contains("<!--"), "{name} documents where it was written from");
        assert!(text.trim().len() > 500, "{name} is a whole instruction");
    }
}

// GRL-FR-GADT: the merge instructions do not borrow the stream update's frame:
// no `/rebase` mount, no semantic reason and no unresolved questions.
#[test]
fn the_merge_instructions_do_not_use_the_update_frame() {
    for (name, source) in [("merge-work.md", MERGE_WORK), ("merge-review.md", MERGE_REVIEW)] {
        let text = instruction(source);
        for forbidden in ["/rebase", "semantic_reason", "unresolved_questions", "author_decisions"] {
            assert!(!text.contains(forbidden), "{name} names `{forbidden}`");
        }
    }
}

// GRL-FR-MRVK, GRB-FR-YCMH: the review runs the project's own build and test
// commands, names `task specs` and `task tests`, and invents none.
#[test]
fn the_merge_review_names_the_project_commands_and_invents_none() {
    let text = instruction(MERGE_REVIEW);
    assert!(text.contains("`task specs`"), "the review names task specs");
    assert!(text.contains("`task tests`"), "the review names task tests");
    assert!(text.contains("invent none"), "the review invents no command");
    let lower = text.to_lowercase();
    assert!(lower.contains("build and test commands"));
    assert!(
        lower.contains("never return `ready` over a merge whose required commands you could not run"),
        "a command that cannot run is never a pass"
    );
    assert!(MERGE_WORK.contains("invent none"), "the work turn invents none either");
}

// GRL-FR-MRVK, GRB-FR-YCMH, GRL-FR-UQNV: the review judges every changed path
// against both pinned versions and the specifications, the Git-clean paths
// included, and every `revise` is acted on whatever its severity.
#[test]
fn the_merge_review_judges_every_changed_path_against_both_branches() {
    let text = instruction(MERGE_REVIEW);
    for needed in [
        "input.merge.base_tip",
        "input.merge.stream_tip",
        "input.merge.changed_paths",
        "input.merge.reconciled_paths",
        "the paths Git merged cleanly included",
        "Specifications",
    ] {
        assert!(text.contains(needed), "the review does not say `{needed}`");
    }
    assert!(
        text.contains("every finding of a `revise` verdict is acted on, whatever its severity"),
        "no advisory-minor rule reaches a merge review"
    );
    assert!(text.contains("You return `ready` **only when**"));
}

// GRL-FR-MRVK: the review changes nothing of the project and works in a fresh
// checkout of its own.
#[test]
fn the_merge_review_changes_nothing_of_the_project() {
    let text = instruction(MERGE_REVIEW);
    assert!(text.contains("You **change none of it**"));
    assert!(text.contains("Edit, create and delete no file of the project"));
    assert!(text.contains("thrown away when it ends"));
}

// GRL-FR-SLQF, GRB-FR-LTVI: the work instruction makes the unresolved paths and
// the conflicts the task, points at both pinned versions and at the repository's
// own instructions, permits a change to any other path the merge needs, and
// requires that no conflict marker stand.
#[test]
fn the_merge_work_instruction_states_the_task_and_its_limits() {
    let text = instruction(MERGE_WORK);
    for needed in [
        "input.merge.unresolved_paths",
        "input.merge.conflicts",
        "input.merge.base_tip",
        "input.merge.stream_tip",
        "input.loop_instruction",
        "CLAUDE.md",
        "specifications",
    ] {
        assert!(text.contains(needed), "the work instruction does not name `{needed}`");
    }
    assert!(text.contains("**may change any other path**"));
    assert!(text.contains("A marker left in an unresolved path is a path you did not settle"));
    assert!(text.contains("**You do not commit.**"));
    assert!(text.contains("escalate_to_user"));
}
