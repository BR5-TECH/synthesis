//! The author comments of the run (`CMS-comments-storage.md` CMS-FR-VRZA,
//! `AGC-agent-conversations.md` AGC-FR-TFEN).
//!
//! Each human write command gives the id of each human comment it appends to
//! `note_author_comment`, and no other path does, so a decision tool can tell
//! a comment the author wrote from one the app wrote in the author's name.

use tauri::Manager;

use super::question_sets::{answer_for, set_for, set_path};
use super::*;
use crate::agent_conversations::authority_for_test::{is_author, register_turn};

/// An app with a project open on a fresh root, a settings store, and a turn
/// registry, so the commands run as the window calls them.
struct Opened {
    app: tauri::App<tauri::test::MockRuntime>,
    _dir: tempfile::TempDir,
    root: crate::fs::RootFs,
    draft_id: String,
}

impl Opened {
    fn new() -> Opened {
        let dir = temp_root();
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        let draft_id = make_draft(&root, "spec");
        let app = tauri::test::mock_app();
        app.manage(crate::project::ProjectState::default());
        app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
        app.state::<crate::project::ProjectState>().set_root(path);
        // The registry exists, as it does in the application.
        register_turn(
            &app.handle().clone(),
            crate::agent_conversations::ConversationOrigin::stub_draft("t", &draft_id, false),
            None,
            None,
        );
        Opened { app, _dir: dir, root, draft_id }
    }

    fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    fn open(&self, body: &str) -> Discussion {
        crate::comments::open_discussion(
            self.handle(),
            DiscussionTarget::Draft { draft_id: self.draft_id.clone() },
            None,
            body.into(),
            Vec::new(),
            None,
            self.app.state(),
            self.app.state(),
        )
        .expect("opened")
    }

    fn add(&self, discussion_id: &str, body: &str) -> Discussion {
        crate::comments::add_comment(
            self.handle(),
            discussion_id.into(),
            body.into(),
            Vec::new(),
            Vec::new(),
            None,
            self.app.state(),
            self.app.state(),
        )
        .expect("added")
    }
}

// CMS-FR-VRZA / AGC-FR-TFEN: `open_discussion` and `add_comment` note the
// human comment they append.
#[test]
fn the_comments_the_author_posts_are_author_comments() {
    let o = Opened::new();
    let opened = o.open("@arch decompose this draft");
    let opening = opened.comments[0].id.clone();
    assert!(is_author(&o.handle(), &opening));
    let added = o.add(&opened.id, "@arch accept it");
    let reply = added.comments.last().expect("reply").id.clone();
    assert_ne!(reply, opening);
    assert!(is_author(&o.handle(), &reply));
}

// CMS-FR-VRZA / AGC-FR-TFEN: a comment the app appends in the author's name,
// and an agent comment, are not author comments.
#[test]
fn a_decision_comment_and_an_agent_comment_are_not_author_comments() {
    let o = Opened::new();
    let opened = o.open("@arch decompose");
    let record = crate::notes::new_note_id();
    add_comment_with_id(
        &o.root,
        ThreadRef::discussion(&o.draft_id),
        &opened.id,
        record.clone(),
        "Accepted the proposed draft “A”, part 1 of 2.".into(),
        &human("raver119"),
        "2026-01-01T00:00:01Z",
    )
    .expect("appended");
    assert!(!is_author(&o.handle(), &record));
    let agent_comment = crate::notes::new_note_id();
    let by_agent = append_agent_comment_with(
        &o.root,
        ThreadRef::discussion(&o.draft_id),
        &opened.id,
        agent_comment.clone(),
        "On it.".into(),
        Vec::new(),
        Stamp { by: &agent("a1", "arch"), at: "2026-01-01T00:00:02Z" },
    );
    by_agent.expect("agent append");
    assert!(!is_author(&o.handle(), &agent_comment));
}

