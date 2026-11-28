//! Tests for the open proposal that ends `discussion_history`
//! (`AGC-agent-conversations.md` AGC-FR-EPNB, AGC-FR-TCCV).
//!
//! Fixtures build files with `std::fs`, which `fs/access_tests.rs` allows in a
//! file named `tests.rs` alone.

use tempfile::TempDir;

use super::*;
use crate::comments::{Participant, Stamp, ThreadRef};
use crate::text_hunks::hunks::ProposedHunk;

const FILE: &str = "spec.md";

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: None,
        title: None,
    }
}

fn hunk(before: &str, after: &str) -> ProposedHunk {
    ProposedHunk::from_fields(Some(before.into()), Some(after.into()), None, None, None, None)
}

/// A draft whose prompt holds three lines, a discussion of it, and an open
/// proposal of three hunks. The second "before" text is long.
struct Draft {
    _dir: TempDir,
    root: crate::fs::RootFs,
    draft_id: String,
    thread_id: String,
    proposal_id: String,
    hunks: Vec<String>,
}

fn long_text() -> String {
    format!("Beta {} end.", "word ".repeat(80))
}

fn draft() -> Draft {
    let dir = TempDir::new().expect("tempdir");
    let root = crate::fs::RootFs::for_root(crate::changes::canonicalize_lenient(dir.path()));
    let created = crate::drafts::create_draft_at_root(&root, Some("spec")).expect("draft");
    let prompt = format!("# Spec\n\nAlpha line.\n\n{}\n\nGamma line.\n", long_text());
    crate::drafts::save_draft_file_impl(&root, &created.draft.id, FILE, &prompt).expect("seed");
    let thread = crate::comments::open_discussion_in(
        &root,
        &root,
        &DiscussionTarget::Draft { draft_id: created.draft.id.clone() },
        None,
        "@arch what would you change?".into(),
        Vec::new(),
        Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
    )
    .expect("discussion");
    let app = crate::tools::tests::mock_app();
    let proposed = vec![
        hunk("Alpha line.", "Alpha better."),
        hunk(&long_text(), "Beta short."),
        hunk("Gamma line.", "Gamma better."),
    ];
    let recorded = crate::draft_proposals::record_proposal(
        app.handle(),
        &root,
        &root,
        crate::draft_proposals::NewProposal {
            draft_id: &created.draft.id,
            path: FILE,
            hunks: &proposed,
            rationale: "Sharper. <spoken>Sharper.</spoken>",
            agent: &agent(),
            target: ThreadRef::discussion(&created.draft.id),
            thread_id: &thread.id,
        },
    )
    .expect("recorded");
    Draft {
        _dir: dir,
        root,
        draft_id: created.draft.id,
        thread_id: thread.id,
        proposal_id: recorded.id.clone(),
        hunks: recorded.ledger.iter().map(|row| row.id.clone()).collect(),
    }
}

impl Draft {
    fn roots(&self) -> Roots<'_> {
        Roots { worktree: &self.root, store: &self.root }
    }

    fn thread(&self) -> comments::Discussion {
        crate::comments::fold_discussion(&self.root, &self.draft_id)
            .into_iter()
            .find(|t| t.id == self.thread_id)
            .expect("thread")
    }

    fn history(&self) -> InputSection {
        discussion_history_section(self.roots(), &self.thread(), "")
    }

    /// AGC-FR-06: the `discussion_state` text beside the history.
    fn state(&self) -> String {
        discussion_state_text(self.roots(), &self.thread(), "")
    }
}

