//! The instructions this loop uses (GRL-FR-GADT).
//!
//! Every one is a file under `resources/prompts/graduation/`, compiled into the
//! binary at build time. No run reads one from disk, an author cannot edit or
//! replace one, and no operation accepts, returns or overrides one. **No
//! instruction text is composed in this module**: everything that varies per run
//! travels in the structured task input.

use std::sync::LazyLock;

/// GRL-FR-DXLU: the instruction every work turn carries.
static WORK_SOURCE: &str = include_str!("../../../../resources/prompts/graduation/work.md");
/// GRL-FR-OTRH: the instruction every review turn carries.
static REVIEW_SOURCE: &str = include_str!("../../../../resources/prompts/graduation/review.md");
/// GRB: the instruction the semantic turn of a stream update carries.
static REBASE_SOURCE: &str = include_str!("../../../../resources/prompts/graduation/rebase.md");
/// GRL-FR-SLQF: the instruction every merge work turn carries.
static MERGE_WORK_SOURCE: &str =
    include_str!("../../../../resources/prompts/graduation/merge-work.md");
/// GRL-FR-MRVK: the instruction every merge review turn carries.
static MERGE_REVIEW_SOURCE: &str =
    include_str!("../../../../resources/prompts/graduation/merge-review.md");
/// GRL-FR-TZKR: the instruction the one turn of a PR summary run carries.
static PR_SUMMARY_SOURCE: &str =
    include_str!("../../../../resources/prompts/graduation/pr-summary.md");
/// What an undispatchable tool call is answered with.
static REFUSALS_SOURCE: &str = include_str!("../../../../resources/prompts/graduation/refusals.md");
/// GRL-FR-GADT, GRL-FR-FYKL, GRL-FR-APHV: the two blocks of a propose run.
static OPENSPEC_PROPOSE_SOURCE: &str =
    include_str!("../../../../resources/prompts/graduation/openspec-propose.md");

/// GRL-FR-YIJG: a prompt file's instruction text is its content with every
/// authoring comment removed. The files carry comments naming the requirements
/// they satisfy; an agent never sees them.
pub static WORK_PROMPT: LazyLock<String> =
    LazyLock::new(|| crate::prompts::instruction_text(WORK_SOURCE));
pub static REVIEW_PROMPT: LazyLock<String> =
    LazyLock::new(|| crate::prompts::instruction_text(REVIEW_SOURCE));
pub static REBASE_PROMPT: LazyLock<String> =
    LazyLock::new(|| crate::prompts::instruction_text(REBASE_SOURCE));
pub static MERGE_WORK_PROMPT: LazyLock<String> =
    LazyLock::new(|| crate::prompts::instruction_text(MERGE_WORK_SOURCE));
pub static MERGE_REVIEW_PROMPT: LazyLock<String> =
    LazyLock::new(|| crate::prompts::instruction_text(MERGE_REVIEW_SOURCE));
pub static PR_SUMMARY_PROMPT: LazyLock<String> =
    LazyLock::new(|| crate::prompts::instruction_text(PR_SUMMARY_SOURCE));
pub static OPENSPEC_PROPOSE: LazyLock<String> =
    LazyLock::new(|| crate::prompts::instruction_text(OPENSPEC_PROPOSE_SOURCE));
pub static REFUSALS: LazyLock<String> =
    LazyLock::new(|| crate::prompts::instruction_text(REFUSALS_SOURCE));

/// GRL-FR-TVXI: the section of `refusals.md` a review that could not be read is
/// asked again with. It names what could not be read and what to send instead.
pub const REFUSAL_REVIEW_RESULT: &str = "review_result";

/// The instruction one part carries.
pub fn instruction_for(part: &str) -> &'static str {
    match part {
        super::phases::PART_REVIEW => &REVIEW_PROMPT,
        super::phases::PART_SEMANTIC_MERGE => &REBASE_PROMPT,
        _ => &WORK_PROMPT,
    }
}

/// GRL-GADT: the instruction one turn of one run carries.
///
/// A merge run never compiles `work.md`, `review.md` or `rebase.md`: its two
/// phases have instructions of their own.
pub fn instruction_for_run(run: &crate::graduation::GraduationRun, part: &str) -> &'static str {
    // GRL-FR-GADT: a PR summary run compiles `pr-summary.md` alone.
    if run.is_pr_summary() {
        return &PR_SUMMARY_PROMPT;
    }
    if run.is_merge() {
        // GRL-FR-GADT, GRL-FR-MRVK: the review part under either of its names.
        return match part {
            super::phases::PART_REVIEW | super::phases::PART_MERGE_REVIEW => &MERGE_REVIEW_PROMPT,
            _ => &MERGE_WORK_PROMPT,
        };
    }
    instruction_for(part)
}

/// The heading of the milestone block of GRL-FR-KWPT.
/// GRL-FR-DXLU, GRL-FR-FYKL, GRL-FR-APHV: the whole instruction of a turn. A
/// propose run's work and review turns carry their block after the standing
/// instruction, with the change name filled in.
pub fn instruction_text_for_run(run: &crate::graduation::GraduationRun, part: &str) -> String {
    let base = instruction_for_run(run, part).to_string();
    let Some(data) = run.openspec.as_ref() else {
        return base;
    };
    let key = match part {
        super::phases::PART_WORK => "Propose work",
        super::phases::PART_REVIEW => "Propose review",
        _ => return base,
    };
    let block = propose_block(key).replace("{{ change-name }}", &data.change);
    format!("{}\n\n{}\n", base.trim_end(), block)
}

/// One block of `openspec-propose.md`: the text below its `# ` heading line
/// and up to the next one.
fn propose_block(key: &str) -> String {
    let text = OPENSPEC_PROPOSE.as_str();
    let head = format!("## {key}\n");
    let Some(start) = text.find(&head).map(|at| at + head.len()) else {
        return String::new();
    };
    let rest = &text[start..];
    let end = ["## Propose work\n", "## Propose review\n"]
        .iter()
        .filter_map(|marker| rest.find(marker))
        .min()
        .unwrap_or(rest.len());
    rest[..end].trim().to_string()
}

const MILESTONES_HEADING: &str = "## Milestones";

/// GRL-FR-LCSB: an instruction without its milestone block, which is what a
/// correction turn carries. The block is the section from its heading to the
/// next `## ` heading, or to the end of the text.
pub fn without_milestones(instruction: &str) -> String {
    let Some(start) = instruction.find(MILESTONES_HEADING) else {
        return instruction.to_string();
    };
    let rest = &instruction[start + MILESTONES_HEADING.len()..];
    let end = rest.find("\n## ").map(|at| start + MILESTONES_HEADING.len() + at + 1);
    let mut kept = instruction[..start].to_string();
    if let Some(end) = end {
        kept.push_str(&instruction[end..]);
    } else {
        kept = kept.trim_end().to_string();
        kept.push('\n');
    }
    kept
}

/// The `## key` section of a document that holds several.
pub fn prompt_section(text: &str, key: &str) -> Option<String> {
    let head = format!("## {key}");
    let start = text.find(&head)? + head.len();
    let rest = &text[start..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}
