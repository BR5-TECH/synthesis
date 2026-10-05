# The graduation end-to-end suite

The specification is `specifications/infra/GTE-graduation-end-to-end-tests.md`.
This file is what GTE-FR-CSEH asks for: the scenario language, the harness
boundary, how a journey is added, and the coverage matrix.

## What the suite drives

Every scenario drives the **production** graduation loop and the production
state machine. It starts a real run over a real Git repository, dispatches real
turns through `drive_for_test`, reads the run back out of the run store, and
inspects the commits the run made.

The one thing a scenario substitutes is the dispatch seam
`GXD-graduation-execution.md` GXD-FR-DRFF names. No command, project setting or
provider record selects between the production seam and the scripted one, so a
substitution is the test's own act rather than a configuration.

## The harness boundary

```
        the scenario language          ← e2e/scenario.rs and the files beside it
                  |
   Fixture · ScriptedDispatch · Turn   ← ../mod.rs, shared with the other suites
                  |
============ the dispatch seam ============  GXD-FR-QLFA
                  |
   graduation::driver · transitions · commit · streams · git   ← production
```

Everything under the seam is shipped code. Everything above it is the harness.
A scenario that needed to reach around the seam — to set a run's state directly,
to write a checkpoint by hand, to call a private function of the loop — would be
testing the harness rather than the application, and is a fault rather than a
shortcut.

A **hook** (GTE-FR-HWQM) runs inside a scripted turn and acts through the
production commands and the production state: the author's pause and discard,
the application's shutdown, the first queue read after a relaunch. The
obstructions only a machine makes — Git's own index lock, a directory where a
log file belongs, a branch where a scratch branch needs a directory — are
made on the scenario's own temporary directories.

Nothing here reaches a network, a provider credential or a container runtime,
and nothing waits on a clock. A timeout and a cancellation are outcomes the
script returns, not conditions the suite sits through.

No scenario lets the production queue start a run (GTE-FR-YAEB). Every
scenario installs a tripwire where the queue dispatches: an attempt is recorded
rather than launched, and a scenario that ends with an attempt it did not
expect with `expect_queue_offered_dispatch` fails. A run that waits on a stream
is enqueued with auto-start off, and every command that could start a run is
called with the queue's dispatch turned off.

## The scenario language

One chain per journey, in four stages and always in this order.

```rust
Scenario::named("ready first pass")           // arrange
    .with_setting(Setting::PassBudget, 2)
    .with_stream("editor")
    .with_prompt("Add the empty state to the panel.")
    .work_turn(Script::reports_success()      // script
        .writes("src/panel.ts", "export const panel = 1;\n"))
    .review_turn(Script::answers_ready("The work answers the prompt."))
    .run()                                    // act
    .expect_state(GraduationRunState::Completed)   // assert
    .expect_committed_paths(&["src/panel.ts"])
    .expect_stream_released()
    .expect_dispatch(0, |d| d.part("work").purpose("generate").pass(1));
```

The language stands in several files, each under a thousand lines:

| File | Holds |
| --- | --- |
| `scenario.rs` | `Scenario`, its arrangement, `run`, and the binding of scripts to the seam |
| `script.rs` | `TurnScript`: answers, effects and hooks |
| `hooks.rs` | `TurnContext`, what a hook acts on |
| `settings.rs` | `Setting` and the project settings file |
| `outcome.rs` | `Outcome`, the record and dispatch assertions |
| `repository_assert.rs` | the repository assertions |
| `acts.rs` | the acts on a resting run, the repairs and the refusals |
| `reconcile.rs` | the update requests, their refusals and their assertions, and the base branch arrangement both languages share |
| `merge_arrange.rs` | what a merge scenario commits before it acts, and the record of what both branches and both working copies held |
| `merge_acts.rs` | the author's Merge, driving the merge run, another author's commits, the apply obstructions and the relaunch |
| `merge_assert.rs` | the assertions on the answer of a merge, on the run it was handed to, and on what it wrote |
| `dispatch_assert.rs` | `DispatchAssert`, the assertions on one recorded dispatch |

### Arrange

