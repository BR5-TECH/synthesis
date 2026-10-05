//! Journeys 26 and 27 of the coverage matrix: updating a graduated stream from
//! its base branch, and the settings its semantic turns run under
//! (`../../../../../specifications/infra/GTE-graduation-end-to-end-tests.md`).

use super::scenario::{Outcome, Scenario, Setting, TurnScript as Script};
use crate::graduation::{GraduationEscalationOrigin, GraduationRunState};
use crate::streams::{StreamUpdateState, StreamUpdateStrategy};
use crate::tools::agent_exec::AgentExecutionError;

/// A graduated stream whose run and the base branch changed the same line of
/// one file, which is the one thing a text merge cannot settle.
fn conflicting(name: &'static str) -> Outcome {
    Scenario::named(name)
        .work_turn(Script::reports_success().writes("README.md", "STREAM-SIDE\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .expect_state(GraduationRunState::Completed)
        .committing_on_the_base("README.md", "BASE-SIDE\n")
}

/// A semantic turn that settles the conflict with both sides.
fn settling() -> Script {
    Script::reports_success().writes("README.md", "BOTH-SIDES\n")
}

// ---------------------------------------------------------------------------
// 26 — updating a graduated stream
// ---------------------------------------------------------------------------

// GTE-FR-NMUF / GRB-FR-NFEB, GRB-FR-HJZC, GRB-FR-RMKD: a base change Git settles
// updates the stream under either strategy with no semantic turn. The stream
// holds the base revision and its graduated work; the base does not move.
#[test]
fn a_clean_base_change_updates_the_stream_with_no_semantic_turn() {
    let strategies: [(&'static str, StreamUpdateStrategy); 2] = [
        ("clean update by merge", StreamUpdateStrategy::MergeSource),
        ("clean update by rebase", StreamUpdateStrategy::RebaseSource),
    ];
    for (name, strategy) in strategies {
        Scenario::named(name)
            .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
            .review_turn(Script::answers_ready("The work answers the prompt."))
            .run()
            .committing_on_the_base("base-only.txt", "from the base\n")
            .updated(strategy, Vec::new())
            .expect_update_state(StreamUpdateState::Updated)
            .expect_update_turns(0)
            .expect_dispatch_count(0)
            .expect_stream_contains_base()
            .expect_base_branch_unchanged()
            .worktree_holds("base-only.txt", "from the base\n")
            .worktree_holds("src/panel.ts", "1\n");
    }
}

// GTE-FR-NMUF / GRB-FR-NFEB: a stream that already holds the pinned revision
// updates nothing, and neither branch moves.
#[test]
fn a_stream_already_holding_the_base_updates_nothing() {
    Scenario::named("nothing to update")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .updated(StreamUpdateStrategy::MergeSource, Vec::new())
        .expect_update_state(StreamUpdateState::NothingToUpdate)
        .expect_dispatch_count(0)
        .expect_branches_unchanged();
}

// GTE-FR-NMUF, GTE-FR-XKGB / GRB-FR-SRVN, GRB-FR-NNLS, GRB-FR-TOOU, GXD-FR-XQJR:
// a conflict Git cannot settle costs one semantic turn, dispatched as the
// masked turn kind with the artifact mounted, and what it settled lands on the
// stream branch alone.
#[test]
fn a_conflicting_update_spends_one_semantic_turn_and_lands_what_it_settled() {
    let strategies: [(&'static str, StreamUpdateStrategy); 2] = [
        ("conflicting update by merge", StreamUpdateStrategy::MergeSource),
        ("conflicting update by rebase", StreamUpdateStrategy::RebaseSource),
    ];
    for (name, strategy) in strategies {
        conflicting(name)
            .updated(strategy, vec![settling()])
            .expect_update_state(StreamUpdateState::Updated)
            .expect_update_turns(1)
            .expect_dispatch_count(1)
            .expect_dispatch(0, |d| d.semantic_turn().list_len("author_decisions", 0))
            .expect_stream_contains_base()
            .expect_base_branch_unchanged()
            .worktree_holds("README.md", "BOTH-SIDES\n")
            .expect_base_holds("README.md", "BASE-SIDE\n");
    }
}

// GTE-FR-NMUF, GTE-FR-RZKC / GRB-FR-LADU, GRB-FR-ZQNW, GRB-FR-KMXT, GRB-FR-WCHL,
// WKS-FR-DPNM: a semantic turn that cannot choose escalates, writes nothing,
// and is not retryable. The author's answer reaches the turn of a new attempt
// as a decision, and that turn settles the update.
#[test]
fn an_escalated_update_is_answered_and_the_answers_reach_the_next_turn() {
    conflicting("escalated update")
        .updated(
            StreamUpdateStrategy::MergeSource,
            vec![Script::escalates(&["Which limit stands?"])],
        )
        .expect_update_state(StreamUpdateState::Escalated)
        .expect_update_escalation_origin(GraduationEscalationOrigin::SemanticMerge)
        .expect_branches_unchanged()
        .refusing_update_retry("update_state_not_permitted")
        .refusing_update_answers(&[], "the answers do not cover every question")
        .update_answered(
            &["The base limit stands."],
            vec![Script::reports_success().writes("README.md", "BASE-SIDE\n")],
        )
        .expect_update_state(StreamUpdateState::Updated)
        .expect_update_under_a_new_attempt()
        .expect_dispatch(0, |d| d.semantic_turn().decision_answers(&["The base limit stands."]))
        .worktree_holds("README.md", "BASE-SIDE\n");
}

// GTE-FR-NMUF / GRB-FR-YRHM, GRB-FR-PBJX, GRB-FR-TXVL: three semantic turns that
// settle nothing end the update `conflicted`, naming the path, with neither
// branch moved. The author's retry grants three more turns.
#[test]
fn three_semantic_turns_that_settle_nothing_leave_the_update_conflicted_and_a_retry_grants_three_more() {
    conflicting("update bound spent")
        .updated(
            StreamUpdateStrategy::MergeSource,
            vec![
                Script::reports_success(),
                Script::reports_success(),
                Script::reports_success(),
            ],
        )
        .expect_update_state(StreamUpdateState::Conflicted)
        .expect_dispatch_count(3)
        .expect_update_unsettled(&["README.md"])
        .expect_branches_unchanged()
        .worktree_holds("README.md", "STREAM-SIDE\n")
        .update_retried(vec![settling()])
        .expect_update_state(StreamUpdateState::Updated)
        .expect_dispatch_count(1)
        .worktree_holds("README.md", "BOTH-SIDES\n");
}

// GTE-FR-HWQM, GTE-FR-NMUF / GRB-FR-MWTC, GRB-FR-TXVL: the author cancels the
// update while its semantic turn runs. The update records `cancelled` and
// neither branch nor working copy moves, even though the turn had settled the
// conflict.
#[test]
fn a_cancelled_update_moves_neither_branch() {
    conflicting("cancelled update")
        .updated(
            StreamUpdateStrategy::MergeSource,
            vec![settling().cancelling_the_update()],
        )
        .expect_update_state(StreamUpdateState::Cancelled)
        .expect_update_failure("update_cancelled")
        .expect_dispatch_count(1)
        .expect_branches_unchanged()
        .worktree_holds("README.md", "STREAM-SIDE\n");
}

// GTE-FR-NMUF / GRB-FR-YRHM, WKS-FR-DPNM: three semantic turns that could not
// be launched end the update `failed` rather than `conflicted`, moving
// nothing, and the author may retry it.
#[test]
fn an_update_whose_turns_cannot_be_launched_fails_and_may_be_retried() {
    conflicting("unlaunchable update")
        .updated(
            StreamUpdateStrategy::MergeSource,
            vec![
                Script::cannot_launch(AgentExecutionError::RuntimeUnavailable),
                Script::cannot_launch(AgentExecutionError::RuntimeUnavailable),
                Script::cannot_launch(AgentExecutionError::RuntimeUnavailable),
            ],
        )
        .expect_update_state(StreamUpdateState::Failed)
        .expect_update_failure("update_attempts_exhausted")
        .expect_dispatch_count(3)
        .expect_branches_unchanged()
        .update_retried(vec![settling()])
        .expect_update_state(StreamUpdateState::Updated)
        .expect_stream_contains_base()
        .worktree_holds("README.md", "BOTH-SIDES\n");
}

// GTE-FR-NMUF / WKS-FR-NRQT, GRD-FR-BNTC: an update is refused with
// `stream_busy` while a `blocked` run holds the stream, and nothing is
// recorded.
#[test]
fn an_update_is_refused_while_a_blocked_run_holds_the_stream() {
    Scenario::named("update of a busy stream")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_unreadably())
        .review_turn(Script::answers_unreadably())
        .run()
        .expect_state(GraduationRunState::Blocked)
        .committing_on_the_base("base-only.txt", "from the base\n")
        .updated(StreamUpdateStrategy::MergeSource, Vec::new())
        .expect_reconciliation_refused("stream_busy")
        .expect_no_update_record()
        .expect_branches_unchanged()
        .expect_dispatch_count(0);
}

// GTE-FR-NMUF / WKS-FR-NRQT, GRD-FR-YBUM: a run resting for the author holds no
// stream but keeps its base commit, so an update of its stream is refused
// with `stream_busy` and writes nothing.
#[test]
fn an_update_is_refused_while_the_stream_holds_a_resting_run() {
    Scenario::named("update beside a resting run")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .expect_state(GraduationRunState::AwaitingAuthor)
        .expect_stream_released()
        .committing_on_the_base("base-only.txt", "from the base\n")
        .updated(StreamUpdateStrategy::MergeSource, Vec::new())
        .expect_reconciliation_refused("stream_busy")
        .expect_no_update_record()
        .expect_branches_unchanged()
        .expect_worktree_lacks("base-only.txt")
        .expect_state(GraduationRunState::AwaitingAuthor);
}

// GTE-FR-NMUF / WKS-FR-NRQT: only a run of this stream that has not ended
// refuses an update. A discarded run of the stream, and a queued run of another
// stream, each leave it accepted.
#[test]
fn an_update_is_accepted_beside_a_discarded_run_and_a_run_of_another_stream() {
    Scenario::named("update beside ended and other-stream runs")
        .with_run_on_stream("header run", "header", "Write the header.")
        .work_turn(Script::escalates(&["Which panel is meant?"]))
        .run()
        .discarded()
        .expect_state(GraduationRunState::Discarded)
        .expect_state_of("header run", GraduationRunState::Queued)
        .committing_on_the_base("base-only.txt", "from the base\n")
        .updated(StreamUpdateStrategy::MergeSource, Vec::new())
        .expect_update_state(StreamUpdateState::Updated)
        .expect_stream_contains_base()
        .worktree_holds("base-only.txt", "from the base\n");
}

// GTE-FR-RZKC / WKS-FR-NRQT: the answers and the retry of an update start it
// again, so each is refused with `stream_busy` while a run of the stream has
// not ended, and neither moves a branch.
#[test]
fn answers_and_retries_are_refused_while_the_stream_holds_a_live_run() {
    conflicting("update answers beside a live run")
        .updated(
            StreamUpdateStrategy::MergeSource,
            vec![Script::escalates(&["Which limit stands?"])],
        )
        .expect_update_state(StreamUpdateState::Escalated)
        .enqueuing_on_the_stream("waiting", "Write the header.")
        .refusing_update_answers(&["The base limit stands."], "stream_busy")
        .expect_update_state(StreamUpdateState::Escalated)
        .expect_state_of("waiting", GraduationRunState::Queued);

    conflicting("update retry beside a live run")
        .updated(
            StreamUpdateStrategy::MergeSource,
            vec![
                Script::reports_success(),
                Script::reports_success(),
                Script::reports_success(),
            ],
        )
        .expect_update_state(StreamUpdateState::Conflicted)
        .enqueuing_on_the_stream("waiting", "Write the header.")
        .refusing_update_retry("stream_busy")
        .expect_update_state(StreamUpdateState::Conflicted);
}

// GTE-FR-HWQM, GTE-FR-YAEB / WKS-FR-RQVM, GRD-FR-BNTC, GRD-FR-EGWS: a run the
// author starts while an update of its stream runs cannot claim the stream. It
// stays queued, the queue offers it a dispatch once the update has written its
// outcome, and it then works from the updated branch.
#[test]
fn a_run_cannot_claim_a_stream_an_update_is_rewriting() {
    Scenario::named("run during an update")
        .work_turn(Script::reports_success().writes("README.md", "STREAM-SIDE\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .committing_on_the_base("README.md", "BASE-SIDE\n")
        .updated(
            StreamUpdateStrategy::MergeSource,
            vec![settling()
                .enqueuing_a_run_beside("behind", "Write the header.")
                .starting_another_run("behind", Vec::new())],
        )
        .expect_update_state(StreamUpdateState::Updated)
        .expect_other_claimed("behind", false)
        .expect_state_of("behind", GraduationRunState::Queued)
        .expect_stream_released()
        .expect_queue_offered_dispatch("behind")
        .driving(
            "behind",
            vec![
                Script::reports_success().writes("src/header.ts", "1\n"),
                Script::answers_ready("The header is there."),
            ],
        )
        .expect_state(GraduationRunState::Completed)
        .expect_base_is_head_before_run()
        .expect_committed_paths(&["src/header.ts"])
        .worktree_holds("README.md", "BOTH-SIDES\n");
}

// GTE-FR-NMUF / WKS-FR-KFVJ, GRB-FR-DYUA, GRB-FR-TXVL: an update pinned to a base
// revision the branch has since left writes nothing to either branch.
#[test]
fn an_update_pinned_to_a_base_revision_the_branch_has_left_writes_nothing() {
    Scenario::named("stale pinned revision")
        .work_turn(Script::reports_success().writes("src/panel.ts", "1\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        .committing_on_the_base("one.txt", "1\n")
        .pinning_the_base()
        .committing_on_the_base("two.txt", "2\n")
        .updated(StreamUpdateStrategy::MergeSource, Vec::new())
        .expect_update_state(StreamUpdateState::Failed)
        .expect_update_failure("stale_base_revision")
        .expect_dispatch_count(0)
        .expect_branches_unchanged()
        .expect_worktree_lacks("one.txt");
}

// ---------------------------------------------------------------------------
// 27 — the settings a semantic turn runs under
// ---------------------------------------------------------------------------

// GTE-FR-VNOU, GTE-FR-KAZX / GRL-FR-KWNP, GXD-FR-MTVR: the settings are read
// before every dispatch, the semantic rebase turn included, so a value changed
// during one semantic turn reaches the next one of the same update.
#[test]
fn a_setting_changed_during_one_semantic_turn_reaches_the_next() {
    Scenario::named("settings per semantic turn")
        .with_setting(Setting::ExecutionTimeoutMs, 60_000)
        .work_turn(Script::reports_success().writes("README.md", "STREAM-SIDE\n"))
        .review_turn(Script::answers_ready("The work answers the prompt."))
        .run()
        // The project settings file stands in the base working copy, so the
        // base ignores it to stay clean for the update.
        .committing_on_the_base(".gitignore", "target/\n.synthesis/\n")
        .committing_on_the_base("README.md", "BASE-SIDE\n")
        .updated(
            StreamUpdateStrategy::MergeSource,
            vec![
                Script::reports_success().changing(Setting::ExecutionTimeoutMs, 120_000),
                settling(),
            ],
        )
        .expect_update_state(StreamUpdateState::Updated)
        .expect_dispatch_count(2)
        .expect_dispatch(0, |d| d.semantic_turn().execution_timeout_ms(60_000))
        .expect_dispatch(1, |d| d.semantic_turn().execution_timeout_ms(120_000));
}
