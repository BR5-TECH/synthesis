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
    let body = d.history().body;
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
    assert!(section.body.ends_with(&listing), "the proposal is whole at the end");
    assert!(section.body.contains("[This material is partial: it was truncated here.]"));
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
    let body = discussion_history_section(roots, &thread, "").body;
    assert!(body.contains("The open proposal to prompts/review.md"), "{body}");
    assert!(body.contains(&format!(
        "  place 1 — id {} — pending — changes: \"Alpha line.\" — offers: \"Alpha better.\"",
        recorded.ledger[0].id
    )));
}
