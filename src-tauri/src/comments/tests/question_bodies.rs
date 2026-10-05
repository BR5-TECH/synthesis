//! The text of a question and of its answer as comments (ADQ-FR-MRSK).

use super::*;
use super::question_sets::{answer_body_of, answer_for, asking_agent, option_at, set_for, set_path};

/// numbered list, and nothing else.
#[test]
fn the_question_body_is_the_text_and_its_numbered_options() {
    let question = PendingQuestion {
        position: 1,
        text: "Should the ontology live in one spec or two?".into(),
        options: vec![
            option_at(1, "one spec covering both layers"),
            option_at(2, "paired ui and core specs"),
        ],
    };
    assert_eq!(
        question_body(&question),
        "Should the ontology live in one spec or two?\n\n\
         1. one spec covering both layers\n\
         2. paired ui and core specs",
    );
}

/// ADQ-FR-MRSK: the answer body is one line, and a second paragraph only where
/// the note carries something.
#[test]
fn the_answer_body_carries_the_note_only_when_one_was_written() {
    assert_eq!(
        answer_body_of("paired specs", None),
        "**Selected option:** paired specs",
    );
    assert_eq!(
        answer_body_of("paired specs", Some("keep the diagram in the ui one")),
        "**Selected option:** paired specs\n\n**Note:** keep the diagram in the ui one",
    );
    // A note of nothing but whitespace is no note.
    assert_eq!(
        answer_body_of("paired specs", Some("   ")),
        "**Selected option:** paired specs",
    );
}

/// ADQ-FR-NUEB: a value's text is **unchanged** by the escaping.
///
/// The rendered reading is what the requirement is about, so the assertion is
/// against the spec's own example rather than against whatever the escaper
/// happens to produce. An escaper that backslashes a character the renderer
/// never consumes fails here, because that backslash survives into the text the
/// author reads.
#[test]
fn escaping_leaves_a_plain_value_byte_for_byte() {
    // ADQ line 89's own note, trailing full stop and all.
    let note = "keep the flow diagram in the ui one.";
    assert_eq!(
        answer_body_of("two — paired ui/ and core/ specs", Some(note)),
        format!("**Selected option:** two — paired ui/ and core/ specs\n\n**Note:** {note}"),
    );
    // The characters an over-eager escaper reaches for, none of which this
    // project's Markdown reads (DFV-FR-17).
    for value in [
        "v1.5",
        "a well-formed name",
        "two specs (ui + core)",
        "a | b",
        "#1 <thing> ~ok~",
        "one — a single spec covering both layers",
    ] {
        assert_eq!(
            answer_body_of(value, None),
            format!("**Selected option:** {value}"),
            "{value:?} was changed by escaping",
        );
    }
}

/// ADQ-FR-NUEB: a value carrying Markdown is escaped, and reads as chosen.
///
/// Only the characters the renderer consumes are escaped, and the renderer takes
/// the backslash back out (`../../../src/diff/markdown.ts`), so what the author
/// reads is the value they chose with no formatting applied.
#[test]
fn escaping_protects_only_what_the_renderer_reads() {
    let body = answer_body_of("use *bold* and `code` and _under_", None);
    assert_eq!(
        body,
        "**Selected option:** use \\*bold\\* and \\`code\\` and \\_under\\_",
    );
    // A value already carrying a backslash has it escaped too, rather than
    // leaving it to be read as the escape of whatever follows. The renderer
    // takes the added one back out, so `C:\Users` reads as it was written.
    assert_eq!(
        answer_body_of("C:\\Users\\demo", None),
        "**Selected option:** C:\\\\Users\\\\demo",
    );
    // And a link's brackets, which the renderer also reads.
    assert_eq!(
        answer_body_of("[not a link](x)", None),
        "**Selected option:** \\[not a link\\](x)",
    );
}

/// ADQ-FR-NUEB: neither body carries a routing tag, an identifier, or any marker
/// that a tool composed it.
#[test]
fn neither_body_carries_a_machine_marker() {
    let question = PendingQuestion {
        position: 1,
        text: "a?".into(),
        options: vec![option_at(1, "one"), option_at(2, "two")],
    };
    for body in [question_body(&question), answer_body_of("one", Some("note"))] {
        assert!(!body.contains("set_id"), "{body}");
        assert!(!body.contains("<!--"), "{body}");
        assert!(!body.contains("ADQ"), "{body}");
    }
}

