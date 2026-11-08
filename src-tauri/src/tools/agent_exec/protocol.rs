//! The task and response envelopes (EAC contract surface).
//!
//! Both are versioned, strictly decoded JSON objects with unknown fields
//! rejected (EAC-FR-07, EAC-FR-18). Strictness is the point rather than a
//! preference: this is a loop-facing tool (TLC-FR-20), so its documents were
//! composed by a compiler on one side and by an agent held to a stated contract
//! on the other. Silently ignoring a key would hide a caller's mistake on the
//! way out and an agent's misunderstanding on the way back.
//!
//! ## Two axes, never one
//!
//! An `AgentResponseEnvelope` is the agent's own report of its turn. It is not
//! proof that a file changed, that a suite passed, or that the work is done
//! (EAC-FR-27). The executor's process-level outcome is recorded separately, so
//! an agent that reports `failure` still produced a *completed* execution —
//! the process ran and answered in the protocol. Collapsing the two would make
//! "the agent says it could not do this" indistinguishable from "Docker was not
//! installed", which are opposite instructions to a caller.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

mod envelope;
pub use envelope::*;

/// The one protocol version this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Byte limits (EAC-FR-08)
// ---------------------------------------------------------------------------
//
// Every caller-visible payload is bounded by a *named* limit, so a refusal can
// say which one was exceeded rather than reporting a generic "too large". The
// serialized task is checked before any container is created, which is what
// keeps an oversized request from costing a pull, a launch, and a timeout
// before it fails.

/// The whole serialized task envelope.
pub const LIMIT_TASK: usize = 1024 * 1024;
/// `instruction`.
pub const LIMIT_INSTRUCTION: usize = 256 * 1024;
/// `input`, serialized.
pub const LIMIT_INPUT: usize = 512 * 1024;
/// Each individual `resume` field.
pub const LIMIT_RESUME_FIELD: usize = 512;
/// Captured stdout.
///
/// Sized for an event *stream* rather than for one document. Both pinned CLIs
/// write a line per event as they work (CCP-FR-25, CDX-FR-03), and a long turn's
/// stream carries every tool result the agent read — file contents included — so
/// the whole run's stdout is now orders of magnitude larger than the result
/// document extraction ends up reading. A bound sized for that document would
/// turn a long, successful turn into `invalid_structured_output` for no reason
/// but its length, which is the one failure a caller cannot correct.
pub const LIMIT_STDOUT: usize = 64 * 1024 * 1024;
/// Captured stderr.
pub const LIMIT_STDERR: usize = 1024 * 1024;
/// The extracted response envelope, serialized.
pub const LIMIT_ENVELOPE: usize = 1024 * 1024;
/// `summary`, in characters.
pub const LIMIT_SUMMARY: usize = 8192;
/// `result`, serialized.
pub const LIMIT_RESULT: usize = 256 * 1024;
/// `metadata`, serialized.
pub const LIMIT_METADATA: usize = 64 * 1024;
/// Each individual `session` field.
pub const LIMIT_SESSION_FIELD: usize = 512;
/// `escalation`, serialized (EAC-FR-08).
pub const LIMIT_ESCALATION: usize = 64 * 1024;
/// An escalation's `reason`, in characters.
pub const LIMIT_ESCALATION_REASON: usize = 4096;
/// Each `question` text, in characters.
pub const LIMIT_ESCALATION_QUESTION: usize = 2048;
/// A proposed response's `answer`, in characters.
pub const LIMIT_OPTION_ANSWER: usize = 512;
/// A proposed response's `summary`, in characters.
pub const LIMIT_OPTION_SUMMARY: usize = 128;
/// A proposed response's `description`, in characters.
pub const LIMIT_OPTION_DESCRIPTION: usize = 256;

/// EAC-FR-18: how many questions one escalation may carry.
pub const MIN_ESCALATION_QUESTIONS: usize = 1;
pub const MAX_ESCALATION_QUESTIONS: usize = 8;
/// EAC-FR-18: how many proposed responses one question may carry.
pub const MAX_QUESTION_OPTIONS: usize = 3;
/// EAC-FR-29: the stderr excerpt a failure record may carry. Small on purpose —
/// it is a diagnostic, not a transcript, and what makes a CLI's refusal legible
/// is the last thing it said rather than everything it said.
pub const LIMIT_STDERR_EXCERPT: usize = 2 * 1024;

