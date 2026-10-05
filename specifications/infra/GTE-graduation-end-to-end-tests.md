# Graduation end-to-end tests

**Spec code:** `GTE`

## Intent
The backend end-to-end suite for graduation, and the scenario language it is written in. A graduation run is a state machine, a working copy, a set of commits, a checkpoint and a stream claim that move together, and a fault in one of them is invisible in a unit test of any one part. This suite drives the **production** loop, the production stream update job and the production merge run (the merge of a stream into its base branch, which is a graduation run that carries `merge` data), through `../core/GXD-graduation-execution.md`'s `dispatch_graduation_turn` seam, with protocol-valid scripted agent answers and a repository of its own, and asserts what the run, the stream and the repository then hold. Each scenario reads as one user journey: arrange the project, script the turns, act on the run, assert the record and the repository. Out of scope: the IPC registration of the commands, UI rendering, and frontend persistence, each of which has its own suite; the loop's decisions, which are `../ai/GRL-graduation-loop.md`'s; and the container a turn runs in, which is `../tools/EAC-execute-agent-cli.md`'s.

## Functional requirements
1. **GTE-FR-QVHM** The suite drives the production graduation loop, the production state machine, the production stream update job, and the production merge run: the pre-conflict merge, the handoff that creates the run, and the run's passes, its apply step and its cleanup. The one thing it substitutes is the dispatch seam of `../core/GXD-graduation-execution.md` GXD-FR-QLFA, and it reaches no other route into them.
2. **GTE-FR-BWKD** A scenario is written in one fluent chain with four stages in a fixed order: **arrange** the project, the stream and the settings; **script** the turns the agent answers with; **act** on the graduation run; and **assert** the run record and the repository. A stage method is named for what the author does, not for what the loop calls.
3. **GTE-FR-ZPNC** Every scenario composes its task input, its turn outcomes, its agent response envelopes, its review verdicts, its escalations and its checkpoint reads from the **production types**, and each is validated by the production rules before the scenario uses it. No scenario asserts a private field of the loop.
4. **GTE-FR-LRJT** Each scenario owns a temporary project repository, a work stream, a working copy and a run store of its own, and nothing of one scenario is readable by another. Every temporary directory is removed when the scenario ends, whether it passed or failed.
5. **GTE-FR-XKGB** The scripted seam records **every** dispatch in order. A scenario asserts the turn kind, the purpose, the pass, the execution directory, the instruction, the resumed vendor session, and each field of the `GraduationTaskInput` — the correction, the agent failure, the loop instruction and the escalation answers included — for any dispatch by its position.
6. **GTE-FR-HNSA** A scripted turn changes the execution directory as a real turn does: it writes, edits, deletes and links paths there before it answers, so what the loop reads back is a working copy rather than a claim in an envelope (per `../ai/GRL-graduation-loop.md` GRL-FR-NVBZ).
7. **GTE-FR-HWQM** A scripted turn may run a **hook** before it answers. The hook receives the application, the run, the stream, the working copy and the store roots, and acts on them as the author, the application or the machine does during a turn: a pause, a discard, a shutdown, a held index, a lost log stream, or a second run's claim.
8. **GTE-FR-TPLD** A scenario acts on a resting run as the author does — continue, answer, discard, restart and revert — and removes the obstruction a hook left before it acts again. Each act rebinds the scripted seam, so the dispatch assertions after it read the turns of that act alone.
9. **GTE-FR-RZKC** A scenario asserts a refused command by its error code, and asserts that the refusal left the run's state, blocker, escalation, commits, pass and branch head unchanged.
10. **GTE-FR-YAEB** No scenario lets the production queue start a run through the production seam. The harness intercepts every dispatch the production queue attempts and fails the scenario over it. A run that waits on a stream is enqueued with auto-start disabled, and the scenario drives it through the scripted seam itself.
11. **GTE-FR-NMUF** A scenario updates a stream from its base branch with the semantic turn scripted through the same seam, and asserts the update record, the paths still unsettled, and what both branches and both working copies hold. A scenario that merges a stream into its base branch does not use this requirement: it follows GTE-FR-TNZF to GTE-FR-DQLR.
12. **GTE-FR-TCUW** A scenario asserts the persisted run state, the pass count, the work and review turn counts, the checkpoint, the escalation, the blocker and the interruption, read back from the run store rather than from a value the loop returned.
13. **GTE-FR-MDVQ** A scenario asserts the observable repository: the paths the run changed, the commits it created, each commit message, and the paths a commit does **not** hold, so work swept in by accident fails a scenario.
14. **GTE-FR-PFEO** A scenario asserts whether the stream is claimed or free at the moment the run comes to rest, so a run that keeps a stream it must release fails.
15. **GTE-FR-JYWB** A failed assertion names the scenario, the turn it belongs to, and the thing that was compared. A dispatch nobody scripted fails the scenario naming the part and the purpose the loop asked for.
16. **GTE-FR-RSQD** The suite is **deterministic**. It takes no sleep, asserts no wall-clock duration, and reaches no network, no provider credential and no container runtime. A timeout and a cancellation are each produced by a bounded control of the harness rather than by waiting.
17. **GTE-FR-KAZX** The harness sets the project settings each scenario runs under, and a scenario may set the execution timeout, the provider-call deadline, the retry budget and the graduation concurrency limit to any valid value (per `../core/PSS-project-settings-storage.md` PSS-FR-TQMV, PSS-FR-HDBN, PSS-FR-WPKS and PSS-FR-JRWC).
18. **GTE-FR-VNOU** A scenario proves that the settings are read **before every dispatch**, the semantic rebase turn included, by changing a value between two turns of one run and asserting the second turn carried the new value (per `../ai/GRL-graduation-loop.md` GRL-FR-KWNP).
19. **GTE-FR-GBLI** A scenario proves that a resumed run uses the values active at each resumed dispatch, by changing a setting while the run rests and asserting the resumed turn carried the changed value.
20. **GTE-FR-WUAP** The suite covers the journeys of the coverage matrix, one scenario each at least. A journey with no scenario is a gap the matrix names.
21. **GTE-FR-DKMF** One scenario launches the `../infra/ACM-agentic-cli-mock.md` executable through the production executor. It proves the mock refuses an invalid argument vector and invalid stdin, and that a valid protocol response is replayed and read, so the integration uses the declared protocol rather than a test-only shortcut.
22. **GTE-FR-OYTZ** The suite runs in isolation by its own module filter and as part of the backend `cargo test` lane, and it needs nothing the lane does not already install.
23. **GTE-FR-CSEH** `src-tauri/src/graduation/tests/e2e/README.md` documents the scenario language, the harness boundary, how a scenario is added, and the coverage matrix. A journey added to the matrix without a scenario, or a scenario naming no journey, is a fault the suite reports.
24. **GTE-FR-TNZF** A scenario starts a merge as the author does, through `merge_work_stream` with a publication choice, and asserts the `StreamMergeResult` it returns. A clean merge returns `merged`, and `nothing_to_merge` is returned where the stream holds nothing the base does not. Both leave **no merge run** in the run store and in the queue index, hold no stream claim and no project slot afterwards, and write the base branch only as the publication choice says. A merge refused with `stream_busy`, `stream_dirty`, `base_dirty`, `base_not_checked_out`, `merge_in_progress`, `update_in_progress`, `unknown_stream` or `not_a_git_repository` writes nothing and leaves both branches, both working copies and the run store unchanged (per GTE-FR-RZKC).
25. **GTE-FR-LMXV** A scenario with a merge Git cannot resolve asserts the **handoff**: the result is `conflicted` and names the run id and the conflicted paths, the run exists in state `queued`, it is named `Merge <stream name>`, it carries `merge` data with both pinned tips, the merge base, the snapshot commit, the changed paths, the unresolved paths and the recorded publication, and it holds no draft. The scenario also asserts that both branch heads, the base working copy and the stream working copy hold exactly what they held before the merge started. A refused image preflight (`vendor_image_unconfigured`, `vendor_image_invalid`, `vendor_execution_unsupported`, `docker_backend_unverified`) creates no run and writes nothing. The dispatch the production queue is offered at the handoff is held by the harness, and the scenario drives the run itself through the scripted seam (per GTE-FR-YAEB).
26. **GTE-FR-RDPE** A scenario scripts the turns of a merge run through the same seam with `merge_work` and `merge_review` turns, and asserts every dispatch by position (per GTE-FR-XKGB): the part is `merge_work` or `merge_review`, the pass, the execution directory (the run's merge worktree for a work turn, a review checkout for a review turn), and the `merge` context of the task input, with its unresolved paths, its changed paths, its conflicts and, on a review turn, the paths the work changed beyond the unresolved ones. A `revise` review carries its findings whole into the next `merge_work` turn. A `ready` review is followed by the apply step and by no further turn. Both branch heads and both working copies are unchanged at every dispatch, up to the apply step.
27. **GTE-FR-YCQW** A scenario proves that a merge run takes its turn from the stream's queue like every run. A merge run enqueued behind a run on the same stream waits until that run rests, a merge run enqueued while the project's graduation concurrency limit is full waits for a free slot (per GTE-FR-KAZX), and the run holds the stream and one project slot while it works and reviews and holds neither once it rests in `awaiting_author`, `interrupted` or a terminal state (per GTE-FR-PFEO). A stream that holds a non-terminal run, a merge run included, refuses a second merge with `stream_busy`.
28. **GTE-FR-HJGA** A scenario proves the **pass budget** of a merge run. The run takes two passes whatever the project's pass budget setting holds. Where the review still answers `revise` after the second pass, the run rests `awaiting_author` with the reason `pass_budget_exhausted`, and releases the stream and the project slot. The author's Continue adds two passes, numbers them after the passes already made, keeps the merge worktree and the snapshot, and resumes without repeating a completed pass. A review that answers `revise` with minor findings alone still starts another pass or rests the run, because the advisory-minor rule does not apply to a merge review. Discard of a resting run leaves both branches and both working copies unchanged, and reclaims the merge worktree, its registration, the scratch branch and the snapshot ref.
29. **GTE-FR-KBVN** A scenario proves the **branch-tip refusals**. The harness moves the base branch tip or the stream branch tip between the handoff and a later step, and the scenario asserts four outcomes. A tip that moved before a dispatch ends the run `failed` with `merge_branch_moved`. Continue and an escalation answer on a resting run are refused with `merge_branch_moved` and leave the run unchanged (per GTE-FR-RZKC). A tip that moved before the apply step ends the run `failed` with `merge_branch_moved`. In every case no branch and no working copy is written by the run.
30. **GTE-FR-SFTU** A scenario proves both **publication** choices at the apply step. With `uncommitted` the merge result stands unstaged in the base working copy and the base branch head is unchanged. With `commit` the base branch holds one new merge commit that has the pinned base tip and the pinned stream tip as its two parents and carries the author's message. In both cases the run is `completed`, its `merge.result` names the publication and the merged paths, and the base branch is written by no earlier step.
31. **GTE-FR-CZPM** A scenario proves the **apply blockers**. A dirty side, a held repository update guard and a failing apply each rest the run `blocked` with `merge_dirty_side`, `merge_guard_held` and `merge_apply_failed`, at the apply phase, with no new turn spent, and Continue retries the apply alone once the obstruction is removed (per GTE-FR-TPLD). A result that still holds a conflict marker in an unresolved path is handled as a `revise` verdict with one `critical` finding, and the scenario asserts the next pass carries it.
32. **GTE-FR-WXEJ** A scenario proves the **controls** of a merge run. An escalation of a merge turn rests the run `awaiting_author`, and the answers reach a turn of the phase that asked. Pause and discard behave as for any run. `restart_graduation_run` is refused with `run_state_not_permitted` and `revert_graduation_run` with `run_not_revertable`, each leaving the run unchanged.
33. **GTE-FR-DQLR** A scenario proves the **restart of the application** for a merge. A relaunch before the handoff leaves nothing durable: no run, no merge worktree, no snapshot ref, and the author starts the merge again. A relaunch after the handoff leaves the run on disk: a run found `working` or `reviewing` with no loop rests `interrupted` with `execution_abandoned` on the first queue read, and Continue resumes it from the checkpoint the run holds. A run that ended before the relaunch has its merge worktree, its registration, its scratch branch and its snapshot ref reclaimed by the first queue read.
34. **GTE-FR-VRHN** A scenario reads the run's logs from the run store and proves that the turns of a merge run are attributed as `GRS-graduation-run-log-storage.md` GRS-FR-HBQT and GRS-FR-WNRC set out, and that the loop wrote no statistic for the run (per `../ai/GLG-graduation-loop-logging.md` GLG-FR-HRTD).

## Contract surface

### Where the suite lives
```
src-tauri/src/graduation/tests/e2e/           the suite
src-tauri/src/graduation/tests/e2e/README.md  the scenario language and the matrix
src-tauri/src/graduation/tests/mod.rs         the fixture and the scripted seam every graduation suite shares
tools/agentic-cli-mock/                       the binary the protocol scenario launches
```

### The scenario language
One chain per journey, in the order of GTE-FR-BWKD:

```text
Scenario::named("ready first pass")
    .with_setting(Setting::PassBudget, 2)              // GTE-FR-KAZX
    .with_stream("editor")
    .with_prompt("Add the empty state to the panel.")
    .work_turn(Script::reports_success().writes("src/panel.ts", "…"))
    .review_turn(Script::answers_ready("The work answers the prompt."))
    .run()
    .expect_state(GraduationRunState::Completed)
    .expect_pass(1)
    .expect_commits(1)
    .expect_committed_paths(&["src/panel.ts"])
    .expect_stream_released()
    .expect_dispatch(0, |d| d.part("work").purpose("generate").pass(1))
```

`expect_dispatch` reads one recorded dispatch through `DispatchAssert` (GTE-FR-XKGB).

### The turn script
What one scripted turn does, and what it then answers:

```text
Script
  answer:  reports_success
         | reports_failure(message)                 // GRL-FR-DNKA
         | answers_ready(rationale) | answers_ready_with(findings)
         | answers_revise(severity, description) | answers_revise_with(findings)
         | answers_revise_without_findings | answers_with_blank_rationale
         | answers_unreadably                       // GRL-FR-TVXI
         | escalates(questions) | escalates_because(reason, questions)
         | claims_paths(paths)                      // GXD-FR-LBYG
         | is_cancelled | times_out | exits_nonzero | answers_malformed | is_terminated
         | cannot_launch(error)                     // GXD-FR-XEUX
  effects: writes, deletes, removes_dir, links     // GTE-FR-HNSA
           leaves_a_session
           changing(setting, value)                 // GTE-FR-VNOU
  hooks:   then(|context| …)                        // GTE-FR-HWQM
           pausing_the_run | discarding_the_run | stopping_the_application(reason)
           holding_the_index | losing_the_log(stream) | abandoned_by_a_relaunch
           starting_another_run(label, scripts)
```

### Acts
```text
run                                   drive the run to rest
continued(scripts)                    the author's Continue            // GTE-FR-TPLD
answered(answers, scripts)            the author's answers
discarded | restarted(choice, scripts) | reverted
releasing_the_index | deleting_branch(name) | repairing_the_log(stream) | removing_the_working_copy
driving(label, scripts)               drive a second run the scenario enqueued   // GTE-FR-YAEB
refusing_continue | refusing_pause | refusing_discard
refusing_revert | refusing_restart | refusing_answers                    // GTE-FR-RZKC
updated(strategy, scripts)                                              // GTE-FR-NMUF
update_answered | update_retried
merging(publication)                  the author's Merge: returns the StreamMergeResult   // GTE-FR-TNZF
relaunching                           drop every in-memory state, read the stores again   // GTE-FR-DQLR
moving_the_base_tip | moving_the_stream_tip       advance a branch as another author does // GTE-FR-KBVN
```

A merge run is driven with the acts above. `driving(label, scripts)` drives it through the scripted seam, `continued`, `answered` and `discarded` act on it, and `refusing_restart` and `refusing_revert` assert the refusals of GTE-FR-WXEJ.

### The merge turn script
The same `Script` answers, effects and hooks serve a merge run. Two chain stages script its turns:

```text
.merge_work_turn(Script)      the answer of one merge_work turn, in dispatch order   // GTE-FR-RDPE
.merge_review_turn(Script)    the answer of one merge_review turn, in dispatch order
```

`expect_dispatch` reads a merge dispatch with `d.part("merge_work")` or `d.part("merge_review")`, and reads the `merge` context of the task input.

### The coverage matrix
```text
 1  ready first pass          work → ready review → completed, only changed paths committed
 2  revise then ready         findings reach the second work turn, ready ends the run
 3  two revise reviews        awaiting_author with the findings, no third pass
 4  advisory ready            revise of at most two minor findings completes, no further work turn
 5  agent-reported failure    review still runs and carries the failure, its verdict decides
 6  escalation and answer     ordered questions persisted, stream released, same pass continues
 7  invalid verdict twice     one corrected review, then blocked with review_verdict_invalid
 8  interruption              cancel, execution_timeout and launch_failed each rest interrupted with its reason
 9  resume from checkpoint    no completed pass is repeated and no scripted context is lost
10  binary protocol           the mock refuses bad arguments and stdin, and replays a valid response
11  budget exhausted          awaiting_author with the reason, Continue resets that budget alone
12  author pause              author_pause kept, Continue resumes at the phase that stopped
13  application stops         application_shutdown, project_changed and execution_abandoned each kept
14  turn with no answer       agent_exited, unreadable_answer and agent_terminated rest the run, no empty commit
15  log persistence           log_persistence_failed, Continue refused until the stream is repaired
16  commit blockers           commit_failed and base_commit_failed block, Continue resumes at the phase
17  repeating blockers        the same code twice rests for the author, stream_missing, review_checkout_failed
18  discard                   refusals after discard, nothing reverted, a turn in flight commits nothing
19  restart                   a new run over the discarded prompt, refused from any other state
20  revert and terminal       revert adds commits, refusals, a completed run refuses every act
21  escalations               session resumed, partial answers refused, review answers reach a review turn
22  change set                ignored output named to the review, review writes reach nothing
23  standing work             keep, commit and commit_and_push each settle the base as recorded
24  queue and streams         a run stacks on the run before it, waits behind a blocker, obeys the limit
25  verdict edges             ready with findings, revise without, blank rationale, mixed severities
26  update after graduation   clean, conflicted, escalated, bounded, cancelled and refused updates
27  semantic turn settings    a setting changed during one semantic turn reaches the next
28  merge, no run             clean and nothing_to_merge merges finish with no run, both publications, refusals write nothing
29  merge handoff             a conflict creates one queued Merge run, branches and working copies unchanged, preflight refusals create none
30  merge revisions           findings reach the next merge_work turn, ready applies once, markers left count as a critical finding
31  merge queue               the run waits behind a run on its stream, obeys the project limit, releases at rest
32  merge budget              two passes, awaiting_author, Continue adds two passes, discard reclaims and writes no branch
33  merge tips moved          dispatch, Continue, answer and apply each refuse or end with merge_branch_moved
34  merge apply               uncommitted and commit publication, dirty side, guard held and failed apply block at the apply phase
35  merge restart             nothing durable before the handoff, interrupted and resumable after it, ended run reclaimed
36  merge controls            escalation answered, pause, discard, restart and revert refused
37  merge logs                turns attributed as work and review turns, apply record, no statistic line
```

## Non-functional requirements
- The whole suite completes in the time its scripted turns take, which is milliseconds each: no scenario waits for a clock.
- A scenario reads as a journey rather than as a call graph, so a reader who knows the specification and not the module can tell what is asserted.
- Adding a journey costs one scenario and one matrix line, and changes no other scenario.
- The suite writes nothing outside its own temporary directories and the build's target directory.
- Every file of the suite holds at most 1000 lines.