/// ADQ-FR-YQTB: the identities are derived from the set id and the position, so
/// a retry derives the same ones and the fold ignores the duplicates.
#[test]
fn the_event_identities_are_derived_from_the_set_and_the_position() {
    assert_eq!(question_identity("s1", 3), "s1:03-1q");
    assert_eq!(answer_identity("s1", 3), "s1:03-2a");
    // Stable across calls, which is the whole of the idempotency.
    assert_eq!(question_identity("s1", 3), question_identity("s1", 3));
}

/// ADQ-FR-YQTB, CMS-FR-09, CMS-FR-TXRB: the derived identities sort into the
/// recorded question order, each question ahead of its own answer.
///
/// The fold tie-breaks on this identity and one submission commits at one
/// instant, so this ordering is the whole of what makes the pairs read right.
#[test]
fn the_identities_sort_into_the_recorded_question_order() {
    let mut identities: Vec<String> = (1..=10)
        .flat_map(|position| {
            [
                question_identity("s1", position),
                answer_identity("s1", position),
            ]
        })
        .collect();
    let expected = identities.clone();
    identities.sort();
    // Ten questions included: `10` must sort after `02`, not after `01`.
    assert_eq!(identities, expected);
}

/// ADQ-FR-MRSK, DQA-FR-GRUV: a ten-question set folds with every question
/// immediately followed by its own answer, which is what the paired history
/// rendering reads.
#[test]
fn a_ten_question_set_folds_with_each_answer_beside_its_question() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let texts: Vec<String> = (1..=10).map(|n| format!("question {n}?")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let set = set_for(&thread.id, "s1", &refs);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();

    let answers: Vec<QuestionAnswer> = (1..=10)
        .map(|position| answer_for(position, 1, "one", None))
        .collect();
    let outcome =
        commit_question_answers(&root, &root, &thread.id, "s1", &answers, &human("raver119")).unwrap();

    let ids: Vec<&str> = outcome
        .submitted
        .discussion
        .comments
        .iter()
        .skip(1)
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(ids.len(), 20);
    for position in 1..=10usize {
        let at = (position - 1) * 2;
        assert_eq!(ids[at], question_identity("s1", position));
        assert_eq!(ids[at + 1], answer_identity("s1", position));
    }
    assert_eq!(outcome.submitted.final_answer_comment_id, "s1:10-2a");
}

// -- The reservation (CMS-FR-QLDW, ADQ-FR-XKVR, ADQ-FR-VDGT) --------------

/// CMS-FR-QLDW, ADQ-FR-GMDK: a valid reservation writes the record and the
/// discussion serves it back.
#[test]
fn reserving_writes_the_record_and_it_reads_back() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["a?", "b?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();

    assert_eq!(read_question_set(&root, &root, &thread.id), Some(set));
}

/// CMS-FR-QLDW, ADQ-FR-XKVR, ADQ-FR-QNJU: a second reservation is refused and the
/// standing set is kept byte-for-byte. Two turns racing on one discussion leave
/// exactly one set, whichever arrived first.
#[test]
fn a_second_reservation_is_refused_and_keeps_the_first_set() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let first = set_for(&thread.id, "s1", &["a?"]);
    reserve_question_set_record(&root, &root, &thread.id, &first).unwrap();
    let bytes_before = root.read_bytes(set_path(&root, &draft_id, &thread.id)).unwrap();

    let second = set_for(&thread.id, "s2", &["b?", "c?"]);
    assert_eq!(
        reserve_question_set_record(&root, &root, &thread.id, &second),
        Err(ERR_QUESTION_SET_ALREADY_PENDING.to_string()),
    );

    let bytes_after = root.read_bytes(set_path(&root, &draft_id, &thread.id)).unwrap();
    assert_eq!(bytes_before, bytes_after, "the standing set was modified");
    assert_eq!(read_question_set(&root, &root, &thread.id), Some(first));
}

