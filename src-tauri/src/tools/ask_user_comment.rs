//! Ask user comment tool (`AUC-ask-user-comment-tool.md`).
//!
//! The tool an agent reaches for when it cannot answer well without knowing
//! something only the author can tell it. It posts **one question** into the
//! conversation the agent was asked in — as an ordinary comment, in the agent's
//! own name — and the turn ends there (CVL-FR-15). The author answers in the
//! next comment, and that reply brings the agent back with the whole
//! conversation in front of it.
//!
//! ## Why it is constructed per turn
//!
//! Every other tool in this group answers the same way whoever called it, so one
//! instance serves the application. This one posts *somewhere*, and where is not
//! a decision a model may make: the origin and the agent participant are bound
//! by the constructor (AUC-FR-03, TLC-FR-15), so no argument can reach a
//! conversation the agent was not asked in. That is why there is no `thread_id`
//! parameter — its absence is the containment.
//!
//! ## Why it is the one tool with a side effect
//!
//! `TLC-tool-conventions.md` distinguishes a tool's *declared* side effects from
//! the read-only posture the rest of the group shares (TLC-FR-17). One
//! successful call appends exactly one comment to the conversation the turn is
//! already in — one line in the log that conversation already lives in, which is
//! the same and only mutation a person posting a comment performs — and nothing
//! else anywhere.
//!
//! ## Why the body is composed rather than templated
//!
//! The hidden requirement is that this read as a person asking a question. So
//! the tool contributes as little of the text as it can: the model's own
//! question, the options as a list, and one fixed closing line (AUC-FR-05).
//! There is no identifier to quote back, no marker, and nothing machine-shaped —
//! a reader cannot tell a tool composed it, and answering it is answering a
//! comment.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::{
    log_tool_refusal_with, log_tool_success, require_open_project, ToolRefusal, QUESTION_BLANK,
};

use crate::agent_conversations::ConversationOrigin;
use crate::comments::{self, Participant};
use crate::log_fields;
use crate::logging::{LogBuffer, BUFFER};

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// The contract surface (AUC contract surface)
// ---------------------------------------------------------------------------

/// AUC-FR-01.
pub const NAME: &str = "ask_user_comment";

/// AUC-FR-02: the fixed text the model reads, compiled into the binary.
pub const DESCRIPTION: &str = "Ask the author one clarifying question, when you cannot answer well without knowing something only they can tell you — which of two readings they meant, or which of several directions they want. Reach for it instead of guessing, and instead of answering every possibility in turn. Write the question as you would write it to a colleague, and pass a couple of candidate answers if you have them; the author may agree with one or say something else entirely. Your question is posted as your own comment in this conversation and your turn ends there — this call brings you no answer back, and you will be asked again later with the author's reply in front of you. Ask one thing at a time: because your turn ends here, anything else you asked for in the same message is not carried out. It posts into the conversation you were asked in and nowhere else, and it changes nothing in the project.";

const QUESTION_DESCRIPTION: &str = "What you want to know, written as you would write it to a colleague in a comment. Ask one thing; if several are unclear, ask the one whose answer changes the most.";

const OPTIONS_DESCRIPTION: &str = "A few candidate answers, if you have them — two or three is usual. Each is a short phrase the author can agree with. Optional: omit it to ask an open question. The author is free to answer something else whichever you pass.";

/// CVL-FR-15: what a second question in one reply is told.
///
/// The loop never dispatches it, so this is the result the model reads in place
/// of one. It exists because a reply *can* ask twice: the first call refusing
/// leaves the loop running (AUC-FR-12), and were the second then carried out a
/// turn would post a question after all — two contributions where AGC-FR-01
/// allows one, and the very thing "one at a time" is about.
pub const ONE_AT_A_TIME: &str = "Only one question can be asked at a time, so this one was not posted. Ask the single thing whose answer changes the most, in one call.";

/// AUC-FR-05: the one sentence this tool contributes to the comment.
///
/// Only where options were passed. An open question needs no invitation to
/// answer it openly, and a closing line under one would be the tool talking for
/// the sake of it.
pub const CLOSING_LINE: &str = "Or say what you'd rather do.";

/// The arguments, as the model composed them.
///
/// Unknown fields are ignored rather than rejected (TLC-FR-07). There is
/// deliberately no parameter naming a conversation: see the module docs.
#[derive(Debug, Clone, Deserialize)]
pub struct AskUserCommentArgs {
    pub question: String,
    #[serde(default, deserialize_with = "lenient_options")]
    pub options: Option<Vec<String>>,
}