/// EAC-FR-32: the longest single event this module keeps out of an observed
/// line. Above it the text is cut and said to be cut; the line itself is still
/// counted, so a reader is never shown a shortened event as a whole one.
pub const LIMIT_ACTIVITY_EVENT: usize = 64 * 1024;

/// What stands in for a value a record may not carry, where the value is too
/// short to fingerprint.
pub const REDACTED: &str = "<redacted>";

/// EAC-FR-29: the shortest value that is written as a fingerprint rather than
/// removed outright.
///
/// Below this a first-four-and-last-four rendering shows most of the value, so
/// the fingerprint stops being a hint about which credential ran and starts
/// being the credential. Sixteen keeps at most half of anything fingerprinted.
pub const MIN_FINGERPRINT_LEN: usize = 16;

/// How a masked value appears (EAC-FR-29).
///
/// A long value keeps its first four and last four characters around an ellipsis
/// — `sk-a…9f2x` — which is enough to tell one token from another, to line a
/// record up against the integration it came from, and to see that a stale
/// credential rather than a missing one was used, without carrying a value
/// anybody could authenticate with. A short value is removed entirely, because
/// eight characters of a twelve-character value is not a fingerprint.
///
/// Character counts, not byte counts: a non-ASCII value cut at four *bytes*
/// would end mid-character.
pub fn masked_form(secret: &str) -> String {
    if secret.chars().count() < MIN_FINGERPRINT_LEN {
        return REDACTED.to_string();
    }
    let head: String = secret.chars().take(4).collect();
    let tail: String = {
        let all: Vec<char> = secret.chars().collect();
        all[all.len() - 4..].iter().collect()
    };
    format!("{head}…{tail}")
}

/// Every occurrence of every secret, replaced by its masked form (EAC-FR-29).
///
/// By value against the secrets the caller passes, rather than by any guess at
/// what a credential looks like. A pattern would both miss a credential shaped
/// differently and mangle text that only resembled one; an exact match does
/// neither. What it cannot cover is a value the executor never holds: a vendor
/// whose credential is a directory it is forbidden to read (EAC-FR-16) can be
/// masked by its location but not by its contents, so a CLI that prints its own
/// login material is bounded by that vendor's protocol rather than by anything
/// here.
///
/// Longest first, so a secret that contains another — a session directory whose
/// path contains the project key, say — is masked as the whole value rather than
/// being broken up by the shorter one and left partly readable.
pub fn mask_secrets(text: &str, secrets: &[&str]) -> String {
    let mut ordered: Vec<&str> = secrets
        .iter()
        .copied()
        // An empty secret would match everywhere and replace nothing usefully.
        .filter(|secret| !secret.is_empty())
        .collect();
    ordered.sort_unstable_by_key(|secret| std::cmp::Reverse(secret.len()));
    ordered.dedup();

    let mut text = text.to_string();
    for secret in ordered {
        if text.contains(secret) {
            text = text.replace(secret, &masked_form(secret));
        }
    }
    text
}

/// The longest secret in a set, in bytes.
///
/// What an incremental masker has to hold back across a delivery boundary: a
/// value split between two pieces is a whole value in neither, so nothing
/// shorter than this can be safely emitted before the next piece arrives.
pub fn longest_secret(secrets: &[&str]) -> usize {
    secrets.iter().map(|s| s.len()).max().unwrap_or(0)
}