/// ADQ-FR-LFDX: a passage under review is one remark rather than a set of open
/// points, so an anchored thread takes no set.
#[test]
fn an_anchored_thread_refuses_a_reservation() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    touch(dir.path(), "spec.md");
    let thread = open(&root, "spec.md", anchor(0, 1, "x"), "a remark", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["a?"]);
    assert_eq!(
        reserve_question_set_record(&root, &root, &thread.id, &set),
        Err(ERR_NOT_SUPPORTED.to_string()),
    );
}

/// CMS-FR-QLDW, ADQ-FR-VDGT: a conversation that takes no further contribution
/// takes no further question either, and nothing is written.
#[test]
fn a_locked_discussion_refuses_a_reservation() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");
    set_lock_to(
        &root,
        ThreadRef::discussion(&draft_id),
        &thread.id,
        true,
        &human("raver119"),
        "2026-09-10T11:30:00Z",
    )
    .unwrap();

    let set = set_for(&thread.id, "s1", &["a?"]);
    assert_eq!(
        reserve_question_set_record(&root, &root, &thread.id, &set),
        Err(ERR_DISCUSSION_LOCKED.to_string()),
    );
    assert!(read_question_set(&root, &root, &thread.id).is_none());
}

// -- The submission (CMS-FR-TXRB, CMS-FR-OKMU, CMS-FR-GAVT) ---------------

/// CMS-FR-TXRB, ADQ-FR-MRSK: two comments per question in the recorded order,
/// the question stamped with the set's agent and the answer with the acting
/// human.
#[test]
fn a_submission_appends_paired_comments_in_the_recorded_order() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["first?", "second?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();

    let answers = vec![
        answer_for(1, 2, "two", Some("a note")),
        answer_for(2, 1, "one", None),
    ];
    let outcome =
        commit_question_answers(&root, &root, &thread.id, "s1", &answers, &human("raver119")).unwrap();

    // The opening comment, then question/answer, question/answer.
    let ids: Vec<&str> = outcome
        .submitted
        .discussion
        .comments
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    assert_eq!(&ids[1..], &["s1:01-1q", "s1:01-2a", "s1:02-1q", "s1:02-2a"]);

    let by: Vec<&Participant> = outcome
        .submitted
        .discussion
        .comments
        .iter()
        .skip(1)
        .map(|c| &c.author)
        .collect();
    assert_eq!(by, vec![&asking_agent(), &human("raver119"), &asking_agent(), &human("raver119")]);

    let bodies: Vec<&str> = outcome
        .submitted
        .discussion
        .comments
        .iter()
        .skip(1)
        .map(|c| c.body.as_str())
        .collect();
    assert_eq!(bodies[0], "first?\n\n1. one\n2. two");
    assert_eq!(bodies[1], "**Selected option:** two\n\n**Note:** a note");
    assert_eq!(bodies[3], "**Selected option:** one");

    // DQA-FR-CIRK: the final answer is the trigger the routing batch names.
    assert_eq!(outcome.submitted.final_answer_comment_id, "s1:02-2a");
}

/// CMS-FR-OKMU, ADQ-FR-LVOC: the set is deleted only after the append commits.
#[test]
fn an_accepted_submission_removes_the_set() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["a?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();
    commit_question_answers(
        &root,
        &root,
        &thread.id,
        "s1",
        &[answer_for(1, 1, "one", None)],
        &human("raver119"),
    )
    .unwrap();

    assert!(read_question_set(&root, &root, &thread.id).is_none());
    // And the slot is free, so the next turn may ask again.
    let next = set_for(&thread.id, "s2", &["b?"]);
    assert!(reserve_question_set_record(&root, &root, &thread.id, &next).is_ok());
}

