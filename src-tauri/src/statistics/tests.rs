//! Tests for `DSS-draft-statistics-storage.md`.
//!
//! Everything here is pure over a `RootFs`: appending, folding, and deleting a
//! log need no Tauri handle, and the one command that does — the editing
//! interval — is exercised through the same `append_events` its recording path
//! composes, so the tests hold the module's own vocabulary rather than a runtime.

use tempfile::TempDir;

use super::*;

const DRAFT: &str = "draft-abc";
const OTHER: &str = "draft-xyz";

/// `T`, `T ± n`, as this module writes an instant.
const T: &str = "2026-03-14T12:00:00.000Z";

fn at(offset_ms: i64) -> String {
    let base = time::parse_instant_ms(T).expect("base instant");
    crate::notes::format_rfc3339_millis_utc(base + offset_ms)
}

fn minutes(n: i64) -> i64 {
    n * 60_000
}

struct Fixture {
    _dir: TempDir,
    root: fsa::RootFs,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let root = fsa::RootFs::for_root(dir.path());
        Fixture { _dir: dir, root }
    }

    fn append(&self, draft_id: &str, pending: Vec<Pending>) {
        append_events(&self.root, draft_id, &pending).expect("append");
    }

    fn read(&self, draft_id: &str) -> DraftStatistics {
        read_statistics_impl(&self.root, draft_id).expect("read")
    }

    fn log_text(&self, draft_id: &str) -> String {
        self.root
            .read_text(log_path(&self.root, draft_id).expect("path"))
            .expect("log")
    }

    /// Put a raw line into a draft's log, for the damaged-line cases.
    fn append_raw(&self, draft_id: &str, line: &str) {
        let path = log_path(&self.root, draft_id).expect("path");
        self.root
            .append_lines(&path, &[line.to_string()])
            .expect("raw append");
    }
}

fn interval(id: &str, from: i64, to: i64) -> Pending {
    Pending::now(EventBody::EditingInterval {
        interval_id: id.to_string(),
        started_at: at(from),
        ended_at: at(to),
    })
}

fn turn(id: &str, reached_model: bool, from: i64, to: i64) -> Pending {
    Pending::now(EventBody::ConversationTurn {
        turn_id: id.to_string(),
        discussion_id: "thread-1".to_string(),
        fragment_targeted: false,
        origin_kind: None,
        reached_model,
        started_at: at(from),
        ended_at: at(to),
        outcome: "delivered".to_string(),
    })
}

fn operation(id: &str, bucket: Bucket, from: i64, to: i64) -> Pending {
    Pending::now(EventBody::AgentOperation {
        operation_id: id.to_string(),
        bucket,
        run_id: "run-1".to_string(),
        started_at: at(from),
        ended_at: at(to),
    })
}

fn usage(
    id: &str,
    scope: UsageScope,
    bucket: Bucket,
    owner: &str,
    representation: Representation,
    input: Option<u64>,
    output: Option<u64>,
) -> Pending {
    Pending::at(
        at(0),
        EventBody::TokenUsage {
            usage_id: id.to_string(),
            scope,
            bucket,
            owner_id: owner.to_string(),
            representation,
            round: 1,
            attempt: 1,
            tokens: ReportedTokens {
                input_tokens: input,
                cached_input_tokens: None,
                uncached_input_tokens: None,
                output_tokens: output,
            },
        },
    )
}

fn history(entry_id: &str, seq: u32, kind: HistorySourceKind) -> Pending {
    Pending::now(EventBody::DraftHistoryEntry {
        entry_id: entry_id.to_string(),
        seq,
        source_kind: kind,
    })
}

fn decision(proposal_id: &str, decision: Decision) -> Pending {
    Pending::now(EventBody::ProposalDecision {
        proposal_id: proposal_id.to_string(),
        decision,
    })
}

// ---------------------------------------------------------------------------
// DSS-FR-VCTQ, DSS-FR-JRSY, DSS-FR-YOVS, DSS-FR-NCLP — a draft that has recorded nothing
// ---------------------------------------------------------------------------