/// The tail of a failed run's stderr, bounded and with every credential this
/// launch holds masked out (EAC-FR-29).
///
/// The *tail* rather than the head: a CLI that refuses its arguments and a CLI
/// that dies deep into a run both put the sentence that explains them last, and
/// a head would show the banner of the one and the warm-up of the other.
///
/// Masking is [`mask_secrets`]'s, which is the same rule every other thing this
/// module hands a reader is masked by — one rule rather than one per surface.
///
/// `None` where there is nothing to say. The field is still written, as a null
/// — `log_fields!` inserts every key it is given — but a null reads as "the
/// agent said nothing on stderr", which is itself worth knowing, rather than as
/// an empty string a reader has to interpret.
pub fn stderr_excerpt(stderr: &[u8], truncated: bool, secrets: &[&str]) -> Option<String> {
    // Masked over the whole stream and cut afterwards, never the other way
    // round. A credential lying across the cut is present in neither half as a
    // whole value, so masking the tail alone would leave the end of one in the
    // excerpt with nothing left to match it against — and EAC-FR-15 admits no
    // fragment. The cost is one copy of a stream `LIMIT_STDERR` already bounds,
    // on a path a run reaches only by failing.
    let mut text = mask_secrets(&String::from_utf8_lossy(stderr), secrets);

    // The same hazard one cut earlier, and the one masking cannot answer.
    // Capture keeps the *first* `LIMIT_STDERR` bytes, so a stream that outran
    // that bound ends mid-line — and a credential lying across *that* cut is a
    // whole value nowhere in what was captured, leaving nothing for the loop
    // above to have matched. The final line of a truncated capture is
    // incomplete by definition and is what the excerpt would otherwise end on,
    // so it goes whole. A capture with no line ending at all is one incomplete
    // line and has nothing left to say once it is dropped.
    if truncated {
        match text.rfind('\n') {
            Some(end) => text.truncate(end),
            None => return None,
        }
    }

    let text = text.trim();
    if text.is_empty() {
        return None;
    }

    let cut = text.len().saturating_sub(LIMIT_STDERR_EXCERPT);
    if cut == 0 {
        return Some(text.to_string());
    }
    // A cut lands mid-character on a multi-byte sequence. Advancing to the next
    // boundary drops at most three bytes and keeps the excerpt from opening on
    // half a character.
    let start = (cut..text.len()).find(|i| text.is_char_boundary(*i)).unwrap_or(text.len());
    // Said rather than implied: an excerpt that begins mid-sentence is not the
    // whole of what the agent wrote, and a reader deciding whether they have the
    // cause needs to know which they are looking at.
    Some(format!("…{}", text[start..].trim_start()))
}

/// The narrowest deadline a turn may be given. Below this no agent can reach
/// its own service and answer, so a smaller value does not express impatience —
/// it guarantees a `timeout` outcome that says nothing about the work.
pub const MIN_TIMEOUT_MS: u64 = 1_000;
/// The widest. A turn is one agent call, not a background job; a deadline
/// beyond this holds a container, a worktree mount, and a live credential open
/// for longer than any caller here has a reason to.
pub const MAX_TIMEOUT_MS: u64 = 6 * 60 * 60 * 1_000;

/// Which named limit a payload exceeded. Carried by `RequestTooLarge` so a
/// caller learns what to shrink (EAC-FR-08).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitName {
    Task,
    Instruction,
    Input,
    ResumeField,
}

impl LimitName {
    pub fn as_str(self) -> &'static str {
        match self {
            LimitName::Task => "task",
            LimitName::Instruction => "instruction",
            LimitName::Input => "input",
            LimitName::ResumeField => "resume_field",
        }
    }
}

// ---------------------------------------------------------------------------
// The task envelope
// ---------------------------------------------------------------------------

/// How cancellation is arranged. One variant, spelled out rather than implied,
/// so a task document that claims anything else is rejected instead of being
/// read as the default (EAC-FR-07).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Cancellation {
    #[default]
    #[serde(rename = "caller_controlled")]
    CallerControlled,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionControls {
    pub timeout_ms: u64,
    pub cancellation: Cancellation,
}

/// A resumable-session reference, on the way out (a task asking to continue) or
/// on the way back (an envelope reporting what could be continued).
///
/// Never carries a credential — a session id is a handle the vendor issued, and
/// EAC-FR-21 keeps it to that.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SessionRef {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub continuation_token: Option<String>,
}

