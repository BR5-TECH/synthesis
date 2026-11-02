//! Tests for the pending question set that ends `discussion_history`
//! (`AGC-agent-conversations.md` AGC-FR-QSWD).

use tempfile::TempDir;

use super::*;
use crate::comments::{
    Participant, PendingQuestion, PendingQuestionSet, QuestionOption, Stamp, ThreadRef,
};

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

pub(in crate::agent_conversations) fn question_set(discussion_id: &str) -> PendingQuestionSet {
    let option = |position: usize, value: &str, description: Option<&str>| QuestionOption {
        position,
        value: value.into(),
        description: description.map(str::to_string),
    };
    PendingQuestionSet {
        set_id: "set-1".into(),
        discussion_id: discussion_id.into(),
        asked_by: agent(),
        asked_at: "2026-01-01T00:00:00Z".into(),
        questions: vec![
            PendingQuestion {
                position: 1,
                text: "One spec or two?".into(),
                options: vec![
                    option(1, "One spec", Some("Easy to find, but it grows.")),
                    option(2, "Paired specs", Some("Each stays small.")),
                ],
            },
            PendingQuestion {
                position: 2,
                text: "Keep the diagram?".into(),
                options: vec![option(1, "Yes", None), option(2, "No", None)],
            },
        ],
    }
}

/// The file with this name under `dir`, at any depth.
fn find_file(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

struct Fixture {
    _dir: TempDir,
    root: crate::fs::RootFs,
    draft_id: String,
    thread_id: String,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().expect("tempdir");
    let root = crate::fs::RootFs::for_root(crate::changes::canonicalize_lenient(dir.path()));
    let created = crate::drafts::create_draft_at_root(&root, Some("spec")).expect("draft");
    let thread = crate::comments::open_discussion_in(
        &root,
        &root,
        &DiscussionTarget::Draft { draft_id: created.draft.id.clone() },
        None,
        "@arch what is open?".into(),
        Vec::new(),
        Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
    )
    .expect("discussion");
    Fixture { _dir: dir, root, draft_id: created.draft.id, thread_id: thread.id }
}

impl Fixture {
    fn roots(&self) -> Roots<'_> {
        Roots { worktree: &self.root, store: &self.root }
    }

    fn reserve(&self) {
        crate::comments::reserve_question_set_record(
            &self.root,
            &self.root,
            &self.thread_id,
            &question_set(&self.thread_id),
        )
        .expect("reserved");
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

// AGC-FR-QSWD
#[test]
fn the_rendering_names_the_agent_each_question_and_each_option() {
    let text = render_pending_questions(&question_set("d1"));
    assert!(text.contains("A question set from arch waits for the author's answers."));
    let lines: Vec<&str> = text.lines().filter(|l| l.starts_with("  ")).collect();
    assert_eq!(
        lines,
        vec![
            "  question 1 — One spec or two?",
            "    option 1 — One spec — Easy to find, but it grows.",
            "    option 2 — Paired specs — Each stays small.",
            "  question 2 — Keep the diagram?",
            "    option 1 — Yes",
            "    option 2 — No",
        ]
    );
}

// AGC-FR-QSWD
#[test]
fn the_history_ends_with_the_pending_question_set() {
    let f = fixture();
    assert!(!f.history().body.contains("A question set from"), "no set, no listing");
    f.reserve();
    let body = f.history().body;
    let listing = render_pending_questions(&question_set(&f.thread_id));
    assert!(body.ends_with(&listing), "the set ends the section: {body}");
}

// AGC-FR-QSWD
#[test]
fn a_truncation_cuts_comments_and_keeps_the_question_set_whole() {
    let f = fixture();
    f.reserve();
    for _ in 0..12 {
        crate::comments::add_comment_to(
            &f.root,
            ThreadRef::discussion(&f.draft_id),
            &f.thread_id,
            "filler ".repeat(700),
            Vec::new(),
            Vec::new(),
            Stamp { by: &human(), at: "2026-01-02T00:00:00Z" },
        )
        .expect("comment");
    }
    let section = f.history();
    assert!(section.truncated, "the comments run past the bound");
    let listing = render_pending_questions(&question_set(&f.thread_id));
    assert!(section.body.ends_with(&listing), "the set is whole at the end");
}

// AGC-FR-QSWD: a record that cannot be parsed adds no set, and the history
// says which kind of failure it was, for the warning of the caller.
#[test]
fn a_broken_record_adds_no_set_and_reports_its_kind() {
    let f = fixture();
    f.reserve();
    let thread = f.thread();
    let path = find_file(f._dir.path(), &format!("{}.json", f.thread_id)).expect("the record");
    std::fs::write(path, b"{ not json").expect("broken");
    let (section, unreadable) = discussion_history_with_status(f.roots(), &thread, "");
    assert_eq!(unreadable, Some("malformed"));
    assert!(!section.body.contains("A question set from"));
}

// AGC-FR-QSWD: an absent set is no failure.
#[test]
fn an_absent_set_reports_no_failure() {
    let f = fixture();
    let (_, unreadable) = discussion_history_with_status(f.roots(), &f.thread(), "");
    assert_eq!(unreadable, None);
}
