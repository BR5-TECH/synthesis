//! A discussion's pending question set: recording it, reading it back, and
//! turning it into comments (CMS-FR-XWDA … CMS-FR-EKUP).

use super::*;

pub(super) fn asking_agent() -> Participant {
    Participant::Agent {
        agent_id: "a1".into(),
        handle: "arch".into(),
        model: Some("claude-opus-5".into()),
        title: Some("Architect".into()),
    }
}

pub(super) fn option_at(position: usize, value: &str) -> QuestionOption {
    QuestionOption {
        position,
        value: value.into(),
    }
}

/// A set of `texts.len()` questions, each carrying two options.
pub(super) fn set_for(thread_id: &str, set_id: &str, texts: &[&str]) -> PendingQuestionSet {
    PendingQuestionSet {
        set_id: set_id.into(),
        discussion_id: thread_id.into(),
        asked_by: asking_agent(),
        asked_at: "2026-09-10T12:00:00Z".into(),
        questions: texts
            .iter()
            .enumerate()
            .map(|(index, text)| PendingQuestion {
                position: index + 1,
                text: (*text).into(),
                options: vec![option_at(1, "one"), option_at(2, "two")],
            })
            .collect(),
    }
}

pub(super) fn answer_for(question_position: usize, option_position: usize, value: &str, note: Option<&str>) -> QuestionAnswer {
    QuestionAnswer {
        question_position,
        option_position: Some(option_position),
        option_value: Some(value.into()),
        own_answer: None,
        note: note.map(str::to_string),
    }
}

/// DQA-FR-FCZL: an entry the author answered in their own words.
pub(super) fn own_answer_for(question_position: usize, words: &str) -> QuestionAnswer {
    QuestionAnswer {
        question_position,
        option_position: None,
        option_value: None,
        own_answer: Some(words.into()),
        note: None,
    }
}

/// The body of a chosen option, for the tests that are about the body alone.
pub(super) fn answer_body_of(value: &str, note: Option<&str>) -> String {
    answer_body(&answer_for(1, 1, value, note))
}

/// The path a discussion's set is written to, for a test that reads or corrupts
/// it directly.
pub(super) fn set_path(root: &crate::fs::RootFs, draft_id: &str, thread_id: &str) -> PathBuf {
    ThreadRef::discussion(draft_id)
        .scope
        .question_set_path(root, thread_id)
        .unwrap()
}

// -- Recording (CMS-FR-XWDA, CMS-FR-QLDW, CMS-FR-JWVH) --------------------

/// CMS-FR-XWDA, CMS-FR-JWVH, ADQ-FR-GMDK: a recorded set reads back whole,
/// in the order it was written.
#[test]
fn a_recorded_set_reads_back_with_its_questions_in_order() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["one spec or two?", "where does the diagram go?"]);
    let path = set_path(&root, &draft_id, &thread.id);
    root.create_file(&path, crate::fs::CreateMode::Exclusive).unwrap();
    root.write_bytes_atomic(&path, &serde_json::to_vec(&set).unwrap())
        .unwrap();

    let read = read_question_set(&root, &root, &thread.id).unwrap();
    assert_eq!(read, set);
    assert_eq!(
        read.questions.iter().map(|q| q.position).collect::<Vec<_>>(),
        vec![1, 2],
    );
}

/// CMS-FR-XWDA: the record is not a log line. The discussion folds exactly as it
/// did before the set was written (ADQ-FR-DHZK).
#[test]
fn a_standing_set_adds_no_comment_to_the_discussion() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");
    let before = ThreadRef::discussion(&draft_id).find(&root, &thread.id).unwrap();

    let set = set_for(&thread.id, "s1", &["one spec or two?"]);
    let path = set_path(&root, &draft_id, &thread.id);
    root.create_file(&path, crate::fs::CreateMode::Exclusive).unwrap();
    root.write_bytes_atomic(&path, &serde_json::to_vec(&set).unwrap())
        .unwrap();

    let after = ThreadRef::discussion(&draft_id).find(&root, &thread.id).unwrap();
    assert_eq!(before, after);
}

/// CMS-FR-JWVH: a discussion holding none reads as none rather than as a refusal.
#[test]
fn a_discussion_holding_no_set_reads_as_none() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    assert!(read_question_set(&root, &root, &thread.id).is_none());
}