| Method | What it does |
| --- | --- |
| `Scenario::named(name)` | Names the journey. Every assertion failure carries it. |
| `with_setting(setting, value)` | Writes one project setting (PSS-FR-TQMV, PSS-FR-WPKS, PSS-FR-JRWC). |
| `with_raw_setting(setting, value)` | Writes a value the settings must repair rather than use (PSS-FR-ZLCF). |
| `with_stream(name)` | Names the work stream. |
| `with_prompt(text)` | The prompt the run captures. |
| `with_standing_work(choice)` | The standing-work choice the run is enqueued with (GRD-FR-HQPD). |
| `with_standing_message(text)` | The message a standing commit takes (GRD-FR-RJFC). |
| `with_standing_file(path, content)` | A file left uncommitted in the stream before the run starts. |
| `with_index_held()` | Git's own index lock stands in the stream before the run starts. |
| `with_branch(name)` | A branch of the project's repository. |
| `with_execution_allowed()` | A machine configured to execute an agent, so a restart is reachable. |
| `with_run_on_stream(label, stream, prompt)` | A second run, enqueued with auto-start off. |
| `with_stream_commit(path, content)` / `with_base_commit(path, content)` | A commit on the stream's branch, or on the branch the stream was created from, made as another author makes it (GTE-FR-TNZF). |
| `without_a_draft_run()` | The scenario starts no draft run, so its stream holds what the arrangement committed and the focus names no run. |
| `with_merge(publication)` | The author's Merge, started when the scenario runs (GTE-FR-TNZF). A merge Git cannot settle is handed to a run, and the scenario's merge turns drive it. With none scripted the run stays `queued`. |
| `with_slot_held_by(stream)` | A claim on another stream for the whole scenario, so it takes one project slot (GTE-FR-YCQW). |

### Script

`work_turn(script)` and `review_turn(script)` append to one list. The **loop**
decides which part comes next; the two names say what the author expects that
position to be. A dispatch nobody scripted fails the scenario and names the part
and the purpose the loop asked for.

One `Script` says what the turn answers:

| Answer | Meaning |
| --- | --- |
| `reports_success()` | The agent reported the task finished. |
| `reports_failure(message)` | GRL-FR-DNKA: it reported what it could not do. |
| `answers_ready(rationale)` | A `ready` verdict. |
| `answers_revise(severity, description)` | A `revise` verdict of one finding. |
| `answers_revise_with(findings)` | A `revise` verdict the scenario composed. |
| `answers_ready_with(findings)` | A `ready` verdict carrying findings, which the loop cannot read. |
| `answers_revise_without_findings()` | A `revise` verdict carrying none, which the loop cannot read. |
| `answers_with_blank_rationale()` | A verdict whose rationale says nothing. |
| `answers_unreadably()` | GRL-FR-TVXI: a result the loop cannot read. |
| `escalates(questions)` | GRL-FR-VBCL: the turn stops to ask the author. |
| `escalates_because(reason, questions)` | The same, under a reason the scenario names. |
| `is_cancelled()` / `times_out()` | The turn stopped where it stood. |
| `exits_nonzero()` / `answers_malformed()` / `is_terminated()` | The process ended without an answer. |
| `cannot_launch(error)` | Nothing was launched at all. |
| `claims_paths(paths)` | GXD-FR-LBYG: an envelope claiming a change set of its own. |

what it does to its execution directory first:

| Effect | Meaning |
| --- | --- |
| `writes(path, content)` | Writes into the execution directory. |
| `deletes(path)` | Removes a path from it. |
| `removes_dir(path)` | Removes a whole directory, as a reorganization does. |
| `links(path, target)` | Installs a symbolic link, as a dependency install does. |
| `leaves_a_session()` | Answers with a resumable vendor session. |
| `finishing_anyway()` | Answers as scripted even where the run was stopped while it ran. |
| `changing(setting, value)` | GTE-FR-VNOU: changes a project setting mid-turn. |

and what it does to the application while it runs (GTE-FR-HWQM):

| Hook | Meaning |
| --- | --- |
| `then(\|context\| …)` | Any act on the `TurnContext`. |
| `pausing_the_run()` | The author's pause (GRD-FR-MDQZ). |
| `discarding_the_run()` | The author's discard (GRD-FR-EWTN). |
| `stopping_the_application(reason)` | A shutdown or a project change (GRD-FR-TWMA). |
| `holding_the_index()` | Another write takes the stream's index (GTC-FR-19). |
| `losing_the_log(stream)` | A log stream of the run can no longer be written (GRD-FR-IKVE). |
| `abandoned_by_a_relaunch()` | The process is gone and the next launch reads the queue (GRD-FR-XVUD). |
| `starting_another_run(label, scripts)` | A second run is driven while this turn runs. |
| `cancelling_the_update()` | The author stops the update a semantic turn belongs to. |
| `enqueuing_a_run_beside(label, prompt)` | The author starts another run on the same stream, with auto-start on. |

### Act

`run()` drives the run to rest. These act on the same run afterwards:

