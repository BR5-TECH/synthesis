# Graduation

**Spec code:** `GRD`

## Intent
The backend module that turns a finished draft prompt into committed work. A draft is refined in conversation until its author judges the prompt ready (per `../ui/NAW-new-artifact.md`); **graduation** is what happens next. The prompt is captured, an execution agent is driven against a work stream's working copy until it reports the work finished, a fresh agent session reviews what it did, and the application commits the result onto the stream's branch. The same state machine also carries the **merge of a work stream into its base branch** when Git cannot settle that merge alone: a **merge run** drives an agent to resolve the conflicts, a fresh session reviews the result, and the application applies it to the base branch only after a `ready` review. The application enforces no shape on the work itself: what a change must contain is what the project's own instructions and skills say, read by the agent in the repository it stands in. A run works in one of three places: a work stream's working copy; for a **direct run**, the worktree that was active when the author confirmed, on the branch it held then; or, for a **merge run**, an isolated worktree the run owns, so that no live branch and no live worktree is written before the merge is approved. This module owns the run's state machine, its durable checkpoint, the queues that decide which run writes a working copy, the durable history of every run the project has made, and the archive classification that history is filed under. Out of scope: the stream itself, and the Git merge that runs before any merge run exists and hands a conflict to one, which are `WKS-work-streams.md`'s; the reconciliation of a stream update, which is `GRB-graduation-rebase.md`'s; and the loop that drives the agent turns, which is `../ai/GRL-graduation-loop.md`'s.

