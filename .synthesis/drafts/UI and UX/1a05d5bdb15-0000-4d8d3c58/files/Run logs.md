## Intent

User must be able to see the logs generated during graduation run in correspondence to the stage where they were generated.

## User journey

- User should be able to open the Runs panel, and see the progress bar of the run.
- Progress bar stages with available logs should be enabled. A stage is eligible when the persisted log indexes report at least one record for that stage, whether the record belongs to a pass or is a run-level record with `pass = null`. Stages without logs should be visibly unavailable and should not open an empty log window by mistake.
- The progress bar and log viewer must use the ordered stage descriptors supplied by the run observability data. The feature must not hard-code stage names, stage ids, or a fixed stage count. Adding or removing stages must not change the log-viewer behavior.
- All enabled items on the progress bar should be clickable if there are logs emitted during the stage.
- When user clicks on a progress bar stage, such as Specification authoring, a modal overlay should open for that run and stage.
- The overlay should identify the selected run and stage, and should not allow the user to switch to another run from inside the overlay.
- The overlay should be split into two columns: a narrow left column with the pass history for the selected stage, plus a clearly labelled run-level entry when the selected stage has records with `pass = null`, and a wider right column with the log for the selected pass or run-level entry.
- The newest pass with logs should be selected when the overlay opens. If the stage has no pass-scoped logs but has run-level logs, the run-level entry should be selected. If both exist, the run-level entry must remain available but must not replace the newest pass as the default selection.
- User should be able to select another pass or the run-level entry and read its logs without closing the overlay. Run-level logs must be labelled as run-level logs and must not be presented as belonging to a pass.
- User should be able to view either the raw Source log or the parsed Structured log by using the corresponding toggle. Only one stream should be visible at a time.
- User should be able to scroll through the selected log and load older content when the log is longer than the visible page.
- While the selected stage is still running, user should be able to follow new log entries in real time. Scrolling away from the end should stop following until the user resumes it.
- User should be able to search for specific text in the selected log and see the number of matches. A search with no matches should show a clear no-results state.
- When no logs exist for the selected pass or run-level entry and stream, the overlay should show a clear empty state and allow the user to switch to the other stream.
- If logs cannot be read or log persistence failed, the overlay should show the failure and explain whether the run is affected. It should not present a read failure as an empty log.
- User should be able to use the overlay with the keyboard, close it with the close control or Escape, and return focus to the stage that opened it.
- Opening logs for another run should show only that run's logs and should not reuse the previous run's selected stage, pass, stream, search, or scroll position.

## Requirements

- Review and follow the existing decisions in `specifications/ui/GLW-graduation-log-window.md`, `specifications/ui/GSR-graduation-stage-report.md`, `specifications/ui/GRU-graduation-runs.md`, and `specifications/core/GRS-graduation-run-log-storage.md`. Use **pass** as the only unit for the repeated work represented in the log viewer. Do not introduce a separate iteration concept, terminology, data model, or compatibility layer. A run-level entry for `pass = null` is not a repeated-work unit and must be labelled as run-level rather than renamed as a pass.
- Reconcile the known terminology conflict before or during implementation: `specifications/ui/GLW-graduation-log-window.md` must use `pass` consistently where it currently says `iteration`; `specifications/core/GRS-graduation-run-log-storage.md` is the authority for the `pass`-scoped read command and log indexes. Update any dependent references required by this change rather than silently translating between two concepts.
- Implement the Runs-panel integration and graduation log modal for the existing Tauri/TypeScript/React frontend. Keep the log window bound to the run and stage that opened it; do not add a run selector inside the modal.
- Treat the stage as data supplied by the run's ordered observability configuration. Do not branch on particular stage names or ids, assume four stages, or encode stage-specific log behavior in the feature. The implementation must continue to work when the configured stage list has a different number of stages, subject to the backend contracts supplying matching persisted stage ids and indexes.
- Use the persisted run data and log indexes to determine stage eligibility. A stage is eligible when any record is indexed for that stage, including records with `pass = null`. Disabled stages must explain why they cannot open and must not invoke a log read.
- Implement the two-column modal with pass selection, a labelled run-level selection for `pass = null` records, exclusive Source/Structured stream selection, newest-page loading, older-page loading, search, match counts, empty state, unavailable state, persistence-failure state, and no-match state as defined by the UI specifications.
- Implement live updates through the existing graduation log update mechanism. Follow mode must stop when the user scrolls away from the end and must resume only through the labelled resume control.
- Preserve state isolation between runs and scopes. Discard stale reads and cursors when the run, stage, pass-or-run-level scope, or stream changes.
- Implement modal focus containment, keyboard operation, Escape handling, accessible names and selected states, and focus return to the stage that opened the modal. Render log content as escaped plain text without treating model or executor output as markup or actions.
- Reuse existing application layout, theme, typography, status treatments, modal primitives, and notification/error patterns. Do not introduce a separate visual language for the log viewer.
- Keep log rendering paged and bounded. Do not load the complete history for a large log when the newest page is sufficient.
- Add or update frontend tests for stage eligibility with pass-scoped and run-level records, dynamic stage counts and ids, run and scope isolation, pass and run-level selection, stream changes, paging, live follow, search, empty and failure states, keyboard access, focus return, and stale-response handling.
- If implementation reveals a missing or contradictory product decision, identify the affected specification paths and update those specifications as part of the graduated work rather than silently inventing a new behavior. At minimum, review `specifications/ui/GLW-graduation-log-window.md` for its current iteration terminology and fixed assumptions, and review `specifications/ui/GRU-graduation-runs.md`, `specifications/core/GOB-graduation-observability.md`, and `specifications/core/GRS-graduation-run-log-storage.md` if the configured stage list is no longer fixed to four stages.
- Do not change how graduation runs execute or how logs are persisted unless an existing backend contract is proven insufficient for the specified UI behavior. If such a gap exists, document the required contract change and its impact on the related core specifications.