impl SessionRef {
    pub fn is_empty(&self) -> bool {
        self.session_id.is_none() && self.continuation_token.is_none()
    }

    /// A reference the vendor supplied, reduced to what may be carried
    /// (EAC-FR-21).
    ///
    /// Two things are dropped rather than passed on. A field past its byte
    /// bound is not a session handle any vendor issued — a bound applies to a
    /// value whatever produced it, and the envelope's own fields are already
    /// checked, so this is the path that would otherwise return one unbounded.
    /// An *empty* field is worse than absent: `Some("")` reads as "a session
    /// exists" to a caller, and a later turn resuming on it is silently
    /// filtered back to no-resume, so the turn starts fresh while the caller
    /// believes it continued.
    pub fn sanitized(mut self) -> Self {
        for field in [&mut self.session_id, &mut self.continuation_token] {
            if field.as_ref().is_some_and(|v| v.is_empty() || v.len() > LIMIT_SESSION_FIELD) {
                *field = None;
            }
        }
        self
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentTaskRequest {
    pub protocol_version: u32,
    pub instruction: String,
    #[serde(default)]
    pub input: Option<Map<String, Value>>,
    /// EAC-FR-43: which fixed result shape this task's answer must take, named
    /// rather than supplied. `None` leaves `result` free-form.
    #[serde(default)]
    pub result_contract: Option<ResultContract>,
    #[serde(default)]
    pub resume: Option<SessionRef>,
    pub execution: ExecutionControls,
}

/// EAC-FR-43: the closed set of result shapes a caller may name.
///
/// An identifier is all a caller supplies. The document itself is the
/// executor's, compiled in and selected by this name, which is what keeps a
/// caller from routing project material onto an argument vector through a
/// schema of its own (EAC-FR-09). An identifier outside the set does not decode,
/// so it is refused before any container is created (EAC-FR-07).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResultContract {
    /// The verdict of `../core/GRD-graduation.md` GRL-FR-VIAT, or the
    /// `escalate_to_user` request GRL-FR-VBCL admits in place of one.
    ReviewVerdict,
    /// GRL-FR-OTRH: the verdict a validation turn returns. `GraduationVerdict`
    /// and `ReviewVerdict` carry the same finding shape and stay two schemas
    /// (GRL-FR-OTRH), so neither is read where the other is expected.
    GraduationVerdict,
    /// EAC-FR-43 / EAC-FR-ESXZ: the `PullRequestSummary` of
    /// `../ai/GRL-graduation-loop.md` GRL-FR-ZZGO that a PR summary run answers
    /// with.
    PullRequestSummary,
    /// EAC-FR-43 / EAC-FR-OFIV: the `WorkResult` of
    /// `../ai/GRL-graduation-loop.md` GRL-FR-IJTT that a `work` and a
    /// `merge_work` turn answer success with.
    WorkResult,
}

impl ResultContract {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResultContract::ReviewVerdict => "review_verdict",
            ResultContract::GraduationVerdict => "graduation_verdict",
            ResultContract::PullRequestSummary => "pull_request_summary",
            ResultContract::WorkResult => "work_result",
        }
    }

    /// EAC-FR-43 / EAC-FR-44: the document this name selects, as fixed text.
    pub fn schema_text(&self) -> &'static str {
        match self {
            ResultContract::ReviewVerdict => REVIEW_VERDICT_SCHEMA,
            ResultContract::GraduationVerdict => GRADUATION_VERDICT_SCHEMA,
            ResultContract::PullRequestSummary => PULL_REQUEST_SUMMARY_SCHEMA,
            ResultContract::WorkResult => WORK_RESULT_SCHEMA,
        }
    }

    /// The same document parsed once, for the task document that carries it to
    /// every vendor.
    pub fn schema_value(&self) -> &'static Value {
        match self {
            ResultContract::ReviewVerdict => &REVIEW_VERDICT_SCHEMA_VALUE,
            ResultContract::GraduationVerdict => &GRADUATION_VERDICT_SCHEMA_VALUE,
            ResultContract::PullRequestSummary => &PULL_REQUEST_SUMMARY_SCHEMA_VALUE,
            ResultContract::WorkResult => &WORK_RESULT_SCHEMA_VALUE,
        }
    }
}

