## Intent

Let an author graduate a draft either in an existing or new work stream, or directly in the worktree that is active when they confirm. Direct runs use the same agent, review, commit, and Docker execution lifecycle as stream runs, but write to the pinned worktree and branch. The author starts from the existing Graduate action and needs clear destination choices, preflight feedback, and protection against concurrent runs changing the same checkout.

## User journey

- The author finishes a draft and selects **Graduate**.
- The existing graduation start dialog opens. The author selects an existing stream, creates a stream, or selects **Work directly**.
- For **Work directly**, the dialog identifies the active worktree and branch. The author can confirm only when that worktree has a branch and is clean. A detached-HEAD worktree disables this choice.
- On confirmation, the run pins the worktree path and branch. A queued run keeps that target if the author switches worktrees before dispatch. If the pinned branch changes while queued, dispatch waits until the author restores it; it never redirects to another checkout.
- A direct run on a stream worktree joins that stream’s existing queue in normal run order. A direct run on another worktree joins that worktree’s direct-run queue. Runs that share a worktree never write at the same time. Direct runs count against the existing project concurrency limit.
- From first dispatch until the run is terminal, block worktree activation, branch checkout, and other worktree switches in the project. The author may still edit the pinned worktree while the run works. At review, include all changes then present in the run’s result commit, including author edits made after confirmation.
- If the author creates a stream in the dialog but graduation start is refused, keep the stream.
- A restart of a discarded direct run uses its original pinned worktree and branch.

## Scope and requirements

- Extend the existing graduation start dialog; do not add a separate start surface. Keep existing stream selection and stream-run behavior unchanged.
- Keep the existing **standing-work** choice for stream runs. Direct mode requires a clean worktree at confirmation and does not ask for a standing-work choice.
- Bind direct start to the active worktree and branch at confirmation. Check cleanliness only at confirmation. If the target identity or branch changed before the start is accepted, refuse without creating a run. Do not repeat the clean check at dispatch.
- Pin the target in the durable run record. Preserve it through queueing, pauses, interruptions, restarts, and application relaunch. Report the target and branch in run history and status. A run’s final commit must be guarded so it cannot land on a different active worktree or branch.
- Use the existing run review and commit lifecycle. For direct runs, define the baseline at first dispatch and commit the changes present at review, including author edits made after confirmation. Keep the existing Docker isolation and agent execution rules unchanged.
- Coordinate all writes to a worktree. Direct runs on the same ordinary worktree use one durable queue. A direct run targeting a stream worktree uses that stream’s queue, so stream and direct runs share one order and lock. Apply the project concurrency limit to dispatched direct and stream runs together.
- While a direct run is queued, allow switching away from its pinned worktree. Once it first dispatches, block worktree switches until it reaches a terminal state, including while it waits for the author or is paused. Allow file edits in the pinned worktree; include edits present at review in the result commit.
- Hide stream worktrees from the general worktree selector, but retain access through the stream selector. Keep stream branches visible in the Git panel’s existing branch list. Changes to the bottom Git panel are out of scope.
- Preserve existing stream creation and deletion cleanup behavior. Do not add a Git-panel stream-management surface.
- Add frontend and backend coverage for destination selection, detached and dirty worktrees, target pinning, stale-branch queue refusal and recovery, direct and stream queue ordering, shared concurrency, switch blocking, review commits that include author edits, restart destination, and stream-worktree visibility.

## Specifications to update

Update these specifications together so the run records, queues, preflight, and UI agree:

- `specifications/core/GSU-graduation-start.md` — direct-start inputs and preflight, clean-worktree refusal, and retaining a newly created stream when start is refused.
- `specifications/core/GRD-graduation.md` — direct-run target and provenance, durable worktree queueing and locking, direct commit scope, restart behavior, and events.
- `specifications/core/WKS-work-streams.md` — include direct runs targeting stream worktrees in the existing stream queue, busy state, and queue depth.
- `specifications/core/WTC-worktree-context.md` — enforce direct-run worktree-switch and branch-change rules through every switching route.
- `specifications/ui/GSD-graduation-start-dialog.md` — existing stream, new stream, and Work directly choices; direct-mode status, preflight, and refusal states.
- `specifications/ui/WTS-worktree-selector.md` — hide stream worktrees from its general worktree list while retaining other worktree behavior.
- `specifications/ui/GRU-graduation-runs.md` — render direct target provenance and direct queue position.
- `specifications/ui/GRT-graduation-restart.md` — restart a direct run on its pinned worktree and branch.
- `specifications/core/DRS-draft-storage.md` — ensure a successful direct graduation applies the same graduated-draft lifecycle as a successful stream run.

Reuse existing specifications for Git identity guards, stream selection, and Docker execution. Do not change the bottom Git panel’s stream-branch listing or add stream worktrees to it.