/// CMS-FR-JWVH: a record this build cannot decode is **no set**, and it still
/// holds the slot — a corrupt file never fabricates questions for the author.
#[test]
fn an_unreadable_record_reads_as_no_set() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let path = set_path(&root, &draft_id, &thread.id);
    root.create_file(&path, crate::fs::CreateMode::Exclusive).unwrap();
    root.write_bytes_atomic(&path, b"not json at all").unwrap();

    assert!(read_question_set(&root, &root, &thread.id).is_none());
    // And the slot is still occupied, so a second agent cannot record over it.
    assert!(matches!(
        root.create_file(&path, crate::fs::CreateMode::Exclusive),
        Err(crate::fs::FsError::AlreadyExists { .. }),
    ));
}

/// CMS-FR-XWDA, CMS-FR-EKUP: the set sits inside the draft's own comments
/// folder, which is what a draft deletion already removes whole.
#[test]
fn the_set_is_written_inside_the_drafts_own_comments_folder() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let path = set_path(&root, &draft_id, &thread.id);
    let folder = draft_comments_dir(&root, &draft_id).unwrap();
    assert!(path.starts_with(&folder), "{path:?} is outside {folder:?}");
    assert!(path.ends_with(format!("{}.json", thread.id)));
}

/// CMS-FR-XWDA: a thread id that is not a bare token never becomes a path, so a
/// crafted id cannot name the log beside the folder.
#[test]
fn a_thread_id_that_is_not_a_bare_token_resolves_to_no_path() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");

    let refused = ThreadRef::discussion(&draft_id)
        .scope
        .question_set_path(&root, "../discussion");
    assert_eq!(refused, Err(ERR_DISCUSSION_NOT_FOUND.to_string()));
}

// -- The submission's parts (CMS-FR-TXRB, CMS-FR-PJBV) --------------------

/// CMS-FR-PJBV: answers that cover every recorded question exactly once, with an
/// option the record holds, are accepted.
#[test]
fn a_complete_answer_set_validates() {
    let set = set_for("t1", "s1", &["a?", "b?"]);
    let answers = vec![
        answer_for(1, 2, "two", None),
        answer_for(2, 1, "one", Some("with a note")),
    ];
    assert!(validate_answers(&set, &answers).is_ok());
}

/// CMS-FR-PJBV: a question left unanswered refuses the whole submission.
#[test]
fn answers_that_miss_a_question_are_refused() {
    let set = set_for("t1", "s1", &["a?", "b?"]);
    let answers = vec![answer_for(1, 1, "one", None)];
    assert_eq!(
        validate_answers(&set, &answers),
        Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string()),
    );
}

/// CMS-FR-PJBV: two answers for one question are refused, the choice otherwise
/// depending on which was read first.
#[test]
fn two_answers_for_one_question_are_refused() {
    let set = set_for("t1", "s1", &["a?", "b?"]);
    let answers = vec![answer_for(1, 1, "one", None), answer_for(1, 2, "two", None)];
    assert_eq!(
        validate_answers(&set, &answers),
        Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string()),
    );
}

/// CMS-FR-PJBV: an option position the question does not hold is refused.
#[test]
fn an_option_position_outside_the_record_is_refused() {
    let set = set_for("t1", "s1", &["a?"]);
    let answers = vec![answer_for(1, 9, "one", None)];
    assert_eq!(
        validate_answers(&set, &answers),
        Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string()),
    );
}

/// CMS-FR-PJBV: a value that differs from the recorded one is refused, so a
/// surface answering a stale set records nothing.
#[test]
fn an_option_value_that_differs_from_the_record_is_refused() {
    let set = set_for("t1", "s1", &["a?"]);
    let answers = vec![answer_for(1, 1, "something else", None)];
    assert_eq!(
        validate_answers(&set, &answers),
        Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string()),
    );
}

/// CMS-FR-GNTB: an answer in the author's own words validates, carrying no
/// option of any kind.
#[test]
fn an_answer_in_the_authors_own_words_validates() {
    let set = set_for("t1", "s1", &["a?", "b?"]);
    let answers = vec![
        answer_for(1, 2, "two", None),
        own_answer_for(2, "three, one per layer"),
    ];
    assert!(validate_answers(&set, &answers).is_ok());
}

/// CMS-FR-GNTB: an entry naming both a chosen option and own words is refused,
/// there being no rule that says which of them the author meant.
#[test]
fn an_answer_naming_both_an_option_and_own_words_is_refused() {
    let set = set_for("t1", "s1", &["a?"]);
    let mut both = own_answer_for(1, "my own");
    both.option_position = Some(1);
    both.option_value = Some("one".into());
    assert_eq!(
        validate_answers(&set, &[both]),
        Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string()),
    );
}

/// CMS-FR-GNTB: an entry naming neither is refused, an answer of nothing being
/// no answer.
#[test]
fn an_answer_naming_neither_an_option_nor_own_words_is_refused() {
    let set = set_for("t1", "s1", &["a?"]);
    let empty = QuestionAnswer {
        question_position: 1,
        option_position: None,
        option_value: None,
        own_answer: None,
        note: None,
    };
    assert_eq!(
        validate_answers(&set, &[empty]),
        Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string()),
    );
}

