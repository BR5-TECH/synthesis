//! Ask discussion questions tool (`ADQ-ask-discussion-questions-tool.md`).
//!
//! The tool a discussion agent reaches for when several things are unsettled at
//! once. A discussion is where material is decided rather than corrected, and an
//! agent that has read a draft usually finds two or three open points instead of
//! one; asking them one at a time costs the author a round trip each and costs
//! the agent the answers it needed together. This puts the whole set at once.
//!
//! ## Why it records rather than posts
//!
//! `ask_user_comment` appends a comment and the author replies with another. A
//! set cannot work that way: the author answers question by question, choosing
//! an option for each, and only what they submit becomes conversation. So one
//! call records a **pending question set** against the discussion
//! (`../comments/question_sets.rs`) and appends **nothing** (ADQ-FR-DHZK). The
//! set is what the author sees until they submit, and it survives a restart
//! because the turn that asked does not (ADQ-FR-PZWD).
//!
//! ## Why nothing malformed is salvaged
//!
//! One bad question or one blank option refuses the **whole** call and records
//! nothing (ADQ-FR-CJRM). A silently trimmed set costs a decision taken on less
//! than the agent meant to ask, where a refused call costs one round of the
//! loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal, SET_OPTION_BLANK,
    SET_OPTION_TOO_LONG, SET_QUESTION_BLANK, SET_QUESTION_TOO_LONG, WRONG_OPTION_COUNT,
    WRONG_QUESTION_SET_COUNT,
};

use crate::agent_conversations::ConversationOrigin;
use crate::comments::{self, Participant, PendingQuestion, PendingQuestionSet, QuestionOption};
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (ADQ contract surface)
// ---------------------------------------------------------------------------

/// ADQ-FR-KZNV.
pub const NAME: &str = "ask_discussion_questions";

/// ADQ-FR-WBQL: the fixed text the model reads, compiled into the binary.
///
/// Written so a model reading every tool description in a discussion turn can
/// tell this one from `ask_user_comment`'s on its first sentence: this one asks
/// several things with options, that one asks one thing openly.
pub const DESCRIPTION: &str = "Put several questions to the author at once, when more than one thing about this discussion is unsettled and you can propose the likely answers to each. Ask up to ten, each with two or three options the author picks between. Reach for it instead of guessing and instead of answering every reading in turn. Your questions are recorded against this discussion and your turn ends there — this call brings you no answer back, and you will be asked again later with every question and the author's chosen answers in front of you. Ask everything you are stuck on here: anything else you asked for in the same message is not carried out. Use it only where you can propose options; for one open question with no options to offer, use `ask_user_comment` instead. It records against the discussion you were asked in and nowhere else, and it changes no file of the project.";

const QUESTIONS_DESCRIPTION: &str = "Everything about this discussion you are stuck on, between one and ten entries, in the order you want them answered. Ask them all here: your turn ends on this call, so a thing you leave out is a thing you cannot ask about later.";

const QUESTION_DESCRIPTION: &str = "One question, written as you would put it to a colleague. Ask one thing; a question with three parts comes back with one answer to whichever part they read last.";

const OPTIONS_DESCRIPTION: &str = "Two or three answers the author picks between, each a short phrase. Every question needs them. Where you have none to propose, do not use this tool: ask that one question with `ask_user_comment` instead.";

/// CVL-FR-15: what a second turn-ending call in one reply is told.
///
/// The loop never dispatches it, so this is the result the model reads in place
/// of one. It exists because a reply *can* ask twice: the first call refusing
/// leaves the loop running (ADQ-FR-BSYE), and were the second then carried out a
/// turn would leave two contributions where AGC-FR-01 allows one.
pub const ONE_AT_A_TIME: &str = "Only one question set can be recorded at a time, so this one was not recorded. Ask everything you are stuck on in one call.";

/// ADQ-FR-PVXK: the count of questions one call may carry.
pub const MIN_QUESTIONS: usize = 1;
pub const MAX_QUESTIONS: usize = 10;

/// ADQ-FR-PVXK: the count of options one question must carry.
pub const MIN_OPTIONS: usize = 2;
pub const MAX_OPTIONS: usize = 3;

