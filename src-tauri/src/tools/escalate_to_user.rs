//! Escalate to user tool (`ESU-escalate-to-user-tool.md`).
//!
//! The tool the graduation loop reaches for when it cannot decide without
//! something only the author knows. A graduation run is unattended by design —
//! the author started it and went back to writing the next prompt — so a loop
//! that hits a genuine fork has two bad options left: guess, and hand back a
//! change set nobody vouched for, or approve provisionally, which is guessing
//! with a better name. This is the third option.
//!
//! ## Why it is not a comment
//!
//! There is no conversation to append to, nobody is watching a thread, and the
//! answer has to come back as structured data the next execution turn can carry
//! (ESU-FR-13). What the call records is a question against the run and a
//! durable state that survives a relaunch and keeps blocking the project's
//! queue.


use serde::{Deserialize, Serialize};

use super::ToolRefusal;
use crate::graduation::{GraduationEscalationQuestion, GraduationProposedResponse};
use crate::tools::agent_exec::protocol::{description_shape_ok, summary_shape_ok};

// ---------------------------------------------------------------------------
// The contract surface (ESU contract surface)
// ---------------------------------------------------------------------------

/// ESU-FR-03.
pub const NAME: &str = "escalate_to_user";

/// ESU-FR-19: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Stop and ask the person who started this graduation what you cannot answer yourself. Use this when deciding whether the work is finished turns on something only they know — which of two readings of their prompt they meant, whether a change they did not ask for is wanted, whether a gap you found should be filled here or left. The run pauses where it stands and waits, however long that takes, so ask everything you are stuck on in this one call rather than stopping the run again later. Ask between one and eight questions, each a single clear thing rather than three things in one sentence, and say enough about what you found that they can answer without opening anything. Give each question up to three concrete responses where you can see them, and none where no fixed choice fits; each response carries the answer itself, a label of at most five words, and a line of at most two sentences and twelve words saying what choosing it means. They may always answer in their own words instead. Do not use it to report progress, to explain what you did, or to ask something you could settle by reading a file. Calling it ends your turn: nothing else you asked for in the same reply is done, and no verdict you give alongside it is read.";








/// ESU-FR-11: the named limits. Each exists because the recorded escalation is
/// persisted in the run's checkpoint, rendered in the run surface, and travels
/// back to the execution agent inside a structured input already bounded by
/// `EAC-execute-agent-cli.md` EAC-FR-08 — an unbounded question set would be
/// refused at dispatch, after the run had already stopped for it.
///
/// Taken from the executor rather than restated, so the bound a question is
/// refused at here is the bound it would be refused at there.
pub use crate::tools::agent_exec::protocol::{
    LIMIT_ESCALATION_QUESTION as QUESTION_LIMIT, LIMIT_ESCALATION_REASON as REASON_LIMIT,
    LIMIT_OPTION_ANSWER as OPTION_ANSWER_LIMIT, LIMIT_OPTION_DESCRIPTION as OPTION_DESCRIPTION_LIMIT,
    LIMIT_OPTION_SUMMARY as OPTION_SUMMARY_LIMIT, MAX_ESCALATION_QUESTIONS as MAX_QUESTIONS,
    MAX_QUESTION_OPTIONS as MAX_OPTIONS, MIN_ESCALATION_QUESTIONS as MIN_QUESTIONS,
};

const REASON_BLANK: &str =
    "The reason must say why you cannot finish without an answer. Write a sentence or two.";
const QUESTION_BLANK: &str =
    "The question text must say what you want to know. Write it as you would write it to a colleague.";
const REASON_TOO_LONG: &str =
    "The reason is longer than 4 KiB, which is its limit. Say the same thing more briefly.";
const QUESTION_TOO_LONG: &str =
    "The question is longer than 2 KiB, which is its limit. Ask one thing, more briefly.";
const WRONG_QUESTION_COUNT: &str = "This call must carry between one and eight questions, and it carried a number outside that. Ask everything you are stuck on, in that many entries or fewer.";
const TOO_MANY_OPTIONS: &str =
    "This question carries more than three proposed responses, which is the limit. Offer the choices that actually differ.";
const OPTION_ANSWER_BLANK: &str =
    "A proposed response must carry the answer it stands for. Write what would be given if it were chosen.";
const OPTION_SUMMARY_SHAPE: &str = "A proposed response's summary must be one sentence of at most five words. Write a label rather than a sentence about it.";
const OPTION_DESCRIPTION_SHAPE: &str = "A proposed response's description must be at most two sentences and twelve words altogether. Say what choosing it means, more briefly.";
const OPTION_ANSWER_TOO_LONG: &str =
    "A proposed response's answer is longer than 512 bytes, which is its limit. Shorten it.";
const OPTION_SUMMARY_TOO_LONG: &str =
    "A proposed response's summary is longer than 128 bytes, which is its limit. Shorten it.";
const OPTION_DESCRIPTION_TOO_LONG: &str =
    "A proposed response's description is longer than 256 bytes, which is its limit. Shorten it.";

/// ESU contract surface: one proposed response, as the model writes it.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProposedResponseArgs {
    pub answer: String,
    pub summary: String,
    pub description: String,
}