// AGC-FR-EPNB, AGC-FR-TCCV
#[test]
fn the_history_ends_with_every_hunk_of_the_open_proposal_by_place() {
    let d = draft();
    // The third hunk is withdrawn and the first is held for discussion.
    crate::draft_proposals::set_hunk_state(
        &d.root,
        &d.draft_id,
        &d.proposal_id,
        &d.hunks[2],
        crate::draft_proposals::hunks::HunkState::Withdrawn,
    )
    .expect("withdrawn");
    crate::draft_proposals::set_hunk_state(
        &d.root,
        &d.draft_id,
        &d.proposal_id,
        &d.hunks[0],
        crate::draft_proposals::hunks::HunkState::Discussing,
    )
    .expect("held");
    let body = d.state();
    let at = body.find("The open proposal to spec.md").expect("the open proposal is listed");
    let listing = &body[at..];
    assert!(body.trim_end().ends_with(listing.trim_end()), "it ends the section");
    let lines: Vec<&str> = listing.lines().filter(|l| l.starts_with("  ")).collect();
    assert_eq!(lines.len(), 3, "every hunk, in proposal order");
    assert!(lines[0].starts_with(&format!("  place 1 — id {} — discussing", d.hunks[0])));
    assert!(
        lines[0].contains("changes: \"Alpha line.\"")
            && lines[0].contains("offers: \"Alpha better.\"")
    );
    assert!(lines[1].starts_with(&format!("  place 2 — id {} — pending", d.hunks[1])));
    assert!(lines[2].starts_with(&format!("  no place — id {} — withdrawn", d.hunks[2])));
    // At most 200 characters of the text it changes.
    let changes = lines[1].split(" — changes: ").nth(1).expect("changes");
    let quoted = changes.split(" — offers: ").next().expect("quoted");
    let inner = quoted.trim_matches('"');
    assert!(inner.chars().count() <= OPEN_PROPOSAL_EXCERPT_MAX_CHARS, "{}", inner.chars().count());
    assert!(inner.ends_with('…'));
}

// AGC-FR-EPNB
#[test]
fn a_target_with_no_open_proposal_lists_none() {
    let d = draft();
    for hunk_id in &d.hunks {
        crate::draft_proposals::set_hunk_state(
            &d.root,
            &d.draft_id,
            &d.proposal_id,
            hunk_id,
            crate::draft_proposals::hunks::HunkState::Rejected,
        )
        .expect("rejected");
    }
    assert!(!d.history().body.contains("The open proposal to"));
    let note = DiscussionTarget::Note { note_id: "n1".into() };
    assert!(open_proposal_text(d.roots(), &note).is_none());
}

// AGC-FR-TCCV
#[test]
fn a_truncation_cuts_comments_and_keeps_the_open_proposal_whole() {
    let d = draft();
    for _ in 0..12 {
        crate::comments::add_comment_to(
            &d.root,
            ThreadRef::discussion(&d.draft_id),
            &d.thread_id,
            "filler ".repeat(700),
            Vec::new(),
            Vec::new(),
            Stamp { by: &human(), at: "2026-01-02T00:00:00Z" },
        )
        .expect("comment");
    }
    let section = d.history();
    assert!(section.truncated, "the comments run past the bound");
    let listing = open_proposal_text(d.roots(), &d.thread().target).expect("open");
    assert!(d.state().ends_with(&listing), "the proposal is whole in the state");
    assert!(section.body.contains("[Earlier comments omitted:"));
}