- `continued(scripts)` — the author's Continue, then the run driven again.
- `answered(answers, scripts)` — the author answers every question the run
  asked, then the run driven again.
- `discarded()`, `reverted()` — the author's discard and revert.
- `restarted(choice, scripts)` — the author's restart; the focus moves to the
  new run.
- `driving(label, scripts)` — a run the scenario enqueued is driven; the focus
  moves to it.
- `enqueuing_on_the_stream(label, prompt)` — the author starts another run on
  the stream in focus while the run rests.
- `continuing_without_a_dispatch()` — the author's Continue, leaving the run
  `queued`.
- `holding_the_index_while_resting()` — another write takes the stream's index
  while the run rests.
- `releasing_the_index()`, `deleting_branch(name)`, `repairing_the_log(stream)`,
  `removing_the_working_copy()` — what clears an obstruction, or makes one
  while the run rests.
- `refusing_continue(code)`, `refusing_pause(code)`, `refusing_discard(code)`,
  `refusing_revert(code)`, `refusing_restart(code)`,
  `refusing_answers(answers, code)` — the command is refused with the code, and
  the run's state, blocker, escalation, commits, pass, branch head and stream
  claim are what they were (GTE-FR-RZKC).

A graduated stream is reconciled with its base branch (GTE-FR-NMUF) through
the production commands and jobs, with its semantic turns scripted on the same
seam:

- `committing_on_the_base(path, content)`, `pinning_the_base()`,
  `dirtying_the_base(path, content)`, `dirtying_the_stream(path, content)` —
  what the base branch and both working copies hold before the request.
- `updated(strategy, scripts)`, `update_answered(answers, scripts)`,
  `update_retried(scripts)` — the author's update, answers and retry.
- `refusing_update_retry(code)`, `refusing_update_answers(answers, code)` — a
  refused request starts nothing and moves neither branch.

### The merge language

A merge of a stream into its base branch is a graduation run when Git cannot
settle it (GTE-FR-QVHM). The scenario starts it as the author does, through
`merge_work_stream`, and drives the merge run through the same scripted seam
(GTE-FR-TNZF to GTE-FR-DQLR). `merge_work_turn(script)` and
`merge_review_turn(script)` append to the same list as `work_turn` and
`review_turn`: the loop decides which part comes next.

```rust
conflicted_merge("one merge pass")                 // arrange: both branches changed README.md
    .with_merge(as_one_commit())                   // the author's Merge
    .merge_work_turn(Script::reports_success().writes("README.md", "both sides\n"))
    .merge_review_turn(Script::answers_ready("Both intents stand."))
    .run()                                         // hand off, then drive the run to rest
    .expect_state(GraduationRunState::Completed)
    .expect_base_holds_the_merge_commit("Merge the feature stream")
    .expect_dispatch(0, |d| d.part("merge_work").merge_paths("unresolved_paths", &["README.md"]));
```

The acts that make and drive a merge run:

- `merging(publication)` — the author's Merge. The answer is kept, and a
  `conflicted` answer moves the focus to the run, which waits `queued`. The
  queue's own offer of a dispatch is held by the tripwire of GTE-FR-YAEB and
  kept for `expect_queue_offered_the_merge_run`. `merging_stream(id,
  publication)` and `merging_without_a_repository(publication)` name the
  stream or hide the repository.
- `driving_the_merge(scripts)` — the merge run in focus is driven through the
  scripted seam. `expect_the_merge_cannot_start()` asserts that the stream or
  the project's slots refuse it.
- `continued`, `answered`, `discarded`, `refusing_continue`,
  `refusing_answers`, `refusing_restart` and `refusing_revert` act on a merge
  run as on every run.
- `moving_the_base_tip()`, `moving_the_stream_tip()`,
  `committing_on_the_stream(path, content)` — another author commits.
- `relaunching()` — no claim of the old process stands, and the first read of
  the queue and of the streams happens. `stranding_what_the_run_owned()` puts
  the merge worktree and the snapshot ref of an ended run back, as a crash
  between the end of the run and its cleanup leaves them.
- `holding_the_update_guard_now()`, `holding_another_merge_now()`,
  `releasing_the_guard()`, `cleaning_the_base(path)`,
  `cleaning_the_stream(path)`, `releasing_the_base_index()` — what makes an
  apply obstruction and what removes it.
- `setting_the_machine_up_to(stage)`, `checking_out(branch)`,
  `releasing_the_held_slots()`, `turning_auto_start_off()`,
  `offering_the_queue_a_dispatch()`, `archiving()` and
  `reordering_the_merge_run(from, to)`.

