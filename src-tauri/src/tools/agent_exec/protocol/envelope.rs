//! The response envelope an agent answers with, and the rules that decode it
//! (EAC-FR-18, EAC-FR-19, EAC-FR-36).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::*;

// ---------------------------------------------------------------------------
// The response envelope
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentOutcome {
    Success,
    Failure,
    EscalationRequired,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

/// EAC-FR-08: the length of a text limit, in Unicode code points.
fn chars(text: &str) -> usize {
    text.chars().count()
}

/// EAC-FR-18: one concrete response an agent proposes to one of its questions.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentProposedResponse {
    /// The value submitted when this response is chosen.
    pub answer: String,
    /// A label for the choice, submitted with the answer.
    pub summary: String,
    /// Display-only.
    pub description: String,
}

/// EAC-FR-18 / EAC-FR-36: one question an agent stopped to ask.
///
/// It carries no identity of its own. The order the questions arrive in is the
/// only identity a question has, and the caller numbers them itself.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentEscalationQuestion {
    pub question: String,
    /// 0 to 3; empty where no fixed response suits.
    #[serde(default)]
    pub options: Vec<AgentProposedResponse>,
}

/// EAC-FR-18: the whole of what an `escalation_required` turn stopped for.
///
/// A singular `question` or `options` field at this level is an undefined field
/// and is refused as one, which `deny_unknown_fields` is the whole of: an agent
/// answering the previous single-question shape is corrected rather than half
/// understood.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentEscalation {
    pub reason: String,
    /// 1 to 8, in the order they are to be answered.
    pub questions: Vec<AgentEscalationQuestion>,
}

impl AgentEscalation {
    /// EAC-FR-18 / EAC-FR-36: whole or not at all.
    ///
    /// One malformed question or one malformed proposed response refuses the
    /// envelope entire rather than yielding an escalation carrying the questions
    /// that happened to parse: a caller that paused a run on a partial question
    /// set would ask its author to settle less than the agent stopped for, and
    /// the agent would receive answers to questions it did not recognise.
    fn validate(&self) -> Result<(), EnvelopeInvalid> {
        if self.reason.trim().is_empty() {
            return Err(EnvelopeInvalid::EscalationShape);
        }
        if !(MIN_ESCALATION_QUESTIONS..=MAX_ESCALATION_QUESTIONS).contains(&self.questions.len()) {
            return Err(EnvelopeInvalid::EscalationShape);
        }
        for question in &self.questions {
            if question.question.trim().is_empty() {
                return Err(EnvelopeInvalid::EscalationShape);
            }
            if question.options.len() > MAX_QUESTION_OPTIONS {
                return Err(EnvelopeInvalid::EscalationShape);
            }
            for option in &question.options {
                if option.answer.trim().is_empty()
                    || option.summary.trim().is_empty()
                    || option.description.trim().is_empty()
                {
                    return Err(EnvelopeInvalid::EscalationShape);
                }
            }
        }
        self.check_limits()
    }

