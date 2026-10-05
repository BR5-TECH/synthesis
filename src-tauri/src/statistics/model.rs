//! The event vocabulary and the totals it folds to
//! (`../../../specifications/core/DSS-draft-statistics-storage.md`).
//!
//! Every line of a draft's log is one [`StatisticsEvent`]: the envelope of
//! DSS-FR-MHJC — `v`, `event_id`, `draft_id`, `at`, `type` — and the fields that
//! type defines. The vocabulary is closed (DSS-FR-XBGA): there is no event that
//! edits, retracts, or corrects another, because the fold reconciles duplicates
//! by identity rather than by amendment (DSS-FR-PNUE).
//!
//! DSS-FR-XRDM: no shape here has a field that could carry a prompt, a
//! candidate, a rationale, a comment body, an exchange message, a tool argument,
//! a tool result, agent output, or any part of a credential. An event carries
//! instants, identities, counts, a bucket, a decision, and reported token
//! numbers, and the log is committed, so that closure is what keeps a draft's
//! account safe to share.

use serde::{Deserialize, Serialize};

/// DSS-FR-LDFK: the schema version of every line this build writes.
pub const SCHEMA_VERSION: u32 = 1;

/// DSS-FR-QLEH: the work an agent or model operation supports, which is what
/// decides which bucket its duration and its reported tokens are read in.
///
/// Settled by the module that records the event — `GRD-graduation.md`
/// GRD-FR-NHRY for a run's operations, and this module's own conversational
/// path for `Refinement` — and applied rather than re-derived by the fold.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    Refinement,
    Authoring,
    ValidationHandoffPublication,
    Implementation,
    Reviews,
    Reconciliation,
}

impl Bucket {
    /// DSS-FR-IYRG: the six, in the one order every reading of them uses.
    pub const ORDER: [Bucket; 6] = [
        Bucket::Refinement,
        Bucket::Authoring,
        Bucket::ValidationHandoffPublication,
        Bucket::Implementation,
        Bucket::Reviews,
        Bucket::Reconciliation,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Bucket::Refinement => "refinement",
            Bucket::Authoring => "authoring",
            Bucket::ValidationHandoffPublication => "validation_handoff_publication",
            Bucket::Implementation => "implementation",
            Bucket::Reviews => "reviews",
            Bucket::Reconciliation => "reconciliation",
        }
    }
}

/// DSS-FR-SVJU: which reading of the token totals a usage record belongs to.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum UsageScope {
    Conversation,
    Graduation,
}

/// DSS-FR-AWKD: whether the provider reported a record against one call or for
/// the turn as a whole. The per-call representation wins where an owner has
/// both, and the two are never combined.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Representation {
    PerCall,
    TurnAggregate,
}

/// DSS-FR-OPMB: the way a draft change proposal went.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Accepted,
    Rejected,
}

/// DHS-FR-ZQNM: which of the two things a settled history entry is.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HistorySourceKind {
    Original,
    ProposalAccepted,
}

/// The token values a provider actually reported for one usage record.
///
/// Every field is optional and every absent one means *not reported* rather than
/// zero (DSS-FR-SVJU): a direction the provider declined to state is left absent
/// so the fold can report it unavailable instead of filling it with a number
/// nobody said.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReportedTokens {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cached_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub uncached_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub output_tokens: Option<u64>,
}

impl ReportedTokens {
    /// DSS-FR-SVJU: the record's input value — the explicit total where the
    /// provider reported one, and otherwise the sum of the non-overlapping
    /// cached and uncached components it did report.
    ///
    /// A reported total is **never** added to the components it is a total of,
    /// which is the whole reason this is a match rather than a sum.
    pub fn input(&self) -> Option<u64> {
        if let Some(total) = self.input_tokens {
            return Some(total);
        }
        match (self.cached_input_tokens, self.uncached_input_tokens) {
            (None, None) => None,
            (cached, uncached) => Some(cached.unwrap_or(0) + uncached.unwrap_or(0)),
        }
    }

