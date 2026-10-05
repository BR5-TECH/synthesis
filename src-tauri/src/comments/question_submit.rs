//! Turning a pending question set into comments (CMS-FR-TXRB through CMS-FR-GAVT).
//!
//! The author answers the whole set at once, and this is the one operation that
//! commits it: two comments per recorded question in the recorded order — the
//! agent-authored question, then the human-authored answer — and **then** the
//! set is deleted (CMS-FR-OKMU).
//!
//! ## Why the append comes first
//!
//! The two halves cannot be made one filesystem operation, so one of them has to
//! be second, and which one decides what a crash between them costs. Deleting
//! first would lose the set and leave the author with nothing to retry from.
//! Appending first costs a retry that appends lines the fold already ignores —
//! every event carries an identity derived from the set id and the question's
//! position (ADQ-FR-YQTB), so the duplicates collapse (CMS-FR-06) and the retry
//! completes by deleting the set. A crash therefore never leaves an ambiguous
//! pair, a missing comment, or a lost set (CMS-FR-OKMU).
//!
//! ## Why the comment ids are derived too
//!
//! ADQ-FR-YQTB fixes the *event* identity. CMS-FR-GAVT then requires a repeated
//! submission to return the same `final_answer_comment_id` after the set is
//! already gone, which is computable without a scan of the raw log only if the
//! comment ids are derived on the same terms. They are: one string names both
//! the event and the comment it adds.
//!
//! ## Why the identity leads with the position
//!
//! The fold replays events in `at` order and tie-breaks on the event identity
//! (CMS-FR-09), and one submission commits at one instant, so the identity alone
//! decides the order the pairs read in. ADQ-FR-YQTB therefore puts the
//! zero-padded position first and a rank digit after it: ten questions order as
//! the author was asked them rather than as text, and each question stands ahead
//! of its own answer. The pairs are joined for the reader by adjacency alone
//! (DQA-FR-GRUV), so an identity that sorted otherwise would separate every
//! question from its answer and no rendering could put them back together.

use tauri::State;

use super::*;

/// ADQ-FR-YQTB: the identity of the question half of one entry.
///
/// The position is written as two digits so `10` sorts after `02` rather than
/// after `01`, and the rank digit `1` puts it ahead of its own answer.
pub fn question_identity(set_id: &str, position: usize) -> String {
    format!("{set_id}:{position:02}-1q")
}

/// ADQ-FR-YQTB: the identity of the answer half of one entry.
pub fn answer_identity(set_id: &str, position: usize) -> String {
    format!("{set_id}:{position:02}-2a")
}

/// ADQ-FR-MRSK: the body of a question comment — the recorded text, then the
/// recorded options as an ordered numbered Markdown list.
///
/// Nothing else is added: no closing line, no routing tag, no identifier, and no
/// marker that a tool composed it (ADQ-FR-NUEB).
pub fn question_body(question: &PendingQuestion) -> String {
    let mut body = question.text.clone();
    body.push_str("\n\n");
    for (index, option) in question.options.iter().enumerate() {
        body.push_str(&format!("{}. {}\n", index + 1, escape_markdown(&option.value)));
    }
    body.trim_end().to_string()
}

/// ADQ-FR-MRSK: the body of an answer comment — one line, and a second paragraph
/// where the note is non-blank.
pub fn answer_body(answer: &QuestionAnswer) -> String {
    // ADQ-FR-RECR: the opening line says which kind of answer this is, and it
    // is the whole of what says so — no marker in the body is permitted
    // (ADQ-FR-NUEB), so a reader and the surface that pairs the comments both
    // read this line and nothing else.
    if let Some(own) = answer.own_answer.as_deref() {
        // DQA-FR-JJON: no note follows own words, and the validation refuses
        // an entry that carries one (CMS-FR-GNTB).
        return format!("**Own answer:** {}", escape_markdown(own.trim()));
    }
    let value = answer.option_value.as_deref().unwrap_or_default();
    let mut body = format!("**Selected option:** {}", escape_markdown(value));
    if let Some(note) = answer.note.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        body.push_str(&format!("\n\n**Note:** {}", escape_markdown(note)));
    }
    body
}

