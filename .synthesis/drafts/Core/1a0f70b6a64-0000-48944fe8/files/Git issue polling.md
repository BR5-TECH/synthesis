## Intent

Recently we implemented **Publish to GitHub**, which publishes a draft as a GitHub issue. This feature adds GitHub polling for project work that is ready to start: the application finds eligible GitHub Tasks, lets the author claim one, stores an immutable local shadow draft, and opens the existing graduation-start flow.

The feature must use the project's existing GitHub token integration and publication-remote resolution. It must not create a second publication flow, edit the issue body, close the issue, or decide the final GitHub completion state.

## User journey

- The author opens Project settings and selects one accessible GitHub Project. The selected Project's stable GitHub node ID is stored in project-public settings.
- The author selects a polling interval from **1, 5, 15, 30, or 60 minutes**. No interval means that launch and timed polling are disabled, but the author can still use **Refresh** manually.
- When the project opens, and then at the configured interval while the project remains open, the application polls the repository resolved by the existing GitHub publication remote rules. Polling continues when the Git bottom panel is hidden.
- The Git bottom panel shows a **Ready tasks** section. A ready task is an open GitHub issue whose GitHub issue type is **Task** and whose Status in the selected GitHub Project is exactly **Ready**. No label is required. The section shows unclaimed eligible issues and all persisted GitHub-shadow drafts as separate rows; a claimed issue is represented by its shadow-draft row and is not shown as an unclaimed Ready task.
- The author can activate **Refresh** at any time. The section shows loading, empty, stale, and configuration-error states without blocking the rest of the application.- After a successful poll finds one or more eligible issues that were absent from the previous successful result, the application raises one coalesced in-app notification for that poll, naming the new issue count or titles and routing activation to the Git bottom panel. It raises no notification for an empty delta, a failed or stale poll, or an issue already found in the current polling session.
- For an unclaimed task, the author activates **Claim and graduate**. The application re-fetches the issue, changes its selected Project Status from **Ready** to **In Progress**, creates a persisted read-only shadow draft at the drafts root using the latest issue title and body, and immediately opens the existing graduation-start dialog for that draft.- For an unclaimed task, the author activates **Claim and graduate**. The application re-fetches the issue, changes its selected Project Status from **Ready** to **In Progress**, persists a pending-claim record in project-local `.synthesis/local.toml`, creates a persisted read-only shadow draft at the drafts root using the latest issue title and body, and immediately opens the existing graduation-start dialog for that draft. If shadow-draft creation or dialog opening fails, the pending claim remains and the Git section offers **Retry**. Retry re-fetches the issue and performs only the unfinished local steps; it does not move the issue again. The pending record is cleared after the dialog opens successfully.
- The shadow draft name is the issue title and its immutable prompt is the issue body. Its metadata stores the repository owner and name, issue number, issue URL, selected Project node ID, and claim state, using draft metadata conventions similar to GitHub publication metadata.
- The shadow draft appears in the Drafts panel with a GitHub-shadow state. It cannot be edited, renamed, moved, archived, or deleted; its only available action is **Graduate**. The author can open the issue URL through the existing external-browser behaviour.
- If the author cancels the graduation dialog, or graduation start fails, the issue remains **In Progress** and the shadow draft remains available for retry. A successful graduation changes the shadow draft to `graduated`, keeps it permanently, and shows its completed graduation. The issue remains **In Progress**; changing it to Done or closing it is outside this feature.
- If a poll fails, the section keeps the last successful rows, marks them stale, shows an inline error, and emits a diagnostic log record. If the selected Project, its access, or its exact `Ready` / `In Progress` options are invalid, polling is disabled and Project settings shows the configuration error.

## Requirements

### Polling and GitHub rules

- Use the existing GitHub token integration. Do not store, copy, or expose the token in polling state, UI state, logs, errors, or draft metadata.
- Resolve the polling repository through the existing GitHub publication remote resolution and its persisted remote selection. Do not add a separate repository setting.
- Persist one selected GitHub Project as its stable node ID in project-public settings. Project settings must list accessible GitHub Projects and allow the author to select one.
- Match the selected Project's Status options by exact names **Ready** and **In Progress**. If the Project is missing, inaccessible, or lacks either exact option, disable polling and show an actionable Project settings error.
- An eligible issue must satisfy all of these conditions: the issue is open, its GitHub issue type is `Task`, and its Status in the selected Project is `Ready`. Do not require a `ready-for-synthesis` label. Features are not eligible.
- Poll on project launch and at the selected interval while the project is open, even when the Git bottom panel is hidden. Avoid overlapping polls and discard stale results.
- Add a manual **Refresh** action to the Git section. It remains available when no interval is selected, unless polling configuration is invalid.- After a successful poll finds one or more eligible issues that were absent from the previous successful result, raise one coalesced in-app notification for that poll through `specifications/ui/NTF-notifications.md`, addressed to the Git bottom panel. The notification names the new issue count or titles. Do not raise for an empty delta, a failed or stale poll, or an issue already reported in the current polling session.
- Use the interval presets `1`, `5`, `15`, `30`, and `60` minutes. No selected interval disables launch and timed polling.

### Git bottom-panel experience

