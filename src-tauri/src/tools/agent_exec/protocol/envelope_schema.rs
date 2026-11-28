//! The response envelope's JSON Schema (EAC-FR-18), as one fixed document.
//!
//! Split out of `protocol.rs` so that each vendor module reads the executor's
//! own document rather than another vendor's (EAC-FR-11, EAC-FR-12).

/// The `AgentResponseEnvelope` schema, as fixed text compiled into the binary.
///
/// The executor's own document: Claude Code's `--json-schema` carries it
/// (CCP-FR-07, CCP-FR-08), and Codex's strict schema is composed from it
/// (CDX-FR-XIGW).
///
/// It is never assembled per call, never derived from the task, and carries no
/// project material and no credential — which is the whole reason a schema may
/// ride on the argument vector while the task may not (EAC-FR-09).
///
/// It states every constraint `protocol.rs` enforces, and not a weaker set. A
/// field the schema offers more freely than the envelope rules accept is a trap
/// rather than a convenience: the agent fills it in, the CLI validates it, and
/// the turn is refused afterwards for something the agent was told it could do.
///
/// Draft-07, because that is the draft Claude Code validates against.
pub const ENVELOPE_SCHEMA: &str = concat!(
    r#"{"$schema":"http://json-schema.org/draft-07/schema#","type":"object","#,
    r#""properties":{"#,
    r#""protocol_version":{"type":"integer","enum":[1]},"#,
    r#""outcome":{"type":"string","enum":["success","failure","escalation_required"]},"#,
    r#""summary":{"type":"string","minLength":1,"maxLength":8192},"#,
    r#""result":{"type":["object","null"]},"#,
    r#""failure":{"type":["object","null"],"additionalProperties":false,"properties":{"#,
    r#""code":{"type":"string"},"message":{"type":"string"},"retryable":{"type":"boolean"}},"#,
    r#""required":["code","message","retryable"]},"#,
    // EAC-FR-18 / EAC-FR-36: the escalation is stated to its full depth — the
    // question count, the option count each question allows, and the format each
    // proposed response's three fields keep. A question set composed to a shape
    // nobody stated is refused after a whole turn was spent on it, and an agent
    // that must guess how many questions it may ask asks one and loses the rest.
    r#""escalation":{"type":["object","null"],"additionalProperties":false,"#,
    // ESU-FR-VTAZ / EAC-FR-18: a plan update names its kind and may ask no
    // question. Every other escalation asks one at least.
    r#""if":{"required":["kind"]},"then":{"properties":{"questions":{"minItems":0}}},"#,
    r#""else":{"properties":{"questions":{"minItems":1}}},"properties":{"#,
    r#""kind":{"type":"string","enum":["plan_update"]},"#,
    // `pattern` rather than `minLength` alone: the decoder trims before it
    // tests for emptiness, so a schema that accepted `"   "` would let an agent
    // answer inside the schema and be refused by the decoder afterwards — the
    // one failure CCP-FR-07 is about.
    //
    // CCP-FR-07: each `maxLength` equals its EAC-FR-08 limit, which counts
    // characters as JSON Schema does, so the CLI corrects an overlong value
    // inside the run. Word and sentence counts are guidance (EAC-FR-35), so this
    // schema does not state them and the decoder does not enforce them.
    r#""reason":{"type":"string","minLength":1,"maxLength":4096,"pattern":"\\S"},"#,
    r#""questions":{"type":"array","minItems":0,"maxItems":8,"items":{"#,
    r#""type":"object","additionalProperties":false,"properties":{"#,
    r#""question":{"type":"string","minLength":1,"maxLength":2048,"pattern":"\\S"},"#,
    // Not required: `AgentEscalationQuestion` defaults it, and a question with
    // no fixed set of answers leaves it out on purpose. Demanding it here would
    // re-prompt an agent that had answered correctly.
    r#""options":{"type":"array","maxItems":3,"items":{"#,
    r#""type":"object","additionalProperties":false,"properties":{"#,
    r#""answer":{"type":"string","minLength":1,"maxLength":512,"pattern":"\\S"},"#,
    r#""summary":{"type":"string","minLength":1,"maxLength":128,"pattern":"\\S"},"#,
    r#""description":{"type":"string","minLength":1,"maxLength":256,"pattern":"\\S"}},"#,
    r#""required":["answer","summary","description"]}}},"#,
    r#""required":["question"]}}},"#,
    r#""required":["reason","questions"]},"#,
    r#""session":{"type":["object","null"],"additionalProperties":false,"properties":{"#,
    r#""session_id":{"type":["string","null"]},"continuation_token":{"type":["string","null"]}}},"#,
    // EAC-FR-20: the v1 allowlist is empty, and the schema says so. Without
    // `maxProperties`, the schema offers a free-form object that the envelope
    // rules then refuse — so an agent fills the field in good faith, the CLI
    // validates it, and the turn is rejected after it has already been paid
    // for. Stating the bound here is what makes CCP-FR-07 correct the agent
    // inside the run instead.
    r#""metadata":{"type":["object","null"],"maxProperties":0}},"#,
    // `deny_unknown_fields` on the envelope and on each of its objects, said in
    // the schema's own words. Without it an agent's extra key passes this
    // validation and is refused by the decoder afterwards — the same shape of
    // loss the `metadata` bound above exists to prevent, at a different field.
    // `result` and `metadata` are deliberately not closed: `result` is
    // free-form by contract, and `metadata` is bounded to no keys at all.
    r#""additionalProperties":false,"#,
    r#""required":["protocol_version","outcome","summary"]}"#,
);