    /// EAC-FR-08: the whole object in bytes, and each text in characters, so
    /// that a text limit equals the `maxLength` of the vendor schema.
    fn check_limits(&self) -> Result<(), EnvelopeInvalid> {
        if serde_json::to_vec(self).map_or(usize::MAX, |b| b.len()) > LIMIT_ESCALATION {
            return Err(EnvelopeInvalid::TooLarge);
        }
        if chars(&self.reason) > LIMIT_ESCALATION_REASON {
            return Err(EnvelopeInvalid::TooLarge);
        }
        for question in &self.questions {
            if chars(&question.question) > LIMIT_ESCALATION_QUESTION {
                return Err(EnvelopeInvalid::TooLarge);
            }
            for option in &question.options {
                if chars(&option.answer) > LIMIT_OPTION_ANSWER
                    || chars(&option.summary) > LIMIT_OPTION_SUMMARY
                    || chars(&option.description) > LIMIT_OPTION_DESCRIPTION
                {
                    return Err(EnvelopeInvalid::TooLarge);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentResponseEnvelope {
    pub protocol_version: u32,
    pub outcome: AgentOutcome,
    pub summary: String,
    #[serde(default)]
    pub result: Option<Map<String, Value>>,
    #[serde(default)]
    pub failure: Option<AgentFailure>,
    #[serde(default)]
    pub escalation: Option<AgentEscalation>,
    #[serde(default)]
    pub session: Option<SessionRef>,
    #[serde(default)]
    pub metadata: Option<Map<String, Value>>,
}

/// Why an envelope is not a valid report of a turn.
///
/// Every one of these becomes `invalid_structured_output` (EAC-FR-19), and the
/// result carries the variant as its `refusal` (EAC-FR-22). A caller can then
/// tell the agent what to send again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeInvalid {
    /// Nothing decodable was found where the descriptor said to look.
    NotFound,
    /// Prose, partial JSON, a wrong type, or an unknown field.
    Malformed,
    /// More than one response document.
    MultipleDocuments,
    /// Bytes that are not valid UTF-8 where JSON was required.
    NotUtf8,
    /// Not version 1.
    ProtocolVersion,
    /// `summary` is empty.
    SummaryEmpty,
    /// The `outcome`/`result`/`failure`/`escalation` exclusivity rules.
    Exclusivity,
    /// A structure rule of the `escalation` object (EAC-FR-18).
    EscalationShape,
    /// A field exceeded its EAC-FR-08 bound.
    TooLarge,
    /// stdout hit its capture bound, so what is there is a prefix rather than a
    /// document. Never parsed (EAC-FR-23).
    OutputTruncated,
    /// The vendor's own output says the run failed — a result document whose
    /// subtype is not success or whose error flag is set (CCP-FR-12), or a
    /// stream carrying a turn-failed event (CDX-FR-16).
    ///
    /// Distinct from every other variant because nothing was wrong with the
    /// *shape* of what arrived: the CLI reported a failed run, which is not an
    /// agent-reported outcome and never becomes one. It is reached under exit 0
    /// as well as under a non-zero exit, which is precisely why the exit status
    /// alone cannot stand in for it.
    VendorRunFailed,
    /// The run reported a session identity other than the one the executor
    /// assigned it (CCP-FR-16). Accepting it would hand the caller a reference
    /// that resumes a different conversation than the one that just ran.
    SessionMismatch,
}

impl EnvelopeInvalid {
    pub fn as_str(&self) -> &'static str {
        match self {
            EnvelopeInvalid::NotFound => "not_found",
            EnvelopeInvalid::Malformed => "malformed",
            EnvelopeInvalid::MultipleDocuments => "multiple_documents",
            EnvelopeInvalid::NotUtf8 => "not_utf8",
            EnvelopeInvalid::ProtocolVersion => "protocol_version",
            EnvelopeInvalid::SummaryEmpty => "summary_empty",
            EnvelopeInvalid::Exclusivity => "exclusivity",
            EnvelopeInvalid::EscalationShape => "escalation_shape",
            EnvelopeInvalid::TooLarge => "too_large",
            EnvelopeInvalid::OutputTruncated => "output_truncated",
            EnvelopeInvalid::VendorRunFailed => "vendor_run_failed",
            EnvelopeInvalid::SessionMismatch => "session_mismatch",
        }
    }

    /// EAC-FR-THUT: why the answer was refused, in fixed words. It holds no
    /// agent output, so a log or a run detail can carry it.
    pub fn sentence(&self) -> &'static str {
        match self {
            EnvelopeInvalid::NotFound => "the turn gave no answer",
            EnvelopeInvalid::Malformed => "the answer was not in the required shape",
            EnvelopeInvalid::MultipleDocuments => "the answer held more than one document",
            EnvelopeInvalid::NotUtf8 => "the output was not valid UTF-8 text",
            EnvelopeInvalid::ProtocolVersion => "the answer named an unknown protocol version",
            EnvelopeInvalid::SummaryEmpty => "the answer had an empty summary",
            EnvelopeInvalid::Exclusivity => {
                "the outcome of the answer did not match the fields it carried"
            }
            EnvelopeInvalid::EscalationShape => "the escalation broke a structure rule",
            EnvelopeInvalid::TooLarge => "a value in the answer was too long",
            EnvelopeInvalid::OutputTruncated => "the output went past the capture limit",
            EnvelopeInvalid::VendorRunFailed => "the agent CLI reported a failed run",
            EnvelopeInvalid::SessionMismatch => "the answer came from another session",
        }
    }
}

impl AgentResponseEnvelope {
    /// EAC-FR-18 / EAC-FR-21: everything beyond decoding that a turn can fail.
    ///
    /// EAC-FR-20's allowlist is not here. It removes a field rather than
    /// refusing an envelope, so it belongs beside the decode
    /// ([`AgentResponseEnvelope::allowlist_metadata`]) and not among the rules
    /// that end a turn.
    ///
    /// Private, and that is what keeps the two halves of EAC-FR-20 in step:
    /// [`decode_envelope`] is the only way to reach this, and it discards
    /// before it validates. A caller that could validate an envelope directly
    /// could hold one whose `metadata` was never emptied.
    ///
    /// Exclusivity is checked in both directions — the variant a turn claims
    /// must carry its own detail, *and* must carry neither of the others — so a
    /// `success` smuggling a `failure` object is as invalid as a `failure`
    /// without one. A caller reads `outcome` and acts on it; an envelope where
    /// the tag and the payload disagree has no reading that is safe to pick.
    fn validate(&self) -> Result<(), EnvelopeInvalid> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(EnvelopeInvalid::ProtocolVersion);
        }
        if self.summary.trim().is_empty() {
            return Err(EnvelopeInvalid::SummaryEmpty);
        }

        let ok = match self.outcome {
            AgentOutcome::Success => self.failure.is_none() && self.escalation.is_none(),
            AgentOutcome::Failure => {
                self.failure.is_some() && self.result.is_none() && self.escalation.is_none()
            }
            AgentOutcome::EscalationRequired => {
                self.escalation.is_some() && self.result.is_none() && self.failure.is_none()
            }
        };
        if !ok {
            return Err(EnvelopeInvalid::Exclusivity);
        }

        // EAC-FR-18: the escalation object is closed the same way the envelope
        // around it is. Checked here rather than at decode so an `escalation`
        // smuggled onto a `success` is refused for the exclusivity it broke
        // rather than for the shape of a field that outcome may not carry.
        if let Some(escalation) = &self.escalation {
            escalation.validate()?;
        }

        self.check_limits()
    }