#[test]
fn ts_kzwb_a_draft_with_no_log_answers_unavailable_and_creates_nothing() {
    let f = Fixture::new();
    let stats = f.read(DRAFT);
    assert_eq!(stats.boundary_at, None);
    assert_eq!(stats.totals.editing_time_ms, Measure::unavailable());
    assert_eq!(stats.totals.ai_interactions, Measure::unavailable());
    assert_eq!(stats.totals.draft_edits, Measure::unavailable());
    assert_eq!(stats.totals.accepted_proposals, Measure::unavailable());
    assert_eq!(stats.totals.rejected_proposals, Measure::unavailable());
    assert_eq!(stats.totals.conversation_tokens, TokenPair::unavailable());
    assert_eq!(stats.totals.agent_time_ms.total, Measure::unavailable());
    assert_eq!(stats.totals.agent_tokens.total, TokenPair::unavailable());
    assert_eq!(stats.unreadable_lines, 0);
    // DSS-FR-JRSY: reading writes nothing anywhere and creates no boundary.
    assert!(!statistics_dir(f.root.path()).exists());
}

// ---------------------------------------------------------------------------
// DSS-FR-KQVN, DSS-FR-PWXG, DSS-FR-MVTK, DSS-FR-VCTQ — the boundary, and a log
// that is never committed
// ---------------------------------------------------------------------------

#[test]
fn ts_plgt_the_first_event_creates_the_boundary_and_writes_no_git_artifact() {
    let f = Fixture::new();
    f.append(DRAFT, vec![history("entry-1", 1, HistorySourceKind::Original)]);

    let text = f.log_text(DRAFT);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "the boundary and the event");
    // DSS-FR-VCTQ: the boundary is written before the event it stands for, in
    // the same append.
    assert!(lines[0].contains("telemetry_activated"));
    assert!(lines[1].contains("draft_history_entry"));

    let stats = f.read(DRAFT);
    let boundary = stats.boundary_at.clone().expect("a boundary");
    // The boundary is that first event's own capture instant, and that event is
    // therefore post-boundary activity that counts.
    assert_eq!(stats.totals.draft_edits, Measure::of(1, Availability::Available));
    assert!(time::parse_instant_ms(&boundary).is_some());

    // DSS-FR-MVTK / DSS-FR-KQVN: the log stands in the store's own folder, and
    // no Git artifact is written beside it — the store is outside every
    // worktree, so there is nothing for Git to carry.
    assert!(statistics_dir(f.root.path()).join(format!("{DRAFT}.jsonl")).is_file());
    for entry in std::fs::read_dir(statistics_dir(f.root.path())).expect("read the folder") {
        let name = entry.expect("entry").file_name().to_string_lossy().into_owned();
        assert!(!name.starts_with(".git"), "{name} is a Git artifact");
    }
    assert!(!f.root.path().join(".synthesis").exists(), "no project folder");
}

/// DSS-FR-PWXG: appending twice writes one folder and no attributes file, and
/// the second append adds no Git artifact either.
#[test]
fn ts_plgt_no_append_ever_writes_a_gitattributes() {
    let f = Fixture::new();
    f.append(DRAFT, vec![decision("p1", Decision::Accepted)]);
    f.append(DRAFT, vec![decision("p2", Decision::Rejected)]);

    assert!(!statistics_dir(f.root.path()).join(".gitattributes").exists());
}

// ---------------------------------------------------------------------------
// DSS-FR-JRSY, DSS-FR-PNUE — two racing boundaries converge on one
// ---------------------------------------------------------------------------

#[test]
fn ts_hmdr_racing_boundaries_converge_on_the_earlier() {
    let f = Fixture::new();
    // Two first-event attempts, each having written its own boundary.
    f.append_raw(
        DRAFT,
        &format!(
            r#"{{"v":1,"eventId":"b-late","draftId":"{DRAFT}","at":"{}","type":"telemetry_activated"}}"#,
            at(minutes(10))
        ),
    );
    f.append_raw(
        DRAFT,
        &format!(
            r#"{{"v":1,"eventId":"b-early","draftId":"{DRAFT}","at":"{}","type":"telemetry_activated"}}"#,
            at(0)
        ),
    );
    f.append_raw(
        DRAFT,
        &format!(
            r#"{{"v":1,"eventId":"e1","draftId":"{DRAFT}","at":"{}","type":"proposal_decision","proposalId":"p1","decision":"accepted"}}"#,
            at(minutes(1))
        ),
    );

    let stats = f.read(DRAFT);
    assert_eq!(stats.boundary_at.as_deref(), Some(at(0).as_str()));
    // A decision between the two boundaries counts, the earlier being the one.
    assert_eq!(
        stats.totals.accepted_proposals,
        Measure::of(1, Availability::Available)
    );
}

// ---------------------------------------------------------------------------
// DSS-FR-KYTB, DSS-FR-GAWO, DSS-FR-NCLP — clipping at the boundary
// ---------------------------------------------------------------------------