// AGC-FR-EPNB, AGC-FR-TCCV
#[test]
fn an_artifact_target_reads_its_pending_proposal() {
    let dir = TempDir::new().expect("tempdir");
    let path = crate::changes::canonicalize_lenient(dir.path());
    let root = crate::fs::RootFs::for_root(&path);
    let prompt = "prompts/review.md";
    std::fs::create_dir_all(path.join("prompts")).expect("dirs");
    std::fs::write(path.join(prompt), "# Review\n\nAlpha line.\n").expect("write");
    let mut assignments = crate::scanning::LibraryAssignments::default();
    assignments.assignments.insert(
        prompt.to_string(),
        crate::scanning::Assignment {
            artifact_type: crate::scanning::ArtifactType::Prompt,
            scope: crate::scanning::Scope::File,
        },
    );
    std::fs::create_dir_all(path.join(".synthesis")).expect("dirs");
    std::fs::write(
        path.join(".synthesis/library.toml"),
        toml::to_string(&assignments).expect("toml"),
    )
    .expect("write");
    let thread = crate::comments::open_discussion_in(
        &root,
        &root,
        &DiscussionTarget::Artifact { artifact_id: prompt.into() },
        None,
        "@arch thoughts?".into(),
        Vec::new(),
        Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
    )
    .expect("discussion");
    let app = crate::tools::tests::mock_app();
    let proposed = vec![hunk("Alpha line.", "Alpha better.")];
    let recorded = crate::prompt_proposals::record_prompt_proposal(
        app.handle(),
        &root,
        &root,
        crate::prompt_proposals::NewPromptProposal {
            artifact_id: prompt,
            change: crate::prompt_proposals::ProposedChange::Hunks(&proposed),
            rationale: "Sharper. <spoken>Sharper.</spoken>",
            agent: &agent(),
            thread_id: &thread.id,
        },
    )
    .expect("recorded");
    let thread = crate::comments::fold_artifact_discussion(&root, prompt)
        .into_iter()
        .find(|t| t.id == thread.id)
        .expect("thread");
    let roots = Roots { worktree: &root, store: &root };
    let body = discussion_state_text(roots, &thread, "");
    assert!(body.contains("The open proposal to prompts/review.md"), "{body}");
    assert!(body.contains(&format!(
        "  place 1 — id {} — pending — changes: \"Alpha line.\" — offers: \"Alpha better.\"",
        recorded.ledger[0].id
    )));
}

// AGC-FR-EPNB, AGC-FR-QSWD
#[test]
fn the_open_proposal_stands_before_the_pending_question_set() {
    let d = draft();
    let set = super::super::pending_questions::tests::question_set(&d.thread_id);
    crate::comments::reserve_question_set_record(&d.root, &d.root, &d.thread_id, &set)
        .expect("reserved");
    let body = d.state();
    let proposal = body.find("The open proposal to spec.md").expect("the proposal");
    let questions = body.find("A question set from arch").expect("the set");
    assert!(proposal < questions, "the proposal comes first");
    assert!(body.ends_with(&super::super::render_pending_questions(&set)));
}

// AGC-FR-MATQ, AGC-FR-EPNB: a bounded listing keeps its hunks in order while
// they fit and counts the rest; with room for all it is the whole listing.
#[test]
fn a_bounded_listing_keeps_hunks_in_order_and_counts_the_rest() {
    let d = draft();
    let whole = open_proposal_text(d.roots(), &d.thread().target).expect("open");
    let target = d.thread().target;
    assert_eq!(open_proposal_text_within(d.roots(), &target, usize::MAX).unwrap(), whole);
    assert_eq!(
        open_proposal_text_within(d.roots(), &target, whole.chars().count()).unwrap(),
        whole
    );

    let short = open_proposal_text_within(d.roots(), &target, whole.chars().count() - 1).unwrap();
    assert!(short.ends_with("  …and 1 more edits.\n"), "{short}");
    assert!(short.chars().count() < whole.chars().count());
    // The lines it keeps are the first lines of the whole listing.
    let kept = short.trim_end_matches("  …and 1 more edits.\n");
    assert!(whole.starts_with(kept));
    assert!(kept.contains(&d.hunks[0]) && kept.contains(&d.hunks[1]));
    assert!(!kept.contains(&d.hunks[2]));

    // Room for the opening line alone counts every hunk.
    let none = open_proposal_text_within(d.roots(), &target, 10).unwrap();
    assert!(none.ends_with("  …and 3 more edits.\n"));
    assert!(!d.hunks.iter().any(|id| none.contains(id)));
}