The hooks a merge turn may carry: `checking_the_live_trees_stand()`,
`holding_the_stream_and_a_slot(state)`, `moving_the_base_tip_meanwhile()`,
`moving_the_stream_tip_meanwhile()`, `dirtying_the_base_meanwhile(path,
content)`, `dirtying_the_stream_meanwhile(path, content)`,
`holding_the_update_guard()` and `holding_the_base_index()`.

The assertions on a merge: `expect_merged`, `expect_nothing_to_merge`,
`expect_conflicted` and `expect_merge_refused` read the answer.
`expect_no_merge_run` and `expect_no_merge_leftovers` read the run store, the
queue index and the repository. `expect_live_state_unchanged` compares both
branch heads and both working copies with what they held when the merge was
started. `expect_merge_run_named`, `expect_merge_snapshot`,
`expect_merge_paths`, `expect_publication`, `expect_listings_carry_the_merge_run`
and `expect_stream_row_merge_run` read the run the merge was handed to.
`expect_merge_workspace_standing` and `expect_merge_workspace_reclaimed` read
the merge worktree, its registration, the scratch branch and the snapshot ref.
`expect_merge_result`, `expect_base_holds_the_merge_commit`,
`expect_base_working_copy_holds` and `expect_base_branch_at_the_pinned_tip`
read what the apply wrote. `expect_failure_code`, `expect_slots_in_use`,
`expect_waiting_for_a_slot`, `expect_queue_offered_the_merge_run` and
`expect_queue_offered_nothing` read the failure, the project's slots and the
queue. A dispatch of a merge turn is read with `merge_field`, `merge_paths`,
`merge_conflict`, `merge_has_no`, `merge_work_instruction`,
`merge_review_instruction`, `no_supplementary_mount` and `directory_held`.

`rewriting_checkpoint(edit)` puts the record into a shape an older build wrote,
so a journey can prove what this build does with a record it did not create.

Each act that drives a run rebinds the scripted seam, so the dispatch
assertions after it read the turns of that act alone.

### Assert

The record: `expect_state`, `expect_pass`, `expect_turns`,
`expect_pass_window`, `expect_loop_instruction_contains`,
`expect_escalation_questions`, `expect_no_escalation`,
`expect_escalation_origin`, `expect_blocker`, `expect_blocker_attempt`,
`expect_blocker_absent`, `expect_interruption`, `expect_no_interruption`,
`expect_interruption_detail_contains`, `expect_resume_phase`,
`expect_checkpoint_paths`, `expect_hidden_paths`,
`expect_agent_account_contains`, `expect_standing_work`,
`expect_base_is_standing_commit`, `expect_base_is_head_before_run`,
`expect_base_is_last_commit_of_previous`, `expect_restarted_from_previous`,
`expect_previous_state`, `expect_previous_holds_its_stream`,
`expect_state_of`, `expect_other_claimed`,
`expect_cannot_start`, `expect_queue_offered_dispatch`, `expect_logs_failed`, `expect_logs_healthy`,
`expect_draft_locked`, and `expect_rest_reason`, which reads the run's own
durable log.

The repository: `expect_commits`, `expect_committed_paths`,
`expect_paths_since_run_base`, `expect_commit_message_contains`,
`expect_branch_head_message_contains`, `expect_branch_commits_since_base`,
`expect_history_kept_and_grown_by`, `expect_last_commit_is_branch_head`,
`worktree_holds`,
`expect_worktree_lacks`, `expect_working_copy_present`,
`expect_review_checkout_reclaimed`, `expect_stream_released`,
`expect_stream_held`.

The reconciliation: `expect_update_state`, `expect_update_failure`,
`expect_update_turns`, `expect_update_escalation_origin`,
`expect_update_unsettled`, `expect_update_under_a_new_attempt`,
`expect_no_update_record`,
`expect_reconciliation_refused`, `expect_branches_unchanged`,
`expect_stream_branch_unchanged`, `expect_base_branch_unchanged`, `expect_base_head_message_contains`,
`expect_base_holds` and `expect_stream_contains_base`.

The dispatches: `expect_dispatch_order`, `expect_dispatch_count`, and
`expect_dispatch(index, |d| …)`, whose own chain reads `part`, `purpose`,
`pass`, `turn_kind`, `directory`, `directory_is_not`, `instruction`, `field`,
`field_contains`, `has_no_field`, `escalation_answers`, `execution_timeout_ms`,
`changed_paths`, `hidden_paths`, `list_len`, `decision_answers`, `resumes_session`,
`resumes_no_session`, `semantic_turn` and the merge reads above.

## Adding a journey

