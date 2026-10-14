## Intent

Every graduation run has a certain number of passes the agents are allowed to run.

We need to make this configurable as sliders, in project-level settings in Graduation tab. The author also gets two graduation-only time limits: one for each working or review turn, and one for the whole run. The authors of projects use these settings to give long runs more room, or to stop a run that costs too much.

Graduation stops reading the shared execution timeout and the shared retry budget. Conversation turns and conversational loops keep both shared settings and are not changed in any way.

## Features

- User must be able to configure the number of passes budget, with 1 being min, 2 being default and 10 being max, in steps of 1. It applies to runs that are not merge runs.
- User must be able to configure the amount of time per single stage run (working/review) with 30m being min, 3h being default, 48h being max, in fixed steps of 30 minutes. This is a new graduation-only **stage time limit**. It applies to every graduation turn, merge run turns and the semantic rebase turn included.
- User must be able to configure the amount of total time for the whole graduation run with 30m being min, 5h being default, 96h being max, in fixed steps of 30 minutes. This **total time limit** counts only the time spent in working and reviewing turns. Time that the run rests in `awaiting_author`, `blocked` or `interrupted` is not counted. It applies to merge runs too.
- The three sliders are in the Graduation section of the project settings. Each slider shows its current value as text (for example "3 h 30 min") and has a **Reset to default** control. A slider with no stored value shows the default.

## Requirements

- Sliders must be visually consistent with what we already have in the project, i.e. Isolation resource sliders.
- Number of passes must be a positive integer number above zero. Everything else must be rejected. Each setting is stored only when the author moves its slider. **Reset to default** clears it to the unset state (per PSS-FR-ZLCF). A stored value outside its bounds, or one that is not an integer, is repaired to unset on read. A save with such a value is refused.
- Every slider change saves at once, like the Isolation sliders: no dirty state, no part in the save-before-close sweep. Quick changes in sequence must all be kept. A refused save keeps the previous value in the control and shows the refusal inline. The sliders must be keyboard operable and expose their value as text to assistive technology.
- When the stage time limit shown is higher than the total time limit shown, the section allows it and shows a warning that the total limit ends the run first.
- Graduation reads the three settings before every dispatch, as it does today (GRL-FR-KWNP). An unset setting uses the new default (3 h, 5 h, 2 passes). The old shared execution timeout and shared retry budget values that a project may already store are ignored for graduation. The shared settings stay stored and valid for conversation turns.
- Each turn's effective timeout is the smaller of the stage time limit and the remaining total time (total limit minus the working and reviewing time the run has used). The used time is stored with the run, so it survives an application restart.
- When a turn reaches the stage time limit, the run rests `interrupted` with `execution_timeout`, as today. When the remaining total time ends first, or is already zero before a dispatch (for example after the author lowers the limit), the turn is cancelled at once and the run rests `awaiting_author` with a new reason, `total_time_limit`. It holds its queue and project slot and keeps the checkpoint. What the turn wrote is committed as for any cancelled turn.
- A run at rest with `total_time_limit` behaves like one with `pass_budget_exhausted`. **Continue** resets the used total time alone and resumes without repeating a completed pass. Continue after `pass_budget_exhausted` resets the pass budget alone and does not reset the used time.
- The Runs surface shows the `total_time_limit` reason, the limit and the time used, and offers a **Change the time limit** control to the Graduation section of the project settings, like the one for `execution_timeout`. That control for `execution_timeout` now leads to the stage time limit slider.
- Merge runs: the stage and total time limits apply. The pass budget stays the constant of 2 (GRB-FR-TGAZ), and the pass slider does not change it.
- The existing **Execution time limit** row of the Graduation section stays, but its helper line must say that it now bounds conversation turns only, not graduation turns.
- Conversational loops should NOT be affected by this change, the shared execution timeout and retry budget included, and must work "as is” today.

## Specification impact

Update these specifications. Where text conflicts with this prompt, change that text.

- `specifications/core/PSS-project-settings-storage.md`: three new graduation-only project settings with their bounds and unset rules; PSS-FR-TQMV and PSS-FR-WPKS no longer govern graduation.
- `specifications/ai/GRL-graduation-loop.md`: GRL-FR-REPL, GRL-FR-KWNP and GRL-FR-XBUE read the new settings; add the total time limit and the `total_time_limit` rest.
- `specifications/core/GRD-graduation.md`: add the used total time to the run record, and the new reason and its Continue reset in GRD-FR-CYIB.
- `specifications/core/GXD-graduation-execution.md`: the effective turn timeout, and the pass budget window source.
- `specifications/core/GRB-graduation-rebase.md`: merge runs and the semantic rebase turn use the stage and total limits.
- `specifications/ui/SET-project-settings.md`: the three sliders, Reset to default and the stage-over-total warning; the helper line of SET-FR-DHFS.
- `specifications/ui/GRU-graduation-runs.md`: GRU-FR-QLRQ and GRU-FR-FZCN for the stage limit and `total_time_limit`.
- `specifications/infra/GTE-graduation-end-to-end-tests.md`: GTE-FR-KAZX and a scenario for the total limit rest and its Continue.
- The End-to-end graduation tests drafts say graduation reads the shared settings. They conflict with this draft and the author must align them.