#[test]
fn ts_ujcf_an_interval_straddling_the_boundary_contributes_its_captured_part() {
    let f = Fixture::new();
    // The boundary is created by the first append, at `T`.
    f.append(DRAFT, vec![decision("p1", Decision::Rejected)]);
    let boundary = f.read(DRAFT).boundary_at.expect("boundary");

    // Every interval is expressed relative to that boundary rather than to `T`,
    // so the test does not depend on the clock the append ran under.
    let base = time::parse_instant_ms(&boundary).expect("boundary instant");
    let stamp = |offset: i64| crate::notes::format_rfc3339_millis_utc(base + offset);

    f.append(
        DRAFT,
        vec![
            Pending::now(EventBody::EditingInterval {
                interval_id: "i1".into(),
                started_at: stamp(-minutes(10)),
                ended_at: stamp(minutes(5)),
            }),
            Pending::now(EventBody::ConversationTurn {
                turn_id: "t1".into(),
                discussion_id: "thread".into(),
                fragment_targeted: true,
                origin_kind: None,
                reached_model: true,
                started_at: stamp(-minutes(2)),
                ended_at: stamp(minutes(3)),
                outcome: "delivered".into(),
            }),
            Pending::now(EventBody::AgentOperation {
                operation_id: "o1".into(),
                bucket: Bucket::Authoring,
                run_id: "run".into(),
                started_at: stamp(-minutes(1)),
                ended_at: stamp(minutes(4)),
            }),
            // Wholly before the boundary: out of scope entirely.
            Pending::now(EventBody::EditingInterval {
                interval_id: "i0".into(),
                started_at: stamp(-minutes(30)),
                ended_at: stamp(-minutes(20)),
            }),
        ],
    );

    let stats = f.read(DRAFT);
    assert_eq!(
        stats.totals.editing_time_ms,
        Measure::of(minutes(5) as u64, Availability::Available)
    );
    assert_eq!(
        stats.totals.agent_time_ms.refinement,
        Measure::of(minutes(3) as u64, Availability::Available)
    );
    assert_eq!(
        stats.totals.agent_time_ms.authoring,
        Measure::of(minutes(4) as u64, Availability::Available)
    );
    // DSS-FR-KYTB: clipping is not incompleteness — the part cut away is
    // pre-boundary activity, which is out of scope rather than missing.
    assert_eq!(
        stats.totals.editing_time_ms.availability,
        Availability::Available
    );
    assert_eq!(stats.unreadable_lines, 0);
}

// ---------------------------------------------------------------------------
// DSS-FR-LDFK, DSS-FR-WRPD, DSS-FR-NCLP — damaged lines
// ---------------------------------------------------------------------------