1. Add a line to the coverage matrix below and to the one in
   `GTE-graduation-end-to-end-tests.md`.
2. Write one `#[test]` in the journey file its subject belongs to, holding one
   `Scenario` chain. `matrix.rs` reads every `.rs` file of this directory
   that is not one of the language's own files as a journey file.
3. Tag it with the requirement ids it verifies, in the comment directly above
   the test function. Both the `GTE` requirements the suite itself answers and
   the `GRL` / `GXD` / `GRD` requirements the journey proves.
4. Assert the record **and** the repository. A journey that only asserts a state
   passes for a run that committed the wrong thing.
5. `cargo test graduation::tests::e2e` runs the suite alone.

Do not add an assertion that reads a private field of the loop, and do not add a
sleep. If a journey seems to need one, the harness is missing a control and the
control is the thing to add.

## Coverage matrix

| # | Journey | Test |
| --- | --- | --- |
| 1 | A `ready` review completes the run and commits only what changed | `a_ready_review_completes_the_run_and_commits_only_what_changed`, `a_commit_holds_what_the_working_copy_holds_and_not_what_an_envelope_claimed`, `each_turn_stands_where_its_phase_says_and_carries_that_phases_instruction`, `a_turn_that_removes_a_path_has_the_removal_committed`, `a_symbolic_link_a_turn_installs_is_read_back_like_any_other_path` |
| 2 | A `revise` review sends its findings into the next work turn, and a later `ready` completes | `a_revise_review_sends_its_findings_into_the_next_work_turn` |
| 3 | Two substantive `revise` reviews rest the run with the findings and no third pass | `two_revise_reviews_rest_the_run_for_the_author_with_no_third_pass` |
| 4 | A `revise` of at most two minor findings completes as advisory-ready | `two_minor_findings_complete_the_run_as_advisory_remarks`, `three_minor_findings_are_a_revision_rather_than_a_remark` |
| 5 | An agent-reported failure still reaches review, and the verdict decides | `an_agent_reported_failure_reaches_the_review_that_judges_what_it_wrote`, `an_agent_reported_failure_the_review_passes_still_completes_the_run` |
| 6 | An escalation persists its ordered questions, releases the stream, and its answers continue the same pass | `an_escalation_rests_the_run_and_its_answers_continue_the_same_pass`, `a_review_turn_escalates_on_the_same_terms_a_work_turn_does` |
| 7 | One corrected review, then a second invalid verdict blocks the run | `a_second_unreadable_verdict_blocks_the_run_with_the_named_code`, `one_unreadable_verdict_is_asked_again_and_the_run_goes_on` |
| 8 | Cancellation, `execution_timeout` and `launch_failed` each rest the run with its reason | `a_cancelled_turn_commits_its_work_as_abandoned_and_releases_the_stream`, `a_timed_out_turn_rests_the_run_rather_than_failing_it`, `a_turn_that_could_not_be_launched_rests_the_run_and_ends_nothing`, `a_timeout_names_the_limit_it_was_dispatched_under`, `a_timeout_that_wrote_nothing_says_nothing_was_committed`, `a_timeout_whose_stop_cannot_be_logged_rests_on_the_log_failure` |
| 9 | A stopped run resumes from its checkpoint without repeating a pass | `a_stopped_run_resumes_from_its_checkpoint_without_repeating_a_pass`, `a_run_that_blocked_in_the_review_resumes_in_the_review`, `a_run_recorded_before_the_window_existed_resets_on_the_first_continue` |
| 10 | The binary mock refuses bad arguments and stdin, and replays a valid protocol response | `binary_protocol.rs`, four tests |
| 11 | A spent retry/pass budget rests the run, and Continue resets only that budget | `a_spent_pass_budget_rests_the_run_and_continue_resets_only_that_budget`, `a_wider_pass_budget_grants_more_passes_before_the_run_rests`, `continue_leaves_the_window_alone_for_a_run_resting_on_something_else` |
| 12 | The author's pause is kept as the reason, and Continue resumes at the phase that stopped | `an_author_pause_mid_work_rests_the_run_and_continue_resumes_it`, `an_author_pause_during_the_review_resumes_at_the_review`, `the_review_resume_point_is_spent_once_the_review_has_run`, `a_turn_that_finishes_as_the_author_pauses_still_rests_paused`, `a_timeout_after_an_earlier_pause_is_not_called_a_pause`, `a_pause_that_races_the_time_limit_reads_as_the_pause`, `a_shutdown_that_races_an_exit_reads_as_the_shutdown` |
| 13 | A shutdown, a project change and a relaunch each stop the running turn and name themselves | `an_application_shutdown_stops_the_turn_and_names_itself`, `a_project_change_during_the_review_resumes_at_the_review`, `a_shutdown_leaves_a_blocked_run_on_another_stream_blocked`, `a_run_abandoned_by_a_relaunch_rests_as_execution_abandoned`, `an_application_shutdown_during_the_review_resumes_at_the_review` |
| 14 | `agent_exited`, `unreadable_answer` and `agent_terminated` rest the run with its work committed, and no commit is ever empty | `a_turn_that_exits_nonzero_answers_malformed_or_is_terminated_rests_with_its_work_committed`, `a_review_turn_that_times_out_commits_the_unreviewed_work_and_resumes_with_a_work_turn`, `a_second_stop_that_wrote_nothing_new_makes_no_empty_commit`, `a_resumed_turn_that_restores_the_base_commits_the_base_back`, `only_the_abandoned_commit_at_the_head_takes_the_graduation_message`, `a_rewrite_the_index_refuses_blocks_the_run_and_continue_retries_it` |
| 15 | A lost log stream rests the run, and Continue waits for the repair | `a_log_stream_lost_mid_run_rests_the_run_and_continue_waits_for_the_repair` |
| 16 | A refused commit and a refused standing commit block the run, and Continue resumes at the phase | `a_commit_the_index_refuses_blocks_the_run_and_continue_resumes_at_the_review`, `a_standing_commit_the_index_refuses_blocks_before_any_turn` |
| 17 | The same blocker twice rests the run for the author; a missing working copy and a refused review checkout block it | `a_blank_prompt_blocks_twice_and_rests_for_the_author_keeping_the_blocker`, `an_unreadable_verdict_repeated_after_continue_rests_the_run_for_the_author`, `a_stream_whose_working_copy_went_while_resting_blocks_on_stream_missing`, `a_review_checkout_that_cannot_be_made_blocks_and_continue_retries_the_review` |
| 18 | A discarded run refuses every act, reverts nothing, and a turn in flight commits nothing | `a_discarded_run_refuses_continue_answers_pause_and_discard`, `discarding_a_run_reverts_nothing_and_keeps_its_branch_and_working_copy`, `a_run_discarded_mid_turn_commits_nothing_and_leaves_the_work_uncommitted` |
| 19 | A restart answers the discarded prompt as a new run, and is refused from any other state | `a_restarted_run_answers_the_discarded_prompt_to_completion`, `a_restart_over_a_discarded_turns_work_takes_it_as_standing_work`, `a_restart_is_refused_from_any_state_but_discarded` |
| 20 | A revert adds commits and rewrites nothing; a completed run refuses every act | `reverting_a_completed_run_adds_commits_and_rewrites_nothing`, `revert_is_refused_where_nothing_was_committed_or_the_run_holds_its_stream`, `a_completed_run_refuses_continue_pause_answers_and_discard`, `a_resting_or_discarded_run_that_made_commits_can_be_reverted`, `a_revert_of_a_run_that_waits_on_the_author_ends_it`, `a_queued_run_refuses_revert` |
| 21 | Answers resume the session that asked, partial sets are refused, and a review's answers reach a fresh review | `an_answered_escalation_resumes_the_session_that_asked`, `a_partial_answer_is_refused_and_changes_nothing`, `an_answered_review_escalation_asks_a_fresh_review_turn`, `a_review_escalation_answered_into_a_revision_sends_no_answers_to_the_work_turn`, `a_review_escalation_answered_into_an_unreadable_verdict_keeps_the_answers`, `an_escalation_outside_the_rules_is_not_recorded`, `an_escalation_on_the_second_pass_keeps_the_pass_and_the_findings`, `a_pause_during_the_answered_review_keeps_the_answers_for_the_resumed_review`, `answers_to_a_run_that_asks_nothing_are_refused` |
| 22 | Ignored output is named to the review and not committed; review writes reach nothing | `ignored_output_is_not_committed_and_is_named_to_the_review`, `what_a_review_turn_writes_reaches_neither_the_stream_nor_the_commit` |
| 23 | Keep, commit, and commit-and-push each settle the run's base as recorded | `keep_leaves_the_standing_work_to_the_runs_own_commit`, `commit_makes_the_standing_work_the_base_and_keeps_it_out_of_the_runs_commit`, `commit_and_push_without_a_remote_records_the_refused_push_and_completes` |
| 24 | A run stacks on the run before it, waits behind a blocked one, and obeys the stream limit | `a_run_behind_a_completed_run_starts_from_its_commit`, `a_run_behind_a_blocked_run_waits_until_the_repeat_rests_the_first_for_the_author`, `the_project_stream_limit_refuses_a_second_stream_while_one_works`, `a_wider_stream_limit_lets_a_second_stream_work_alongside`, `a_run_behind_a_run_discarded_mid_turn_takes_the_stream_and_its_work` |
| 25 | A verdict that breaks its own shape is asked again; mixed severities are a revision | `a_verdict_that_breaks_its_own_shape_is_refused_and_asked_again`, `one_minor_and_one_major_finding_are_a_revision` |
| 26 | A graduated stream is updated clean, conflicted, escalated, bounded, cancelled, refused and stale, and no run claims it meanwhile | `a_clean_base_change_updates_the_stream_with_no_semantic_turn`, `a_stream_already_holding_the_base_updates_nothing`, `a_conflicting_update_spends_one_semantic_turn_and_lands_what_it_settled`, `an_escalated_update_is_answered_and_the_answers_reach_the_next_turn`, `three_semantic_turns_that_settle_nothing_leave_the_update_conflicted_and_a_retry_grants_three_more`, `a_cancelled_update_moves_neither_branch`, `an_update_whose_turns_cannot_be_launched_fails_and_may_be_retried`, `an_update_is_refused_while_a_blocked_run_holds_the_stream`, `a_run_cannot_claim_a_stream_an_update_is_rewriting`, `an_update_pinned_to_a_base_revision_the_branch_has_left_writes_nothing`, `an_update_is_refused_while_the_stream_holds_a_resting_run`, `answers_and_retries_are_refused_while_the_stream_holds_a_live_run`, `an_update_is_accepted_beside_a_discarded_run_and_a_run_of_another_stream` |
| 27 | A setting changed during one semantic turn reaches the next | `a_setting_changed_during_one_semantic_turn_reaches_the_next` |
| 28 | A clean merge and a stream with nothing to merge finish with no run, and a refused merge writes nothing | `a_clean_merge_commits_once_and_leaves_no_run`, `a_clean_uncommitted_merge_leaves_the_result_unstaged_and_no_run`, `a_stream_with_nothing_to_merge_reports_it_and_writes_nothing`, `a_merge_refused_for_a_dirty_side_writes_nothing`, `a_merge_refused_while_the_stream_holds_a_run_writes_nothing`, `a_merge_refused_while_the_update_guard_is_held_writes_nothing`, `a_merge_refused_for_what_it_cannot_reach_writes_nothing`, `a_clean_merge_needs_no_agent_and_no_image` |
| 29 | A conflict hands off to one queued Merge run, both branches and working copies unchanged, and a refused preflight makes none | `a_conflict_hands_off_to_one_queued_merge_run_and_writes_nothing`, `the_handoff_records_an_uncommitted_publication`, `a_refused_image_preflight_creates_no_run_and_writes_nothing` |
| 30 | Findings reach the next merge work turn, a ready review applies once, and a marker left counts as a critical finding | `a_ready_merge_review_applies_the_merge_after_one_pass_and_no_further_turn`, `a_revise_merge_review_sends_its_findings_whole_into_the_next_merge_work_turn`, `a_single_minor_finding_of_a_merge_review_still_starts_another_pass`, `a_ready_review_over_a_result_with_a_marker_is_replaced_by_a_critical_finding`, `an_ordinary_run_carries_no_merge_context_and_no_merge_instruction` |
| 31 | A merge run waits for a free slot, shares its stream's queue, and releases the stream and the slot at rest | `a_merge_run_holds_the_stream_and_a_slot_while_it_works_and_neither_at_rest`, `a_merge_run_waits_for_a_free_project_slot_when_the_limit_is_full`, `a_wider_limit_offers_the_merge_run_a_dispatch_beside_another_stream`, `a_run_behind_a_merge_run_waits_until_the_merge_run_rests`, `a_second_merge_is_refused_while_the_stream_holds_a_merge_run` |
| 32 | Two passes, then awaiting_author; Continue adds two passes; discard reclaims and writes no branch | `a_merge_run_takes_two_passes_whatever_the_project_budget_and_then_rests`, `continue_adds_two_passes_and_resumes_the_same_merge_worktree`, `continue_again_after_a_second_exhaustion_grants_two_more_passes`, `discarding_a_resting_merge_run_reclaims_what_it_owns_and_writes_no_branch` |
| 33 | A moved tip fails the run at a dispatch or at the apply, and refuses Continue and an answer, writing nothing | `a_base_tip_that_moved_before_the_first_dispatch_fails_the_run`, `a_stream_tip_that_moved_before_the_first_dispatch_fails_the_run`, `continue_and_an_answer_are_refused_when_a_tip_moved`, `continue_after_a_spent_budget_is_refused_when_the_base_tip_moved`, `a_tip_that_moves_during_a_turn_fails_the_run_at_the_next_dispatch`, `a_tip_that_moves_before_the_apply_fails_the_run_and_writes_nothing` |
| 34 | Uncommitted and commit publication; a dirty side, a held guard and a failing write block at the apply phase | `a_commit_publication_makes_one_merge_commit_with_both_pinned_tips_as_parents`, `an_uncommitted_publication_leaves_the_result_unstaged_in_the_base_working_copy`, `a_dirty_base_blocks_the_apply_and_continue_retries_it_alone`, `a_dirty_stream_blocks_the_apply_and_continue_retries_it`, `a_held_update_guard_blocks_the_apply_and_continue_retries_it`, `a_failing_write_blocks_the_apply_and_continue_retries_it` |
| 35 | Nothing is durable before the handoff, a stopped run is interrupted and resumable after it, and an ended run is reclaimed | `a_relaunch_before_the_handoff_leaves_nothing_durable`, `a_working_merge_run_found_after_a_relaunch_rests_interrupted_and_continue_resumes_it`, `a_reviewing_merge_run_found_after_a_relaunch_rests_interrupted_and_continue_resumes_the_pass`, `the_first_queue_read_reclaims_what_an_ended_merge_run_left` |
| 36 | An escalation is answered into its phase, pause and discard behave as for every run, restart and revert are refused | `an_escalation_of_a_merge_work_turn_is_answered_into_the_work_turn`, `an_escalation_of_a_merge_review_turn_is_answered_into_a_fresh_review`, `a_pause_rests_a_merge_run_and_continue_resumes_the_phase_that_stopped`, `a_discard_during_a_merge_turn_writes_nothing_and_reclaims_what_the_run_owned`, `restart_and_revert_are_refused_for_a_merge_run`, `auto_start_reorder_and_archive_apply_to_a_merge_run`, `a_discard_of_a_working_merge_run_reclaims_nothing_until_the_turn_has_returned`, `a_discard_of_a_reviewing_merge_run_reclaims_nothing_until_the_turn_has_returned`, `a_discard_of_a_resting_merge_run_reclaims_at_once`, `a_discard_between_the_verdict_and_the_apply_applies_nothing`, `a_pause_between_the_verdict_and_the_apply_applies_nothing`, `a_ready_verdict_that_arrives_with_a_discard_or_a_pause_applies_nothing`, `a_lost_apply_record_does_not_undo_a_completed_apply` |
| 37 | The turns of a merge run are attributed as work and review turns, the apply is a commit record, and no statistic line is written | `the_turns_of_a_merge_run_are_attributed_as_work_and_review_turns`, `a_blocked_apply_writes_no_apply_record_and_the_retry_writes_one`, `a_merge_run_writes_no_statistic_line` |