/// EAC-FR-44: the `review_verdict` document.
///
/// Draft-07, like the envelope schema it is placed inside (CCP-FR-28). It admits
/// exactly three answers and no other: a `ready` verdict carrying no finding, a
/// `revise` verdict carrying at least one, and an `escalate_to_user` request in
/// place of a verdict.
///
/// **It states what the application's own validation states, and no rule more
/// freely.** Two rules of that validation are absent, both because this document
/// cannot carry them: the stable finding order of GRL-FR-VIAT, which is a
/// comparison between findings rather than a shape, and the rule that every
/// affected path is one the implementation change set holds, which is knowable
/// only from the manifest the application computed and is exactly the project
/// material EAC-FR-09 keeps off an argument vector. Both stay with the
/// application and both still refuse a verdict whole.
///
/// The closed field set of the verdict and of each finding is the one rule
/// stated here that the validation does not enforce, and it is deliberate: an
/// undefined key is the mark of an answer written to a **different** contract —
/// the agent's own tools include a findings-shaped one — and the answer that
/// borrows another contract's field names borrows its `summary` where this one
/// requires a `description`. Refusing it costs one re-prompt inside the run
/// (CCP-FR-28); accepting it costs the whole turn, outside it. The
/// `escalate_to_user` branch is left open because the application reads that
/// request from a result whatever else the result carries (GRL-FR-VBCL).
pub const REVIEW_VERDICT_SCHEMA: &str = concat!(
    r#"{"oneOf":[{"type":"object","additionalProperties":false,"properties":{"#,
    r#""verdict":{"type":"string","enum":["ready"]},"#,
    r#""rationale":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""findings":{"type":"array","maxItems":0},"#,
    // EAC-FR-ZWYE: the optional run summary of GRL-FR-FRNJ, on a `ready`
    // verdict alone, with a closed field set. The caller drops a summary that
    // breaks the rules and keeps the verdict (GRL-FR-ECES).
    r#""summary":{"type":"object","additionalProperties":false,"properties":{"#,
    r#""title":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""description":{"type":"string","minLength":1,"pattern":"\\S"}},"#,
    r#""required":["title","description"]}},"#,
    r#""required":["verdict","rationale","findings"]},"#,
    r#"{"type":"object","additionalProperties":false,"properties":{"#,
    r#""verdict":{"type":"string","enum":["revise"]},"#,
    r#""rationale":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""findings":{"type":"array","minItems":1,"items":{"#,
    r#""type":"object","additionalProperties":false,"properties":{"#,
    r#""severity":{"type":"string","enum":["critical","major","minor"]},"#,
    r#""description":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    // Null is admitted because the validation reads an absent or null list as
    // the empty one, and a schema that refused it would refuse in the run what
    // the application accepts after it.
    r#""affected_files":{"type":["array","null"],"items":{"type":"string"}},"#,
    r#""correction":{"type":"string","minLength":1,"pattern":"\\S"}},"#,
    r#""required":["severity","description","correction"]}}},"#,
    r#""required":["verdict","rationale","findings"]},"#,
    // The escalation branch, on the terms `escalate_to_user` is decoded under:
    // its own decoder tolerates a key it does not define, so nothing is closed
    // here either.
    r#"{"type":"object","properties":{"escalate_to_user":{"#,
    r#""type":"object","properties":{"#,
    r#""reason":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""questions":{"type":"array","minItems":1,"maxItems":8,"items":{"#,
    r#""type":"object","properties":{"#,
    r#""question":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    // Null on the same terms `affected_files` admits it: this request's own
    // decoder reads an absent or null list as no options at all, and a schema
    // that refused it would refuse in the run what the application accepts
    // after it.
    r#""options":{"type":["array","null"],"maxItems":3,"items":{"#,
    r#""type":"object","properties":{"#,
    r#""answer":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""summary":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""description":{"type":"string","minLength":1,"pattern":"\\S"}},"#,
    r#""required":["answer","summary","description"]}}},"#,
    r#""required":["question"]}}},"#,
    r#""required":["reason","questions"]}},"#,
    r#""required":["escalate_to_user"]}]}"#,
);