/// ADQ-FR-NUEB: escape a value for Markdown **without changing its text**.
///
/// A value carrying an asterisk or a backtick must read in the comment exactly
/// as the author chose it, so a character the renderer would otherwise consume
/// is backslash-escaped rather than removed or replaced. The renderer strips the
/// backslash again (`../../../src/diff/markdown.ts`), so a reader sees the
/// original and the formatting is never applied.
///
/// **Only what the renderer actually reads is escaped**, which is the whole set
/// this project's Markdown recognises: code spans, emphasis, strong emphasis,
/// and links (per `../../../specifications/ui/DFV-diff-view.md` DFV-FR-17).
/// Escaping more would be worse than escaping nothing: a backslash before a
/// character the renderer never consumes survives into the rendered text, so
/// `one.` would read `one\.` and the value would be changed by the very step
/// meant to preserve it.
fn escape_markdown(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        // The backslash itself leads the list: without it, a value that already
        // carries one would have it read as the escape of whatever follows.
        if matches!(ch, '\\' | '`' | '*' | '_' | '[' | ']') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// CMS-FR-PJBV, CMS-FR-GNTB: the answers cover every recorded question exactly
/// once, and each names either an option the record holds or the author's own
/// words.
///
/// Pure, so the whole of the validation is testable without a store.
pub fn validate_answers(
    set: &PendingQuestionSet,
    answers: &[QuestionAnswer],
) -> Result<(), String> {
    if answers.len() != set.questions.len() {
        return Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string());
    }
    for question in &set.questions {
        let mut matching = answers
            .iter()
            .filter(|answer| answer.question_position == question.position);
        let Some(answer) = matching.next() else {
            return Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string());
        };
        // Exactly once: a second entry for one question would make the choice
        // depend on which of them was read first.
        if matching.next().is_some() {
            return Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string());
        }
        // CMS-FR-GNTB: an option or own words, never both and never neither.
        // A note is a chosen option's addition alone (DQA-FR-JJON), and a blank
        // one is no note at all, on the terms ADQ-FR-RECR composes the body.
        let wrote_own = answer.own_answer.is_some();
        let chose_option = answer.option_position.is_some() || answer.option_value.is_some();
        if wrote_own == chose_option {
            return Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string());
        }
        if let Some(own) = answer.own_answer.as_deref() {
            if own.trim().is_empty() {
                return Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string());
            }
            if answer.note.as_deref().map(str::trim).is_some_and(|n| !n.is_empty()) {
                return Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string());
            }
            continue;
        }
        // Both halves of a chosen option travel, so an entry naming one of them
        // is as incomplete as one naming neither.
        let (Some(position), Some(value)) = (answer.option_position, answer.option_value.as_deref())
        else {
            return Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string());
        };
        let Some(option) = question
            .options
            .iter()
            .find(|option| option.position == position)
        else {
            return Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string());
        };
        // The value is checked as well as the position, so a surface answering a
        // set it read before the set changed is refused rather than recording
        // whichever option now stands there.
        if option.value != value {
            return Err(ERR_QUESTION_ANSWERS_INCOMPLETE.to_string());
        }
    }
    Ok(())
}

/// The ordered events one accepted submission appends (CMS-FR-TXRB).
///
/// Alternating: the agent-authored question stamped with the set's own recorded
/// participant (CMS-FR-41), then the human-authored answer stamped with the
/// acting identity (CMS-FR-11).
fn submission_events(
    set: &PendingQuestionSet,
    answers: &[QuestionAnswer],
    acting: &Participant,
) -> Vec<(String, String, Participant, EventBody)> {
    let mut events = Vec::with_capacity(set.questions.len() * 2);
    for question in &set.questions {
        let answer = answers
            .iter()
            .find(|answer| answer.question_position == question.position)
            .expect("validated above");
        events.push((
            set.discussion_id.clone(),
            question_identity(&set.set_id, question.position),
            set.asked_by.clone(),
            EventBody::CommentAdded {
                comment_id: question_identity(&set.set_id, question.position),
                body: question_body(question),
                quotes: Vec::new(),
                attachments: Vec::new(),
            },
        ));
        events.push((
            set.discussion_id.clone(),
            answer_identity(&set.set_id, question.position),
            acting.clone(),
            EventBody::CommentAdded {
                comment_id: answer_identity(&set.set_id, question.position),
                body: answer_body(answer),
                quotes: Vec::new(),
                attachments: Vec::new(),
            },
        ));
    }
    events
}

/// CMS-FR-TXRB: `"submit discussion question answers"`.
#[tauri::command]
pub fn submit_discussion_question_answers<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    discussion_id: String,
    set_id: String,
    answers: Vec<QuestionAnswer>,
    store: State<'_, GlobalSettingsStore>,
    project: State<'_, ProjectState>,
) -> Result<QuestionAnswersSubmitted, String> {
    let thread_id = discussion_id;
    let root = project.require_store()?;
    let worktree = project.require_root()?;
    let acting = acting_participant(&store, &project)?;
    let outcome = commit_question_answers(&root, &worktree, &thread_id, &set_id, &answers, &acting)?;
    crate::logging::log_info(
        &app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        "discussion question answers submitted",
        // The count and never a question, an option, a note, or a body
        // (ADQ-FR-LZHV).
        crate::log_fields! {
            "threadId" => &thread_id,
            "questions" => outcome.appended_questions as u64,
            "alreadyCommitted" => outcome.already_committed,
        },
    );
    // AGC-FR-TQLC, ADQ-FR-ZXAF: the turn that asked ended before there was an
    // answer and is never resumed. Retired here rather than at dispatch, because
    // a submission may reach a different agent than the one that asked, or none
    // at all, and the asking turn is retired either way.
    if outcome.appended_questions > 0 {
        crate::agent_conversations::retire_awaiting_for_thread(&app, &thread_id);
    }
    emit_discussion_changed(&app, &outcome.submitted.discussion);
    if outcome.appended_questions > 0 {
        // CMS-FR-BQEN: the set is gone. A retry that appended nothing changed no
        // set either, so it announces nothing.
        emit_question_set_changed(&app, &thread_id, None);
    }
    Ok(outcome.submitted)
}

