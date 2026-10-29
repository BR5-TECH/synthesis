//! The option descriptions of a question set: where they are stored, which
//! comment carries them, and how a set or a line written without them reads
//! (CMS-FR-QAHZ, CMS-FR-TVOF, ADQ-FR-QUKU, ADQ-FR-RTDD).

use super::question_sets::{answer_for, described_at, option_at, set_for, set_path};
use super::*;

/// One discussion of a fresh draft, for the tests below.
fn discussion() -> (tempfile::TempDir, String, Discussion) {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");
    (dir, draft_id, thread)
}

fn submit(root: &crate::fs::RootFs, thread: &Discussion, set: &PendingQuestionSet) -> Discussion {
    reserve_question_set_record(root, root, &thread.id, set).unwrap();
    let answers: Vec<QuestionAnswer> = set
        .questions
        .iter()
        .map(|q| answer_for(q.position, 1, &q.options[0].value, None))
        .collect();
    commit_question_answers(root, root, &thread.id, &set.set_id, &answers, &human("raver119"))
        .unwrap()
        .submitted
        .discussion
}

/// ADQ-FR-QUKU, CMS-FR-QAHZ, ADQ-FR-NUEB: each question comment carries the
/// recorded descriptions in option order, each answer comment carries none, and
/// no body carries a description.
#[test]
fn a_question_comment_carries_its_descriptions_and_the_body_keeps_labels_only() {
    let (dir, _, thread) = discussion();
    let root = crate::fs::RootFs::for_root(dir.path());
    let mut set = set_for(&thread.id, "s1", &["one spec or two?"]);
    set.questions[0].options = vec![
        described_at(1, "One spec", "Easy to find; it grows large."),
        described_at(2, "Paired specs", "Each stays small; a change crosses two files."),
        described_at(3, "Three specs", "One per layer and one for the join."),
    ];

    let folded = submit(&root, &thread, &set);

    let question = folded.comments.iter().find(|c| c.id == "s1:01-1q").unwrap();
    assert_eq!(
        question.option_descriptions,
        vec![
            "Easy to find; it grows large.",
            "Each stays small; a change crosses two files.",
            "One per layer and one for the join.",
        ],
    );
    assert_eq!(question.body, "one spec or two?\n\n1. One spec\n2. Paired specs\n3. Three specs");
    let answer = folded.comments.iter().find(|c| c.id == "s1:01-2a").unwrap();
    assert!(answer.option_descriptions.is_empty());
    for comment in &folded.comments {
        assert!(!comment.body.contains("grows large"), "{:?}", comment.body);
    }
}

/// ADQ-FR-QUKU: a question whose options are not all described carries no
/// descriptions, so a reader never pairs a description with the wrong option.
#[test]
fn a_question_with_an_undescribed_option_carries_no_descriptions() {
    let (dir, _, thread) = discussion();
    let root = crate::fs::RootFs::for_root(dir.path());
    let mut set = set_for(&thread.id, "s1", &["one spec or two?"]);
    set.questions[0].options =
        vec![described_at(1, "One spec", "Easy to find."), described_at(2, "Paired", "   ")];

    let folded = submit(&root, &thread, &set);

    let question = folded.comments.iter().find(|c| c.id == "s1:01-1q").unwrap();
    assert!(question.option_descriptions.is_empty());
}

/// ADQ-FR-RTDD, CMS-FR-TVOF: a set recorded without descriptions submits, and
/// its question comments carry an empty field.
#[test]
fn a_set_recorded_without_descriptions_still_submits() {
    let (dir, _, thread) = discussion();
    let root = crate::fs::RootFs::for_root(dir.path());
    let mut set = set_for(&thread.id, "s1", &["first?", "second?"]);
    for question in &mut set.questions {
        question.options = vec![option_at(1, "one"), option_at(2, "two")];
    }

    let folded = submit(&root, &thread, &set);

    let questions: Vec<&Comment> =
        folded.comments.iter().filter(|c| c.id.ends_with("-1q")).collect();
    assert_eq!(questions.len(), 2);
    assert!(questions.iter().all(|c| c.option_descriptions.is_empty()));
    assert_eq!(questions[0].body, "first?\n\n1. one\n2. two");
}

/// CMS-FR-TVOF, ADQ-FR-RTDD: a record an older build wrote, with no
/// `description` on any option, reads back with none.
#[test]
fn a_record_written_without_descriptions_reads_back() {
    let (dir, draft_id, thread) = discussion();
    let root = crate::fs::RootFs::for_root(dir.path());
    let legacy = serde_json::json!({
        "setId": "s1",
        "threadId": thread.id,
        "askedBy": { "kind": "agent", "agentId": "a1", "handle": "arch" },
        "askedAt": "2026-09-10T12:00:00Z",
        "questions": [{ "position": 1, "text": "one or two?", "options": [
            { "position": 1, "value": "one — a single spec" },
            { "position": 2, "value": "two — paired specs" },
        ] }],
    });
    let path = set_path(&root, &draft_id, &thread.id);
    root.create_file(&path, crate::fs::CreateMode::Exclusive).unwrap();
    root.write_bytes_atomic(&path, &serde_json::to_vec(&legacy).unwrap()).unwrap();

    let read = read_question_set(&root, &root, &thread.id).expect("the legacy record");
    let options = &read.questions[0].options;
    assert_eq!(options[0].value, "one — a single spec");
    assert!(options.iter().all(|o| o.description.is_none()));
}