/// CMS-FR-ZRHF, CMS-FR-GAVT, ADQ-FR-EJHR: a retry after the set is gone appends
/// nothing, duplicates no comment, and returns the same final answer id.
#[test]
fn a_retry_after_the_set_is_gone_appends_nothing_and_returns_the_same_result() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["a?", "b?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();
    let answers = vec![answer_for(1, 1, "one", None), answer_for(2, 2, "two", None)];
    let first =
        commit_question_answers(&root, &root, &thread.id, "s1", &answers, &human("raver119")).unwrap();

    let retry =
        commit_question_answers(&root, &root, &thread.id, "s1", &answers, &human("raver119")).unwrap();

    assert!(retry.already_committed);
    assert_eq!(retry.appended_questions, 0);
    assert_eq!(
        retry.submitted.final_answer_comment_id,
        first.submitted.final_answer_comment_id,
    );
    assert_eq!(
        retry.submitted.discussion.comments.len(),
        first.submitted.discussion.comments.len(),
        "the retry duplicated a comment",
    );
}

/// CMS-FR-PJBV: a `set_id` naming a set the discussion never held is refused and
/// appends nothing.
#[test]
fn a_submission_naming_an_unknown_set_is_refused() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");
    let before = ThreadRef::discussion(&draft_id).find(&root, &thread.id).unwrap();

    let set = set_for(&thread.id, "s1", &["a?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();

    assert_eq!(
        commit_question_answers(
            &root,
            &root,
            &thread.id,
            "some-other-set",
            &[answer_for(1, 1, "one", None)],
            &human("raver119"),
        )
        .err(),
        Some(ERR_QUESTION_SET_NOT_FOUND.to_string()),
    );
    let after = ThreadRef::discussion(&draft_id).find(&root, &thread.id).unwrap();
    assert_eq!(before.comments.len(), after.comments.len());
    // And the standing set is untouched.
    assert!(read_question_set(&root, &root, &thread.id).is_some());
}

/// CMS-FR-WNQD: a locked discussion refuses a submission, appends nothing, and
/// keeps its pending set so the author retries without re-entering anything.
#[test]
fn a_locked_discussion_refuses_a_submission_and_keeps_its_set() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["a?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();
    set_lock_to(
        &root,
        ThreadRef::discussion(&draft_id),
        &thread.id,
        true,
        &human("raver119"),
        "2026-09-10T11:30:00Z",
    )
    .unwrap();

    assert_eq!(
        commit_question_answers(
            &root,
            &root,
            &thread.id,
            "s1",
            &[answer_for(1, 1, "one", None)],
            &human("raver119"),
        )
        .err(),
        Some(ERR_DISCUSSION_LOCKED.to_string()),
    );
    assert_eq!(read_question_set(&root, &root, &thread.id), Some(set));
}

/// CMS-FR-EKUP, CMS-FR-39, ADQ-FR-OFZM: a deleted draft takes its discussions'
/// sets with it, and nothing else collects, expires, or times one out.
#[test]
fn deleting_a_draft_removes_the_set_it_held() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["a?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();
    let path = set_path(&root, &draft_id, &thread.id);
    assert!(root.read_bytes(&path).is_ok());

    delete_draft_comments(&root, &draft_id).unwrap();
    assert!(root.read_bytes(&path).is_err(), "the set outlived its draft");
}

/// CMS-FR-QLDW, ADQ-FR-XKVR: eight turns racing on one discussion leave exactly
/// **one** set, whichever arrived first.
///
/// Raced rather than sequenced, because the sequential case is decided by the
/// exclusive create alone and proves nothing about the exclusion this
/// requirement is named for. Several agent turns run at once inside one
/// application (AGC-FR-03), and this is the case they produce.
#[test]
fn eight_turns_racing_on_one_discussion_leave_exactly_one_set() {
    let dir = temp_root();
    let root_path = dir.path().to_path_buf();
    let root = crate::fs::RootFs::for_root(&root_path);
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let outcomes: Vec<Result<(), String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|index| {
                let root_path = root_path.clone();
                let thread_id = thread.id.clone();
                scope.spawn(move || {
                    let root = crate::fs::RootFs::for_root(&root_path);
                    let set = set_for(&thread_id, &format!("s{index}"), &["a?"]);
                    reserve_question_set_record(&root, &root, &thread_id, &set)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let recorded = outcomes.iter().filter(|r| r.is_ok()).count();
    assert_eq!(recorded, 1, "exactly one call recorded: {outcomes:?}");
    for outcome in outcomes.iter().filter(|r| r.is_err()) {
        assert_eq!(
            outcome.as_ref().unwrap_err(),
            ERR_QUESTION_SET_ALREADY_PENDING,
            "a losing call refused for the wrong reason",
        );
    }
    // And the discussion holds one whole set rather than a torn one.
    let held = read_question_set(&root, &root, &thread.id).expect("one set stands");
    assert_eq!(held.questions.len(), 1);
    assert_eq!(held.discussion_id, thread.id);
}

/// CMS-FR-OKMU, CMS-FR-ZRHF, ADQ-FR-EJHR: the crash window — the append
/// committed and the set is still standing.
///
/// This is the branch a submission interrupted between its two steps resumes
/// into, and it is not the CMS-FR-GAVT path: the set is still there, so the
/// retry validates, appends again, and rests entirely on the fold collapsing the
/// duplicate identities (CMS-FR-06). Nothing else in the suite reaches it.
#[test]
fn a_retry_with_the_set_still_standing_appends_no_duplicate() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["a?", "b?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();
    let answers = vec![answer_for(1, 1, "one", None), answer_for(2, 2, "two", None)];

    // The first half of the transaction, by hand: the append commits and the
    // set is left standing, which is exactly where a crash lands.
    let target = ThreadRef::discussion(&draft_id);
    let acting = human("raver119");
    let events: Vec<(String, String, Participant, EventBody)> = set
        .questions
        .iter()
        .flat_map(|question| {
            let answer = answers
                .iter()
                .find(|a| a.question_position == question.position)
                .unwrap();
            [
                (
                    thread.id.clone(),
                    question_identity("s1", question.position),
                    asking_agent(),
                    EventBody::CommentAdded {
                        comment_id: question_identity("s1", question.position),
                        body: question_body(question),
                        quotes: Vec::new(),
                        attachments: Vec::new(),
                    },
                ),
                (
                    thread.id.clone(),
                    answer_identity("s1", question.position),
                    acting.clone(),
                    EventBody::CommentAdded {
                        comment_id: answer_identity("s1", question.position),
                        body: answer_body(answer),
                        quotes: Vec::new(),
                        attachments: Vec::new(),
                    },
                ),
            ]
        })
        .collect();
    append_events_with_identities(
        &root,
        target.scope,
        target.file_rel,
        "2026-09-10T12:00:00Z",
        events,
    )
    .unwrap();
    assert!(
        read_question_set(&root, &root, &thread.id).is_some(),
        "the set is still standing, which is the window this test is about",
    );
    let committed = target.find(&root, &thread.id).unwrap();
    assert_eq!(committed.comments.len(), 5, "the opening comment and four");

    // The retry: it appends the same identities again and completes.
    let outcome =
        commit_question_answers(&root, &root, &thread.id, "s1", &answers, &acting).unwrap();

    assert_eq!(
        outcome.submitted.discussion.comments.len(),
        5,
        "CMS-FR-ZRHF: the fold ignored the duplicate lines rather than folding them twice",
    );
    assert_eq!(outcome.submitted.final_answer_comment_id, answer_identity("s1", 2));
    assert!(
        read_question_set(&root, &root, &thread.id).is_none(),
        "CMS-FR-OKMU: the retry completed by deleting the set",
    );
}

/// CMS-FR-PJBV: the answers may arrive in any order — the recorded order is what
/// decides, so the appended pairs read as the author was asked.
#[test]
fn answers_sent_out_of_order_append_in_the_recorded_order() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["first?", "second?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();
    // Sent second-then-first.
    let answers = vec![answer_for(2, 1, "one", None), answer_for(1, 2, "two", None)];
    let outcome =
        commit_question_answers(&root, &root, &thread.id, "s1", &answers, &human("raver119")).unwrap();

    let bodies: Vec<&str> = outcome
        .submitted
        .discussion
        .comments
        .iter()
        .skip(1)
        .map(|c| c.body.as_str())
        .collect();
    assert_eq!(bodies[0], "first?\n\n1. one\n2. two");
    assert_eq!(bodies[1], "**Selected option:** two");
    assert_eq!(bodies[2], "second?\n\n1. one\n2. two");
    assert_eq!(bodies[3], "**Selected option:** one");
}

/// AUC-FR-QSVN, CMS-FR-QLDW: the discussion's lock genuinely excludes a
/// reservation while `ask_user_comment` decides and acts.
///
/// Read and reserve unguarded, a set landing between the two would leave the
/// discussion holding both a standing set and a question the author cannot reply
/// to while the composer is disabled — the pair the refusal exists to prevent.
///
/// Asserted by **timing** rather than by outcome, because the outcome alone
/// cannot tell a lock that held from one that was never taken: the reservation
/// succeeds either way. A holder that keeps the lock for a measured interval and
/// a reservation that cannot complete inside it is what mutual exclusion means.
#[test]
fn a_reservation_waits_while_the_pending_set_check_holds_the_lock() {
    use std::time::{Duration, Instant};

    let dir = temp_root();
    let root_path = dir.path().to_path_buf();
    let root = crate::fs::RootFs::for_root(&root_path);
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    const HELD: Duration = Duration::from_millis(150);
    let (released_at, reserved_at) = std::thread::scope(|scope| {
        let holder = {
            let root_path = root_path.clone();
            let thread_id = thread.id.clone();
            scope.spawn(move || {
                let root = crate::fs::RootFs::for_root(&root_path);
                // The shape `ask_user_comment` takes: decide and act under the
                // discussion's own lock.
                let _guard = question_set_lock(&thread_id);
                assert!(read_question_set(&root, &root, &thread_id).is_none());
                std::thread::sleep(HELD);
                Instant::now()
            })
        };
        // Started after the holder has certainly taken the lock, so what this
        // measures is the wait rather than the spawn.
        std::thread::sleep(Duration::from_millis(20));
        let reserver = {
            let root_path = root_path.clone();
            let thread_id = thread.id.clone();
            scope.spawn(move || {
                let root = crate::fs::RootFs::for_root(&root_path);
                let set = set_for(&thread_id, "s1", &["a?"]);
                reserve_question_set_record(&root, &root, &thread_id, &set).unwrap();
                Instant::now()
            })
        };
        (holder.join().unwrap(), reserver.join().unwrap())
    });

    assert!(
        reserved_at >= released_at,
        "the reservation landed while the check still held the lock",
    );
    assert!(read_question_set(&root, &root, &thread.id).is_some());
}

/// CMS-FR-WNQD: the lock refuses **unconditionally** — including a retry of a
/// submission that already committed.
///
/// The set is gone by then, so the refusal has no pending state to protect: what
/// it protects is the rule. A locked conversation answers from none of this
/// module's operations, so a reader recovers the committed result from the
/// conversation rather than from a report this one would give.
#[test]
fn a_locked_discussion_refuses_even_a_retry_of_committed_work() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Ontology");
    let thread = open_discussion(&root, &draft_id, "shall we?", "2026-09-10T11:00:00Z");

    let set = set_for(&thread.id, "s1", &["a?"]);
    reserve_question_set_record(&root, &root, &thread.id, &set).unwrap();
    let answers = [answer_for(1, 1, "one", None)];
    commit_question_answers(&root, &root, &thread.id, "s1", &answers, &human("raver119")).unwrap();
    // The work is committed and the set is gone; now the conversation is locked.
    set_lock_to(
        &root,
        ThreadRef::discussion(&draft_id),
        &thread.id,
        true,
        &human("raver119"),
        "2026-09-10T12:00:00Z",
    )
    .unwrap();
    let before = ThreadRef::discussion(&draft_id).find(&root, &thread.id).unwrap();

    assert_eq!(
        commit_question_answers(&root, &root, &thread.id, "s1", &answers, &human("raver119")).err(),
        Some(ERR_DISCUSSION_LOCKED.to_string()),
        "CMS-FR-WNQD: the lock is checked before anything is read or reported",
    );
    // And the conversation is byte-for-byte what it was.
    let after = ThreadRef::discussion(&draft_id).find(&root, &thread.id).unwrap();
    assert_eq!(before, after);
}