/// CMS-FR-GNTB: half of a chosen option is as incomplete as none of it, both
/// halves travelling so the value can be checked against the record.
#[test]
fn half_of_a_chosen_option_is_refused() {
    let set = set_for("t1", "s1", &["a?"]);
    let mut position_only = answer_for(1, 1, "one", None);
    position_only.option_value = None;
    assert_eq!(
        validate_answers(&set, &[position_only]),
        Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string()),
    );
    let mut value_only = answer_for(1, 1, "one", None);
    value_only.option_position = None;
    assert_eq!(
        validate_answers(&set, &[value_only]),
        Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string()),
    );
}

/// CMS-FR-GNTB: own words of nothing but whitespace are no answer.
#[test]
fn blank_own_words_are_refused() {
    let set = set_for("t1", "s1", &["a?"]);
    assert_eq!(
        validate_answers(&set, &[own_answer_for(1, "   \n ")]),
        Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string()),
    );
}

/// CMS-FR-GNTB, DQA-FR-JJON: a note beside own words is refused, a note being a
/// chosen option's addition alone. A note of nothing is no note, so it passes.
#[test]
fn a_note_beside_own_words_is_refused_and_a_blank_one_is_not() {
    let set = set_for("t1", "s1", &["a?"]);
    let mut noted = own_answer_for(1, "my own");
    noted.note = Some("and this".into());
    assert_eq!(
        validate_answers(&set, &[noted]),
        Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string()),
    );
    let mut blank = own_answer_for(1, "my own");
    blank.note = Some("  ".into());
    assert!(validate_answers(&set, &[blank]).is_ok());
}

/// ADQ-FR-RECR: an own answer carries the other opening line, and no note
/// paragraph follows it.
#[test]
fn an_own_answer_body_is_the_other_opening_line() {
    assert_eq!(
        answer_body(&own_answer_for(1, "three, one per layer")),
        "**Own answer:** three, one per layer",
    );
    // The two lines are what tells a reader which kind of answer this is, so
    // they must not be the same line.
    assert_ne!(
        answer_body(&own_answer_for(1, "x")),
        answer_body_of("x", None),
    );
    // ADQ-FR-NUEB: the author's own words are escaped on the same terms as an
    // option value, and read back as written.
    assert_eq!(
        answer_body(&own_answer_for(1, "use *bold* and `code`")),
        "**Own answer:** use \\*bold\\* and \\`code\\`",
    );
    // Surrounding whitespace is not part of what the author said.
    assert_eq!(
        answer_body(&own_answer_for(1, "  spaced  ")),
        "**Own answer:** spaced",
    );
}

/// ADQ-FR-RECR: a `**Note:**` paragraph follows a chosen option and nothing
/// else.
///
/// `validate_answers` refuses the combination (CMS-FR-GNTB), so this holds the
/// body composition to the same rule on its own — an entry that reached it
/// carrying both still composes no note.
#[test]
fn no_note_paragraph_follows_own_words() {
    let mut noted = own_answer_for(1, "three, one per layer");
    noted.note = Some("and this as well".into());
    let body = answer_body(&noted);
    assert_eq!(body, "**Own answer:** three, one per layer");
    assert!(!body.contains("**Note:**"));
    assert!(!body.contains("and this as well"));
}

/// ADQ-FR-RECR, CMS-FR-TXRB: a submission answered in the author's own words
/// commits the other answer body, in the same paired order.
#[test]
fn a_submission_carries_an_own_answer_into_the_committed_comment() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["first?", "second?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();

    let answers = vec![
        own_answer_for(1, "three — one per layer and one for the join"),
        answer_for(2, 1, "one", Some("a note")),
    ];
    let outcome =
        commit_question_answers(&root, &root, &thread.id, "s1", &answers, &human("raver119")).unwrap();

    let bodies: Vec<&str> = outcome
        .submitted
        .discussion
        .comments
        .iter()
        .map(|c| c.body.as_str())
        .collect();
    assert_eq!(
        bodies[2],
        "**Own answer:** three — one per layer and one for the join",
    );
    // The other question keeps the chosen-option body and its note, so one
    // submission carries both shapes.
    assert_eq!(bodies[4], "**Selected option:** one\n\n**Note:** a note");
    // And the pairing order is the recorded one either way (ADQ-FR-YQTB).
    let ids: Vec<&str> = outcome
        .submitted
        .discussion
        .comments
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(&ids[1..], &["s1:01-1q", "s1:01-2a", "s1:02-1q", "s1:02-2a"]);
}
