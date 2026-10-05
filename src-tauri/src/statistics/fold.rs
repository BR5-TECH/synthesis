//! The fold that turns a draft's log into its totals
//! (`../../../specifications/core/DSS-draft-statistics-storage.md` DSS-FR-YOVS).
//!
//! Total and deterministic: the same log folds to the same totals whatever
//! order its lines physically appear in, because two application processes
//! appending to one log interleave their lines by arrival and physical order
//! therefore carries no meaning (per `RMS-repository-machine-storage.md`
//! RMS-FR-PFOB). Events are ordered by their recorded instant, tie-broken by
//! `event_id`, which is the only ordering both writers agree on.
//!
//! Three rules run through everything below:
//!
//! * **Identity, not amendment** (DSS-FR-PNUE). The first event for an
//!   `event_id` wins, and each subject identity — interval, turn, operation,
//!   usage record, history entry, proposal — is counted at most once whatever
//!   number of events name it.
//! * **The boundary clips durations and nothing else** (DSS-FR-KYTB). An event
//!   carrying an interval contributes the part at or after the boundary; a
//!   counted event is counted whole; a token record is summed whole.
//! * **A missing figure is never a zero** (DSS-FR-NCLP). Each statistic names a
//!   source family, and its availability follows **that family alone** — a line
//!   the fold could not read makes the family it belongs to incomplete and
//!   leaves every other family exactly as it was.

use std::collections::{HashMap, HashSet};

use super::model::*;
use super::time::parse_instant_ms;

/// DSS-FR-NCLP: which statistics a line the fold could not read reaches.
///
/// Damage is attributed to a **family** rather than to the log as a whole,
/// because the requirement is about a statistic's own source family: a
/// malformed conversational turn says nothing about how completely the
/// proposals were recorded, and marking the proposal counters incomplete
/// because of it would report a doubt the log does not raise.
///
/// A line whose family cannot be read at all — one that is not JSON, or that
/// carries a `type` this build does not know — is attributed to **every**
/// family, which is the only honest reading: it could have belonged to any of
/// them, so no statistic may claim to be complete over it.
#[derive(Default)]
struct Damage {
    /// A line whose family could not be established, which reaches all of them.
    unattributable: bool,
    editing: bool,
    turn: bool,
    history: bool,
    proposal: bool,
    /// The `agent_operation` buckets a damaged line named, or every bucket
    /// where the line named none this build could read.
    operations: HashSet<Bucket>,
    all_operations: bool,
    /// The same for `token_usage`, which carries a scope as well as a bucket.
    usage_buckets: HashSet<Bucket>,
    all_usage: bool,
}

impl Damage {
    fn editing(&self) -> bool {
        self.unattributable || self.editing
    }

    fn turn(&self) -> bool {
        self.unattributable || self.turn
    }

    fn history(&self) -> bool {
        self.unattributable || self.history
    }

    fn proposal(&self) -> bool {
        self.unattributable || self.proposal
    }

    fn operation(&self, bucket: Bucket) -> bool {
        self.unattributable || self.all_operations || self.operations.contains(&bucket)
    }

    fn usage(&self, bucket: Bucket) -> bool {
        self.unattributable || self.all_usage || self.usage_buckets.contains(&bucket)
    }

    /// Everything, for the one case that leaves no statistic trustworthy: a log
    /// holding events and no `telemetry_activated` line has lost the boundary
    /// every one of them is read against.
    fn all(&mut self) {
        self.unattributable = true;
    }

    /// Attribute a line the fold could not read to the family its `type` names.
    ///
    /// `raw` is whatever the line parsed to as JSON, which is how a line that
    /// failed the *typed* deserialization can still say which family it belongs
    /// to: a `draft_history_entry` missing its `entry_id` is unreadable as an
    /// event and perfectly readable as a family.
    fn attribute(&mut self, raw: Option<&serde_json::Value>) {
        let Some(kind) = raw.and_then(|value| value.get("type")).and_then(|t| t.as_str()) else {
            self.unattributable = true;
            return;
        };
        match kind {
            // The boundary belongs to no statistic's family, but every statistic
            // is read against it, so a damaged one reaches all of them.
            "telemetry_activated" => self.unattributable = true,
            "editing_interval" => self.editing = true,
            "conversation_turn" => self.turn = true,
            "draft_history_entry" => self.history = true,
            "proposal_decision" => self.proposal = true,
            "agent_operation" => match bucket_of(raw) {
                Some(bucket) => {
                    self.operations.insert(bucket);
                }
                None => self.all_operations = true,
            },
            "token_usage" => match bucket_of(raw) {
                Some(bucket) => {
                    self.usage_buckets.insert(bucket);
                }
                None => self.all_usage = true,
            },
            // A `type` this build does not know names no family it could be
            // attributed to.
            _ => self.unattributable = true,
        }
    }
}