/// ADQ-FR-ZBQH: the named limits.
///
/// Each exists because the set is persisted and rendered, and the comments it
/// generates travel to a later turn inside an input already bounded by
/// `AGC-agent-conversations.md` AGC-FR-11.
pub const QUESTION_LIMIT: usize = 2 * 1024;
pub const OPTION_LIMIT: usize = 512;

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07). There is
/// deliberately no parameter naming a conversation: the discussion is bound by
/// the constructor (ADQ-FR-HTGC), and its absence is the containment.
#[derive(Debug, Clone, Deserialize)]
pub struct AskDiscussionQuestionsArgs {
    #[serde(default)]
    pub questions: Vec<AskedQuestion>,
}

/// One question as the model wrote it, before a position is assigned.
#[derive(Debug, Clone, Deserialize)]
pub struct AskedQuestion {
    #[serde(default)]
    pub question: String,
    #[serde(default)]
    pub options: Vec<String>,
}

/// TLC-FR-08: a JSON object with stable named fields.
///
/// ADQ-FR-FQPA: the fact of the recording is the whole of the result. The turn
/// ends on the call that recorded (CVL-FR-15), so no further model call reads
/// this — it is what the exchange records rather than anything the model acts on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AskDiscussionQuestionsOutput {
    pub recorded: bool,
}

/// TLC-FR-06: the JSON Schema for [`AskDiscussionQuestionsArgs`].
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "questions": {
                "type": "array",
                "description": QUESTIONS_DESCRIPTION,
                "minItems": MIN_QUESTIONS,
                "maxItems": MAX_QUESTIONS,
                "items": {
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": QUESTION_DESCRIPTION,
                        },
                        "options": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": OPTIONS_DESCRIPTION,
                            "minItems": MIN_OPTIONS,
                            "maxItems": MAX_OPTIONS,
                        },
                    },
                    "required": ["question", "options"],
                },
            },
        },
        "required": ["questions"],
    })
}

// ---------------------------------------------------------------------------
// Validation (ADQ-FR-PVXK, ADQ-FR-CJRM, ADQ-FR-ZBQH, ADQ-FR-TWNS)
// ---------------------------------------------------------------------------

/// Turn what the model composed into the questions a set records, or refuse the
/// whole call.
///
/// Pure, so the whole of the validation is testable without an application. The
/// position is assigned **here** rather than taken from the model (ADQ-FR-TWNS):
/// two questions may read alike, so an answer matched by text would attach to
/// whichever matched first.
pub fn normalize(questions: &[AskedQuestion]) -> Result<Vec<PendingQuestion>, ToolRefusal> {
    if questions.len() < MIN_QUESTIONS || questions.len() > MAX_QUESTIONS {
        return Err(ToolRefusal::InvalidArguments(WRONG_QUESTION_SET_COUNT));
    }
    let mut out = Vec::with_capacity(questions.len());
    for (index, asked) in questions.iter().enumerate() {
        // ADQ-FR-CJRM: the position the author would count from, so a refusal
        // names the question at fault rather than leaving the model to guess.
        let position = index + 1;
        let text = asked.question.trim();
        if text.is_empty() {
            return Err(ToolRefusal::InvalidQuestion(SET_QUESTION_BLANK, position));
        }
        if text.len() > QUESTION_LIMIT {
            return Err(ToolRefusal::InvalidQuestion(SET_QUESTION_TOO_LONG, position));
        }
        if asked.options.len() < MIN_OPTIONS || asked.options.len() > MAX_OPTIONS {
            return Err(ToolRefusal::InvalidQuestion(WRONG_OPTION_COUNT, position));
        }
        let mut options = Vec::with_capacity(asked.options.len());
        for (option_index, value) in asked.options.iter().enumerate() {
            let value = value.trim();
            if value.is_empty() {
                return Err(ToolRefusal::InvalidQuestion(SET_OPTION_BLANK, position));
            }
            if value.len() > OPTION_LIMIT {
                return Err(ToolRefusal::InvalidQuestion(SET_OPTION_TOO_LONG, position));
            }
            options.push(QuestionOption {
                position: option_index + 1,
                value: value.to_string(),
            });
        }
        out.push(PendingQuestion {
            position,
            text: text.to_string(),
            options,
        });
    }
    Ok(out)
}

