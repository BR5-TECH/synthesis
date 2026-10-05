## Intent

Give authors live access to a graduation run's persisted logs while a phase is still running. Keep each phase's records available after the run moves to another phase, so an author can inspect earlier Review passes after a review sends the run back to Working.

## User journey

- The author opens **Runs**, selects a running graduation, and activates a phase with logs.
- The log window opens for that run and phase. It shows the newest available records without waiting for the phase or turn to finish.
- While the window is open, new records appear as they are durably appended. The author can follow the end of the log or scroll back without the view forcing them to the end.
- If Review sends the run back to Working, the author can still open Review. Its pass list shows each pass that entered Review, with that pass's own records, so the author can compare earlier and later reviews. Working passes remain separate in the Working log view.
- The author can select Source or Structured logs and use the existing log-window controls. Closing the window does not change the run or its logs.

## Requirements

- Implement the live-log behavior in the existing Tauri/TypeScript/React frontend. Do not add a log surface or a run selector to the log window.
- Follow `specifications/ui/GLW-graduation-log-window.md`, `specifications/ui/GRU-graduation-runs.md`, `specifications/core/GRS-graduation-run-log-storage.md`, `specifications/core/GOB-graduation-observability.md`, and `specifications/ui/RPV-run-progress.md`. Treat the selected stage descriptor as the source of the phase scope; do not hard-code stage names or counts.
- When an eligible phase is opened, request its newest page at once. Do not wait for a phase transition, turn completion, or run-state change before showing records already persisted.
- While the window is open, handle `graduation log records appended` for its run and selected stream by reading records after its held cursor. Render records from the read result, not from the event payload. Apply new records to the selected phase and pass-or-run-level scope, in sequence order, with no duplicates. Do not require the selected phase to remain current.
- Build the phase's pass list from the persisted `stage_history` entries whose destination is the selected phase, using each entry's persisted pass (per `GOB`). Do not infer phase entries from the current phase, current pass, log text, or log presence. Keep every pass that entered the phase as its own entry, even when it has no records in the selected stream. A later pass must not replace or merge earlier records. Keep `pass = null` records in the separately labelled run-level entry defined by the log-window contract. Do not add a phase-entry-interval field or backend operation.
- Preserve the existing log-window behavior for stream selection, paging, search, follow and resume, empty and failure states, keyboard access, focus return, accessibility, and escaped plain-text rendering.
- Do not change graduation execution or log persistence. Use the existing `GOB` `stage_history` and `GRS` log indexes, read operation, and append event; do not add a backend field or operation. Update `specifications/ui/GLW-graduation-log-window.md` to replace its unsupported phase-entry-interval rule with pass derivation from persisted `stage_history`. Keep the other cited contracts unchanged unless implementation proves one cannot support this behavior; identify any precise gap before proposing a backend change. Do not change `RUN-runs.md` to use its separate agent-activity stream for graduation logs.
- Add or update frontend tests to verify: records appear while a phase is still running; an append event updates the open view before phase completion; Review remains openable after the run returns to Working; the Review pass list reflects persisted `stage_history` entries and each listed pass shows only its own records; and stale reads, duplicate records, and records from another run, phase, pass, or stream are not shown.