/// [`REVIEW_VERDICT_SCHEMA`] parsed once. A document that does not parse is a
/// programming error in this file rather than anything a caller can reach, so it
/// resolves to `null` rather than taking the process down — a task document
/// carrying a null schema is one the agent reads no shape from, which is the
/// same position it stands in when no contract is named.
static REVIEW_VERDICT_SCHEMA_VALUE: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(REVIEW_VERDICT_SCHEMA).unwrap_or(Value::Null));

/// EAC-FR-44: the `graduation_verdict` document (GRL-FR-OTRH).
///
/// It admits exactly two answers: a `ready` verdict carrying no finding and a
/// `revise` verdict carrying at least one. It carries **no `escalate_to_user`
/// branch**, a validation turn asking by answering `escalation_required`
/// instead (GXD-FR-HGSU), so a request written into the result is an answer to a
/// contract this turn was not given.
///
/// It states what the application's own validation states and no rule more
/// freely. Two rules of that validation are absent because this document cannot
/// carry them: the stable finding order of GRL-FR-VIAT, which is a comparison
/// between findings rather than a shape, and the set an affected path may be
/// drawn from (GRL-FR-VIAT), which is knowable only from the manifest and the
/// classification the application computed. Both stay with the application and
/// both still refuse a verdict whole.
pub const GRADUATION_VERDICT_SCHEMA: &str = concat!(
    r#"{"oneOf":[{"type":"object","additionalProperties":false,"properties":{"#,
    r#""verdict":{"type":"string","enum":["ready"]},"#,
    r#""rationale":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""findings":{"type":"array","maxItems":0}},"#,
    r#""required":["verdict","rationale","findings"]},"#,
    r#"{"type":"object","additionalProperties":false,"properties":{"#,
    r#""verdict":{"type":"string","enum":["revise"]},"#,
    r#""rationale":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""findings":{"type":"array","minItems":1,"items":{"#,
    r#""type":"object","additionalProperties":false,"properties":{"#,
    r#""severity":{"type":"string","enum":["critical","major","minor"]},"#,
    r#""description":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    // Null on the same terms the review document admits it: the validation
    // reads an absent or null list as the empty one, and a schema that refused
    // it would refuse in the run what the application accepts after it.
    r#""affected_files":{"type":["array","null"],"items":{"type":"string"}},"#,
    r#""correction":{"type":"string","minLength":1,"pattern":"\\S"}},"#,
    r#""required":["severity","description","correction"]}}},"#,
    r#""required":["verdict","rationale","findings"]}]}"#,
);

/// [`GRADUATION_VERDICT_SCHEMA`] parsed once, on the terms
/// [`REVIEW_VERDICT_SCHEMA_VALUE`] is parsed under.
static GRADUATION_VERDICT_SCHEMA_VALUE: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(GRADUATION_VERDICT_SCHEMA).unwrap_or(Value::Null));

/// EAC-FR-ESXZ: the `pull_request_summary` document (GRL-FR-ZZGO).
///
/// It admits exactly one object: a non-blank `title` and a non-blank
/// `description`, with a closed field set. It has **no `escalate_to_user`
/// branch**, because a PR summary run has no answer to an escalation
/// (GRL-FR-UNKD). The rule that the title is one line stays with the
/// application, which trims it and refuses a title that holds a line break.
pub const PULL_REQUEST_SUMMARY_SCHEMA: &str = concat!(
    r#"{"type":"object","additionalProperties":false,"properties":{"#,
    r#""title":{"type":"string","minLength":1,"pattern":"\\S"},"#,
    r#""description":{"type":"string","minLength":1,"pattern":"\\S"}},"#,
    r#""required":["title","description"]}"#,
);

/// [`PULL_REQUEST_SUMMARY_SCHEMA`] parsed once, on the terms
/// [`REVIEW_VERDICT_SCHEMA_VALUE`] is parsed under.
static PULL_REQUEST_SUMMARY_SCHEMA_VALUE: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(PULL_REQUEST_SUMMARY_SCHEMA).unwrap_or(Value::Null));

