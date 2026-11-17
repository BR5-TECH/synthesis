//! Tests for diagram hunks: finding Mermaid fences, and turning a diagram
//! change into fence text before it is placed (`DCP-draft-change-proposals.md`
//! DCP-FR-QRVD, DCP-FR-WSLE, DCP-FR-HXNP, DCP-FR-GMYT).

use super::anchors::HunkKind;
use super::fences::{comparable, find_fences, source_of};
use super::hunks::{apply_all, plan, ContextSide, PlanError, ProposedHunk};
use super::updates::{plan_update, Standing, UpdateEntry, UpdateError};

fn ids() -> impl FnMut() -> String {
    let mut n = 0;
    move || {
        n += 1;
        format!("h{n}")
    }
}

fn some(text: &str) -> Option<String> {
    Some(text.to_string())
}

fn change(before: &str, after: &str) -> ProposedHunk {
    ProposedHunk::diagram_fields(some(before), some(after), None, None, None, None)
}

fn change_in(before: &str, after: &str, lead: Option<&str>, trail: Option<&str>) -> ProposedHunk {
    ProposedHunk::diagram_fields(
        some(before),
        some(after),
        None,
        lead.map(str::to_string),
        trail.map(str::to_string),
        None,
    )
}

fn remove(before: &str) -> ProposedHunk {
    ProposedHunk::diagram_fields(some(before), None, None, None, None, None)
}

fn add_after(after_text: &str, after: &str) -> ProposedHunk {
    ProposedHunk::diagram_fields(None, some(after), some(after_text), None, None, None)
}

/// Plan `hunks` against `text` and apply them.
fn build(text: &str, hunks: &[ProposedHunk]) -> Result<String, PlanError> {
    let doc = plan(text, hunks, ids())?;
    Ok(apply_all(text, &doc).expect("a planned document applies"))
}

const PROMPT: &str = "# Title\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nEnd.\n";

// DCP-FR-QRVD: a diagram hunk is a hunk of the kinds a text hunk has, and the
// fields of a model give its kind.
#[test]
fn the_kind_of_a_diagram_hunk_is_read_from_its_fields() {
    assert_eq!(change("a", "b").kind, HunkKind::Replace);
    assert_eq!(remove("a").kind, HunkKind::Del);
    assert_eq!(add_after("x", "b").kind, HunkKind::Add);
    assert!(change("a", "b").diagram);
    assert!(!ProposedHunk::from_fields(some("a"), some("b"), None, None, None, None).diagram);
}

// DCP-FR-HXNP: a fence is a fenced block whose info string starts with
// `mermaid`, with the range of the whole fence and its source.
#[test]
fn a_mermaid_fence_is_found_with_its_range_and_source() {
    let fences = find_fences(PROMPT);
    assert_eq!(fences.len(), 1);
    let fence = &fences[0];
    assert_eq!(&PROMPT[fence.start..fence.end], "```mermaid\nflowchart TD\n  A --> B\n```");
    assert_eq!(fence.source, "flowchart TD\n  A --> B");
    assert_eq!(fence.opening, "```mermaid");
    assert_eq!(fence.closing, "```");
    assert!(fence.closed);
}

// DCP-FR-HXNP: a fence of another language is never found, and a Mermaid fence
// quoted inside it is not a fence.
#[test]
fn a_fence_of_another_language_is_never_found() {
    let text = "```text\nx\n```\n\n````md\n```mermaid\nA-->B\n```\n````\n\n```mermaidish\nx\n```\n";
    assert!(find_fences(text).is_empty());
    // An info string with a backtick is inline code and no fence.
    assert!(find_fences("```mermaid `x`\nA\n```\n").is_empty());
}

// DCP-FR-HXNP: a fence may open and close with tildes, a closing line may be
// longer than the opening line, and the info string may carry more words.
#[test]
fn tilde_fences_and_longer_closing_lines_are_fences() {
    let text = "~~~mermaid\nA-->B\n~~~~\n\n```Mermaid theme=dark\nC-->D\n`````\n";
    let fences = find_fences(text);
    let sources: Vec<&str> = fences.iter().map(|f| f.source.as_str()).collect();
    assert_eq!(sources, ["A-->B", "C-->D"]);
    // A shorter closing line does not close the fence.
    let open = find_fences("````mermaid\nA\n```\nB\n");
    assert_eq!(open.len(), 1);
    assert!(!open[0].closed);
    assert_eq!(open[0].source, "A\n```\nB");
}