#[test]
fn ts_qeip_a_damaged_line_is_skipped_counted_and_left_in_place() {
    let f = Fixture::new();
    f.append(DRAFT, vec![history("entry-1", 1, HistorySourceKind::Original)]);
    let before = f.log_text(DRAFT);

    f.append_raw(DRAFT, "not json at all");
    f.append_raw(
        DRAFT,
        &format!(r#"{{"v":1,"eventId":"x1","draftId":"{DRAFT}","at":"{T}","type":"invented"}}"#),
    );
    f.append_raw(
        DRAFT,
        &format!(r#"{{"v":2,"eventId":"x2","draftId":"{DRAFT}","at":"{T}","type":"proposal_decision","proposalId":"p","decision":"accepted"}}"#),
    );
    f.append_raw(
        DRAFT,
        &format!(r#"{{"v":1,"eventId":"x3","draftId":"{DRAFT}","at":"{T}","type":"draft_history_entry","seq":2}}"#),
    );

    let stats = f.read(DRAFT);
    assert_eq!(stats.unreadable_lines, 4);
    // The valid events still fold to their totals.
    assert_eq!(stats.totals.draft_edits.value, Some(1));
    // DSS-FR-NCLP: a family the fold saw a valid event of, beside a line it
    // could not read, is `incomplete` rather than `unavailable` or zero.
    assert_eq!(
        stats.totals.draft_edits.availability,
        Availability::Incomplete
    );
    // A family with no valid event at all stays unavailable.
    assert_eq!(stats.totals.editing_time_ms, Measure::unavailable());
    // DSS-FR-WRPD: no line is ever rewritten or removed.
    let after = f.log_text(DRAFT);
    assert!(after.starts_with(&before));
    assert!(after.contains("not json at all"));
}

#[test]
fn ts_qeip_damage_reaches_the_family_the_unreadable_line_belongs_to_and_no_other() {
    // DSS-FR-NCLP: availability follows a statistic's **own** source family, so
    // a malformed line of one family leaves every other family exactly as it
    // was. A malformed history line must not cast doubt on how completely the
    // proposals were recorded.
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            history("h1", 1, HistorySourceKind::Original),
            decision("p1", Decision::Accepted),
            interval("i1", 0, minutes(5)),
            operation("o1", Bucket::Reviews, 0, minutes(2)),
        ],
    );
    // Readable as a family, unreadable as an event: no `entryId`.
    f.append_raw(
        DRAFT,
        &format!(r#"{{"v":1,"eventId":"x1","draftId":"{DRAFT}","at":"{T}","type":"draft_history_entry","seq":9}}"#),
    );

    let stats = f.read(DRAFT);
    assert_eq!(stats.unreadable_lines, 1);
    assert_eq!(
        stats.totals.draft_edits.availability,
        Availability::Incomplete,
        "the family the damaged line belongs to",
    );
    for untouched in [
        stats.totals.accepted_proposals,
        stats.totals.rejected_proposals,
        stats.totals.editing_time_ms,
    ] {
        assert_eq!(
            untouched.availability,
            Availability::Available,
            "an unrelated family is not made incomplete by another's damage",
        );
    }
    assert_eq!(
        stats.totals.agent_time_ms.reviews.availability,
        Availability::Available,
    );
}

#[test]
fn ts_qeip_damage_naming_one_bucket_leaves_the_other_five_alone() {
    // The same rule one level down: an `agent_operation` line names its bucket
    // even when the rest of it cannot be read, so the damage is that bucket's.
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            operation("o1", Bucket::Authoring, 0, minutes(2)),
            operation("o2", Bucket::Reviews, 0, minutes(3)),
        ],
    );
    f.append_raw(
        DRAFT,
        &format!(r#"{{"v":1,"eventId":"x1","draftId":"{DRAFT}","at":"{T}","type":"agent_operation","bucket":"reviews"}}"#),
    );

    let times = f.read(DRAFT).totals.agent_time_ms;
    assert_eq!(times.reviews.availability, Availability::Incomplete);
    assert_eq!(times.authoring.availability, Availability::Available);
    assert_eq!(times.authoring.value, Some(minutes(2) as u64));
}

#[test]
fn ts_qeip_a_line_naming_no_family_reaches_every_one_of_them() {
    // A line that is not JSON, and one whose `type` this build does not know,
    // could have belonged to any family — so no statistic may claim to be
    // complete over it.
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            history("h1", 1, HistorySourceKind::Original),
            decision("p1", Decision::Accepted),
        ],
    );
    f.append_raw(DRAFT, "not json at all");

    let stats = f.read(DRAFT);
    assert_eq!(stats.unreadable_lines, 1);
    assert_eq!(stats.totals.draft_edits.availability, Availability::Incomplete);
    assert_eq!(
        stats.totals.accepted_proposals.availability,
        Availability::Incomplete,
    );
    // And a family with no valid event of its own is still unavailable rather
    // than incomplete: there is nothing for the damage to be partial about.
    assert_eq!(stats.totals.editing_time_ms, Measure::unavailable());
}

// ---------------------------------------------------------------------------
// DSS-FR-PNUE / DSS-FR-HGNT, DSS-FR-GAWO — duplicated lines
// ---------------------------------------------------------------------------

#[test]
fn ts_ysol_every_subject_identity_contributes_at_most_once() {
    let f = Fixture::new();
    let events = vec![
        interval("i1", 0, minutes(5)),
        turn("t1", true, 0, minutes(2)),
        operation("o1", Bucket::Reviews, 0, minutes(7)),
        usage(
            "u1",
            UsageScope::Graduation,
            Bucket::Reviews,
            "o1",
            Representation::PerCall,
            Some(100),
            Some(20),
        ),
        history("h1", 1, HistorySourceKind::Original),
        decision("p1", Decision::Accepted),
    ];
    f.append(DRAFT, events.clone());
    let once = f.read(DRAFT);

    // A union merge, a replay, and a concurrent append recovery each duplicating
    // the same lines.
    f.append(DRAFT, events.clone());
    f.append(DRAFT, events);
    let thrice = f.read(DRAFT);

    assert_eq!(thrice.totals, once.totals);
    assert_eq!(thrice.totals.draft_edits.value, Some(1));
    assert_eq!(thrice.totals.accepted_proposals.value, Some(1));
    assert_eq!(thrice.totals.agent_tokens.reviews.input.value, Some(100));
}