// CMS-FR-VRZA: `get_or_create_note_discussion` notes the opening comment it
// appends, and a second call that appends nothing notes nothing new.
#[test]
fn the_opening_comment_of_a_note_discussion_is_an_author_comment() {
    let o = Opened::new();
    let note = crate::notes::create_note_in(
        &o.root,
        crate::notes::NoteScope::Project,
        "a note".into(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .expect("note");
    let opened = crate::comments::get_or_create_note_discussion(
        o.handle(),
        note.id.clone(),
        "@arch read this note".into(),
        Vec::new(),
        None,
        o.app.state(),
        o.app.state(),
    )
    .expect("opened");
    assert!(is_author(&o.handle(), &opened.comments[0].id));
}

// CMS-FR-VRZA: `submit_discussion_question_answers` notes each answer comment
// and no question comment.
#[test]
fn each_answer_comment_of_a_submission_is_an_author_comment() {
    let o = Opened::new();
    let opened = o.open("@arch ask me");
    let set = set_for(&opened.id, "s1", &["one spec or two?", "where?"]);
    let path = set_path(&o.root, &o.draft_id, &opened.id);
    o.root.create_file(&path, crate::fs::CreateMode::Exclusive).unwrap();
    o.root.write_bytes_atomic(&path, &serde_json::to_vec(&set).unwrap()).unwrap();
    let submitted = crate::comments::submit_discussion_question_answers(
        o.handle(),
        opened.id.clone(),
        "s1".into(),
        vec![answer_for(1, 1, "one", None), answer_for(2, 2, "two", None)],
        o.app.state(),
        o.app.state(),
    )
    .expect("submitted");
    let comments = &submitted.discussion.comments;
    let answers: Vec<_> =
        comments.iter().filter(|c| matches!(c.author, Participant::Human { .. })).skip(1).collect();
    assert_eq!(answers.len(), 2);
    for answer in answers {
        assert!(is_author(&o.handle(), &answer.id), "{}", answer.id);
    }
    for question in comments.iter().filter(|c| matches!(c.author, Participant::Agent { .. })) {
        assert!(!is_author(&o.handle(), &question.id));
    }
    assert!(is_author(&o.handle(), &submitted.final_answer_comment_id));
}

// CMS-FR-VRZA: the append answers the id it minted, and `add_comment` notes
// that id. A decision comment of the same author that the app appends after
// it is not noted.
#[test]
fn the_append_reports_its_own_id_and_only_that_comment_is_noted() {
    let o = Opened::new();
    let opened = o.open("@arch decompose");
    let (folded, minted) = add_comment_reporting_id(
        &o.root,
        ThreadRef::discussion(&o.draft_id),
        &opened.id,
        "accept it".into(),
        Vec::new(),
        Vec::new(),
        None,
        Stamp { by: &human("raver119"), at: "2026-01-01T00:00:01Z" },
    )
    .expect("appended");
    let appended = folded.comments.iter().find(|c| c.id == minted).expect("the minted id");
    assert_eq!(appended.body, "accept it");

    let added = o.add(&opened.id, "@arch accept it");
    let reply = added.comments.last().expect("reply").id.clone();
    let record = crate::notes::new_note_id();
    add_comment_with_id(
        &o.root,
        ThreadRef::discussion(&o.draft_id),
        &opened.id,
        record.clone(),
        "Accepted the proposed draft “A”, part 1 of 2.".into(),
        &human("raver119"),
        "2026-01-01T00:00:02Z",
    )
    .expect("decision comment");
    assert!(is_author(&o.handle(), &reply));
    assert!(!is_author(&o.handle(), &record));
}

// CMS-FR-VRZA: a submission retried after its first call committed (the
// CMS-FR-GAVT path) still notes the answer comments, so the turn the answers
// dispatch carries the author's authority.
#[test]
fn a_retried_submission_notes_the_answers_it_committed() {
    let o = Opened::new();
    let opened = o.open("@arch ask me");
    let set = set_for(&opened.id, "s2", &["one spec or two?"]);
    let path = set_path(&o.root, &o.draft_id, &opened.id);
    o.root.create_file(&path, crate::fs::CreateMode::Exclusive).unwrap();
    o.root.write_bytes_atomic(&path, &serde_json::to_vec(&set).unwrap()).unwrap();
    let answers = vec![answer_for(1, 1, "one", None)];
    // The first call committed outside the command, so nothing noted it.
    let committed =
        commit_question_answers(&o.root, &o.root, &opened.id, "s2", &answers, &human("raver119"))
            .expect("committed");
    let answer = committed.submitted.final_answer_comment_id.clone();
    assert!(!is_author(&o.handle(), &answer));
    let retried = crate::comments::submit_discussion_question_answers(
        o.handle(),
        opened.id.clone(),
        "s2".into(),
        answers,
        o.app.state(),
        o.app.state(),
    )
    .expect("the retry reports the committed submission");
    assert_eq!(retried.final_answer_comment_id, answer);
    assert!(is_author(&o.handle(), &answer));
}