## Functional requirements
1. **GRD-FR-QJHM** A run stands in exactly one state: `queued`, `working`, `reviewing`, `awaiting_author`, `blocked`, `interrupted`, `completed`, `discarded`, or `failed`. The last three are terminal.
2. **GRD-FR-PZAK** Every stream run, a merge run included, names the **work stream** it runs in, recorded once when the run is enqueued and never changed. The name outlives the stream: a run whose stream was deleted still names it (per `WKS-work-streams.md` WKS-FR-FJYN). A direct run names its pinned target (GRD-FR-BSNI) and names a stream only where that target is a stream's working copy.
3. **GRD-FR-BNTC** **One run of a queue works at a time.** A run holds its stream, or its pinned worktree, while its state is `working`, `reviewing`, or `blocked`; every other state holds nothing. Queues work at the same time up to the project's limit (GRD-FR-KKKN).
4. **GRD-FR-VLFO** The project keeps **one durable run order** holding every run it has ever made. A new run is inserted at the latest end. Each queue is the subsequence of that order holding the queue's members, read from the earliest end. A queue belongs to one stream or to one ordinary worktree (GRD-FR-ZVNO).
5. **GRD-FR-EGWS** A queue dispatches the **earliest eligible member**: the first member in `queued` whose auto-start is enabled, or an author-paused run the author has resumed. A member that is working, waiting, blocked, or terminal is passed over rather than waited for. A direct run whose pinned worktree is not on its pinned branch is not eligible (GRD-FR-XRDY).
6. **GRD-FR-TKUR** Every run entering `queued` has `auto_start = true`. `set_graduation_auto_start(run_id, enabled)` is accepted from `queued` alone. Disabling keeps the run in its place and makes it ineligible, so the scheduler passes it over and it blocks nothing behind it.
7. **GRD-FR-MDQZ** `pause_graduation_run(run_id)` is accepted while the run is doing agent work — `working` or `reviewing` — and is refused elsewhere with `run_state_not_permitted` naming the state. It cancels the turn through the one cancellation path (per `../ai/GRL-graduation-loop.md` GRL-FR-ZDKP), persists the checkpoint, and releases the stream. The interruption reason stays `author_pause` after the stopped turn returns.
8. **GRD-FR-CYIB** `continue_graduation_run(run_id)` is the one operation behind both **Continue** and **Resume**. Against an interrupted or blocked run it retries at once and clears the blocker but not its repeat count. Against a run whose pass budget is spent it resets that budget alone and resumes from the checkpoint, repeating no completed pass (per `../ai/GRL-graduation-loop.md` GRL-FR-XBUE; a merge run's budget is `../ai/GRL-graduation-loop.md` GRL-FR-CZBT). Against a merge run it first checks the pinned tips and refuses on a moved tip (GRD-FR-XHSE); against a merge run `blocked` at the apply it retries the apply alone (GRD-FR-JSBE). Against an author-paused run it sets `auto_start`, returns the run to the earliest position of its own stream's queue, and dispatches when the stream is free.
9. **GRD-FR-RHNP** `reorder_graduation_run(run_id, expected_position, new_position)` moves one run within **its own** queue. Positions index that queue from its earliest member as zero. A stale `expected_position` is refused with `queue_position_stale` carrying the index the run holds.
10. **GRD-FR-WSXA** The four operations that settle order, membership and eligibility — pause, resume, auto-start and reorder — are serialized against each other and against dispatch, under one exclusive hold on the project's store. Each reads, checks its precondition, writes, and only then emits.
11. **GRD-FR-JOFE** A run carries an **archive state independent of its lifecycle state**. `archive_graduation_run` and `unarchive_graduation_run` are accepted from every state, are idempotent, and change nothing else: not the run's state, its queue place, its stream, or its observability.
12. **GRD-FR-LGDV** A run's record is the project's durable history. `list_graduation_queue` and `get_graduation_run` answer for every run the project has made, terminal runs included, with the complete record of each. A record survives an application restart and the deletion of the stream the run ran in.
13. **GRD-FR-FTBQ** `start_graduation(draft_id, stream_id, standing_work, standing_work_message)` captures the draft's prompt, its name, and the checksum of the prompt bytes as a `CapturedGraduationInput`. The capture is taken once and is immutable for the run's life.
    - *Why:* A run answers the prompt as it stood when the author judged it finished, so an edit made afterwards is a different prompt and belongs to a different run.
14. **GRD-FR-DXWL** A draft a **non-terminal** run holds is locked, and the lock is enforced in draft storage rather than trusted to a caller (per `DRS-draft-storage.md` DRS-FR-19). A draft with any `completed` run is not deletable.
15. **GRD-FR-JGEC** A GitHub-shadow draft (per `DRS-draft-storage.md` DRS-FR-XDWS) graduates on the terms of every other draft. A run that commits makes it report `graduated` (DRS-FR-EZDB), and no run changes, closes, or reads the GitHub issue the draft names (per `GPP-github-polling.md` GPP-FR-CRWY).
15. **GRD-FR-YBUM** The run records the stream branch's revision as its **base commit** at the moment its first turn is dispatched, before the container starts. Every commit the run makes is measured from that revision. A merge run records its merge snapshot commit instead (GRD-FR-KZPT).
16. **GRD-FR-HQPD** What a run does with work standing uncommitted in its stream when it is dispatched is the `standing_work` choice recorded when the run was enqueued. `keep` commits nothing. `commit` commits those paths as the author's own commit, which becomes the run's base commit. `commit_and_push` commits them the same way. A merge run's choice is `keep` and it runs no standing-work step.
17. **GRD-FR-RJFC** A commit the standing-work choice makes takes the `standing_work_message` recorded with the choice. A run that carries none commits under its own name, which is the draft name its captured input holds.
18. **GRD-FR-PXVJ** A run whose choice is `commit_and_push` pushes the stream branch to the project's primary remote before its first turn starts (per `GTC-git.md` GTC-FR-YHDU). A push the remote refuses does not change the run's state, does not stop the dispatch, and is not retried.
19. **GRD-FR-KDWA** The run records what the standing-work step did: the commit it made or none, and, where a push was asked for, whether the remote took it and the typed reason where it did not. The record is written before the first turn starts.
20. **GRD-FR-ARLT** A review that answers `ready` commits the work onto the target's branch and moves the run to `completed`. A merge run commits nothing at its review: its counterpart is the apply onto the base branch (GRD-FR-AQNW). The commit holds exactly the paths that differ from the base commit; in a stream, a path created after dispatch by anything but the turn is not swept into it. Work whose tree equals the branch head's tree creates no commit.
20. **GRD-FR-WQTN** Where the branch head already holds the reviewed tree and that head is the run's own abandoned-turn commit, a `ready` verdict rewrites that commit's message to the graduation message. The run records the rewritten revision in place of the abandoned one and moves to `completed`.
21. **GRD-FR-SWOJ** A turn that stopped without a review — cancelled, timed out, or ended by a failure — has whatever it wrote committed as an **abandoned turn**, marked as one in the commit message. A tree equal to the branch head's creates no commit. The run rests `interrupted` and the stream is left usable by the run behind it. A merge run commits no abandoned turn: what its stopped turn wrote stays in its merge worktree, and the run rests `interrupted`.
    - *Why:* A shared stream cannot be discarded to clean up after a run, so the alternative to committing is a working copy the next run silently builds on.
21. **GRD-FR-PUXO** An interrupted run records one **interruption reason**: `author_pause`, `application_shutdown`, `project_changed`, `execution_abandoned`, `log_persistence_failed`, `execution_timeout`, `agent_exited`, `agent_terminated`, `unreadable_answer`, `launch_failed`, or `retryable_failure`. It also records when the run stopped and a short author-facing detail. `retryable_failure` names a stop whose cause was not recorded.
22. **GRD-FR-BLCR** `revert_graduation_run(run_id)` reverts the commits one run made, as new commits on the target's branch, and rewrites no history. It is refused with `run_state_not_permitted` for a queued run and while the run holds its stream, and with `run_not_revertable` where the run made none and for every merge run. Reverting an interrupted or awaiting-author run moves it to `discarded`.
23. **GRD-FR-EWTN** `discard_graduation_run(run_id)` is accepted from every state but `completed` and `discarded`. It cancels any turn in flight, moves the run to `discarded`, releases the stream, and unlocks the draft. The cancelled turn commits nothing, and its work stays uncommitted in the working copy. It reverts no commit and removes no branch or working copy of a stream or of the author. A merge run that is discarded writes no branch and no live worktree, and reclaims what the run itself owns (GRD-FR-KZPT).
    - *Why:* A run nobody can discard is a captured prompt nobody can run again, and a failure is the case that most needs a second attempt.
24. **GRD-FR-ZAMI** `restart_graduation_run(run_id, stream_id, standing_work, standing_work_message)` is accepted from `discarded` alone and, for a run that is not a merge run, creates a **new run** over the discarded run's captured input, with its own id, place, checkpoint and observability. It records `restarted_from_run_id`, and the discarded run is not reopened. A discarded direct run restarts as a direct run (GRD-FR-PNMU). A merge run has no captured draft input and is refused with `run_state_not_permitted` (GRD-FR-YTCR).
25. **GRD-FR-XVUD** On the first read of a project's queue after launch, a run the store holds in `working` or `reviewing` with no loop behind it moves to `interrupted` with the reason `execution_abandoned`, and the stream it held is released (per `WKS-work-streams.md` WKS-FR-LWEI).
26. **GRD-FR-GMTX** A run reaching a terminal state reclaims **nothing of the stream**: no branch is deleted, no working copy is removed, and no commit is undone. A stream outlives every run that ran in it and is removed only by its own deletion. A merge run reclaims what it owns itself, which is neither the stream nor a branch of the author (GRD-FR-KZPT).
27. **GRD-FR-NHRY** Every agent or model operation a run that has a source draft performs is counted against that draft. A merge run has no source draft and writes no statistics line. When one finishes, this module appends one `agent_operation` line and one `token_usage` line for each provider-reported usage record (per `DSS-draft-statistics-storage.md` DSS-FR-SVJU). Nothing is estimated.
28. **GRD-FR-KRTU** The interval recorded is the operation's actual execution and never anything else. Time a run spends queued, waiting for the author, blocked, or interrupted is not agent time and is recorded in no line.
29. **GRD-FR-OSCG** A run's two persisted log streams live in the run's own directory — `short_data_dir()/g/<run-id>/logs/source.jsonl` and `.../logs/structured.jsonl` — and are retained for the lifetime of the run record (per `GRS-graduation-run-log-storage.md`).
30. **GRD-FR-IKVE** A required log stream that cannot be written stops the run before anything else happens. The run rests `interrupted` carrying the reason `log_persistence_failed`, and `continue_graduation_run` retries the pending writes first, before it attempts anything else.
31. **GRD-FR-EFAU** `"graduation queue changed"` carrying `{ project_key, stream_id, worktree_path }` follows every durable change to a queue. `stream_id` is empty for the queue of an ordinary worktree, and `worktree_path` is empty for a stream run. `"graduation run changed"` carrying `{ run_id, state }` follows every state change. Nothing is emitted before the change is durable.
32. **GRD-FR-TWMA** A project change and an application shutdown stop every running turn. The interruption each records carries `project_changed` or `application_shutdown` after the stopped turn returns. **A change of active worktree stops nothing**: a run works in its target's working copy. A dispatched direct run also prevents the change (`WTC-worktree-context.md` WTC-FR-FBJQ).
33. **GRD-FR-KVNZ** Every behaviour of this module is exercisable on a machine with no container runtime, no agentic CLI, no provider credential and no network. The state machine, the queues, the ordering rules, the refusals and the record are reached through the dispatch seam of `GXD-graduation-execution.md` alone.
34. **GRD-FR-WKMD** A run rests `awaiting_author` on an escalation, on a review that did not settle, on a spent pass budget (per `../ai/GRL-graduation-loop.md` GRL-FR-XBUE), or on a blocker that repeated (per `../ai/GRL-graduation-loop.md` GRL-FR-QZFB). A run resting on a blocker or a spent budget keeps its reason, holds no escalation, and releases its stream.

35. **GRD-FR-BSNI** A direct run records a **direct target**: the absolute path of the worktree, the branch it held when the author confirmed, and the worktree's name. It is written when the run is enqueued, never changed, and kept through queueing, pauses, interruptions, restarts and relaunch. A stream run carries none.
36. **GRD-FR-ZVNO** A direct run on an ordinary worktree belongs to that worktree's **durable queue**. A direct run on a stream's working copy belongs to that stream's queue, so stream runs and direct runs of one working copy share one order and one lock. Runs that share a working copy never work at the same time.
37. **GRD-FR-KKKN** A run holds one **project slot** while it holds its stream or its pinned worktree (per GRD-FR-BNTC). Stream runs and direct runs share the slots of one project-wide limit (per `PSS-project-settings-storage.md` PSS-FR-JRWC). A claim past the limit leaves the run queued. The limit is the only cap on the number of slots.
38. **GRD-FR-XRDY** A direct run is dispatched only while its pinned worktree stands on its pinned branch. A run for which that is false stays `queued`, records a **target hold** naming both branches, and is dispatched when the branch is restored. It is never redirected to another worktree or branch. `continue_graduation_run` against it offers it a dispatch at once.
39. **GRD-FR-RFRU** A direct run records its base commit at first dispatch as the pinned branch's head and commits no standing work. Its commit at review holds every path that then differs from the base commit, including edits the author made in the worktree after confirmation.
40. **GRD-FR-VAUE** Every commit a direct run makes is made in the pinned worktree and only while that worktree stands on the pinned branch. Otherwise the run commits nothing and rests `blocked` with `direct_target_changed`. A commit cannot land on another worktree or branch.
41. **GRD-FR-PNMU** A restart of a discarded direct run creates a direct run on the discarded run's pinned worktree and branch. It takes no stream and no standing-work choice, and a later change of the active worktree does not change its target.
42. **GRD-FR-OYPY** A direct run is **dispatched** from its first dispatch until it is terminal. A direct run that holds its claim is dispatched from the claim, even before its run record is saved as dispatched. The run order index names each run's worktree, whether it is a dispatched direct run, and whether it is a merge run, so a guard learns of one, and a stream listing finds the stream's merge run, without opening a run record.
43. **GRD-FR-NYSH** When the project has a free slot, the scheduler starts the **oldest eligible queued run in the project's run order**, taken across every queue. It skips a run that is not eligible (GRD-FR-EGWS, GRD-FR-XRDY) and a run whose own queue is occupied. A skipped run blocks no run of another queue.
44. **GRD-FR-IJKV** A limit lowered below the number of held slots stops no run and is refused by no operation. The scheduler starts no run until the held slots are fewer than the limit. A limit raised offers every queue a dispatch at once.
45. **GRD-FR-KPET** Under `unlimited` the scheduler starts every eligible run whose own queue is free. The rule of one working run for each queue (GRD-FR-BNTC) still holds, so two runs never write to one stream or one worktree at the same time.
46. **GRD-FR-GRHC** `get_graduation_capacity()` reports the limit, the number of held slots, and the ids of the queued runs that wait for a slot alone. A run waits for a slot alone when it is the earliest eligible member of its queue, no run holds that queue, and the limit is full. The report holds no run that waits for its own queue.
47. **GRD-FR-ZHNV** A run that is executing is reported through `PRG-progress-reporting.md` as one in-flight operation of kind `graduation`. The operation's activation destination is `{ type: "graduation_run", runId }` with the run's id (per PRG-FR-KXQW). Its label is supplied by this module and names the run's title, the draft's name for a draft run and `Merge <stream>` for a merge run.
    - A run is executing from the moment it enters `working` or `reviewing`, and the operation is registered at that moment. A run that enters `working` or `reviewing` while it has an operation registers no second one.
    - The operation ends when the run enters any other state, `queued`, `awaiting_author`, `blocked`, `interrupted`, `completed`, `discarded`, or `failed` among them, and it ends as `finished` when the run enters `completed`, as `failed` when the run enters `failed`, and as `cancelled` in every other state the run leaves execution for.
    - A run in `queued` is waiting and a run in `awaiting_author` or `blocked` is waiting on the author or on a condition, so none of the three is reported as work in flight. The loop ending for any reason ends the run's operation, so no run's operation outlives its loop.
    - The operation is not scoped to a content root, because a run works in its target's working copy and a change of the active worktree stops nothing (GRD-FR-TWMA).
    - *Why:* The author watches concurrent runs in the status bar and follows one to the Runs panel, and a status bar that said a waiting run was working would report a machine as busy while nothing ran.
47. **GRD-FR-MRNQ** A run is a **merge run** when its record carries `merge` data (`GraduationMergeData`). A merge run is the only durable record of the merge of a stream whose Git merge has conflicts. It is enqueued in state `queued` by the stream merge handoff of `WKS-work-streams.md` WKS-FR-GKPX. `start_graduation` and `start_direct_graduation` create no merge run, and no other operation creates one. A run that carries no `merge` data is not a merge run.
48. **GRD-FR-VCTH** A merge run has **no draft**. Its title is `Merge <stream name>`, recorded as `merge.name`, and it is never a draft name. Its `input.draft_id` and `input.draft_name` are empty, its `input.prompt` is one sentence that states the merge, and its `input.prompt_checksum` is the sha256 of that sentence. It takes no draft lock (GRD-FR-DXWL), its standing-work choice is `keep` (GRD-FR-HQPD), it has no direct target, it writes no statistics line (GRD-FR-NHRY), and it makes no draft report `graduated`.
49. **GRD-FR-DLWB** A merge run belongs to the queue of the stream it merges, and its stream runs, direct runs and merge runs share that queue's order and its one lock. It is dispatched, claims the stream and holds one project slot (GRD-FR-KKKN) on the terms of every run: it holds them while it is `working`, `reviewing` or `blocked`, and holds nothing in every other state. Pause, auto-start, reorder, archive, discard, Continue and the answering of an escalation apply to it as they apply to every run.
50. **GRD-FR-KZPT** A merge run pins, when it is enqueued, the **tips** of the stream branch and of the base branch (`streamTip`, `baseTip`), the merge base (`mergeBase`), and a **merge snapshot**: a Git commit whose tree is the Git merge of the two tips with diff3 conflict markers written into every path Git could not merge, and whose two parents are the pinned base tip and the pinned stream tip. The application keeps the snapshot reachable under the private ref `refs/synthesis/merge/<run-id>`. The run owns a **merge worktree** at `short_data_dir()/g/<run-id>/mw/`, registered under the name `<run-id>-mw`, on the scratch branch `synthesis/merge-run/<run-id>`, seeded from the snapshot. Work turns run in the merge worktree. Building the snapshot and creating the merge worktree move neither branch and write neither live worktree. The run's base commit (GRD-FR-YBUM) is the snapshot commit. When the run reaches a terminal state, the merge worktree, its registration, its scratch branch and the private ref are removed.
51. **GRD-FR-XHSE** Before every dispatch of a turn, a merge run checks that the stream branch and the base branch still stand at their pinned tips. A moved tip ends the run `failed` with the typed `merge_branch_moved`, and writes no branch and no worktree. `continue_graduation_run` and `answer_graduation_escalation` against a merge run make the same check first and, where a tip moved, are refused with `merge_branch_moved`, change nothing and write no branch and no worktree.
52. **GRD-FR-AQNW** A merge run that receives a `ready` review **applies** the merge before it completes. The apply takes the repository update guard (per `WKS-work-streams.md` WKS-FR-HLGN), verifies both pinned tips, derives the merge result tree from the merge worktree by the one derivation the review stood in (per `../ai/GRL-graduation-loop.md` GRL-FR-XNQU), checks that no conflict marker stands in a path Git could not merge (per `../ai/GRL-graduation-loop.md` GRL-FR-NDHW), verifies that the stream's working copy and the base branch's worktree are clean, and applies the tree on the recorded publication: `uncommitted` leaves the result unstaged in the base worktree, and `commit` makes one merge commit with the recorded message, whose parents are the two pinned tips. The run then moves to `completed` and records `merge.result` (`GraduationMergeResult`). A merge run is `completed` only when its merge is applied on the base branch on the recorded publication. A merge commit is recorded in the run's `commits`. A tip that moved before the apply ends the run `failed` with `merge_branch_moved` and writes nothing. The run records `merge.result` before it moves to `completed`, and a run found with a recorded result that is not `completed` is completed by its next dispatch and is not applied again.
53. **GRD-FR-JSBE** An apply that cannot proceed leaves the run `blocked`, holding its stream and its slot. The blocker code is `merge_dirty_side` where a side is not clean, `merge_guard_held` where the repository update guard is held, and `merge_apply_failed` where the application of the tree fails. The checkpoint records the resume phase `apply` (per `GXD-graduation-execution.md` GXD-FR-GMDI), and a resume at that phase dispatches no turn and spends no pass. `continue_graduation_run` retries the apply. A blocked apply leaves no partial result on the base branch or in the base worktree.
54. **GRD-FR-YTCR** `restart_graduation_run` against a merge run is refused with `run_state_not_permitted`, and `revert_graduation_run` against a merge run is refused with `run_not_revertable`. A merge run holds no draft and no standing work, so neither operation has anything to act on. Discard, cancellation by pause, failure, and a review that is not `ready` leave the stream branch, the base branch, the stream's working copy and the base worktree unchanged.
55. **GRD-FR-PFMD** A merge run exists only from the moment the stream merge handoff makes it durable. A restart of the application before that moment leaves no run and no record, and the author starts the merge again. After that moment, a merge run the store holds in `working` or `reviewing` with no loop behind it moves to `interrupted` with the reason `execution_abandoned` on the first queue read (GRD-FR-XVUD), and Continue resumes it. On that same first queue read, every merge run that has ended has its merge worktree, its registration, its scratch branch and its private ref removed, and a removal that fails is logged and does not stop the read.
56. **GRD-FR-BHCS** The title of a merge run, `Merge <stream name>`, is the name every listing of runs carries for it. `list_graduation_queue` and `get_graduation_run` answer for a merge run with its `merge` data, its pass history, its findings and its logs, as for every run (GRD-FR-LGDV). A merge run's `merge.result` is present from the moment it is `completed` and absent before.

## Contract surface

### Storage
Everything one run owns stands under `FSA-filesystem-access.md`'s `short_data_dir()`:

```
short_data_dir()/q/<project-key>.toml      the project's one run order
short_data_dir()/g/<run-id>/run.toml       the run record
short_data_dir()/g/<run-id>/logs/          source.jsonl, structured.jsonl
short_data_dir()/g/<run-id>/rv/            the review turn's throwaway checkout
short_data_dir()/g/<run-id>/mw/            a merge run's merge worktree (GRD-FR-KZPT)
short_data_dir()/w/<stream-id>/            the stream's working copy (WKS)
```

A direct run's working copy is the author's own worktree and stands wherever that worktree does.

A merge run also owns, in the repository, the scratch branch `synthesis/merge-run/<run-id>`, the worktree registration `<run-id>-mw`, and the private ref `refs/synthesis/merge/<run-id>` that keeps its merge snapshot reachable. Nothing of the three outlives the run (GRD-FR-KZPT).

`<project-key>` is the project key of `GSS-global-settings-storage.md` GSS-FR-18, so every worktree of one repository shares one run order. Nothing of this store is written into a project's `.synthesis/`, and none of it is committed.

### Tauri commands
- `start_graduation(draft_id, stream_id, standing_work, standing_work_message)` → `GraduationRun`
- `start_direct_graduation(draft_id, expected_worktree, expected_branch)` → `GraduationRun` (per `GSU-graduation-start.md`)
- `preflight_direct_graduation()` → `DirectGraduationPreflight` (per `GSU-graduation-start.md`)
- `list_graduation_queue()` → `GraduationQueue`
- `get_graduation_run(run_id)` → `GraduationRun`
- `get_graduation_capacity()` → `GraduationCapacity`
- `get_draft_graduation(draft_id)` → `GraduationRun?`
- `continue_graduation_run(run_id)` → `GraduationRun`
- `pause_graduation_run(run_id)` → `GraduationRun`
- `set_graduation_auto_start(run_id, enabled)` → `GraduationRun`
- `reorder_graduation_run(run_id, expected_position, new_position)` → `GraduationQueue`
- `answer_graduation_escalation(run_id, answers)` → `GraduationRun`
- `revert_graduation_run(run_id)` → `GraduationRun`
- `discard_graduation_run(run_id)` → `GraduationRun`
- `restart_graduation_run(run_id, stream_id, standing_work, standing_work_message)` → `GraduationRun`
- `archive_graduation_run(run_id)` / `unarchive_graduation_run(run_id)` → `GraduationRun`

### Events (Tauri event bus)
- `"graduation queue changed"` — `{ project_key, stream_id, worktree_path }`
- `"graduation run changed"` — `{ run_id, state }`

### Record shapes
```
GraduationRunState =
    "queued"            // waiting for its stream, or for a project slot
  | "working"           // a work turn holds the stream
  | "reviewing"         // a review turn is judging what the work turn wrote
  | "awaiting_author"   // an escalation, or a review that did not settle
  | "blocked"           // a condition the author clears
  | "interrupted"       // stopped; GraduationInterruption says why
  | "completed"         // terminal: the work is committed on the stream, or a merge run's merge is applied
  | "discarded"         // terminal
  | "failed"            // terminal

CapturedGraduationInput {
  draft_id, draft_name, prompt, prompt_checksum, captured_at
}

StandingWork =
    "keep"              // the run works on top of what stands in the stream
  | "commit"            // commit it first, under a message naming the author
  | "commit_and_push"   // commit it first, then push the stream branch

StandingWorkOutcome {
  commit?,                       // the revision the standing work was committed as
  pushed?,                       // null where the choice asked for no push
  push_failure?                  // { code, message }; the run is not stopped by it
}

DirectTarget {
  worktree_path,                 // GRD-FR-BSNI; absolute
  worktree_name,                 // the directory's basename
  branch                         // the branch pinned at confirmation
}

TargetHold {
  code,                          // "target_branch_changed"
  expected_branch,
  actual_branch?                 // absent where the worktree is detached or gone
}

GraduationInterruptionReason =   // GRD-FR-PUXO
    "author_pause" | "application_shutdown" | "project_changed"
  | "execution_abandoned" | "log_persistence_failed"
  | "execution_timeout" | "agent_exited" | "agent_terminated"
  | "unreadable_answer" | "launch_failed"
  | "retryable_failure"          // the cause was not recorded

GraduationInterruption {
  reason,                        // GraduationInterruptionReason
  at,                            // RFC 3339 UTC
  detail,                        // GXD-FR-WJOW; author-facing, no file content
  stream_released,
  resume_requested_at?
}

GraduationRun {
  id,
  stream_id, stream_name,        // GRD-FR-PZAK; empty for a direct run on an ordinary worktree
  direct_target?,                // DirectTarget; present on a direct run alone
  target_hold?,                  // TargetHold; GRD-FR-XRDY
  state,
  input,                         // CapturedGraduationInput
  standing_work,                 // GRD-FR-HQPD; chosen when the run was enqueued
  merge?,                        // GraduationMergeData; present on a merge run alone (GRD-FR-MRNQ)
  standing_work_message?,        // GRD-FR-RJFC; null takes the run's own name
  standing_work_outcome?,        // GRD-FR-KDWA; written at the first dispatch
  base_commit,                   // GRD-FR-YBUM; null before the first dispatch; the snapshot of a merge run
  commits,                       // the revisions this run created, in order
  auto_start,
  archived, archived_at,
  work_turns, review_turns,      // spent against the bounds of GRL
  checkpoint,                    // GraduationCheckpoint
  observability,                 // per GOB-graduation-observability.md
  escalation?,                   // the questions waiting on the author
  blocker?,                      // what blocks it and the act that clears it
  interruption?,                 // GraduationInterruption; why it stopped
  restarted_from_run_id?,
  failure?                       // { code, message, retryable }
}

GraduationMergeData {
  name,                          // "Merge <stream name>"; the run's title (GRD-FR-VCTH)
  streamBranch,
  baseBranch,
  baseTip,                       // the pinned full object id of the base branch (GRD-FR-KZPT)
  streamTip,                     // the pinned full object id of the stream branch
  mergeBase,                     // the full object id both branches last shared
  snapshotCommit,                // the merge snapshot commit id
  publication,                   // { kind: "uncommitted" } | { kind: "commit", message }
  changedPaths,                  // project-relative: every path the Git merge changes against baseTip,
                                 //   Git-clean paths and unresolved paths included
  unresolvedPaths,               // project-relative: the paths Git could not merge
  conflicts,                     // [{ path, baseChange, streamChange }], each change one of
                                 //   created | updated | deleted | unchanged
  result?                        // GraduationMergeResult; written when the merge is applied (GRD-FR-AQNW)
}

GraduationMergeResult {
  published,                     // "uncommitted" | "commit"
  commit?,                       // the merge commit id where the publication committed
  mergedPaths                    // project-relative
}

QueueIndexEntry {
  run_id, stream_id, draft_id, state,
  worktree_path,                 // GRD-FR-OYPY
  dispatched_direct,             // GRD-FR-OYPY
  merge                          // true on a merge run (GRD-FR-OYPY)
}

GraduationQueue {
  project_key,
  runs                           // every run, in the project's run order
}

GraduationCapacity {
  limit,                         // integer of 1 or more, or "unlimited" (PSS-FR-JRWC)
  in_use,                        // slots held now (GRD-FR-KKKN)
  waiting_for_slot               // ids of queued runs that wait for a slot alone (GRD-FR-GRHC)
}
```

The fields of `GraduationMergeData` and `GraduationMergeResult` are camelCase on the wire and in `run.toml`.

A merge run's failure code is `merge_branch_moved` (GRD-FR-XHSE). Its blocker codes are `merge_dirty_side`, `merge_guard_held` and `merge_apply_failed` (GRD-FR-JSBE).

Typed errors: `merge_branch_moved`, `direct_target_changed`, `run_state_not_permitted`, `queue_position_stale`, `queue_position_out_of_range`, `run_archived`, `run_not_revertable`, `unknown_run`, `draft_locked_by_graduation`, `draft_not_single_file`, `stream_busy`, `unknown_stream`.

### Internal (Rust API, not registered as Tauri commands)
- `dispatch_graduation_turn` — the single seam to an execution agent (`GXD-graduation-execution.md` GXD-FR-DRFF).
- The run's checkpoint read and write, and the stream claim and release of `WKS-work-streams.md`.

## Non-functional requirements
- Reading the queue reads a summary index rather than every run record, so a draft write that checks the lock opens no run file.
- The queue and its runs are recoverable after an application restart, which is what makes an interrupted run something to continue rather than something to notice missing.
- No log record and no event carries a credential, a token, or a byte of a project file.
