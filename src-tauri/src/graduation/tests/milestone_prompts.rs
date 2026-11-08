//! The milestone block of the four graduation instructions
//! (`GRL-graduation-loop.md` GRL-FR-KWPT, GRL-FR-LCBJ, GRL-FR-PCRD, GRL-FR-BZMH, GRL-FR-DJPE, GRL-FR-NDRY,
//! GRL-FR-LCSB).

use super::*;

fn sources() -> [(&'static str, &'static str); 4] {
    [
        ("work.md", include_str!("../../../../resources/prompts/graduation/work.md")),
        ("review.md", include_str!("../../../../resources/prompts/graduation/review.md")),
        ("merge-work.md", include_str!("../../../../resources/prompts/graduation/merge-work.md")),
        (
            "merge-review.md",
            include_str!("../../../../resources/prompts/graduation/merge-review.md"),
        ),
    ]
}

fn instruction(source: &str) -> String {
    crate::prompts::instruction_text(source)
}

/// GRL-FR-KWPT, GRL-FR-LCBJ, GRL-FR-PCRD, GRL-FR-BZMH, GRL-FR-DJPE, GRL-FR-NDRY — each of the four
/// instructions carries the block: the first message, the move to the next milestone, the last
/// done marker, the format with its full example, the display-only statement, and no deliverable.
#[test]
fn each_of_the_four_instructions_carries_the_milestone_block() {
    for (name, source) in sources() {
        let text = instruction(source);
        let block = driver::prompts::prompt_section(&text, "Milestones")
            .unwrap_or_else(|| panic!("{name} has no milestone block"));
        for needed in [
            "SYNTHESIS_MILESTONE",
            r#"{"event":"plan","milestones":["#,
            r#"{"event":"start","index":1}"#,
            r#"{"event":"done","index":1}"#,
            r#"{"event":"start","index":2}"#,
            r#"{"event":"done","index":3}"#,
            "3 to 5",
            "Before any other work",
            "first text message",
            "the start marker of milestone 1",
            "move to the next milestone",
            "the done marker of the milestone you finished and the start marker of the next one",
            "before you start the work of the next milestone",
            "before you report",
            "Before each progress message",
            "A start marker marks every earlier milestone done",
            "an earlier call of this pass",
            "write the start marker of the milestone you are in",
            "on its own line",
            "final result document",
            "for display only",
            "ignores a marker it cannot read",
            "depends on them",
            "write every marker",
            "changes no state, no stage, and no verdict",
            "50 characters",
        ] {
            assert!(block.contains(needed), "{name} block lacks `{needed}`");
        }
        for deliverable in ["specification", "write a test", "implementation"] {
            assert!(!block.contains(deliverable), "{name} block names a deliverable");
        }
        assert!(!block.contains("never a failure"), "{name} block makes markers optional");
        assert!(
            !text.contains("<!--"),
            "{name}: authoring comments are removed before the agent reads the file"
        );
    }
}

/// GRL-FR-LCSB — the other instructions and `refusals.md` carry no block.
#[test]
fn the_other_instructions_carry_no_milestone_block() {
    for source in [
        include_str!("../../../../resources/prompts/graduation/rebase.md"),
        include_str!("../../../../resources/prompts/graduation/pr-summary.md"),
        include_str!("../../../../resources/prompts/graduation/refusals.md"),
    ] {
        assert!(!instruction(source).contains("SYNTHESIS_MILESTONE"));
    }
}

/// GRL-FR-LCSB — without the block, an instruction keeps every other section.
#[test]
fn an_instruction_without_its_block_keeps_every_other_section() {
    for (name, source) in sources() {
        let text = instruction(source);
        let stripped = driver::prompts::without_milestones(&text);
        assert!(!stripped.contains("SYNTHESIS_MILESTONE"), "{name}");
        assert!(!stripped.contains("## Milestones"), "{name}");
        let headings = |t: &str| -> Vec<String> {
            t.lines().filter(|l| l.starts_with("## ")).map(str::to_string).collect()
        };
        let mut expected = headings(&text);
        expected.retain(|h| h != "## Milestones");
        assert_eq!(headings(&stripped), expected, "{name}");
    }
    assert_eq!(driver::prompts::without_milestones("no block\n"), "no block\n");
    assert_eq!(driver::prompts::without_milestones("a\n\n## Milestones\n\nx\n"), "a\n");
}

/// GRL-FR-LCSB, GRL-FR-KWPT — a review turn carries the block, and the correction turn that asks
/// it again does not.
#[test]
fn a_correction_turn_carries_no_milestone_block() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work().writing("src/panel.ts", "1\n"),
        Turn::answering(Answer::RawResult(serde_json::json!({
            "verdict": "maybe",
            "rationale": "I am not sure."
        }))),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());
    assert_eq!(run.state, GraduationRunState::Completed);

    let work = dispatch.of_part("work");
    assert!(work[0].instruction.contains("SYNTHESIS_MILESTONE"));
    let review = dispatch.of_part("review");
    assert_eq!(review.len(), 2);
    assert!(review[0].instruction.contains("SYNTHESIS_MILESTONE"));
    assert!(!review[1].instruction.contains("SYNTHESIS_MILESTONE"));
    assert!(review[1].instruction.contains("## What you have been given"));
}