// DCP-FR-HXNP: a fence that is not closed runs to the end of the text.
#[test]
fn an_unclosed_fence_runs_to_the_end_of_the_text() {
    let text = "Intro\n```mermaid\nA-->B\nB-->C\n";
    let fences = find_fences(text);
    assert_eq!(fences.len(), 1);
    assert_eq!(&text[fences[0].start..fences[0].end], "```mermaid\nA-->B\nB-->C");
    assert_eq!(fences[0].source, "A-->B\nB-->C");
    assert!(!fences[0].closed);
}

// DCP-FR-HXNP: the lines of an indented fence lose up to the indent of its
// opening line, and the text is normalised for line endings first.
#[test]
fn an_indented_fence_is_read_without_its_indent_and_crlf_is_normalised() {
    let text = "- item\r\n\r\n  ```mermaid\r\n  A-->B\r\n    C-->D\r\n  ```\r\n";
    let fences = find_fences(text);
    assert_eq!(fences.len(), 1);
    assert_eq!(fences[0].indent, "  ");
    assert_eq!(fences[0].source, "A-->B\n  C-->D");
    assert_eq!(source_of("\n\n  ```mermaid\n  A-->B\n  ```"), "A-->B");
    assert_eq!(source_of("no fence"), "no fence");
}

// DCP-FR-HXNP: sources are the same when only blank lines at the ends and white
// space at the end differ.
#[test]
fn sources_compare_without_blank_ends() {
    assert_eq!(comparable("\n\nA-->B  \n\n"), comparable("A-->B"));
    assert_ne!(comparable("A-->B"), comparable("A --> B"));
}

// DCP-FR-WSLE: a change keeps the opening and the closing line, and the whole
// of the rest of the prompt.
#[test]
fn a_change_replaces_the_source_between_the_same_marker_lines() {
    let out =
        build(PROMPT, &[change("flowchart TD\n  A --> B", "flowchart TD\n  A --> C\n")]).unwrap();
    assert_eq!(out, "# Title\n\n```mermaid\nflowchart TD\n  A --> C\n```\n\nEnd.\n");
}

// DCP-FR-WSLE, DCP-FR-QRVD: the record holds fence text, and it holds the flag.
#[test]
fn the_record_holds_fence_text_and_the_flag() {
    let doc =
        plan(PROMPT, &[change("flowchart TD\n  A --> B", "graph LR\n  X --> Y")], ids()).unwrap();
    let hunk = &doc.hunks[0];
    assert!(hunk.diagram);
    assert_eq!(hunk.kind, HunkKind::Replace);
    assert_eq!(hunk.before_text(), "```mermaid\nflowchart TD\n  A --> B\n```");
    assert_eq!(hunk.after_text(), "```mermaid\ngraph LR\n  X --> Y\n```");
    assert_eq!(hunk.readable_before(), "flowchart TD\n  A --> B");
    assert_eq!(hunk.readable_after(), "graph LR\n  X --> Y");
    assert_eq!((hunk.anchor.hint_start, hunk.anchor.hint_end), (9, 9 + hunk.before_text().len()));
}

// DCP-FR-WSLE: a change of a fence in a list takes the indent of its opening
// line.
#[test]
fn a_change_of_an_indented_fence_keeps_the_indent() {
    let text = "- step\n\n  ```mermaid\n  A-->B\n  ```\n- next\n";
    let out = build(text, &[change("A-->B", "A-->B\n\nB-->C")]).unwrap();
    assert_eq!(out, "- step\n\n  ```mermaid\n  A-->B\n\n  B-->C\n  ```\n- next\n");
}

// DCP-FR-WSLE: a tilde fence keeps its own marker lines.
#[test]
fn a_change_of_a_tilde_fence_keeps_its_marker_lines() {
    let out = build("~~~mermaid\nA-->B\n~~~~\n", &[change("A-->B", "A-->C")]).unwrap();
    assert_eq!(out, "~~~mermaid\nA-->C\n~~~~\n");
}