The settings journeys stand beside 11 in `budget.rs`, because they are the same
contract read from the other end: `a_setting_changed_between_two_turns_reaches_the_second_of_them`,
`a_resumed_run_uses_the_settings_active_at_the_dispatch_that_resumes_it`,
`a_stored_bound_outside_its_range_leaves_the_loop_at_its_own_default`,
`a_budget_lowered_while_a_turn_runs_rests_the_run_at_the_next_dispatch`,
`a_budget_lowered_before_the_review_rests_the_run_rather_than_blocking_it` and
`a_run_resumed_under_a_smaller_budget_rests_again`.

The retry budget's other meaning — the physical attempts of one provider call —
is proved where that loop lives, by
`src-tauri/src/agent_conversations/tests/project_bounds.rs`, which drives real
conversation turns under a configured project. This suite drives the graduation
loop and nothing else.

`matrix.rs` holds the suite to this table: a journey with no test, and a test
this table never names, each fail it (GTE-FR-CSEH).

## What this suite does not cover

- The container argument vector the executor generates. That crosses the same
  process boundary and is asserted by `src/tools/agent_exec/mock_tests/`.
- Tauri commands' IPC registration, UI rendering, and frontend persistence,
  which have suites of their own.
- The conversation loop's half of the shared retry-budget contract, which
  `agent_conversations/tests/project_bounds.rs` proves.

One test is not instant: `binary_protocol.rs` builds `agentic-cli-mock` when the
executable is not there yet, which is a `cargo build` rather than a scripted
turn. It is built rather than skipped, because a test that quietly does nothing
when its double is missing reports success for a run that verified nothing.