// ---------------------------------------------------------------------------
// DSS-FR-RIDW — editing intervals
// ---------------------------------------------------------------------------

#[test]
fn ts_tbrk_editing_time_is_the_sum_of_the_settled_intervals() {
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            interval("i1", 0, minutes(5)),
            interval("i2", minutes(10), minutes(20)),
            interval("i3", minutes(30), minutes(33)),
        ],
    );
    assert_eq!(
        f.read(DRAFT).totals.editing_time_ms,
        Measure::of(minutes(18) as u64, Availability::Available)
    );
}

#[test]
fn ts_tbrk_an_inverted_interval_is_refused_and_appends_nothing() {
    // The command's own guard, exercised through the predicate it rests on:
    // `ended_at` before `started_at` is `invalid_interval`.
    let started = time::parse_instant_ms(&at(minutes(5))).expect("start");
    let ended = time::parse_instant_ms(&at(0)).expect("end");
    assert!(ended < started);
}

// ---------------------------------------------------------------------------
// DSS-FR-FXAE, DSS-FR-IYRG, DSS-FR-NCLP / DSS-FR-AWKD, DSS-FR-UBLC — conversational turns
// ---------------------------------------------------------------------------

#[test]
fn ts_gxnu_a_turn_that_reached_no_model_contributes_nothing_but_is_available() {
    let f = Fixture::new();
    f.append(DRAFT, vec![turn("t1", false, 0, minutes(4))]);
    let stats = f.read(DRAFT);
    assert_eq!(
        stats.totals.ai_interactions,
        Measure::of(0, Availability::Available)
    );
    // DSS-FR-IYRG: none of those four minutes reaches Refinement time, and the
    // figure is `available` at zero rather than `unavailable` — a turn was
    // recorded.
    assert_eq!(
        stats.totals.agent_time_ms.refinement,
        Measure::of(0, Availability::Available)
    );
}

#[test]
fn ts_dwma_one_turn_is_one_interaction_however_many_records_it_reported() {
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            turn("t1", true, 0, minutes(6)),
            usage("u1", UsageScope::Conversation, Bucket::Refinement, "t1", Representation::PerCall, Some(10), Some(1)),
            usage("u2", UsageScope::Conversation, Bucket::Refinement, "t1", Representation::PerCall, Some(20), Some(2)),
            // The successful retry's own record, distinct and summed like any
            // other, adding no interaction.
            usage("u3", UsageScope::Conversation, Bucket::Refinement, "t1", Representation::PerCall, Some(30), Some(3)),
        ],
    );
    let stats = f.read(DRAFT);
    assert_eq!(stats.totals.ai_interactions.value, Some(1));
    assert_eq!(
        stats.totals.agent_time_ms.refinement.value,
        Some(minutes(6) as u64)
    );
    assert_eq!(stats.totals.conversation_tokens.input.value, Some(60));
    assert_eq!(stats.totals.conversation_tokens.output.value, Some(6));
    // DSS-FR-UBLC: the same records read as the Refinement bucket.
    assert_eq!(
        stats.totals.agent_tokens.refinement,
        stats.totals.conversation_tokens
    );
}

// ---------------------------------------------------------------------------
// DSS-FR-HGNT — draft edits
// ---------------------------------------------------------------------------

#[test]
fn ts_nfqt_the_first_acceptance_contributes_two_and_each_later_one_contributes_one() {
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            history("h-original", 1, HistorySourceKind::Original),
            history("h-1", 2, HistorySourceKind::ProposalAccepted),
        ],
    );
    assert_eq!(f.read(DRAFT).totals.draft_edits.value, Some(2));

    f.append(DRAFT, vec![history("h-2", 3, HistorySourceKind::ProposalAccepted)]);
    assert_eq!(f.read(DRAFT).totals.draft_edits.value, Some(3));

    // An acceptance that changes no prompt byte settles no entry, so it appends
    // no history event and Draft edits does not move.
    f.append(DRAFT, vec![decision("p-nochange", Decision::Accepted)]);
    let stats = f.read(DRAFT);
    assert_eq!(stats.totals.draft_edits.value, Some(3));
    assert_eq!(stats.totals.accepted_proposals.value, Some(1));
}