// DCP-FR-HXNP: line endings of the prompt and the source are normalised.
#[test]
fn a_prompt_with_crlf_line_endings_is_planned() {
    let text = PROMPT.replace('\n', "\r\n");
    let out =
        build(&text, &[change("flowchart TD\r\n  A --> B", "flowchart TD\r\n  A --> C")]).unwrap();
    assert_eq!(out, "# Title\n\n```mermaid\nflowchart TD\n  A --> C\n```\n\nEnd.\n");
}

// DCP-FR-WSLE: a removal takes the fence, the line break that ends it, and the
// blank line after it.
#[test]
fn a_removal_takes_the_fence_and_the_blank_line_after_it() {
    let out = build(PROMPT, &[remove("flowchart TD\n  A --> B")]).unwrap();
    assert_eq!(out, "# Title\n\nEnd.\n");
    let doc = plan(PROMPT, &[remove("flowchart TD\n  A --> B")], ids()).unwrap();
    assert_eq!(doc.hunks[0].kind, HunkKind::Del);
    assert_eq!(doc.hunks[0].after, None);
    assert!(doc.hunks[0].before_text().ends_with("```\n\n"));
}

// DCP-FR-WSLE: where no blank line follows, only the line break goes, and where
// nothing follows, nothing goes with the fence.
#[test]
fn a_removal_takes_only_the_breaks_that_exist() {
    let out = build("A\n```mermaid\nx\n```\nB\n", &[remove("x")]).unwrap();
    assert_eq!(out, "A\nB\n");
    let out = build("A\n\n```mermaid\nx\n```", &[remove("x")]).unwrap();
    assert_eq!(out, "A\n\n");
    let out = build("A\n\n```mermaid\nx\n```\n", &[remove("x")]).unwrap();
    assert_eq!(out, "A\n\n");
    let out = build("```mermaid\nx\n```\n\n\nB\n", &[remove("x")]).unwrap();
    assert_eq!(out, "\nB\n");
}

// DCP-FR-WSLE: an addition goes after the end of the line that holds the text
// it follows, as a blank line and a new fence.
#[test]
fn an_addition_goes_after_the_line_that_holds_the_text_it_follows() {
    let text = "# Title\nIntro line one.\nNext line.\n";
    let out = build(text, &[add_after("Intro", "flowchart TD\n  A --> B")]).unwrap();
    assert_eq!(
        out,
        "# Title\nIntro line one.\n\n```mermaid\nflowchart TD\n  A --> B\n```\nNext line.\n"
    );
    let doc = plan(text, &[add_after("Intro", "A-->B")], ids()).unwrap();
    assert_eq!(doc.hunks[0].kind, HunkKind::Add);
    assert_eq!(doc.hunks[0].before, None);
    assert!(doc.hunks[0].diagram);
    assert_eq!(doc.hunks[0].after_text(), "\n\n```mermaid\nA-->B\n```");
}

// DCP-FR-WSLE: an addition after the last line of a prompt without a final line
// break.
#[test]
fn an_addition_after_the_last_line_is_placed_at_the_end() {
    let out = build("Only line", &[add_after("Only line", "A-->B")]).unwrap();
    assert_eq!(out, "Only line\n\n```mermaid\nA-->B\n```");
}

// DCP-FR-WSLE: an empty prompt takes the new fence alone.
#[test]
fn an_addition_into_an_empty_prompt_is_the_fence_alone() {
    let hunk = ProposedHunk::diagram_fields(None, some("A-->B"), None, None, None, None);
    assert_eq!(build("", std::slice::from_ref(&hunk)).unwrap(), "```mermaid\nA-->B\n```");
    // A prompt that holds text and an addition that names no place is refused.
    assert_eq!(build("x\n", &[hunk]).unwrap_err(), PlanError::AnchorLost(1));
}

