//! The words of the agent-facing prompts and built-in skills
//! (`PVC-product-vocabulary.md` PVC-FR-CUAT, `CVL-conversation-loop.md`
//! CVL-FR-WQPN).
//!
//! A model reads the instruction text of a file, so the checks read the same
//! text: the authoring comments are gone. Identifiers stay as they are
//! (PVC-FR-ODXM), so a word in backticks, a material tag, and a name with an
//! underscore are not words of the vocabulary.

use super::instruction_text;

/// Every prompt that the application sends to a model or an execution agent.
const PROMPTS: &[(&str, &str)] = &[
    ("comment.md", include_str!("../../../resources/prompts/comment.md")),
    ("compact.md", include_str!("../../../resources/prompts/compact.md")),
    ("discuss-artifact.md", include_str!("../../../resources/prompts/discuss-artifact.md")),
    ("discuss-change.md", include_str!("../../../resources/prompts/discuss-change.md")),
    ("discuss-draft.md", include_str!("../../../resources/prompts/discuss-draft.md")),
    ("discuss-map.md", include_str!("../../../resources/prompts/discuss-map.md")),
    ("discuss-note.md", include_str!("../../../resources/prompts/discuss-note.md")),
    ("group-advisor.md", include_str!("../../../resources/prompts/group-advisor.md")),
    ("group-classifier.md", include_str!("../../../resources/prompts/group-classifier.md")),
    ("group-meta.md", include_str!("../../../resources/prompts/group-meta.md")),
    ("group-synthesis.md", include_str!("../../../resources/prompts/group-synthesis.md")),
    ("translate-transcript.md", include_str!("../../../resources/prompts/translate-transcript.md")),
    (
        "voice-progress-update.md",
        include_str!("../../../resources/prompts/voice-progress-update.md"),
    ),
    (
        "agent-exec/response-contract.md",
        include_str!("../../../resources/prompts/agent-exec/response-contract.md"),
    ),
    ("graduation/work.md", include_str!("../../../resources/prompts/graduation/work.md")),
    ("graduation/review.md", include_str!("../../../resources/prompts/graduation/review.md")),
    ("graduation/rebase.md", include_str!("../../../resources/prompts/graduation/rebase.md")),
    ("graduation/refusals.md", include_str!("../../../resources/prompts/graduation/refusals.md")),
    (
        "graduation/merge-work.md",
        include_str!("../../../resources/prompts/graduation/merge-work.md"),
    ),
    (
        "graduation/merge-review.md",
        include_str!("../../../resources/prompts/graduation/merge-review.md"),
    ),
    (
        "graduation/pr-summary.md",
        include_str!("../../../resources/prompts/graduation/pr-summary.md"),
    ),
    (
        "graduation/openspec-propose.md",
        include_str!("../../../resources/prompts/graduation/openspec-propose.md"),
    ),
    (
        "graduation/openspec-apply.md",
        include_str!("../../../resources/prompts/graduation/openspec-apply.md"),
    ),
    (
        "graduation/openspec-rework.md",
        include_str!("../../../resources/prompts/graduation/openspec-rework.md"),
    ),
];

/// The material forms that hold an internal name (PVC-FR-ODXM). A builder
/// writes the name of a change in the `draft` attribute of an `<artifact>`
/// section. No builder emits a `<draft>` tag, so the check does not skip one.
const TAGS: &[&str] = &["<artifact draft="];

/// `text` without its fence lines. The text of a fenced block is an example
/// that a model copies, so it is prose. Only the fence markers go.
fn unfenced(text: &str) -> String {
    text.lines().filter(|line| !line.trim_start().starts_with("```")).collect::<Vec<_>>().join("\n")
}

/// The words of `text` that use an internal name of PVC-FR-HMJD or
/// PVC-FR-ZQRE. A GitHub draft pull request keeps its GitHub name
/// (PVC-FR-UFTN).
pub(crate) fn internal_words(text: &str) -> Vec<String> {
    let mut prose = String::new();
    // Inline code holds identifiers: tool names, fields, and values.
    for (index, part) in unfenced(text).split('`').enumerate() {
        if index % 2 == 0 {
            prose.push_str(part);
            prose.push(' ');
        }
    }
    let mut prose = prose.to_lowercase();
    for tag in TAGS {
        prose = prose.replace(tag, " ");
    }
    prose = prose.replace("draft pull request", " ");
    prose
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|word| !word.contains('_'))
        .filter(|word| word.starts_with("draft") || word.contains("graduat"))
        .map(str::to_string)
        .collect()
}