/// EAC-FR-OFIV / EAC-FR-UEFX: the `work_result` document (GRL-FR-IJTT).
///
/// It admits exactly one `WorkResult`, with closed field sets: `checks`, at
/// most 64 records, and an optional non-blank `no_checks_reason`.
///
/// **It states the shape alone.** It admits every integer `exit_status`, a
/// `finished` of `false`, and an empty `checks` without `no_checks_reason`. A
/// CLI that validates the result prompts the agent again on a mismatch, so a
/// document that required exit status 0 would push an honest non-zero status
/// to a false 0. The rules of the check gate (GRL-FR-CPXI) stay with the
/// caller, which applies them after the turn ends. It has no `escalate_to_user`
/// branch: a work turn asks by answering `escalation_required`.
pub const WORK_RESULT_SCHEMA: &str = concat!(
    r#"{"type":"object","additionalProperties":false,"properties":{"#,
    r#""checks":{"type":"array","maxItems":64,"items":{"#,
    r#""type":"object","additionalProperties":false,"properties":{"#,
    r#""name":{"type":"string","minLength":1,"maxLength":128,"pattern":"\\S"},"#,
    r#""command":{"type":"string","minLength":1,"maxLength":1024,"pattern":"\\S"},"#,
    r#""finished":{"type":"boolean"},"#,
    r#""exit_status":{"type":["integer","null"]},"#,
    r#""waived_by":{"type":"integer","minimum":1}},"#,
    r#""required":["name","command","finished","exit_status"]}},"#,
    r#""no_checks_reason":{"type":"string","minLength":1,"maxLength":1024,"pattern":"\\S"}},"#,
    r#""required":["checks"]}"#,
);

/// [`WORK_RESULT_SCHEMA`] parsed once, on the terms
/// [`REVIEW_VERDICT_SCHEMA_VALUE`] is parsed under.
static WORK_RESULT_SCHEMA_VALUE: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(WORK_RESULT_SCHEMA).unwrap_or(Value::Null));

/// EAC-FR-35: the shape of an answer, in words, for every vendor and every
/// caller.
///
/// It is here rather than in each caller's `instruction` because it is the
/// executor's protocol and not any caller's business: a loop that had to restate
/// it would be restating something it does not own, and one that forgot would
/// leave its agent guessing. One pinned vendor cannot be handed a schema at all
/// (CDX-FR-14) and no caller chooses its vendor (EAC-FR-03), so a contract
/// stated only where a schema reaches is one that half the runs never receive.
///
/// It carries no project material, no captured prompt, no path, and no
/// credential, which is what makes it the executor's to add rather than task
/// material a caller routed through (EAC-FR-09).
const RESPONSE_CONTRACT_FILE: &str =
    include_str!("../../../../resources/prompts/agent-exec/response-contract.md");

/// EAC-FR-35: the contract as an agent receives it — the file with its
/// authoring comments removed and the blank space they left closed up.
///
/// A comment naming the requirement the file satisfies is a note to whoever
/// maintains it, and an agent handed one would read it as part of the protocol
/// it is being told to answer under. What reaches stdin therefore begins at the
/// contract's first real word and holds nothing addressed to a reader of this
/// repository.
pub static RESPONSE_CONTRACT: LazyLock<String> =
    LazyLock::new(|| crate::prompts::instruction_text(RESPONSE_CONTRACT_FILE));