    /// EAC-FR-20: every key the v1 allowlist does not hold, removed.
    ///
    /// The allowlist is empty, so this empties the field. Discarding rather
    /// than refusing is the point: the emptiness exists to stop transcript
    /// data, a filesystem claim, a prompt, or a credential arriving through an
    /// unreviewed key, and a key that never reaches the caller cannot carry any
    /// of them. Refusing achieved the same guard by destroying the whole turn
    /// with it — a completed hour of work thrown away over a field nothing
    /// reads. Callers see `None` either way, so no reader can tell the
    /// difference, and the vendor schema is where the agent is told the rule
    /// (`descriptor/claude_code.rs`) so this stays the backstop rather than the
    /// first line.
    fn allowlist_metadata(&mut self) {
        self.metadata = None;
    }

    fn check_limits(&self) -> Result<(), EnvelopeInvalid> {
        if chars(&self.summary) > LIMIT_SUMMARY {
            return Err(EnvelopeInvalid::TooLarge);
        }
        let serialized =
            |v: &Map<String, Value>| serde_json::to_vec(v).map_or(usize::MAX, |b| b.len());
        if self.result.as_ref().is_some_and(|r| serialized(r) > LIMIT_RESULT) {
            return Err(EnvelopeInvalid::TooLarge);
        }
        // EAC-FR-08 names a bound for `metadata`, so the bound is here. Under
        // v1 nothing reaches it: EAC-FR-20's allowlist is empty, so the field
        // is already `None` by the time validation runs. It is kept rather than
        // deleted because the day the allowlist admits its first reviewed key
        // is the day this becomes the only thing standing between that key and
        // an unbounded payload — and a limit re-added later is a limit somebody
        // has to remember.
        if self.metadata.as_ref().is_some_and(|m| serialized(m) > LIMIT_METADATA) {
            return Err(EnvelopeInvalid::TooLarge);
        }
        if let Some(session) = &self.session {
            for field in [&session.session_id, &session.continuation_token] {
                if field.as_ref().is_some_and(|v| v.len() > LIMIT_SESSION_FIELD) {
                    return Err(EnvelopeInvalid::TooLarge);
                }
            }
        }
        Ok(())
    }
}

/// Decode exactly one envelope from the JSON text the descriptor pointed at.
///
/// "Exactly one" is load-bearing: an agent that emitted two documents has told
/// us two different things about one turn, and picking either would be a guess.
/// `StreamDeserializer` is what distinguishes that from trailing whitespace,
/// which is not a second document.
pub fn decode_envelope(text: &str) -> Result<AgentResponseEnvelope, EnvelopeInvalid> {
    // EAC-FR-08 bounds the envelope in its own right, not only the stream that
    // carried it: stdout's bound is 8 MiB, so without this a 5 MiB envelope
    // arrives well inside the stream limit and is parsed in full.
    if text.len() > LIMIT_ENVELOPE {
        return Err(EnvelopeInvalid::TooLarge);
    }

    let mut stream = serde_json::Deserializer::from_str(text).into_iter::<AgentResponseEnvelope>();

    let mut first = match stream.next() {
        Some(Ok(envelope)) => envelope,
        Some(Err(_)) => return Err(EnvelopeInvalid::Malformed),
        None => return Err(EnvelopeInvalid::NotFound),
    };

    // Anything decodable after the first document — valid or not — means the
    // stream held more than one value.
    if stream.next().is_some() {
        return Err(EnvelopeInvalid::MultipleDocuments);
    }

    // EAC-FR-20, before validation and before the caller sees anything: an
    // unreviewed key is removed rather than allowed to end the turn.
    first.allowlist_metadata();
    first.validate()?;
    Ok(first)
}
