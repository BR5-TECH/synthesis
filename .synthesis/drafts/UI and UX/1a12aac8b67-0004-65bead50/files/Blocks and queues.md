## Intent

A graduation run that stops before it completes (out of passes, waiting for an answer, blocked, interrupted, failed) is not finished work. The next run of its queue must not start on top of that unfinished work. A queue moves on only when its run is `completed`. This applies to every queue: stream queues, queues of an ordinary worktree (direct runs), and merge runs.

## User journey

- User starts a graduation run A in stream X.
- User starts a graduation run B in stream X.
- IF: graduation run A rests in any non-terminal state other than working or reviewing (`awaiting_author` for any reason, `blocked`, `interrupted` for any reason) - run B does not start. Run A waits for the author. In the Runs panel, run B shows the existing sentence that it waits behind a run of its own stream.
- IF: run A ends `failed` - run B stays queued. Only the author's Discard of run A releases it.
- IF: the author discards run A (Discard, or Revert, which ends the run as discarded) - run B and every run queued behind A is removed from the Graduation runs. The draft of each removed run is an active draft again and shows in the active drafts, with no run-state tag, so the author sees it is not graduated. The author graduates it again when ready.
- Graduation run B starts only after graduation run A is `completed`, under the normal start rules (project slot, eligibility). 

## Requirements

- - **Queue hold.** A run holds its queue (a stream's queue, which includes direct runs on the stream's working copy, or an ordinary worktree's queue) from first dispatch until it is `completed` or `discarded`. It holds the queue in `working`, `reviewing`, `blocked`, `awaiting_author` (every reason: pass budget, unsettled review, repeated blocker, escalation) and `interrupted` (every reason, `author_pause` and `application_shutdown` included). A `failed` run also holds its queue until the author discards it. The queue dispatches no other run while a run holds it, wherever that run stands in the queue order. This holds across application relaunch.
- **Merge runs.** The same rule applies. A merge run at `pass_budget_exhausted` keeps its stream and its slot.
- **Project slot.** A run in a non-terminal state that holds its queue also holds one project slot (one rule: holds queue = holds slot). A `failed` run holds the queue and no slot. Continue, Resume and an answer to an escalation of a held run take no new claim and dispatch at once. The limit rules (lowered limit, `unlimited`) do not change.
- **Failed run.** Run A `failed`: the queue stays held and every run behind it stays queued. Restart is not offered for a `failed` run. The author discards run A to release the queue.
- **Cascade on discard.** When a run that held its queue ends `discarded` (`discard_graduation_run`, or `revert_graduation_run` on an interrupted or awaiting-author run, or Discard of a `failed` run), every run queued behind it in that queue is removed. This includes runs with auto-start off, paused runs, direct runs and merge runs. Runs ahead of it stay. Discard of a run that never held its queue removes nothing else.
- **Removal.** A removed run is deleted from the run history and from the project's run order. It does not become `discarded`, is not archived and offers no Restart. Its draft is unlocked and is an ordinary active draft. Removal and the discard of run A are one step under the store hold of GRD-FR-WSXA. The backend then emits `"graduation queue changed"`. No notification is raised.
- **Frontend.** The rail, the selected-run region and the draft tabs render from the backend after the event. A removed run leaves the rail. If the author had it selected, the region falls back to its empty state. An open tab of its draft loses the run-state tag and becomes editable. Where run A holds a queue with runs behind it, the confirmation of Discard and of Revert names the runs that will be removed and states that their drafts return to active drafts.
- **Revert.** Revert stays offered for an interrupted or awaiting-author run, which now holds its queue. Revert stays refused for a run that is doing agent work (`working`, `reviewing`).

## Specification impact

Update these specifications together:

- `specifications/core/GRD-graduation.md`: GRD-FR-BNTC (what holds a queue and a slot), GRD-FR-EGWS (a held run is not passed over), GRD-FR-KKKN, GRD-FR-MDQZ and GRD-FR-CYIB (pause and Continue keep the hold), GRD-FR-EWTN and GRD-FR-BLCR (cascade, and Revert refusal narrowed to working and reviewing), GRD-FR-LGDV and GRD-FR-VLFO (the one exception to "every run it has ever made": a removed queued run).
- `specifications/ai/GRL-graduation-loop.md`: GRL-FR-XBUE and the escalation text that says the stream is released.
- `specifications/core/GRB-graduation-rebase.md`: GRB-FR-TGAZ and GRB-FR-JQFO (no release of stream or slot; no requeue at the front).
- `specifications/core/WKS-work-streams.md`: WKS-FR-NDSV and the stream busy rules, so a held stream is busy.
- `specifications/core/DRS-draft-storage.md`: unlock and active status of a draft whose queued run is removed.
- `specifications/ui/GRU-graduation-runs.md`: GRU-FR-DVWY (Revert), the Discard and Revert confirmation, the queue-changed reload. Also the `specifications/ui/GRT-graduation-restart.md` provenance rule GRT-FR-FCUF for a run whose restart source was removed.
- `specifications/ui/GSD-graduation-start-dialog.md`: the busy statement counts a held stream as occupied.
- `specifications/infra/GTE-graduation-end-to-end-tests.md`: cases 6, 11, 24, 31 and 32 ("stream released"), plus new cases for hold, cascade and failed run.