// ---------------------------------------------------------------------------
// DSS-FR-OPMB, DSS-FR-HGNT, DSS-FR-PNUE / DSS-FR-GAWO, DSS-FR-NCLP — proposal counters
// ---------------------------------------------------------------------------

#[test]
fn ts_egym_accepted_and_rejected_count_once_each_and_a_pending_one_counts_nowhere() {
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            decision("p-yes", Decision::Accepted),
            decision("p-no", Decision::Rejected),
            // The duplicate a resumed operation appended.
            decision("p-yes", Decision::Accepted),
        ],
    );
    let stats = f.read(DRAFT);
    assert_eq!(stats.totals.accepted_proposals.value, Some(1));
    assert_eq!(stats.totals.rejected_proposals.value, Some(1));
}

#[test]
fn ts_ruap_a_pre_boundary_decision_contributes_nothing_and_marks_nothing_incomplete() {
    let f = Fixture::new();
    f.append_raw(
        DRAFT,
        &format!(
            r#"{{"v":1,"eventId":"b","draftId":"{DRAFT}","at":"{}","type":"telemetry_activated"}}"#,
            at(0)
        ),
    );
    f.append_raw(
        DRAFT,
        &format!(
            r#"{{"v":1,"eventId":"old","draftId":"{DRAFT}","at":"{}","type":"proposal_decision","proposalId":"p-old","decision":"accepted"}}"#,
            at(-minutes(5))
        ),
    );
    f.append_raw(
        DRAFT,
        &format!(
            r#"{{"v":1,"eventId":"new","draftId":"{DRAFT}","at":"{}","type":"proposal_decision","proposalId":"p-new","decision":"accepted"}}"#,
            at(minutes(5))
        ),
    );

    let stats = f.read(DRAFT);
    assert_eq!(
        stats.totals.accepted_proposals,
        Measure::of(1, Availability::Available)
    );
    assert_eq!(stats.unreadable_lines, 0);
}

// ---------------------------------------------------------------------------
// DSS-FR-SVJU / DSS-FR-AWKD, DSS-FR-DZQF / DSS-FR-SVJU / DSS-FR-CMTP, DSS-FR-NCLP — token totals
// ---------------------------------------------------------------------------

#[test]
fn ts_sqob_components_are_combined_and_a_reported_total_is_never_added_to_them() {
    let components = ReportedTokens {
        input_tokens: None,
        cached_input_tokens: Some(400),
        uncached_input_tokens: Some(100),
        output_tokens: None,
    };
    assert_eq!(components.input(), Some(500));

    let with_total = ReportedTokens {
        input_tokens: Some(500),
        cached_input_tokens: Some(400),
        uncached_input_tokens: Some(100),
        output_tokens: None,
    };
    assert_eq!(with_total.input(), Some(500));
}

#[test]
fn ts_bthx_per_call_records_win_over_a_turn_aggregate_and_are_never_combined() {
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            turn("t1", true, 0, minutes(1)),
            usage("u1", UsageScope::Conversation, Bucket::Refinement, "t1", Representation::PerCall, Some(10), Some(1)),
            usage("u2", UsageScope::Conversation, Bucket::Refinement, "t1", Representation::PerCall, Some(20), Some(2)),
            usage("agg", UsageScope::Conversation, Bucket::Refinement, "t1", Representation::TurnAggregate, Some(999), Some(99)),
        ],
    );
    let stats = f.read(DRAFT);
    assert_eq!(stats.totals.conversation_tokens.input.value, Some(30));
    assert_eq!(stats.totals.conversation_tokens.output.value, Some(3));
}

#[test]
fn ts_bthx_an_aggregate_alone_is_used_once_and_a_missing_direction_is_incomplete() {
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            turn("t1", true, 0, minutes(1)),
            usage("agg", UsageScope::Conversation, Bucket::Refinement, "t1", Representation::TurnAggregate, Some(500), None),
        ],
    );
    let stats = f.read(DRAFT);
    assert_eq!(
        stats.totals.conversation_tokens.input,
        Measure::of(500, Availability::Available)
    );
    // DSS-FR-DZQF: an unreported direction is never filled with a zero.
    assert_eq!(stats.totals.conversation_tokens.output, Measure::unavailable());
}