/// Record one pending new-draft proposal on the fixture draft, with `body`.
fn pending_new_draft(d: &Draft, body: &str) {
    crate::new_draft_proposals::record_new_draft_proposal(
        &crate::tools::tests::mock_app().handle().clone(),
        &d.root,
        &d.root,
        crate::new_draft_proposals::NewDraftInput {
            draft_id: &d.draft_id,
            name: "part-one",
            body,
            position: 1,
            total: 2,
            rationale: "<spoken>A part.</spoken>",
            agent: &agent(),
            target: ThreadRef::discussion(&d.draft_id),
            thread_id: &d.thread_id,
        },
    )
    .expect("new draft");
}

// AGC-FR-MATQ, AGC-FR-YQVB, AGC-FR-QSWD: a tail above half of the bound cuts
// the pending body first and keeps the proposal whole while that is enough;
// at a smaller bound it cuts the proposal too and counts the hunks it left
// out; a question set stays whole through both cuts.
#[test]
fn the_tail_cuts_the_body_before_the_proposal() {
    let d = draft();
    pending_new_draft(&d, &format!("BODY-START{}BODY-END", "b".repeat(3_000)));
    let target = d.thread().target;
    let listing = open_proposal_text(d.roots(), &target).expect("open");
    let (whole, cut) = history_tail(d.roots(), &target, None, usize::MAX);
    assert!(!cut);
    assert!(whole.contains("BODY-END") && whole.ends_with(&listing));

    // Half of the bound is 1,000 under the whole tail: the body cut is enough.
    let bound = 2 * (whole.chars().count() - 1_000);
    let (tail, cut) = history_tail(d.roots(), &target, None, bound);
    assert!(cut);
    assert!(tail.chars().count() <= bound / 2);
    assert!(tail.contains(DRAFT_BODY_PARTIAL_LINE) && !tail.contains("BODY-END"));
    assert!(tail.ends_with(&listing), "the proposal stays whole");

    // Half of the bound below the tail without its body: the proposal is cut.
    let without_body = whole.chars().count() - 3_018;
    let small = 2 * (without_body - 150);
    let questions = "\nQUESTIONS-WHOLE\n".to_string();
    let (tail, cut) = history_tail(d.roots(), &target, Some(questions.clone()), small);
    assert!(cut);
    assert!(tail.contains(DRAFT_BODY_PARTIAL_LINE));
    assert!(tail.contains("more edits.\n"));
    assert!(!tail.contains(&listing));
    assert!(tail.ends_with(&questions), "the question set is never cut");
}

// AGC-FR-DKHT, AGC-FR-RVQP: a diagram hunk is named as a diagram edit,
// addition, or removal, and its excerpts are its sources. A text hunk keeps its
// kind word.
#[test]
fn a_diagram_hunk_is_named_as_a_diagram_change_with_its_source() {
    use crate::draft_proposals::anchors::HunkKind;
    use crate::draft_proposals::hunks::HunkState;
    let change = |kind, diagram| crate::draft_proposals::ProposalChange {
        id: "h1".into(),
        kind,
        state: HunkState::Pending,
        before: "A --> B".into(),
        after: "A --> C".into(),
        diagram,
    };
    let line = super::super::proposal_hunk_line(FILE, &change(HunkKind::Replace, true));
    assert!(line.contains("diagram edit"), "{line}");
    assert!(line.contains("changes: \"A --> B\"") && line.contains("offers: \"A --> C\""));
    let add = super::super::proposal_hunk_line(FILE, &change(HunkKind::Add, true));
    assert!(add.contains("diagram addition"), "{add}");
    let del = super::super::proposal_hunk_line(FILE, &change(HunkKind::Del, true));
    assert!(del.contains("diagram removal"), "{del}");
    let text = super::super::proposal_hunk_line(FILE, &change(HunkKind::Replace, false));
    assert!(text.contains(": replace — undecided"), "{text}");

    assert_eq!(open_hunk(change(HunkKind::Replace, true)).diagram, Some("diagram edit"));
    assert_eq!(open_hunk(change(HunkKind::Replace, false)).diagram, None);
}