// DCP-FR-HXNP: `lead` and `trail` tell two fences with one source apart.
#[test]
fn lead_and_trail_tell_two_equal_fences_apart() {
    let text = "## One\n\n```mermaid\nA-->B\n```\n\n## Two\n\n```mermaid\nA-->B\n```\n";
    assert_eq!(build(text, &[change("A-->B", "X-->Y")]).unwrap_err(), PlanError::Ambiguous(1));
    let out = build(text, &[change_in("A-->B", "X-->Y", Some("## Two\n\n"), None)]).unwrap();
    assert_eq!(out, "## One\n\n```mermaid\nA-->B\n```\n\n## Two\n\n```mermaid\nX-->Y\n```\n");
    let out = build(text, &[change_in("A-->B", "X-->Y", None, Some("\n\n## Two"))]).unwrap();
    assert_eq!(out, "## One\n\n```mermaid\nX-->Y\n```\n\n## Two\n\n```mermaid\nA-->B\n```\n");
    // A removal of one of two equal fences leaves the other.
    let hunk =
        ProposedHunk::diagram_fields(some("A-->B"), None, None, some("## Two\n\n"), None, None);
    assert_eq!(build(text, &[hunk]).unwrap(), "## One\n\n```mermaid\nA-->B\n```\n\n## Two\n\n");
}

// DCP-FR-HXNP: a source no fence holds is lost, and a context no fence has is
// lost too.
#[test]
fn a_missing_source_or_context_is_an_anchor_lost() {
    assert_eq!(build(PROMPT, &[change("A --> Z", "x")]).unwrap_err(), PlanError::AnchorLost(1));
    assert_eq!(
        build(PROMPT, &[change_in("flowchart TD\n  A --> B", "x", Some("nope"), None)])
            .unwrap_err(),
        PlanError::AnchorLost(1)
    );
    assert_eq!(build(PROMPT, &[change("", "x")]).unwrap_err(), PlanError::AnchorLost(1));
}

// DCP-FR-HXNP: a non-Mermaid fence is never the fence a hunk names.
#[test]
fn a_fence_of_another_language_is_not_named() {
    let text = "```text\nA-->B\n```\n";
    assert_eq!(build(text, &[change("A-->B", "x")]).unwrap_err(), PlanError::AnchorLost(1));
}

// DCP-FR-HXNP: a `lead` or a `trail` over its bound is refused as it is for a
// text hunk.
#[test]
fn a_context_over_its_bound_is_refused() {
    let long = "x".repeat(201);
    let err = build(PROMPT, &[change_in("flowchart TD\n  A --> B", "y", Some(&long), None)]);
    assert_eq!(err.unwrap_err(), PlanError::ContextTooLong(1, ContextSide::Lead));
}

// DCP-FR-HXNP: a diagram hunk covers its whole fence, so a text hunk inside it
// overlaps, and a text hunk outside it does not.
#[test]
fn a_diagram_hunk_overlaps_a_text_hunk_that_touches_its_fence() {
    let text_hunk = |before: &str, after: &str| {
        ProposedHunk::from_fields(some(before), some(after), None, None, None, None)
    };
    let diagram = change("flowchart TD\n  A --> B", "flowchart TD\n  A --> C");
    assert_eq!(
        build(PROMPT, &[diagram.clone(), text_hunk("A --> B", "A --> Z")]).unwrap_err(),
        PlanError::Overlap(2)
    );
    let out = build(PROMPT, &[diagram, text_hunk("End.", "The end.")]).unwrap();
    assert_eq!(out, "# Title\n\n```mermaid\nflowchart TD\n  A --> C\n```\n\nThe end.\n");
}

// DCP-FR-HXNP: two hunks on one fence overlap.
#[test]
fn two_diagram_hunks_on_one_fence_overlap() {
    let first = change("flowchart TD\n  A --> B", "x");
    let second = remove("flowchart TD\n  A --> B");
    assert_eq!(build(PROMPT, &[first, second]).unwrap_err(), PlanError::Overlap(2));
}

// DCP-FR-HXNP: where the source does not change the update is a `no_change`,
// and blank ends and trailing white space do not count as a change.
#[test]
fn an_update_that_leaves_the_source_as_it_is_is_refused() {
    let doc = plan(PROMPT, &[change("flowchart TD\n  A --> B", "flowchart TD\n  A --> C")], ids())
        .unwrap();
    let live = |_: &str| Some(Standing::Live);
    let same = change("flowchart TD\n  A --> B", "flowchart TD\n  A --> B  \n\n");
    let entries = [UpdateEntry::Change { id: "h1".into(), hunk: same }];
    assert_eq!(
        plan_update(PROMPT, &doc, live, &entries, ids()).unwrap_err(),
        UpdateError::NoChange(1)
    );
}