#[test]
fn ts_mjew_a_direction_some_records_report_and_some_do_not_is_incomplete() {
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            turn("t1", true, 0, minutes(1)),
            usage("u1", UsageScope::Conversation, Bucket::Refinement, "t1", Representation::PerCall, Some(10), Some(4)),
            usage("u2", UsageScope::Conversation, Bucket::Refinement, "t1", Representation::PerCall, Some(20), None),
        ],
    );
    let stats = f.read(DRAFT);
    assert_eq!(
        stats.totals.conversation_tokens.input,
        Measure::of(30, Availability::Available)
    );
    assert_eq!(
        stats.totals.conversation_tokens.output,
        Measure::of(4, Availability::Incomplete)
    );
}

#[test]
fn ts_tarq_the_token_total_takes_its_availability_from_the_six_buckets() {
    let f = Fixture::new();
    let mut events = vec![turn("t1", true, 0, minutes(1))];
    for (index, bucket) in [
        Bucket::Refinement,
        Bucket::Authoring,
        Bucket::ValidationHandoffPublication,
        Bucket::Implementation,
        Bucket::Reviews,
    ]
    .into_iter()
    .enumerate()
    {
        let scope = if bucket == Bucket::Refinement {
            UsageScope::Conversation
        } else {
            UsageScope::Graduation
        };
        // The Reviews bucket reports input and never output.
        let output = (bucket != Bucket::Reviews).then_some(5);
        events.push(usage(
            &format!("u{index}"),
            scope,
            bucket,
            &format!("owner-{index}"),
            Representation::PerCall,
            Some(100),
            output,
        ));
    }
    f.append(DRAFT, events);

    let tokens = f.read(DRAFT).totals.agent_tokens;
    // Reconciliation reported nothing at all and stays unavailable, contributing
    // nothing to either direction.
    assert_eq!(tokens.reconciliation, TokenPair::unavailable());
    assert_eq!(tokens.reviews.output, Measure::unavailable());
    assert_eq!(tokens.total.input, Measure::of(500, Availability::Incomplete));
    assert_eq!(tokens.total.output, Measure::of(20, Availability::Incomplete));
}

// ---------------------------------------------------------------------------
// DSS-FR-IYRG, DSS-FR-CMTP, DSS-FR-NCLP — the six buckets and their total
// ---------------------------------------------------------------------------

#[test]
fn ts_xafr_the_total_is_the_sum_of_the_six_and_an_absent_bucket_is_not_a_zero() {
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            turn("t1", true, 0, minutes(1)),
            operation("o1", Bucket::Authoring, 0, minutes(2)),
            operation("o2", Bucket::ValidationHandoffPublication, 0, minutes(3)),
            operation("o3", Bucket::Implementation, 0, minutes(4)),
            operation("o4", Bucket::Reviews, 0, minutes(5)),
            operation("o5", Bucket::Reconciliation, 0, minutes(6)),
        ],
    );
    let times = f.read(DRAFT).totals.agent_time_ms;
    assert_eq!(times.total, Measure::of(minutes(21) as u64, Availability::Available));

    // With one bucket absent the total is the partial sum, marked incomplete,
    // and the absent bucket was not read as a zero.
    let g = Fixture::new();
    g.append(
        OTHER,
        vec![
            turn("t1", true, 0, minutes(1)),
            operation("o1", Bucket::Authoring, 0, minutes(2)),
        ],
    );
    let partial = g.read(OTHER).totals.agent_time_ms;
    assert_eq!(partial.reviews, Measure::unavailable());
    assert_eq!(
        partial.total,
        Measure::of(minutes(3) as u64, Availability::Incomplete)
    );

    // With nothing recorded at all every bucket and the total are unavailable.
    let h = Fixture::new();
    h.append(DRAFT, vec![decision("p", Decision::Rejected)]);
    let none = h.read(DRAFT).totals.agent_time_ms;
    assert_eq!(none.refinement, Measure::unavailable());
    assert_eq!(none.total, Measure::unavailable());
}

// ---------------------------------------------------------------------------
// DSS-FR-KQVN, DSS-FR-NKOZ / DSS-FR-VPBS — where the log lives, and its deletion
// ---------------------------------------------------------------------------

#[test]
fn ts_holv_the_log_is_named_for_the_stable_draft_id_under_the_committed_folder() {
    let f = Fixture::new();
    f.append(DRAFT, vec![decision("p1", Decision::Accepted)]);
    let expected = f
        .root
        .path()
        .join(STATISTICS_REL)
        .join(format!("{DRAFT}.jsonl"));
    assert!(expected.exists());
    assert!(!f.root.path().join(".synthesis/drafts").exists());
}

