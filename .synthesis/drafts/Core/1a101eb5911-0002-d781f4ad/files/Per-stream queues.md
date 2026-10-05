## Intent

Let an author control how many graduation runs can work across a project at the same time. The limit is project-wide and applies to stream runs and direct runs together. Each stream or shared worktree still runs one queue member at a time. When the project has no free slot, eligible runs stay queued until a slot opens.

## User journey

### Configure concurrency

- The author opens Project settings and selects the **Graduation** section.
- The section loads the current project-wide concurrency limit. The default is **1**.
- The author selects **1**, **2**, **4**, **8**, or **Unlimited** from a drop-down. If the stored limit is another positive integer, such as **3**, show that value as an option and keep it selected until the author chooses a different value.
- A change marks the Graduation section as pending. Save it with the other pending Project settings changes; do not save it immediately. Show loading and save-failure states, and keep the author's selection if a save fails.

### Start and queue graduation runs

- The author starts a graduation and selects a stream, as in the existing start dialog.
- A stream or worktree still runs its own queue one run at a time. Different queues can work at the same time up to the project-wide limit.
- If fewer runs hold project slots than the configured limit, an eligible run can start when its own queue is free. If the limit is already full, the run stays queued in its own queue until a project slot opens.
- When the author must wait because the selected stream is occupied or because the project-wide limit is full, the start dialog asks for the standing-work choice. It states when the wait is due to the project limit rather than to an occupied stream.
- When a slot opens, start the oldest eligible queued run in project order, while preserving each queue's order and skipping runs that are not eligible.
- If the author lowers the limit below current usage, do not stop current runs or refuse the setting. Keep them running and start no new run until usage is below the new limit.
- **Unlimited** removes the project-wide slot cap; the one-run-at-a-time rule for each stream or shared worktree still applies.

## Requirements

- Existing logic of queues inside a single stream stays the same: graduations run one by one inside a single stream.## Requirements

- Replace the existing stream concurrency limit with one project-wide limit. Count stream runs and direct runs together. Do not add a separate limit for each stream or for direct runs.
- Keep one-run-at-a-time queue behavior for each stream and each direct-run worktree queue. Different queues may work in parallel up to the project limit.
- Provide the Graduation section in Project settings with a drop-down for **1**, **2**, **4**, **8**, and **Unlimited**. Default to **1**. Preserve any stored positive integer not in that list as a selected option until the author changes it; do not normalize it to another value just because it is not a listed choice.
- Store **Unlimited** as a named value, not as an integer sentinel. Keep integer limits positive, and retain the existing repair of malformed or below-one stored integer values to **1**.
- Keep the concurrency setting in the project's shared configuration. Mark the Graduation section dirty when its value changes, save it with Project settings' pending changes, and include it in the existing save-before-close behavior. Do not save it immediately. Retain the selected value and show the save error if the save fails.
- Dispatch a queued run only if its own queue permits it and the project has a free slot. Across queues, choose the oldest eligible queued run in project order. Skip queued runs that are not eligible; do not let them block eligible runs in other queues.
- When usage is below a finite limit, dispatch eligible work up to that limit. When a limit is lowered below current usage, allow current runs to continue and pause new dispatch until usage is below the new limit. Under **Unlimited**, dispatch eligible work without a project-wide cap.
- Keep the standing-work choice for stream runs. Ask for it whenever a run will wait, whether the selected stream is occupied or only the project-wide limit is full. State the wait reason clearly in the start dialog and in the run status where needed.
- Keep the existing per-stream queue order, worktree locks, and direct-run target rules. A project-wide slot limit must not allow two runs to write to the same stream or worktree at once.
- Add or update frontend and backend tests for the settings default and choices, preservation of a stored value such as **3**, pending-save and save-failure behavior, named **Unlimited** persistence, shared counting of stream and direct runs, dispatch order across queues, waiting when the limit is full, lowering the limit below current usage, and the unchanged one-run-per-queue rule.

## Specifications to update

Update these specifications together so the setting, storage, scheduler, start dialog, and run status use one contract:

- `specifications/ui/SET-project-settings.md` — add the Graduation section and its selector, pending-save behavior, loading and save-failure states.
- `specifications/core/PSS-project-settings-storage.md` — store the project-wide stream-and-direct-run limit, its supported values and named Unlimited value, and the project-settings load/save contract.
- `specifications/core/GRD-graduation.md` — define shared project slot use, eligibility across queues, project-order dispatch, limit changes below current usage, and Unlimited behavior.
- `specifications/core/WKS-work-streams.md` — keep one run per stream queue and replace the per-stream concurrency cap with the shared project-wide cap.
- `specifications/ui/GSD-graduation-start-dialog.md` — explain waiting due to a full project limit and ask for the standing-work choice whenever a stream run must wait for either reason.
- `specifications/ui/GRU-graduation-runs.md` — show when an eligible queued run waits for a project slot, distinct from waiting for its own queue.