/// Which refusal a failed reservation is.
///
/// The storage module's typed errors, mapped to the contract surface's refusals.
fn refusal_for(error: &str) -> ToolRefusal {
    match error {
        comments::ERR_QUESTION_SET_ALREADY_PENDING => ToolRefusal::QuestionSetAlreadyPending,
        comments::ERR_DISCUSSION_LOCKED => ToolRefusal::QuestionSetConversationLocked,
        _ => ToolRefusal::QuestionSetNotRecorded,
    }
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// ADQ-FR-KZNV: `ask_discussion_questions` as a `rig` portable tool.
///
/// Constructed per turn (ADQ-FR-HTGC) with the discussion it may record against
/// and the participant the generated question comments will be stamped with
/// already decided, so no argument a model composes reaches a conversation it
/// was not asked in.
///
/// `recorded` is how the turn's loop learns that a set actually went out.
/// CVL-FR-15 turns on the difference between a call that recorded — which ends
/// the turn — and one that refused, which ends nothing.
pub struct AskDiscussionQuestionsTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: crate::agent_conversations::OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    recorded: Arc<AtomicBool>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> AskDiscussionQuestionsTool<R> {
    /// TLC-FR-19: this tool's own constructor, naming the discussion it may
    /// record against and the participant it records as.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: crate::agent_conversations::OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        recorded: Arc<AtomicBool>,
    ) -> Self {
        AskDiscussionQuestionsTool {
            app,
            roots,
            origin,
            author,
            recorded,
            buffer: &BUFFER,
        }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(mut self, buffer: &'static LogBuffer) -> Self {
        self.buffer = buffer;
        self
    }

    /// The whole call, with its logging (TLC-FR-14, ADQ-FR-RMBC).
    fn run(
        &self,
        args: AskDiscussionQuestionsArgs,
    ) -> Result<AskDiscussionQuestionsOutput, ToolRefusal> {
        let count = args.questions.len();
        let outcome = self.ask(args);
        // ADQ-FR-RMBC: the tool, the discussion, and the question count.
        // ADQ-FR-LZHV: no question text, no option value, no note, and no part
        // of the conversation — all of it material the model composed out of a
        // conversation it is having (TLC-FR-14). The count is the one thing this
        // tool names as loggable beyond the discussion its constructor bound it
        // to.
        let conversation = log_fields! {
            "originKind" => self.origin.kind().as_str(),
            "discussionId" => self.origin.discussion_id(),
            "questions" => count as u64,
        };
        match &outcome {
            Ok(_) => log_tool_success(&self.app, self.buffer, NAME, conversation),
            Err(refusal) => {
                log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, conversation)
            }
        }
        outcome
    }

    fn ask(
        &self,
        args: AskDiscussionQuestionsArgs,
    ) -> Result<AskDiscussionQuestionsOutput, ToolRefusal> {
        // ADQ-FR-KOWX: no project, no discussion to record against, so the
        // shared refusal (TLC-FR-13). Checked per call rather than at
        // construction for the reason `ask_user_comment` checks it there: the
        // root this tool holds was bound when the turn began, and a turn can
        // outlive the project being open.
        require_open_project(&self.app)?;
        let questions = normalize(&args.questions)?;
        let roots = self.roots.borrowed();
        let set = PendingQuestionSet {
            set_id: crate::notes::new_note_id(),
            discussion_id: self.origin.discussion_id().to_string(),
            asked_by: self.author.clone(),
            asked_at: crate::notes::now_rfc3339(),
            questions,
        };
        comments::reserve_discussion_question_set(
            &self.app,
            roots.store,
            roots.worktree,
            self.origin.discussion_id(),
            &set,
        )
        .map_err(|error| refusal_for(&error))?;
        // CVL-FR-15: the loop reads this to tell a recorded set from a refused
        // one. Set only after the reservation, so a turn is never ended awaiting
        // a reply nobody was asked for.
        self.recorded.store(true, Ordering::SeqCst);
        Ok(AskDiscussionQuestionsOutput { recorded: true })
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for AskDiscussionQuestionsTool<R> {
    const NAME: &'static str = NAME;

    type Args = AskDiscussionQuestionsArgs;
    type Output = AskDiscussionQuestionsOutput;
    type Error = ToolRefusal;

    fn description(&self) -> String {
        DESCRIPTION.to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        parameters()
    }

    fn map_error(&self, error: Self::Error) -> rig::tool::ToolExecutionError {
        error.to_execution_error()
    }

    fn call(
        &self,
        arguments: Self::Args,
    ) -> impl std::future::Future<Output = Result<Self::Output, Self::Error>> + Send {
        // ADQ-FR-SCUW: it records and returns. It waits for no reply, holds no
        // timer, and opens no watcher, so it resolves before the future is ever
        // polled.
        std::future::ready(self.run(arguments))
    }
}