/// TLC-FR-07: accept the spellings of a list a model plausibly emits.
///
/// A bare string is one option rather than a malformed list, and anything else
/// — a number, an object, an explicit `null` — falls back to no options at all
/// rather than refusing. Losing a perfectly good question over the shape of an
/// optional parameter is what TLC-FR-07 exists to prevent.
fn lenient_options<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<String>>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Array(items) => Some(
            items
                .into_iter()
                .map(|item| match item {
                    serde_json::Value::String(text) => text,
                    // A model that numbered its options meant the numbers to be
                    // read, so they are carried rather than dropped.
                    other => other.to_string(),
                })
                .collect(),
        ),
        serde_json::Value::String(text) => Some(vec![text]),
        _ => None,
    })
}

/// TLC-FR-08: a JSON object with stable named fields.
///
/// AUC-FR-09: the fact of the posting is the whole of the result, and no
/// sentence is composed about it. The turn ends on the call that posted
/// (CVL-FR-15), so no further model call reads this — it is what the exchange
/// records rather than anything the model acts on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AskUserCommentOutput {
    pub posted: bool,
}

/// TLC-FR-06: the JSON Schema for [`AskUserCommentArgs`].
pub fn parameters() -> serde_json::Value {
    serde_json::json!({
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
            },
        },
        "required": ["question"],
    })
}

// ---------------------------------------------------------------------------
// The comment (AUC-FR-05, AUC-FR-07)
// ---------------------------------------------------------------------------