/// ESU contract surface: one question, as the model writes it.
///
/// It carries no position. The model writes an ordered list and the ordering is
/// the whole of what it decides; the application assigns the position
/// (ESU-FR-16).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuestionArgs {
    pub question: String,
    #[serde(default)]
    pub options: Option<Vec<ProposedResponseArgs>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EscalateToUserArgs {
    pub reason: String,
    pub questions: Vec<QuestionArgs>,
}

/// ESU output: the fact that the questions were recorded, and nothing else.
///
/// There is no answer to return, because the run stops here and the answers
/// reach the next execution turn rather than this call.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Escalated {
    pub escalated: bool,
}


// ---------------------------------------------------------------------------
// Validation (ESU-FR-10, ESU-FR-11, ESU-FR-16)
// ---------------------------------------------------------------------------

/// The arguments reduced to what would be recorded, or the refusal that stops
/// them.
///
/// Pure, so every bound is exercisable without a run, a store, or an app.
///
/// ESU-FR-17: a refused call records **nothing at all**. One malformed question
/// or one malformed proposed response refuses the whole call rather than
/// recording the questions that were well formed, so the run is never paused on
/// a question set smaller than the judgement was stuck on and the author is
/// never shown half a decision.
pub fn normalize(
    args: &EscalateToUserArgs,
) -> Result<(String, Vec<GraduationEscalationQuestion>), ToolRefusal> {
    let reason = args.reason.trim();
    if reason.is_empty() {
        return Err(ToolRefusal::InvalidArguments(REASON_BLANK));
    }
    if reason.len() > REASON_LIMIT {
        return Err(ToolRefusal::InvalidArguments(REASON_TOO_LONG));
    }
    if !(MIN_QUESTIONS..=MAX_QUESTIONS).contains(&args.questions.len()) {
        return Err(ToolRefusal::InvalidArguments(WRONG_QUESTION_COUNT));
    }

    let mut questions = Vec::with_capacity(args.questions.len());
    for (index, question) in args.questions.iter().enumerate() {
        // ESU-FR-16: the position is assigned here, in the order the model
        // wrote them, and is what every later act names. The text is never an
        // identifier: two questions may read alike, and an answer matched by
        // text would attach to whichever matched first.
        let position = index + 1;
        let refuse = |message: &'static str| ToolRefusal::InvalidQuestion(message, position);
        let text = question.question.trim();
        if text.is_empty() {
            return Err(refuse(QUESTION_BLANK));
        }
        if text.len() > QUESTION_LIMIT {
            return Err(refuse(QUESTION_TOO_LONG));
        }
        // ESU-FR-10: absent means no options, and an empty list means that no
        // fixed response suits. The two are the same recorded state.
        let supplied = question.options.clone().unwrap_or_default();
        if supplied.len() > MAX_OPTIONS {
            return Err(refuse(TOO_MANY_OPTIONS));
        }
        let mut options = Vec::with_capacity(supplied.len());
        for option in &supplied {
            // Nothing here is dropped and salvaged: a response the author would
            // read as a choice is either offered as the model wrote it or
            // refused so the model can write it again. A whole call refused
            // costs one round; a silently trimmed one costs a decision taken on
            // less than was meant.
            let answer = option.answer.trim();
            let summary = option.summary.trim();
            let description = option.description.trim();
            if answer.is_empty() {
                return Err(refuse(OPTION_ANSWER_BLANK));
            }
            if answer.len() > OPTION_ANSWER_LIMIT {
                return Err(refuse(OPTION_ANSWER_TOO_LONG));
            }
            if summary.len() > OPTION_SUMMARY_LIMIT {
                return Err(refuse(OPTION_SUMMARY_TOO_LONG));
            }
            if description.len() > OPTION_DESCRIPTION_LIMIT {
                return Err(refuse(OPTION_DESCRIPTION_TOO_LONG));
            }
            if !summary_shape_ok(summary) {
                return Err(refuse(OPTION_SUMMARY_SHAPE));
            }
            if !description_shape_ok(description) {
                return Err(refuse(OPTION_DESCRIPTION_SHAPE));
            }
            options.push(GraduationProposedResponse {
                answer: answer.to_string(),
                summary: summary.to_string(),
                description: description.to_string(),
            });
        }
        questions.push(GraduationEscalationQuestion {
            position: position as u32,
            question: text.to_string(),
            options,
        });
    }
    Ok((reason.to_string(), questions))
}

/// ESU-FR-19: the **result form** — an `escalate_to_user` request a Review turn
/// returns in its structured result in place of a verdict.
///
/// It is this escalation in every respect but where it is written: the same
/// `reason`, the same one to eight questions in the order it asked them, and the
/// same up-to-three proposed responses per question. The application validates it
/// on **exactly the rules of the tool form** — which is [`normalize`], reached
/// here rather than restated — and a request that fails any of them is refused as
/// a Review result the application could not read, on the terms
/// `../ai/GRL-graduation-loop.md` GRL-FR-TVXI sets, rather than half-recorded.
pub fn validate_result_form(
    value: &serde_json::Value,
) -> Result<(String, Vec<GraduationEscalationQuestion>), Vec<String>> {
    let reason = |rule: &str, detail: String| format!("{rule}: {detail}");
    let args: EscalateToUserArgs = serde_json::from_value(value.clone()).map_err(|error| {
        vec![reason(
            "escalation_shape",
            format!("The escalate_to_user request could not be read: {error}."),
        )]
    })?;
    normalize(&args).map_err(|refusal| vec![reason("escalation_invalid", refusal.to_string())])
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------