/// The description of a tool and every `description` of its parameter schema,
/// at any depth. A model reads each of them (PVC-FR-CUAT).
pub(crate) fn definition_texts(definition: &rig::completion::ToolDefinition) -> Vec<String> {
    fn walk(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    match child.as_str() {
                        Some(text) if key == "description" => out.push(text.to_string()),
                        _ => walk(child, out),
                    }
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut texts = vec![definition.description.clone()];
    walk(&definition.parameters, &mut texts);
    texts
}

/// The built-in skill files that state how an agent works. The reference files
/// of `a2ui` tell an agent how to write a panel, so the check reads them too.
/// The reference files of `calm` and `mermaid` hold example content, not
/// product words.
fn skill_files() -> Vec<(&'static str, &'static str)> {
    crate::skills::builtin::files()
        .iter()
        .copied()
        .filter(|(path, _)| {
            path.ends_with("/SKILL.md")
                || path.starts_with("roadmap/")
                || path.starts_with("a2ui/references/")
        })
        .collect()
}

// PVC-FR-CUAT, CVL-FR-WQPN: no prompt and no built-in skill calls a change a
// "draft", or an implementation a "graduation", outside its authoring comments
// and its identifiers.
#[test]
fn no_prompt_and_no_skill_uses_an_internal_name_of_the_vocabulary() {
    let skills = skill_files();
    for checked in ["roadmap/SKILL.md", "a2ui/references/cards.md"] {
        assert!(
            skills.iter().any(|(path, _)| *path == checked),
            "{checked} is checked: {:?}",
            skills.iter().map(|(path, _)| *path).collect::<Vec<_>>(),
        );
    }
    assert!(
        !skills.iter().any(|(path, _)| path.starts_with("calm/references/")
            || path.starts_with("mermaid/references/")),
        "the example references of calm and mermaid are not checked",
    );
    for (name, raw) in PROMPTS.iter().copied().chain(skills) {
        let text = instruction_text(raw);
        // An odd count of inline backticks makes the check read code as prose
        // and prose as code, from the unpaired backtick to the end of the file.
        let ticks = unfenced(&text).matches('`').count();
        assert!(ticks.is_multiple_of(2), "{name} holds {ticks} inline backticks, an odd count");
        let words = internal_words(&text);
        assert!(words.is_empty(), "{name} uses the internal words {words:?}");
    }
}

/// Every Markdown file below `dir` of `root`, by its path relative to the root
/// of `root`, with `/` between the parts.
fn markdown_below(root: &crate::fs::RootFs, dir: &str) -> Vec<String> {
    let mut found = Vec::new();
    for entry in root.list_dir(dir).expect("a readable prompts folder") {
        let path =
            if dir.is_empty() { entry.name.clone() } else { format!("{dir}/{}", entry.name) };
        match entry.kind {
            crate::fs::EntryKind::Dir => found.extend(markdown_below(root, &path)),
            crate::fs::EntryKind::File if path.ends_with(".md") => found.push(path),
            _ => {}
        }
    }
    found
}

// PVC-FR-CUAT: the check reads every prompt under `resources/prompts/`. A
// prompt that is added and not listed would go unchecked.
#[test]
fn the_checked_prompts_are_every_prompt_file() {
    let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../resources/prompts");
    let root = crate::fs::RootFs::for_root(crate::changes::canonicalize_lenient(&base));
    let mut on_disk = markdown_below(&root, "");
    on_disk.sort();
    let mut listed: Vec<String> = PROMPTS.iter().map(|(name, _)| name.to_string()).collect();
    listed.sort();
    assert_eq!(listed, on_disk);
}

// PVC-FR-CUAT, PVC-FR-UFTN: the check finds an internal word in prose, and
// leaves a word in an identifier, a tag, an authoring comment, and a GitHub
// draft pull request.
#[test]
fn the_vocabulary_check_reads_prose_and_skips_identifiers() {
    assert_eq!(internal_words("Graduate the Draft."), vec!["graduate", "draft"]);
    assert_eq!(internal_words("Drafting goes on."), vec!["drafting"]);
    for clean in [
        "Read `read_draft` and `draft_id`.",
        "The `<draft>` tag is an identifier.",
        "The <artifact draft=\"Login\"> section holds a change.",
        "The field input.draft_name stays.",
        "Open a draft pull request.",
    ] {
        assert!(internal_words(clean).is_empty(), "{clean}");
    }
    // No builder emits a `<draft>` tag, so in prose it is an internal word.
    assert_eq!(internal_words("The <draft> tag holds a change."), vec!["draft"]);
    let commented = instruction_text("<!-- graduation notes -->\nA change.\n");
    assert!(internal_words(&commented).is_empty(), "{commented}");
}

// PVC-FR-CUAT: the text of a fenced block is prose that a model copies, so the
// check reads it. Its fence markers do not change how the text after the block
// is read, and an identifier in backticks inside it stays an identifier.
#[test]
fn the_vocabulary_check_reads_the_prose_of_a_fenced_block() {
    let text = "Answer like this:\n\n```json\n{ \"title\": \"Graduate the draft\" }\n```\n\nThen drafts.\n";
    assert_eq!(internal_words(text), vec!["graduate", "draft", "drafts"]);
    let fenced_code = "```\nCall `read_draft`.\n```\nA change.\n";
    assert!(internal_words(fenced_code).is_empty(), "{fenced_code}");
}

// CVL-FR-WQPN, CVL-FR-MZTK: the draft discussion prompt calls its material a
// change and each proposed hunk an edit, and says that in OpenSpec mode the
// implementation of a change first writes an OpenSpec change.
#[test]
fn the_change_discussion_prompt_names_a_change_its_edits_and_the_openspec_step() {
    let text = instruction_text(include_str!("../../../resources/prompts/discuss-draft.md"));
    for needle in [
        "You are in a discussion about a change.",
        "In OpenSpec mode, the implementation of a change first writes an OpenSpec change.",
        "## What is worth an edit",
        "### Edits to a Mermaid diagram",
        "offer all the edits of your plan one time",
        "passed to a dedicated agent during implementation",
    ] {
        assert!(text.contains(needle), "discuss-draft.md lacks {needle:?}");
    }
}

// PVC-FR-NEXL: the prompt of an OpenSpec change discussion also names a change
// of the Changes panel (the plan row that makes one), and the input of a
// propose, an apply, or a rework run is the prompt of a change. So none of
// these prompts calls the OpenSpec change a bare "change".
#[test]
fn the_openspec_change_prompt_never_calls_its_subject_a_bare_change() {
    let prompts = [
        ("discuss-change.md", include_str!("../../../resources/prompts/discuss-change.md")),
        (
            "graduation/openspec-propose.md",
            include_str!("../../../resources/prompts/graduation/openspec-propose.md"),
        ),
        (
            "graduation/openspec-apply.md",
            include_str!("../../../resources/prompts/graduation/openspec-apply.md"),
        ),
        (
            "graduation/openspec-rework.md",
            include_str!("../../../resources/prompts/graduation/openspec-rework.md"),
        ),
    ];
    for (name, raw) in prompts {
        let lower = instruction_text(raw).to_lowercase();
        assert!(lower.contains("openspec change"), "{name} names the OpenSpec change");
        for bare in ["the change", "this change", "a change action", "new changes", "a change that"]
        {
            assert!(!lower.contains(bare), "{name} says {bare:?}");
        }
    }
    let text = instruction_text(include_str!("../../../resources/prompts/discuss-change.md"));
    let lower = text.to_lowercase();
    assert!(lower.contains("no change is created"), "the prompt names a change of the panel");
    assert!(text.contains("## Splitting the OpenSpec change"));
    assert!(text.contains("## Applying or reworking the OpenSpec change"));
}