/// AUC-FR-07: the options worth showing, in the order the model gave them.
///
/// Blank entries are dropped; a list that empties out is no options at all. No
/// cap — a model that offers six candidates is offering six, and refusing or
/// truncating would cost the author a choice over a matter of taste.
fn usable_options(options: Option<&Vec<String>>) -> Vec<&str> {
    options
        .map(|items| {
            items
                .iter()
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// AUC-FR-05: the body the author reads.
///
/// The question as the model wrote it, then the options as a Markdown list one
/// to a line, then the closing line. Nothing else: no identifier, no marker, no
/// machine-readable field, and no instruction about the form a reply must take.
pub fn compose_body(question: &str, options: Option<&Vec<String>>) -> String {
    let question = question.trim();
    let options = usable_options(options);
    if options.is_empty() {
        return question.to_string();
    }
    let mut body = String::with_capacity(question.len() + 64);
    body.push_str(question);
    body.push_str("\n\n");
    for option in options {
        body.push_str("- ");
        body.push_str(option);
        body.push('\n');
    }
    body.push('\n');
    body.push_str(CLOSING_LINE);
    body
}

// ---------------------------------------------------------------------------
// The append (AUC-FR-04)
// ---------------------------------------------------------------------------

/// AUC-FR-04: the comment, appended through the agent write path (CMS-FR-41).
///
/// The same path and the same participant shape a delivered answer takes
/// (AGC-FR-16), so the fold, the rail, and the Comments panel read a question
/// exactly as they read any other comment.
pub fn post(
    roots: crate::agent_conversations::Roots<'_>,
    origin: &ConversationOrigin,
    author: &Participant,
    body: String,
    at: &str,
) -> Result<comments::Discussion, String> {
    // CVL-FR-08: asking the author something is a move every agent has
    // everywhere, a note conversation included, and the write goes through the
    // same path whatever the owner and whether or not there is a fragment.
    let discussion = crate::agent_conversations::resolve_origin(roots, origin)
        .ok_or(comments::ERR_DISCUSSION_NOT_FOUND)?;
    comments::append_agent_comment_to(
        roots.store,
        discussion.log_ref(),
        origin.discussion_id(),
        body,
        author,
        at,
    )
}

/// The comments vocabulary in this tool's terms (AUC-FR-10, AUC-FR-11).
fn refusal_for(error: &str) -> ToolRefusal {
    if error == comments::ERR_DISCUSSION_LOCKED {
        ToolRefusal::ConversationLocked
    } else {
        ToolRefusal::QuestionNotPosted
    }
}

// ---------------------------------------------------------------------------
// The tool (TLC-FR-01)
// ---------------------------------------------------------------------------

/// AUC-FR-01: `ask_user_comment` as a `rig` portable tool.
///
/// Constructed per turn (AUC-FR-03) with everything the post needs already
/// decided: the project root, the conversation, and the participant the comment
/// will be attributed to.
///
/// `asked` is how the turn's loop learns that a question actually went out.
/// CVL-FR-15 turns on the difference between a call that posted — which ends the
/// turn — and one that refused, which ends nothing; the flag is that distinction
/// carried back to the loop, which sees only the tool's rendered result
/// otherwise. It is set exactly once, on the append succeeding.
pub struct AskUserCommentTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: crate::agent_conversations::OwnedRoots,
    origin: ConversationOrigin,
    author: Participant,
    asked: Arc<AtomicBool>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> AskUserCommentTool<R> {
    /// TLC-FR-19: this tool's own constructor, naming the conversation it may
    /// post into and the participant it posts as.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: crate::agent_conversations::OwnedRoots,
        origin: ConversationOrigin,
        author: Participant,
        asked: Arc<AtomicBool>,
    ) -> Self {
        AskUserCommentTool {
            app,
            roots,
            origin,
            author,
            asked,
            buffer: &BUFFER,
        }
    }

    /// The same tool, reporting into `buffer`.
    #[cfg(test)]
    pub fn with_buffer(mut self, buffer: &'static LogBuffer) -> Self {
        self.buffer = buffer;
        self
    }

    /// The whole call, with its logging (TLC-FR-14, AUC-FR-17).
    fn run(&self, args: AskUserCommentArgs) -> Result<AskUserCommentOutput, ToolRefusal> {
        let outcome = self.ask(args);
        // AUC-FR-17: the tool and the conversation, and nothing else. The
        // conversation is named because a record that cannot be tied to one
        // cannot be followed — and it is not an argument, having been bound at
        // construction rather than composed by the model. The question, the
        // options, and the body are all material the model composed out of the
        // conversation it is having, so none of them is loggable (TLC-FR-14).
        let conversation = log_fields! {
            "originKind" => self.origin.kind().as_str(),
            "discussionId" => self.origin.discussion_id(),
        };
        match &outcome {
            Ok(_) => log_tool_success(&self.app, self.buffer, NAME, conversation),
            Err(refusal) => {
                log_tool_refusal_with(&self.app, self.buffer, NAME, refusal, conversation)
            }
        }
        outcome
    }

    fn ask(&self, args: AskUserCommentArgs) -> Result<AskUserCommentOutput, ToolRefusal> {
        // AUC-FR-13: no project, no conversation to post into, so the shared
        // refusal (TLC-FR-13). Checked per call rather than at construction for
        // the reason `read_file` resolves its instance per call: the root this
        // tool holds was bound when the turn began, and a turn can outlive the
        // project being open. Without this, a question could be appended into a
        // checkout the application has stopped showing.
        require_open_project(&self.app)?;
        // AUC-FR-06: a question that says nothing is nothing to post.
        if args.question.trim().is_empty() {
            return Err(ToolRefusal::InvalidArguments(QUESTION_BLANK));
        }
        let body = compose_body(&args.question, args.options.as_ref());
        let at = crate::notes::now_rfc3339();
        // AUC-FR-QSVN: a discussion already holding a question set the author is
        // answering would otherwise gain a second question they cannot reply to,
        // the composer being disabled while a set stands (DQA-FR-PXNC).
        //
        // The check and the append are taken under the discussion's own lock —
        // the one a reservation holds (CMS-FR-QLDW) — because several agent
        // turns run at once in one application (AGC-FR-03) and two of them may
        // reach one discussion. Read and post unguarded, a set landing between
        // the two would leave the discussion holding both a standing set and a
        // question the author cannot reply to, which is the pair this refusal
        // exists to prevent.
        let guard = self
            .origin
            .is_discussion()
            .then(|| comments::question_set_lock(self.origin.discussion_id()));
        if guard.is_some() {
            let roots = self.roots.borrowed();
            if comments::read_question_set(roots.store, roots.worktree, self.origin.discussion_id())
                .is_some()
            {
                return Err(ToolRefusal::QuestionSetPendingForComment);
            }
        }
        let thread = post(self.roots.borrowed(), &self.origin, &self.author, body, &at)
            .map_err(|error| refusal_for(&error))?;
        drop(guard);

        // The question is in the conversation from this moment, so the surfaces
        // showing it redraw from the write rather than on a timer (CMS-FR-51).
        // Announced here rather than by the loop because the loop's own
        // announcement is for the answer it delivers, which this turn now never
        // will.
        comments::emit_discussion_changed(&self.app, &thread);
        // CVL-FR-15: the loop reads this to tell a posted question from a
        // refused one. Set only after the append and the announcement, so a
        // turn is never ended awaiting a reply nobody was asked for.
        self.asked.store(true, Ordering::SeqCst);
        Ok(AskUserCommentOutput { posted: true })
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for AskUserCommentTool<R> {
    const NAME: &'static str = NAME;

    type Args = AskUserCommentArgs;
    type Output = AskUserCommentOutput;
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
        // AUC-FR-08: posts and returns. It waits for no reply, holds no timer,
        // and opens no watcher, so it resolves before the future is ever polled.
        std::future::ready(self.run(arguments))
    }
}