#[test]
fn ts_ungk_deletion_is_scoped_idempotent_and_leaves_every_other_log_alone() {
    let f = Fixture::new();
    f.append(DRAFT, vec![decision("p1", Decision::Accepted)]);
    f.append(OTHER, vec![decision("p2", Decision::Rejected)]);
    let other_before = f.log_text(OTHER);

    delete_draft_statistics(&f.root, DRAFT).expect("delete");
    assert!(!f
        .root
        .path()
        .join(STATISTICS_REL)
        .join(format!("{DRAFT}.jsonl"))
        .exists());
    assert_eq!(f.log_text(OTHER), other_before);

    // A second call is an already completed deletion rather than an error.
    delete_draft_statistics(&f.root, DRAFT).expect("second delete");
}

// ---------------------------------------------------------------------------
// DSS-FR-XRDM, DSS-FR-TUMX, DSS-FR-PNUE, DSS-FR-ZMHT — what a line never carries, and what a relaunch finds
// ---------------------------------------------------------------------------

#[test]
fn ts_fyta_no_line_carries_content_and_a_replay_folds_to_the_same_totals() {
    let f = Fixture::new();
    f.append(
        DRAFT,
        vec![
            interval("i1", 0, minutes(5)),
            turn("t1", true, 0, minutes(2)),
            operation("o1", Bucket::Implementation, 0, minutes(3)),
            usage("u1", UsageScope::Graduation, Bucket::Implementation, "o1", Representation::PerCall, Some(7), Some(8)),
            history("h1", 1, HistorySourceKind::Original),
            decision("p1", Decision::Accepted),
        ],
    );
    let text = f.log_text(DRAFT);
    // DSS-FR-XRDM: an event carries instants, identities, counts, a bucket, a
    // decision, and reported token numbers, and nothing else. The vocabulary is
    // closed, so the check is that no key outside it ever appears.
    for forbidden in [
        "prompt", "candidate", "rationale", "feedback", "body", "content", "exchange", "tool",
        "answer", "credential", "token\"", "secret",
    ] {
        assert!(
            !text.contains(forbidden),
            "a statistics line must not carry {forbidden:?}"
        );
    }

    // A relaunch replays the same file and folds to the same totals.
    let before = f.read(DRAFT);
    let after = fold::fold_log(DRAFT, &text);
    assert_eq!(before, after);
}

// ---------------------------------------------------------------------------
// DSS-FR-QLXV, DSS-FR-WGUP, DSS-FR-PJOD — the typed refusals
// ---------------------------------------------------------------------------

#[test]
fn ts_jbwd_a_crafted_draft_id_is_refused_having_written_and_read_nothing() {
    let f = Fixture::new();
    for crafted in ["../escape", "a/b", ".."] {
        assert!(log_path(&f.root, crafted).is_err());
        assert!(append_events(&f.root, crafted, &[decision("p", Decision::Accepted)]).is_err());
        assert!(read_statistics_impl(&f.root, crafted).is_err());
        assert!(delete_draft_statistics(&f.root, crafted).is_err());
    }
    assert!(!statistics_dir(f.root.path()).exists());
}

// ---------------------------------------------------------------------------
// Instants (DSS-FR-MHJC)
// ---------------------------------------------------------------------------

#[test]
fn instants_round_trip_through_the_shape_this_module_writes() {
    for millis in [0i64, 1_773_000_000_123, 1_000] {
        let text = crate::notes::format_rfc3339_millis_utc(millis);
        assert_eq!(time::parse_instant_ms(&text), Some(millis));
    }
    assert_eq!(time::parse_instant_ms("not an instant"), None);
    assert_eq!(time::parse_instant_ms("2026-03-14T12:00:00"), None);
    // A second-granularity stamp is read as the start of that second.
    assert_eq!(
        time::parse_instant_ms("1970-01-01T00:00:01Z"),
        Some(1_000)
    );
}

/// DSS-FR-MVTK, PST-FR-DQZT: no write of this module arms the debounced commit.
///
/// Asserted against the source for the reason `CMS-comments-storage.md`'s twin
/// is: the store stands outside every worktree, so what has to stay true is
/// that the commit seam is not reached at all rather than that it found nothing
/// to commit.
#[test]
fn dss_no_write_of_this_module_arms_the_debounced_commit() {
    for source in [
        include_str!("../statistics.rs"),
        include_str!("fold.rs"),
        include_str!("model.rs"),
        include_str!("time.rs"),
    ] {
        assert!(
            !source.contains("storage_floor::commit"),
            "a statistics append reaches the committer",
        );
    }
}