    /// DSS-FR-SVJU: the record's output value, where the provider reported one.
    pub fn output(&self) -> Option<u64> {
        self.output_tokens
    }

    /// Whether the provider reported anything at all. A record that reports
    /// nothing is not a reliable usage record and is never appended.
    pub fn is_empty(&self) -> bool {
        self.input().is_none() && self.output().is_none()
    }
}

/// One reliable provider-reported usage record, as its producer hands it over
/// (per `../../../specifications/ai/CVL-conversation-loop.md` CVL-FR-WQZD).
///
/// The producer names the record's own stable identity, the logical model-call
/// round and the physical attempt the provider reported it against, and the
/// representation; this module adds the owner, the scope, and the bucket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageRecord {
    pub usage_id: String,
    pub round: u32,
    pub attempt: u32,
    pub representation: Representation,
    pub tokens: ReportedTokens,
}

/// DSS-FR-XBGA: the whole event vocabulary, and nothing beside it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventBody {
    /// DSS-FR-VCTQ: the draft's telemetry activation boundary. The envelope's
    /// `at` is the boundary instant, so the event carries no field of its own.
    TelemetryActivated,
    /// DSS-FR-RIDW: one settled foreground editing interval.
    #[serde(rename_all = "camelCase")]
    EditingInterval {
        interval_id: String,
        started_at: String,
        ended_at: String,
    },
    /// AGC-FR-YQMD: one draft-scoped conversational agent turn.
    #[serde(rename_all = "camelCase")]
    ConversationTurn {
        turn_id: String,
        /// The discussion the turn belongs to. A line an older build wrote
        /// names it `threadId`, and is read as the same value.
        #[serde(alias = "threadId")]
        discussion_id: String,
        /// Whether the discussion targets a fragment of its owner.
        #[serde(default)]
        fragment_targeted: bool,
        /// Only ever read: lines an older build wrote carry the origin kind.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin_kind: Option<String>,
        reached_model: bool,
        started_at: String,
        ended_at: String,
        outcome: String,
    },
    /// GRD-FR-NHRY: one graduation agent or model operation.
    #[serde(rename_all = "camelCase")]
    AgentOperation {
        operation_id: String,
        bucket: Bucket,
        run_id: String,
        started_at: String,
        ended_at: String,
    },
    /// DSS-FR-SVJU: one reliable provider-reported usage record.
    #[serde(rename_all = "camelCase")]
    TokenUsage {
        usage_id: String,
        scope: UsageScope,
        bucket: Bucket,
        owner_id: String,
        representation: Representation,
        round: u32,
        attempt: u32,
        #[serde(flatten)]
        tokens: ReportedTokens,
    },
    /// DHS-FR-ZQNM: one settled draft-history entry.
    #[serde(rename_all = "camelCase")]
    DraftHistoryEntry {
        entry_id: String,
        seq: u32,
        source_kind: HistorySourceKind,
    },
    /// DCP-FR-WTKA: one final draft change-proposal decision.
    #[serde(rename_all = "camelCase")]
    ProposalDecision {
        proposal_id: String,
        decision: Decision,
    },
}

impl EventBody {
    /// DSS-FR-MHJC: the capture instant of the fact this event records.
    ///
    /// For an event carrying an interval it is that interval's own `started_at`,
    /// the fact having begun being captured when the interval opened rather than
    /// when it settled. For every other event the caller supplies the instant the
    /// fact happened, which is what `None` here asks for.
    pub fn capture_instant(&self) -> Option<&str> {
        match self {
            EventBody::EditingInterval { started_at, .. }
            | EventBody::ConversationTurn { started_at, .. }
            | EventBody::AgentOperation { started_at, .. } => Some(started_at),
            _ => None,
        }
    }