- Add a **Ready tasks** section inside the existing Git bottom panel. Do not add a new bottom-panel surface.
- Show unclaimed eligible issues with title, repository, issue number, URL, and current Project Status. Provide a clear **Claim and graduate** action.
- Show unclaimed eligible issues and all persisted GitHub-shadow drafts as separate rows. A shadow-draft row links to its GitHub issue and draft; claimed issues are not shown in the unclaimed Ready list because their Status is `In Progress`.
- Render loading, empty, configuration-error, and stale/error states. A failed poll keeps the last successful rows, marks them stale, shows the error inline, and emits an application diagnostic log record through `LGC-logging.md`.
- Keep polling active while the project is open regardless of panel visibility. Keep the UI responsive while network calls or graduation operations run.
- Support keyboard access, visible busy and disabled states, accessible names for claim and refresh actions, and clear text that distinguishes stale data from an empty result.

### Claim and graduation

- Claim must re-fetch the GitHub issue immediately before creating the shadow draft. Use the re-fetched title and body, not the earlier poll snapshot.
- Change the selected GitHub Project Status from `Ready` to `In Progress` before any shadow draft is created or graduation dialog is opened. If the status update fails, create no shadow draft, create no pending claim, and do not open the graduation dialog.
- After the status update succeeds, persist one pending-claim record in project-local `.synthesis/local.toml` before local recovery work. The record identifies repository owner/name, issue number, issue URL, selected Project node ID, and any shadow-draft ID already created. It survives restart, is gitignored, is not committed project content, and offers **Retry** until the local claim steps finish.
- Create or reuse one persisted shadow draft at the drafts root. Its name is the latest re-fetched issue title and its immutable prompt is the latest re-fetched issue body. Store repository owner/name, issue number, issue URL, selected Project node ID, and claim state in the draft record or its draft metadata, using the existing publication metadata pattern. On Retry, re-fetch the issue and do not repeat the GitHub status update; create the shadow draft only if it does not already exist.
- Open the existing graduation-start dialog unchanged immediately after the shadow draft is available. The dialog keeps its existing stream, standing-work, and optional message choices, and claim never starts graduation automatically. Clear the pending-claim record only after the dialog opens successfully.
- If the author cancels the dialog or graduation start fails, keep the issue `In Progress` and keep the shadow draft. The shadow draft is the retry record. A separate pending-claim store or special recovery workflow is out of scope.
- After graduation completes successfully, set the shadow draft to `graduated`, keep it permanently, and show its completed graduation. Leave the GitHub issue `In Progress`; final state changes belong to the project's CI/CD workflow and are out of scope.

### Shadow-draft lifecycle and immutability

- Add a GitHub-shadow draft state to draft storage. A GitHub-shadow draft is read-only and permits only opening its GitHub metadata and starting graduation.
- Disable prompt editing, renaming, moving, archiving, deletion, publication, and other lifecycle changes for a GitHub-shadow draft. Enforce these restrictions in both the UI and backend.
- Keep the shadow draft in the Drafts panel at the drafts root. It must survive application restart and polling refreshes.
- After successful graduation, use the existing `graduated` draft state and retain the shadow draft and its metadata. Do not remove or replace its immutable prompt.
- Ensure the existing graduation-start and graduation-lock rules remain authoritative for the shadow draft while a run is active.

### Scope exclusions

- Do not poll GitHub Features, closed issues, issues without type `Task`, or issues whose Project Status is not exactly `Ready`.
- Do not require or modify a ready label. Do not close issues, set Done, edit issue title or body, process comments, or upload issue attachments.
- Do not add a second GitHub token flow, repository selector, or publication flow. Pending claims use project-local state in `.synthesis/local.toml`; do not add a separate pending-claim file or store.
- Final GitHub completion handling is owned by CI/CD and is outside this feature.

### Specifications to add or update

- Add `specifications/core/GPP-github-polling.md` for polling, eligibility, GitHub Project access, claim sequencing, persistence, errors, concurrency, and backend events.
- Update `specifications/ui/GIT-git.md` for the Ready tasks section, refresh behaviour, stale/error states, claim action, shadow-draft links, and accessibility requirements.
- Update `specifications/ui/NTF-notifications.md` for one coalesced notification per successful poll that finds new Ready issues, addressed to the Git bottom panel and subject to the existing notification routing and suppression rules.
- Update `specifications/ui/SET-project-settings.md` for GitHub Project selection and the polling interval selector.
- Update `specifications/core/PSS-project-settings-storage.md` for the project-public selected Project node ID and polling interval presets.
- Update `specifications/core/DRS-draft-storage.md` for GitHub-shadow status, immutable issue metadata, root filing, and its lifecycle restrictions.
- Update `specifications/ui/DRP-drafts-panel.md` and `specifications/ui/NAW-new-artifact.md` for GitHub-shadow rendering and the only permitted Graduate action.
- Update `specifications/core/GRD-graduation.md` and its start-flow UI specification so an existing shadow draft can open the graduation-start dialog and follows the stated cancellation and failure rules.
- Reuse the GitHub authentication and remote-resolution contracts of `specifications/core/GHP-github-publication.md`; update that specification only if a shared backend contract must be exposed without changing publication behaviour.
- Use `specifications/core/LGC-logging.md` for poll failures and claim failures. Add or update tests for eligibility, polling cadence, stale results, configuration errors, claim sequencing, pending-claim persistence in `.synthesis/local.toml`, restart recovery, retry of unfinished local steps, notification coalescing, cancellation, and successful graduation.