/// ADQ-FR-PZWD, CMS-FR-TVOF: a recorded set keeps each option's description on
/// disk, and an option without one writes no `description` key.
#[test]
fn a_recorded_set_keeps_its_descriptions_on_disk() {
    let (dir, draft_id, thread) = discussion();
    let root = crate::fs::RootFs::for_root(dir.path());
    let mut set = set_for(&thread.id, "s1", &["one or two?"]);
    set.questions[0].options = vec![described_at(1, "One", "Easy to find."), option_at(2, "Two")];
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();

    assert_eq!(read_question_set(&root, &root, &thread.id), Some(set));
    let raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(set_path(&root, &draft_id, &thread.id)).unwrap())
            .unwrap();
    let options = &raw["questions"][0]["options"];
    assert_eq!(options[0]["description"], "Easy to find.");
    assert!(options[1].get("description").is_none(), "{options}");
}

/// CMS-FR-QAHZ: an ordinary comment's line carries no `optionDescriptions` key,
/// a question comment's line carries the list, and a line written without the
/// key folds to an empty list.
#[test]
fn the_log_line_carries_descriptions_only_where_there_are_some() {
    let (dir, draft_id, thread) = discussion();
    let root = crate::fs::RootFs::for_root(dir.path());
    let set = set_for(&thread.id, "s1", &["one or two?"]);
    let folded = submit(&root, &thread, &set);

    let lines: Vec<serde_json::Value> = discussion_log_lines(&root, &draft_id)
        .iter()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let comment_line = |id: &str| {
        lines.iter().find(|line| line["commentId"] == id).unwrap_or_else(|| panic!("{id}"))
    };
    assert!(comment_line(&folded.comments[0].id).get("optionDescriptions").is_none());
    assert!(comment_line("s1:01-2a").get("optionDescriptions").is_none());
    assert_eq!(
        comment_line("s1:01-1q")["optionDescriptions"],
        serde_json::json!(["The first way.", "The second way."]),
    );

    // The opening comment's line has no key, and it folds to an empty list.
    assert!(folded.comments[0].option_descriptions.is_empty());
}

/// CMS-FR-QAHZ: the frontend receives the field on every comment, empty where
/// the comment carries no descriptions.
#[test]
fn the_comment_wire_shape_names_the_descriptions() {
    let (dir, _, thread) = discussion();
    let root = crate::fs::RootFs::for_root(dir.path());
    let set = set_for(&thread.id, "s1", &["one or two?"]);
    let folded = submit(&root, &thread, &set);

    let wire = serde_json::to_value(&folded).unwrap();
    let comments = wire["comments"].as_array().unwrap();
    assert_eq!(comments[0]["optionDescriptions"], serde_json::json!([]));
    assert_eq!(
        comments[1]["optionDescriptions"],
        serde_json::json!(["The first way.", "The second way."]),
    );
}

/// ADQ-FR-RTDD, CMS-FR-TVOF: a record an older build wrote is read and
/// submitted in one path, and its question comment carries no descriptions.
#[test]
fn a_record_written_without_descriptions_submits_end_to_end() {
    let (dir, draft_id, thread) = discussion();
    let root = crate::fs::RootFs::for_root(dir.path());
    let legacy = serde_json::json!({
        "setId": "old",
        "threadId": thread.id,
        "askedBy": { "kind": "agent", "agentId": "a1", "handle": "arch" },
        "askedAt": "2026-09-10T12:00:00Z",
        "questions": [{ "position": 1, "text": "one or two?", "options": [
            { "position": 1, "value": "one — a single spec" },
            { "position": 2, "value": "two — paired specs" },
        ] }],
    });
    let path = set_path(&root, &draft_id, &thread.id);
    root.create_file(&path, crate::fs::CreateMode::Exclusive).unwrap();
    root.write_bytes_atomic(&path, &serde_json::to_vec(&legacy).unwrap()).unwrap();

    let answers = vec![answer_for(1, 2, "two — paired specs", None)];
    let folded =
        commit_question_answers(&root, &root, &thread.id, "old", &answers, &human("raver119"))
            .unwrap()
            .submitted
            .discussion;

    let question = folded.comments.iter().find(|c| c.id == "old:01-1q").unwrap();
    assert!(question.option_descriptions.is_empty());
    assert_eq!(question.body, "one or two?\n\n1. one — a single spec\n2. two — paired specs");
}

/// ADQ-FR-QUKU: a question with one described option and one recorded without
/// a description carries no descriptions.
#[test]
fn a_question_mixing_a_described_and_an_undescribed_option_carries_none() {
    let (dir, _, thread) = discussion();
    let root = crate::fs::RootFs::for_root(dir.path());
    let mut set = set_for(&thread.id, "s1", &["one or two?"]);
    set.questions[0].options = vec![described_at(1, "One", "Easy to find."), option_at(2, "Two")];

    let folded = submit(&root, &thread, &set);

    let question = folded.comments.iter().find(|c| c.id == "s1:01-1q").unwrap();
    assert!(question.option_descriptions.is_empty());
}