/// The bucket a damaged line names, where it names one this build knows.
fn bucket_of(raw: Option<&serde_json::Value>) -> Option<Bucket> {
    let text = raw?.get("bucket")?.as_str()?;
    Bucket::ORDER.into_iter().find(|b| b.as_str() == text)
}

/// What one pass over a log accumulated, before availability is decided.
#[derive(Default)]
struct Accumulator {
    /// Whether the fold saw at least one valid post-boundary event of a family,
    /// which is what tells an `unavailable` statistic from an `available` zero.
    saw_editing: bool,
    saw_turn: bool,
    saw_history: bool,
    saw_proposal: bool,
    saw_operation: HashMap<Bucket, bool>,

    editing_ms: u64,
    interactions: u64,
    refinement_ms: u64,
    operation_ms: HashMap<Bucket, u64>,
    history_entries: u64,
    accepted: u64,
    rejected: u64,

    /// Every usage record in scope, kept whole until the representation
    /// precedence of DSS-FR-AWKD has been applied over its owner.
    usage: Vec<UsageLine>,
}

/// One `token_usage` event the fold admitted.
struct UsageLine {
    scope: UsageScope,
    bucket: Bucket,
    owner_id: String,
    representation: Representation,
    tokens: ReportedTokens,
}

/// DSS-FR-YOVS: fold a draft's log into its totals.
///
/// `text` is the whole log; lines that cannot be read are skipped, counted, and
/// left in the file (DSS-FR-LDFK).
pub fn fold_log(draft_id: &str, text: &str) -> DraftStatistics {
    let mut events: Vec<StatisticsEvent> = Vec::new();
    let mut unreadable_lines: u32 = 0;
    let mut damage = Damage::default();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<StatisticsEvent>(line) {
            // DSS-FR-LDFK: a line whose `v` this build does not recognise is
            // skipped rather than failing the fold, exactly as an unparseable
            // one is — a log written by a newer build still serves the events
            // this one understands.
            Ok(event) if event.v == SCHEMA_VERSION => {
                events.push(event);
                continue;
            }
            _ => {}
        }
        unreadable_lines += 1;
        // Read again as bare JSON, which is what lets a line that failed the
        // typed read still name the family it damages.
        let raw = serde_json::from_str::<serde_json::Value>(line).ok();
        damage.attribute(raw.as_ref());
    }

    // DSS-FR-PNUE: the first event for an `event_id` is applied and every later
    // one ignored. Done before the sort so the winner is decided by identity
    // rather than by which duplicate happened to sort first.
    let mut seen_event: HashSet<String> = HashSet::new();
    events.retain(|event| seen_event.insert(event.event_id.clone()));
    events.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.event_id.cmp(&b.event_id)));

    // DSS-FR-JRSY: two first-event attempts that race, and two checkouts that
    // each began recording, converge on one boundary — the earliest
    // `telemetry_activated` the log holds, tie-broken by `event_id`. The sort
    // above has already put that one first.
    let boundary_at = events
        .iter()
        .find(|event| matches!(event.body, EventBody::TelemetryActivated))
        .map(|event| event.at.clone());
    let boundary_ms = boundary_at.as_deref().and_then(parse_instant_ms);

    let mut acc = Accumulator::default();
    let mut seen_subject: HashSet<(u8, String)> = HashSet::new();

    for event in &events {
        // DSS-FR-GAWO: the boundary is what the fold collects from. A log
        // holding events and no `telemetry_activated` line lost its boundary,
        // which is damage every statistic is read through.
        let Some(boundary_ms) = boundary_ms else {
            damage.all();
            break;
        };
        match &event.body {
            EventBody::TelemetryActivated => {}
            EventBody::EditingInterval {
                interval_id,
                started_at,
                ended_at,
            } => {
                let Some(span) = clipped(started_at, ended_at, boundary_ms) else {
                    // A required field the fold could not read is damage of the
                    // same kind an unreadable line is, and reaches the same one
                    // family (DSS-FR-NCLP).
                    damage.editing = true;
                    continue;
                };
                let Some(span) = span else { continue };
                if !seen_subject.insert((0, interval_id.clone())) {
                    continue;
                }
                acc.saw_editing = true;
                acc.editing_ms += span;
            }
            EventBody::ConversationTurn {
                turn_id,
                reached_model,
                started_at,
                ended_at,
                ..
            } => {
                let Some(span) = clipped(started_at, ended_at, boundary_ms) else {
                    damage.turn = true;
                    continue;
                };
                let Some(span) = span else { continue };
                if !seen_subject.insert((1, turn_id.clone())) {
                    continue;
                }
                acc.saw_turn = true;
                // DSS-FR-FXAE / DSS-FR-IYRG: a turn that reached no model is
                // recorded so the absence is readable, and contributes neither
                // an interaction nor a second of Refinement time.
                if *reached_model {
                    acc.interactions += 1;
                    acc.refinement_ms += span;
                }
            }
            EventBody::AgentOperation {
                operation_id,
                bucket,
                started_at,
                ended_at,
                ..
            } => {
                let Some(span) = clipped(started_at, ended_at, boundary_ms) else {
                    damage.operations.insert(*bucket);
                    continue;
                };
                let Some(span) = span else { continue };
                if !seen_subject.insert((2, operation_id.clone())) {
                    continue;
                }
                acc.saw_operation.insert(*bucket, true);
                *acc.operation_ms.entry(*bucket).or_insert(0) += span;
            }
            EventBody::TokenUsage {
                usage_id,
                scope,
                bucket,
                owner_id,
                representation,
                tokens,
                ..
            } => {
                if !in_scope(&event.at, boundary_ms) {
                    continue;
                }
                if !seen_subject.insert((3, usage_id.clone())) {
                    continue;
                }
                // DSS-FR-UBLC: the conversation-scoped records **are** the
                // Refinement bucket, so the two readings of them always agree.
                // Asserted rather than assumed: a producer that paired the
                // scope with another bucket would silently desync the two
                // totals, and nothing downstream could tell.
                debug_assert!(
                    *scope != UsageScope::Conversation || *bucket == Bucket::Refinement,
                    "a conversation-scoped usage record must carry the Refinement bucket",
                );
                acc.usage.push(UsageLine {
                    scope: *scope,
                    bucket: *bucket,
                    owner_id: owner_id.clone(),
                    representation: *representation,
                    tokens: *tokens,
                });
            }
            EventBody::DraftHistoryEntry { entry_id, .. } => {
                if !in_scope(&event.at, boundary_ms) {
                    continue;
                }
                if !seen_subject.insert((4, entry_id.clone())) {
                    continue;
                }
                acc.saw_history = true;
                acc.history_entries += 1;
            }
            EventBody::ProposalDecision {
                proposal_id,
                decision,
            } => {
                if !in_scope(&event.at, boundary_ms) {
                    continue;
                }
                // DSS-FR-OPMB: one proposal contributes at most once to at most
                // one counter, so the identity is shared between the two.
                if !seen_subject.insert((5, proposal_id.clone())) {
                    continue;
                }
                acc.saw_proposal = true;
                match decision {
                    Decision::Accepted => acc.accepted += 1,
                    Decision::Rejected => acc.rejected += 1,
                }
            }
        }
    }

    DraftStatistics {
        draft_id: draft_id.to_string(),
        boundary_at,
        totals: totals_of(&acc, &damage),
        unreadable_lines,
    }
}