/// What [`commit_question_answers`] did, so the command can report and announce
/// it without inspecting the log a second time.
pub struct SubmissionOutcome {
    pub submitted: QuestionAnswersSubmitted,
    /// Zero on the CMS-FR-GAVT retry path, where the append had already
    /// committed and only the delete was owed.
    pub appended_questions: usize,
    pub already_committed: bool,
}

/// The submission itself, without the events.
///
/// Split out for the reason the reservation is: this is the ordering CMS-FR-OKMU
/// turns on, and an `AppHandle` needs the Tauri runtime to exist.
pub fn commit_question_answers(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    thread_id: &str,
    set_id: &str,
    answers: &[QuestionAnswer],
    acting: &Participant,
) -> Result<SubmissionOutcome, String> {
    let thread = read_discussion_by_id(root, worktree, thread_id)
        .ok_or_else(|| ERR_DISCUSSION_NOT_FOUND.to_string())?;
    let discussion = discussion_ref_of(&thread).ok_or_else(|| ERR_NOT_SUPPORTED.to_string())?;
    let target = discussion.thread_ref();

    // CMS-FR-WNQD: the refusal is **unconditional** and is checked before
    // anything is read or reported. A locked conversation takes no further
    // contribution and answers from none either — including a retry that would
    // only report work already committed, which a reader recovers from the
    // conversation itself. Checked first so one rule governs a locked discussion
    // with no exception a reader has to know about.
    if thread.locked {
        return Err(ERR_DISCUSSION_LOCKED.to_string());
    }
    let Some(set) = read_question_set(root, worktree, thread_id) else {
        // CMS-FR-GAVT: the set is gone. Either this submission already committed
        // and is being retried, or it names a set this discussion never held.
        return finish_committed_submission(set_id, &thread);
    };
    if set.set_id != set_id {
        return Err(ERR_QUESTION_SET_NOT_FOUND.to_string());
    }
    validate_answers(&set, answers)?;

    let at = now_rfc3339();
    let events = submission_events(&set, answers, acting);
    let final_answer_comment_id = set
        .questions
        .last()
        .map(|question| answer_identity(&set.set_id, question.position))
        .ok_or_else(|| ERR_QUESTION_ANSWERS_INCOMPLETE.to_string())?;

    append_events_with_identities(root, target.scope, target.file_rel, &at, events)?;
    // CMS-FR-OKMU: only now. A failure above leaves the set exactly as it stood.
    delete_question_set(root, target, thread_id)?;

    Ok(SubmissionOutcome {
        submitted: QuestionAnswersSubmitted {
            discussion: target.find(root, thread_id)?,
            final_answer_comment_id,
        },
        appended_questions: set.questions.len(),
        already_committed: false,
    })
}

/// CMS-FR-GAVT: a repeat naming a set the discussion no longer holds.
///
/// Where the log already carries that set's derived question comment, the append
/// committed and only the delete was owed. Return the same ordered result and
/// append nothing. Where it does not, the set was never this discussion's.
fn finish_committed_submission(
    set_id: &str,
    thread: &Discussion,
) -> Result<SubmissionOutcome, String> {
    let first_question = question_identity(set_id, 1);
    if !thread
        .comments
        .iter()
        .any(|comment| comment.id == first_question)
    {
        return Err(ERR_QUESTION_SET_NOT_FOUND.to_string());
    }
    // The last answer identity the log carries is the final one. Read from the
    // committed comments rather than recomputed from a record that is gone.
    let answer_prefix = format!("{set_id}:");
    let final_answer_comment_id = thread
        .comments
        .iter()
        .filter(|comment| comment.id.starts_with(&answer_prefix) && comment.id.ends_with("-2a"))
        .next_back()
        .map(|comment| comment.id.clone())
        .ok_or_else(|| ERR_QUESTION_SET_NOT_FOUND.to_string())?;
    Ok(SubmissionOutcome {
        submitted: QuestionAnswersSubmitted {
            discussion: thread.clone(),
            final_answer_comment_id,
        },
        appended_questions: 0,
        already_committed: true,
    })
}