    /// The event type's wire spelling, for a log record naming what was appended.
    pub fn type_name(&self) -> &'static str {
        match self {
            EventBody::TelemetryActivated => "telemetry_activated",
            EventBody::EditingInterval { .. } => "editing_interval",
            EventBody::ConversationTurn { .. } => "conversation_turn",
            EventBody::AgentOperation { .. } => "agent_operation",
            EventBody::TokenUsage { .. } => "token_usage",
            EventBody::DraftHistoryEntry { .. } => "draft_history_entry",
            EventBody::ProposalDecision { .. } => "proposal_decision",
        }
    }
}

/// DSS-FR-MHJC: one line of a log.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatisticsEvent {
    pub v: u32,
    pub event_id: String,
    pub draft_id: String,
    pub at: String,
    #[serde(flatten)]
    pub body: EventBody,
}

// ---------------------------------------------------------------------------
// The totals (DSS-FR-YOVS)
// ---------------------------------------------------------------------------

/// DSS-FR-NCLP: what the fold found of a statistic's own source family.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    /// Valid post-boundary events of the family, and nothing damaged.
    Available,
    /// Valid post-boundary events of the family, and a line or a field the fold
    /// could not read beside them.
    Incomplete,
    /// No valid post-boundary event of the family at all. The value is null,
    /// because a zero here would be a claim the events do not make.
    Unavailable,
}

/// One statistic: a value, and what the fold knows about the record behind it.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Measure {
    /// Null exactly where `availability` is `unavailable` (DSS-FR-NCLP).
    pub value: Option<u64>,
    pub availability: Availability,
}

impl Measure {
    pub fn unavailable() -> Measure {
        Measure {
            value: None,
            availability: Availability::Unavailable,
        }
    }

    pub fn of(value: u64, availability: Availability) -> Measure {
        match availability {
            Availability::Unavailable => Measure::unavailable(),
            _ => Measure {
                value: Some(value),
                availability,
            },
        }
    }
}

/// DSS-FR-DZQF: the two directions, kept apart and decided independently.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TokenPair {
    pub input: Measure,
    pub output: Measure,
}

impl TokenPair {
    pub fn unavailable() -> TokenPair {
        TokenPair {
            input: Measure::unavailable(),
            output: Measure::unavailable(),
        }
    }
}

/// DSS-FR-IYRG: the six buckets and their total, as durations.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BucketTimes {
    pub refinement: Measure,
    pub authoring: Measure,
    pub validation_handoff_publication: Measure,
    pub implementation: Measure,
    pub reviews: Measure,
    pub reconciliation: Measure,
    pub total: Measure,
}

/// DSS-FR-CMTP: the same six buckets and their total, as reported tokens.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BucketTokens {
    pub refinement: TokenPair,
    pub authoring: TokenPair,
    pub validation_handoff_publication: TokenPair,
    pub implementation: TokenPair,
    pub reviews: TokenPair,
    pub reconciliation: TokenPair,
    pub total: TokenPair,
}

/// DSS-FR-YOVS: what `read_draft_statistics` answers with — aggregate
/// captured-lifetime totals, each carrying its own availability, and nothing per
/// run, per conversation, or per event.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftStatisticsTotals {
    pub editing_time_ms: Measure,
    pub ai_interactions: Measure,
    pub draft_edits: Measure,
    pub accepted_proposals: Measure,
    pub rejected_proposals: Measure,
    pub conversation_tokens: TokenPair,
    pub agent_time_ms: BucketTimes,
    pub agent_tokens: BucketTokens,
}

/// [`DraftStatisticsTotals`] with the draft it is about and the boundary it was
/// collected from.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftStatistics {
    pub draft_id: String,
    /// DSS-FR-JRSY: the draft's one boundary, or null where its log holds no
    /// `telemetry_activated` event and where it has no log at all.
    pub boundary_at: Option<String>,
    #[serde(flatten)]
    pub totals: DraftStatisticsTotals,
    /// DSS-FR-LDFK: how many lines the fold could not read and left in place.
    pub unreadable_lines: u32,
}