/// DSS-FR-GAWO: whether an event carrying no interval is at or after the
/// boundary.
fn in_scope(at: &str, boundary_ms: i64) -> bool {
    parse_instant_ms(at).is_some_and(|at| at >= boundary_ms)
}

/// DSS-FR-KYTB: the part of an interval at or after the boundary.
///
/// `None` where either instant could not be read, which is a required field the
/// fold could not read. `Some(None)` where the whole interval ended before the
/// boundary, which is out of scope entirely rather than damaged.
fn clipped(started_at: &str, ended_at: &str, boundary_ms: i64) -> Option<Option<u64>> {
    let started = parse_instant_ms(started_at)?;
    let ended = parse_instant_ms(ended_at)?;
    if ended < boundary_ms {
        return Some(None);
    }
    let from = started.max(boundary_ms);
    Some(Some((ended - from).max(0) as u64))
}

/// Which availability a family's own presence and its own damage decide.
fn state(saw_family: bool, damaged: bool) -> Availability {
    match (saw_family, damaged) {
        (false, _) => Availability::Unavailable,
        (true, true) => Availability::Incomplete,
        (true, false) => Availability::Available,
    }
}

fn totals_of(acc: &Accumulator, damage: &Damage) -> DraftStatisticsTotals {
    DraftStatisticsTotals {
        editing_time_ms: Measure::of(acc.editing_ms, state(acc.saw_editing, damage.editing())),
        ai_interactions: Measure::of(acc.interactions, state(acc.saw_turn, damage.turn())),
        draft_edits: Measure::of(acc.history_entries, state(acc.saw_history, damage.history())),
        accepted_proposals: Measure::of(acc.accepted, state(acc.saw_proposal, damage.proposal())),
        rejected_proposals: Measure::of(acc.rejected, state(acc.saw_proposal, damage.proposal())),
        // DSS-FR-UBLC: the conversation-scoped records read a second way, which
        // the `debug_assert!` above keeps identical to the Refinement bucket.
        conversation_tokens: token_pair(
            acc,
            damage.usage(Bucket::Refinement),
            |line| line.scope == UsageScope::Conversation,
        ),
        agent_time_ms: bucket_times(acc, damage),
        agent_tokens: bucket_tokens(acc, damage),
    }
}

