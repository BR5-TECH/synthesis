## Intent

Repair the graduation log window so an author can see the same live agent-activity rows shown in the **Agent Output** tab while a graduation phase is running, then inspect the durable activity from earlier passes. Show what the agent is doing without exposing task input, internal reasoning, raw CLI output, or full tool payloads.

## User journey

- The author opens **Runs**, selects a graduation run, and activates a phase on its progress bar.
- The log window opens for that run and phase. It immediately shows the newest persisted agent-activity records, using the Agent Output row presentation: local time, activity kind, and one-line summary.
- While the phase runs, newly persisted activity appears without waiting for a turn or phase to end. Follow mode keeps the newest record in view; scrolling back stops follow until the author resumes it.
- The author selects an earlier pass to inspect its own records. A Review phase remains available after the run returns to Working, and its passes remain separate.
- The author closes the window without changing the run or its records.

## Scope

- Replace the window's current Source/Structured log presentation with one persisted **Agent Activity** stream. Do not show raw CLI output or graduation observability records in this window. Keep graduation observability available to the run-progress UI for run state and phase history.
- Use the executor's normalized CLI activity for graduation turns. This is not a CVL conversation transcript and does not add conversational API turns to graduation.
- The durable activity view includes agent messages, tool actions and results, progress, diagnostics, and errors. Exclude task input, invocation contents, internal reasoning, and full tool arguments, results, or raw protocol payloads. Apply EAC's credential and session-identity masking before persistence. Do not persist or render excluded content in this stream.
- Keep the existing log window bound to the run and phase that opened it; do not add a run selector or a separate log surface.

## Requirements

- Implement the feature in the existing Tauri/TypeScript/React frontend and backend. Use the project's existing modal, theme, keyboard, focus, accessibility, and error-state patterns.
- Make the graduation log window's activity rows match the Agent Output tab's row presentation and meaning: show the activity timestamp in local time, kind, and bounded one-line summary. Do not render raw payloads or add an expansion that reveals excluded content. Keep the existing Agent Output tab's behavior for non-graduation activity unchanged.
- Persist safe normalized activity durably for each graduation run, attributed to its run, phase, and pass (or run-level scope). Records must remain readable after a phase ends and after application relaunch. Do not use the bounded, session-only AGV cache as the durable source.
- Deliver each record to durable storage as it is produced, with mandatory acknowledgement and bounded backpressure; do not silently drop records. A persistence failure must use the existing typed interruption path and must never appear as an empty log.
- Open an eligible phase on its newest page without waiting for a phase transition, turn completion, or run-state change. While the window is open, handle the log-append event for its run by reading after the held cursor. Render read results, not event payloads; keep sequence order and prevent duplicates.
- Derive the phase's pass list from persisted `stage_history` entries whose destination is the selected phase, using their recorded pass. Keep every pass that entered the phase listed, even when it has no records. Keep each pass's records separate. Show `pass = null` records in a separately labelled run-level entry.
- Preserve bounded paging, search over only displayed safe fields, live follow and resume behavior, loading, empty, unavailable, persistence-failure and no-match states, stale-response rejection, run/phase/pass isolation, keyboard operation, focus containment and return, and escaped plain-text rendering.
- Add or update tests for safe activity normalization and exclusions; live display before turn or phase completion; durable reads after relaunch; earlier Review passes after a return to Working; run, phase, pass, and cursor isolation; append ordering and deduplication; persistence failure; paging, search, follow/resume, and accessible keyboard use.

## Specification impact

Update the contracts together; do not make frontend tests pass by working around incompatible specifications:

- `specifications/ui/GLW-graduation-log-window.md`: replace the Source/Structured viewer with the single Agent Activity view, its row presentation, safe fields, and states.
- `specifications/core/GRS-graduation-run-log-storage.md`: define the durable normalized activity stream, record schema, safe-content boundary, run/phase/pass attribution, append/read contract, indexes, and failure behavior. Remove the incompatible raw Source and graduation-observability Structured stream contracts for this viewer.
- `specifications/ai/GLG-graduation-loop-logging.md`: define how each graduation turn supplies run-scoped activity records and awaits durable acknowledgement.
- `specifications/tools/EAC-execute-agent-cli.md`: define the safe normalized activity delivered to the durable sink, including how excluded kinds and payload fields are prevented from persistence while preserving the existing Agent Output activity behavior for other runs.
- `specifications/core/AGV-agent-activity.md` and `specifications/ui/RUN-runs.md`: keep the live Agent Output stream distinct from durable graduation storage, and align graduation activity rows with the same visible row presentation without changing non-graduation output behavior.
- Review `specifications/core/GRD-graduation.md`, `specifications/core/GXD-graduation-execution.md`, and `specifications/core/GOB-graduation-observability.md` for required sink wiring, run attribution, and pass-history compatibility. Update them only where their current contracts prevent this behavior; keep observability state and stage history intact.
- Review `specifications/infra/CCP-claude-code-cli-protocol.md` and `specifications/infra/CDX-codex-cli-protocol.md` only if their protocol-specific normalization needs a contract change. Do not route graduation through CVL or add a raw transcript stream.