// DCP-FR-GMYT: an update names the subject of its tool, in both directions.
#[test]
fn an_update_across_the_two_subjects_is_a_mismatch() {
    let text = ProposedHunk::from_fields(some("End."), some("Fin."), None, None, None, None);
    let diagram = change("flowchart TD\n  A --> B", "flowchart TD\n  A --> C");
    let doc = plan(PROMPT, &[diagram.clone(), text.clone()], ids()).unwrap();
    let live = |_: &str| Some(Standing::Live);
    let refused = |entries: &[UpdateEntry]| plan_update(PROMPT, &doc, live, entries, ids());

    let text_change = UpdateEntry::Change { id: "h1".into(), hunk: text.clone() };
    assert_eq!(refused(&[text_change]).unwrap_err(), UpdateError::SubjectMismatch(1));
    let diagram_change = UpdateEntry::Change { id: "h2".into(), hunk: diagram.clone() };
    assert_eq!(refused(&[diagram_change]).unwrap_err(), UpdateError::SubjectMismatch(1));
    let withdraw = |id: &str, diagram: bool| UpdateEntry::Withdraw { id: id.into(), diagram };
    assert_eq!(refused(&[withdraw("h1", false)]).unwrap_err(), UpdateError::SubjectMismatch(1));
    assert_eq!(refused(&[withdraw("h2", true)]).unwrap_err(), UpdateError::SubjectMismatch(1));

    // The same subject is updated, and the flag stays on the changed hunk.
    let ok = refused(&[
        withdraw("h2", false),
        UpdateEntry::Change {
            id: "h1".into(),
            hunk: change("flowchart TD\n  A --> B", "graph LR\n  Q --> R"),
        },
    ])
    .unwrap();
    assert!(ok.doc.find("h1").unwrap().diagram);
    assert_eq!(ok.doc.find("h1").unwrap().revision, 1);
    assert_eq!(ok.withdrawn, ["h2"]);
}

// DCP-FR-GMYT: an added diagram hunk is placed beside the hunks that stand.
#[test]
fn a_diagram_hunk_is_added_by_an_update() {
    let text = ProposedHunk::from_fields(some("End."), some("Fin."), None, None, None, None);
    let doc = plan(PROMPT, &[text], ids()).unwrap();
    let live = |_: &str| Some(Standing::Live);
    let entries = [UpdateEntry::Add(change("flowchart TD\n  A --> B", "graph LR\n  Q --> R"))];
    let mut n = 10;
    let mint = || {
        n += 1;
        format!("h{n}")
    };
    let planned = plan_update(PROMPT, &doc, live, &entries, mint).unwrap();
    assert_eq!(planned.added, ["h11"]);
    let added = planned.doc.find(&planned.added[0]).unwrap();
    assert!(added.diagram);
    assert_eq!(added.after_text(), "```mermaid\ngraph LR\n  Q --> R\n```");
}

// DCP-FR-QRVD: the flag is absent from a text hunk and present in a diagram
// hunk, and a document written without it reads as text hunks.
#[test]
fn the_flag_round_trips_through_toml_and_old_documents_read_as_text() {
    let doc = plan(
        PROMPT,
        &[
            change("flowchart TD\n  A --> B", "graph LR\n  Q --> R"),
            ProposedHunk::from_fields(some("End."), some("Fin."), None, None, None, None),
        ],
        ids(),
    )
    .unwrap();
    let toml = doc.to_toml();
    assert_eq!(toml.matches("diagram = true").count(), 1);
    assert!(!toml.contains("diagram = false"));
    let back = super::hunks::HunkDocument::from_toml(&toml).unwrap();
    assert_eq!(back, doc);
    assert!(back.hunks[0].diagram);
    assert!(!back.hunks[1].diagram);
    let old = "[[hunk]]\nid = \"h1\"\nkind = \"replace\"\nbefore = \"a\"\nafter = \"b\"\n";
    let read = super::hunks::HunkDocument::from_toml(old).unwrap();
    assert!(!read.hunks[0].diagram);
}