/// What actually reaches the agent on stdin (EAC-FR-35).
///
/// The caller's task, unaltered, carrying the one field the executor supplies
/// and no caller can. Serialization only: `AgentTaskRequest` is what a caller
/// hands in, and it defines no `response_contract`, so a submitted document
/// carrying the key is refused as the undefined field it is (EAC-FR-07).
///
/// Flattened rather than assembled through a `Map` because this build's
/// `serde_json` orders a map's keys alphabetically, which would put `execution`
/// ahead of `instruction` and make the invariant half of every document sort
/// after the part that varies. Field order here is declaration order, so the
/// text that never changes stays the document's prefix.
#[derive(Serialize)]
struct StdinDocument<'a> {
    #[serde(flatten)]
    task: &'a AgentTaskRequest,
    response_contract: &'a str,
    /// EAC-FR-43: the document the caller's `result_contract` names, carried to
    /// **every** vendor. One pinned vendor enforces it inside the run
    /// (CCP-FR-28) and one enforces nothing (CDX-FR-14); the shape reaches the
    /// agent either way, because no caller chooses its vendor (EAC-FR-03).
    #[serde(skip_serializing_if = "Option::is_none")]
    result_schema: Option<&'a Value>,
}

/// Why a task document is not usable. Distinct from a *size* refusal, which
/// names its limit instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskInvalid {
    /// Not version 1.
    ProtocolVersion(u32),
    /// `instruction` is empty or whitespace only.
    InstructionEmpty,
    /// The document did not decode: an unknown field, a wrong type, a missing
    /// required key, or a `cancellation` this build does not recognise.
    Malformed,
    /// `execution.timeout_ms` outside [`MIN_TIMEOUT_MS`]..=[`MAX_TIMEOUT_MS`].
    /// Refused rather than clamped (TLC-FR-20): a caller that asked for an
    /// impossible deadline has made a mistake, and quietly substituting a
    /// workable one hides it behind a turn that behaves nothing like what was
    /// asked for.
    TimeoutOutOfRange(u64),
}

impl TaskInvalid {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskInvalid::ProtocolVersion(_) => "protocol_version",
            TaskInvalid::InstructionEmpty => "instruction_empty",
            TaskInvalid::Malformed => "malformed",
            TaskInvalid::TimeoutOutOfRange(_) => "timeout_out_of_range",
        }
    }
}

impl AgentTaskRequest {
    /// EAC-FR-07: everything the document has to satisfy before it is worth
    /// serializing, let alone launching a container for.
    pub fn validate(&self) -> Result<(), TaskInvalid> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(TaskInvalid::ProtocolVersion(self.protocol_version));
        }
        if self.instruction.trim().is_empty() {
            return Err(TaskInvalid::InstructionEmpty);
        }
        let timeout = self.execution.timeout_ms;
        if !(MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&timeout) {
            return Err(TaskInvalid::TimeoutOutOfRange(timeout));
        }
        Ok(())
    }

    /// EAC-FR-08: the per-field bounds, checked before the whole-document one so
    /// an oversized instruction is named as such rather than as an oversized
    /// task.
    pub fn check_limits(&self) -> Result<(), LimitName> {
        if self.instruction.len() > LIMIT_INSTRUCTION {
            return Err(LimitName::Instruction);
        }
        if let Some(input) = &self.input {
            if serde_json::to_vec(input).map_or(0, |b| b.len()) > LIMIT_INPUT {
                return Err(LimitName::Input);
            }
        }
        if let Some(resume) = &self.resume {
            for field in [&resume.session_id, &resume.continuation_token] {
                if field.as_ref().is_some_and(|v| v.len() > LIMIT_RESUME_FIELD) {
                    return Err(LimitName::ResumeField);
                }
            }
        }
        Ok(())
    }

    /// The bytes that reach the agent's stdin, and the only transport they take
    /// (EAC-FR-09).
    ///
    /// EAC-FR-35's contract is added before the whole-document bound is applied,
    /// so what [`LIMIT_TASK`] is checked against is the document the agent
    /// actually receives rather than the part of it the caller supplied.
    pub fn to_stdin_bytes(&self) -> Result<Vec<u8>, LimitName> {
        self.check_limits()?;
        let document = StdinDocument {
            task: self,
            response_contract: &RESPONSE_CONTRACT,
            result_schema: self.result_contract.map(|c| c.schema_value()),
        };
        let bytes = serde_json::to_vec(&document).map_err(|_| LimitName::Task)?;
        if bytes.len() > LIMIT_TASK {
            return Err(LimitName::Task);
        }
        Ok(bytes)
    }
}