fn bucket_times(acc: &Accumulator, damage: &Damage) -> BucketTimes {
    let value = |bucket: Bucket| -> Measure {
        if bucket == Bucket::Refinement {
            // DSS-FR-IYRG: Refinement folds from the conversational turns rather
            // than from a graduation operation, so its family is the turn.
            return Measure::of(acc.refinement_ms, state(acc.saw_turn, damage.turn()));
        }
        Measure::of(
            acc.operation_ms.get(&bucket).copied().unwrap_or(0),
            state(
                acc.saw_operation.get(&bucket).copied().unwrap_or(false),
                damage.operation(bucket),
            ),
        )
    };
    let each = Bucket::ORDER.map(value);
    BucketTimes {
        refinement: each[0],
        authoring: each[1],
        validation_handoff_publication: each[2],
        implementation: each[3],
        reviews: each[4],
        reconciliation: each[5],
        total: sum_of_buckets(&each),
    }
}

/// DSS-FR-IYRG: the total of the six.
///
/// An `unavailable` bucket contributes **nothing** to the sum and is never read
/// as a known zero — the `incomplete` state is the whole of what says the figure
/// is a partial sum, and the author reads the bucket's own stated absence beside
/// it.
fn sum_of_buckets(each: &[Measure; 6]) -> Measure {
    if each
        .iter()
        .all(|m| m.availability == Availability::Unavailable)
    {
        return Measure::unavailable();
    }
    let sum = each.iter().filter_map(|m| m.value).sum();
    if each
        .iter()
        .all(|m| m.availability == Availability::Available)
    {
        Measure::of(sum, Availability::Available)
    } else {
        Measure::of(sum, Availability::Incomplete)
    }
}

fn bucket_tokens(acc: &Accumulator, damage: &Damage) -> BucketTokens {
    let each =
        Bucket::ORDER.map(|bucket| token_pair(acc, damage.usage(bucket), move |line| line.bucket == bucket));
    BucketTokens {
        refinement: each[0],
        authoring: each[1],
        validation_handoff_publication: each[2],
        implementation: each[3],
        reviews: each[4],
        reconciliation: each[5],
        // DSS-FR-CMTP: the total takes its availability from the six exactly as
        // the agent-time total takes its own, and the two directions decide it
        // independently.
        total: TokenPair {
            input: sum_of_buckets(&each.map(|pair| pair.input)),
            output: sum_of_buckets(&each.map(|pair| pair.output)),
        },
    }
}

/// The input and output totals of the usage records `admits` selects.
///
/// DSS-FR-AWKD: where an owner has both per-call records and a turn-level
/// aggregate, the per-call representation wins and the aggregate is ignored; the
/// two are never combined for one owner.
fn token_pair(acc: &Accumulator, damaged: bool, admits: impl Fn(&UsageLine) -> bool) -> TokenPair {
    let selected: Vec<&UsageLine> = acc.usage.iter().filter(|line| admits(line)).collect();
    if selected.is_empty() {
        return TokenPair::unavailable();
    }
    let owners_with_per_call: HashSet<&str> = selected
        .iter()
        .filter(|line| line.representation == Representation::PerCall)
        .map(|line| line.owner_id.as_str())
        .collect();
    let kept: Vec<&&UsageLine> = selected
        .iter()
        .filter(|line| {
            line.representation == Representation::PerCall
                || !owners_with_per_call.contains(line.owner_id.as_str())
        })
        .collect();
    TokenPair {
        input: direction(&kept, damaged, |tokens| tokens.input()),
        output: direction(&kept, damaged, |tokens| tokens.output()),
    }
}

/// DSS-FR-DZQF: one direction of a token total, decided over the records that
/// report it alone.
///
/// A family whose records never report a direction leaves it `unavailable`; one
/// in which some report it and some do not has it `incomplete`; a direction is
/// never filled with a zero, an estimate, or the other direction's value.
fn direction(
    records: &[&&UsageLine],
    damaged: bool,
    read: impl Fn(&ReportedTokens) -> Option<u64>,
) -> Measure {
    let reported: Vec<u64> = records.iter().filter_map(|line| read(&line.tokens)).collect();
    if reported.is_empty() {
        return Measure::unavailable();
    }
    let total = reported.iter().sum();
    if reported.len() == records.len() && !damaged {
        Measure::of(total, Availability::Available)
    } else {
        Measure::of(total, Availability::Incomplete)
    }
}
