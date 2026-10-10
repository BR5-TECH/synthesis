# GitHub polling

**Spec code:** `GPP`

## Intent
The backend that finds project work which is ready to start on GitHub and lets the author claim it. It polls the repository that GitHub publication resolves, keeps the open GitHub **Tasks** whose Status in one selected GitHub Project is exactly **Ready**, and on a claim moves the issue to **In Progress**, records a pending claim on this machine, and creates an immutable **GitHub-shadow draft** that the existing graduation-start dialog then graduates. It reuses the project's GitHub token and the publication remote resolution, so it adds no token flow and no repository setting. Out of scope: a second publication flow, any edit of an issue's title, body, labels, or comments, closing an issue or setting it to Done, attachments, GitHub Features, and the final completion state, which the project's CI/CD owns.

## Functional requirements
1. **GPP-FR-WOIM** Every GitHub request of this module goes to the host of the polling repository through the GraphQL URL that `GTS-github-token-storage.md` GTS-FR-PDWB resolves from that host, authenticated with the secret the open project resolves through `GTS-github-token-storage.md` (GTS-FR-13). No secret is stored, copied, or returned in any polling state, view, event, error, log record, draft record, or project-local record.
2. **GPP-FR-RGNM** The **polling repository** is the remote that `GHP-github-publication.md`'s `resolve_publication_repository()` resolves, with its persisted remote selection (GHP-FR-HVQG). Where it resolves none, a poll fails with that refusal code. This module stores no repository of its own.
3. **GPP-FR-NLPG** The **polling settings** are one selected GitHub Project, stored as its stable node ID, and one polling interval. Both are project-public and are read and written through `PSS-project-settings-storage.md` (PSS-FR-OPQD). The Project settings window presents them in its **GitHub Project settings** section, beside the publication settings that `GHP-github-publication.md` (GHP-FR-KVRH) owns.
4. **GPP-FR-HZDD** The interval is unset or one of `1`, `5`, `15`, `30`, and `60` minutes. `set_github_polling_settings` refuses any other value with `invalid_interval` and writes nothing.
5. **GPP-FR-WYRP** `list_github_projects()` returns every GitHub Project the token can read that belongs to the viewer or to the polling repository's owner, each with its node ID, title, owner login, and number, without duplicates. It changes nothing on GitHub.
6. **GPP-FR-DEZO** The **configuration** is valid only when the selected Project resolves, the token can read it, and its single-select field named `Status` holds options named exactly `Ready` and `In Progress`. A Project that does not satisfy this carries the typed error naming the cause.
7. **GPP-FR-EHRC** The configuration errors are `project_unavailable` for a missing or unreadable Project, `status_field_missing`, `ready_option_missing`, and `in_progress_option_missing`. Each carries displayable text that tells the author what to change in the GitHub Project settings section of Project settings.
8. **GPP-FR-IURX** `set_github_polling_settings(project_node_id, interval_minutes)` persists both values, then validates a selected Project at once and records the configuration state. A `null` Project clears the selection and puts the configuration in the `unset` state.
9. **GPP-FR-ANDE** While the configuration state is `invalid`, no poll runs, `poll_github_ready_tasks` refuses with `polling_configuration_invalid`, and claims refuse on the same terms. The state stays `invalid` until the next `set_github_polling_settings` validates again.
10. **GPP-FR-BOQX** An issue is **eligible** only when it is open, its GitHub issue type is `Task`, it belongs to the polling repository, and its `Status` value in the selected Project is exactly `Ready`. No label is read or required.
11. **GPP-FR-XSKT** A poll reads the selected Project's items, validates the configuration on the terms of GPP-FR-DEZO, and keeps the eligible issues. Each kept row carries the repository host, owner, and name, the issue number, the title, the issue URL, and the Status name.
12. **GPP-FR-QCAM** A poll excludes an eligible issue that a GitHub-shadow draft or a pending claim already names, by repository host, owner, name, and issue number. Such an issue is shown through its shadow-draft row or its pending claim alone.
13. **GPP-FR-GPYE** A poll that succeeds replaces the snapshot rows, clears the stale mark and the last error, and records the instant of success.
14. **GPP-FR-PUXT** A poll that fails keeps the rows of the last successful poll, marks them stale, records the typed error with displayable text, and emits one diagnostic log record through `LGC-logging.md` naming the error code. A configuration error is recorded as the configuration state instead (GPP-FR-ANDE).
15. **GPP-FR-XZTP** At most one poll runs at a time per open project. A `poll_github_ready_tasks` call made while a poll is in flight starts no second poll and returns the current view with `polling` true.
16. **GPP-FR-ELWS** A poll result is **discarded** when the open project, its active worktree, or the polling settings changed after the poll started. A discarded result changes no row, no stale mark, and no notification delta.
17. **GPP-FR-DATH** The **polling session** starts when a project opens or its active worktree changes and ends when either changes again or the project closes. The session holds the issue keys of the previous successful poll and the keys already reported, in memory only.
18. **GPP-FR-YROY** A successful poll reports as **new** each eligible issue that is absent from the previous successful poll of the session and not yet reported in the session. The first successful poll of a session compares against an empty result.
19. **GPP-FR-RRPB** A failed, discarded, or refused poll reports no new issue. An issue reported once is never reported again in the same session, even when it leaves the result and comes back.
20. **GPP-FR-TYOV** `"github polling changed"` is emitted after every change of the view: a poll that settles, a settings change, a claim, a retry, and an acknowledgement. Its payload carries the new issues of the poll that caused it, each with its number and title, and an empty list for every other cause.
21. **GPP-FR-IFVC** `claim_github_task(issue_number)` re-fetches the issue from GitHub immediately before it changes anything. Where the re-fetched issue is no longer eligible (GPP-FR-BOQX), it refuses with `task_not_ready` and changes nothing.
22. **GPP-FR-ILGG** After the re-fetch, the claim changes the issue's `Status` in the selected Project from `Ready` to `In Progress` before it writes anything locally. Where that update fails, it refuses with `status_update_failed`, writes no pending claim, creates no draft, and opens nothing.
23. **GPP-FR-DHQM** After the status update succeeds, the claim writes one **pending claim** to the project-local store (per `PSS-project-settings-storage.md` PSS-FR-TXBB) before it creates a draft. The record names the repository host, owner, and name, the issue number, the issue URL, the selected Project node ID, and the shadow draft ID once one exists.
24. **GPP-FR-XPUO** The claim then creates one GitHub-shadow draft through `DRS-draft-storage.md`'s `create_github_shadow_draft` (DRS-FR-INCJ), named from the re-fetched title and holding the re-fetched body as its prompt. It reuses a shadow draft that already names the same issue rather than creating a second one.
25. **GPP-FR-CWGH** After the shadow draft exists, the claim writes its ID into the pending claim and returns the draft's ID and name. A failure to create the draft returns `shadow_draft_create_failed` and leaves the pending claim and the GitHub status as they are.
26. **GPP-FR-DGWL** The claim starts no graduation. The caller opens the graduation-start dialog of `../ui/GSD-graduation-start-dialog.md` for the returned draft.
27. **GPP-FR-BSLI** `acknowledge_github_claim(issue_number)` removes the pending claim for that issue of the polling repository. The caller invokes it only after the graduation-start dialog has opened for the shadow draft.
28. **GPP-FR-IGER** A pending claim survives an application restart and appears in the view until it is acknowledged. A claim that refuses before the status update leaves no pending claim.
29. **GPP-FR-TOED** `retry_github_claim(issue_number)` re-fetches the issue for its latest title and body and performs only the unfinished local steps: it creates the shadow draft only where the pending claim names no existing draft and no shadow draft names the issue. It never changes the GitHub status.
30. **GPP-FR-KSMZ** `retry_github_claim` against an issue that holds no pending claim refuses with `no_pending_claim`. A re-fetch that fails refuses with its typed error and keeps the pending claim.
31. **GPP-FR-RAQP** At most one claim or retry runs at a time for one issue. A second call for the same issue while one runs refuses with `claim_in_progress`, and `claim_github_task` against an issue that holds a pending claim refuses with `claim_pending`.
32. **GPP-FR-UBDE** The view lists every GitHub-shadow draft of the active worktree, whatever its status, with its draft ID, name, status, repository host, owner, and name, issue number, issue URL, Project node ID, and whether a graduation run holds it, read from `"list drafts"` data and never cached.
33. **GPP-FR-ZLBF** A cancelled graduation-start dialog and a failed graduation start change nothing here: the issue stays `In Progress` on GitHub, and the shadow draft stays available to graduate.
34. **GPP-FR-CRWY** No operation of this module sets an issue to Done, closes an issue, or edits its title, body, labels, or comments. After a shadow draft graduates, the issue stays `In Progress`.
35. **GPP-FR-WOLE** `open_github_task_issue(issue_number)` opens the OS default browser at the URL of that issue in the current snapshot rows or pending claims. It refuses an issue that neither holds, and any URL that is not an HTTPS issue address of the polling repository on its host.
36. **GPP-FR-WKZF** Every failure of a poll, a claim, a retry, or an acknowledgement emits one log record through `LGC-logging.md` with the operation, the error code, and the issue number where one applies. No record carries a token, an `Authorization` header, an issue body, or a credentialed URL.
37. **GPP-FR-SUFH** The typed error vocabulary is exactly `no_project_open`, `polling_unconfigured`, `polling_configuration_invalid`, `invalid_interval`, `project_unavailable`, `status_field_missing`, `ready_option_missing`, `in_progress_option_missing`, the remote refusals of `GHP-github-publication.md` GHP-FR-ZRFP, `github_host_mismatch`, `github_unreachable`, `github_request_failed`, `task_not_ready`, `claim_pending`, `claim_in_progress`, `no_pending_claim`, `status_update_failed`, `pending_claim_write_failed`, `shadow_draft_create_failed`, and `issue_not_listed`.
38. **GPP-FR-EWLZ** `poll_github_ready_tasks` with no Project selected refuses with `polling_unconfigured` and changes no row.
39. **GPP-FR-JOKP** The publication settings of `GHP-github-publication.md` (GHP-FR-KVRH) change no polling setting, configuration state, snapshot row, claim, eligibility rule, or Status value. A poll and a claim read the `Task` Type of GPP-FR-BOQX and never the parent issue Types or the sub-issue Type. Saving either group of settings leaves the other group's stored values unchanged.
40. **GPP-FR-HSTD** The Project listing, the Project item reads, the `Status` field read, the Status update of a claim, and the issue re-fetch of a claim use the GraphQL URL of the host of the polling repository. Where the host of the token differs from the host of the polling repository, a poll refuses with `github_host_mismatch` and makes no request.
41. **GPP-FR-HSTE** The issue URL of a row, a claim, and a shadow draft is the address that GitHub returned for the issue, on the host of the polling repository. A pending claim and a shadow-draft link keep the repository host with the owner and the name, and one stored without a host reads as `github.com`.

## Contract surface
Every operation is a Tauri command. The Git bottom panel (`../ui/GIT-git.md`) and the GitHub Project settings section of the Project settings window (`../ui/SET-project-settings.md`) invoke them. The quoted name is the canonical operation name and matches those specs' "Delegated to backend (abstract)" lines byte-for-byte.

### Record shapes
```
GithubPollingInterval = 1 | 5 | 15 | 30 | 60

GithubPollingSettings {
  project_node_id,        // string | null (GPP-FR-NLPG)
  interval_minutes        // GithubPollingInterval | null (GPP-FR-HZDD)
}

GithubProjectOption {
  node_id, title, owner_login, number
}

GithubPollingConfiguration {
  state,                  // "unset" | "unchecked" | "valid" | "invalid"
  error_code,             // one GPP-FR-EHRC code; null unless "invalid"
  error,                  // displayable text; null unless "invalid"
  project_title           // string | null
}

GithubReadyTask {
  repository_host, repository_owner, repository_name, issue_number, title, url, status
}

GithubPendingClaim {
  repository_host, repository_owner, repository_name, issue_number, issue_url,
  project_node_id,
  draft_id,               // string | null until the shadow draft exists
  claimed_at              // RFC 3339 UTC
}

GithubShadowDraftRow {
  draft_id, name, status, // status: "github_shadow" | "graduated"
  repository_host, repository_owner, repository_name, issue_number, issue_url, project_node_id,
  locked                  // bool — a non-terminal graduation run holds the draft
}

GithubPollingView {
  settings,               // GithubPollingSettings
  configuration,          // GithubPollingConfiguration
  repository,             // { host, owner, name } | null
  polling,                // bool — a poll is in flight
  tasks,                  // GithubReadyTask[] of the last successful poll
  stale,                  // bool
  last_error_code,        // string | null
  last_error,             // displayable text | null
  last_success_at,        // RFC 3339 UTC | null
  pending_claims,         // GithubPendingClaim[]
  shadows                 // GithubShadowDraftRow[]
}

GithubClaimResult { draft_id, draft_name }
```

`unchecked` is the state of a selected Project that no poll and no settings write has validated yet in this session. It permits polling.

### Tauri commands
- `"list github projects"` → `list_github_projects()` → `GithubProjectOption[]` (GPP-FR-WYRP).
- `"get github polling state"` → `get_github_polling_state()` → `GithubPollingView`. A read that contacts no network.
- `"set github polling settings (project node id, interval minutes)"` → `set_github_polling_settings(project_node_id, interval_minutes)` → `GithubPollingView` (GPP-FR-IURX).
- `"poll github ready tasks"` → `poll_github_ready_tasks()` → `GithubPollingView` (GPP-FR-XSKT, GPP-FR-XZTP).
- `"claim github task (issue number)"` → `claim_github_task(issue_number)` → `GithubClaimResult` (GPP-FR-IFVC).
- `"retry github claim (issue number)"` → `retry_github_claim(issue_number)` → `GithubClaimResult` (GPP-FR-TOED).
- `"acknowledge github claim (issue number)"` → `acknowledge_github_claim(issue_number)` (GPP-FR-BSLI).
- `"open github task issue (issue number)"` → `open_github_task_issue(issue_number)` (GPP-FR-WOLE).

Every command errors with `no_project_open` when no project is open.

### Events
- `"github polling changed"` — payload `{ new_issues: [{ issue_number, title }] }` (GPP-FR-TYOV). Consumed by `../ui/GIT-git.md`, `../ui/SET-project-settings.md`, and `../ui/NTF-notifications.md`.

### Consumed from other modules
- `resolve_github_token_secret(remote_host)` — `GTS-github-token-storage.md` (GTS-FR-13).
- `resolve_publication_repository()` — `GHP-github-publication.md` (GHP-FR-YDAN).
- `load_github_polling_settings()`, `save_github_polling_settings(settings)`, `load_github_pending_claims()`, and `save_github_pending_claims(claims)` — `PSS-project-settings-storage.md` (PSS-FR-OPQD, PSS-FR-TXBB).
- `create_github_shadow_draft(name, prompt, link)` and `list_drafts` — `DRS-draft-storage.md` (DRS-FR-INCJ, DRS-FR-08).

## Non-functional requirements
- A poll is one remote resolution and a small, bounded number of GraphQL reads, whatever the number of drafts.
- Every network call runs off the window thread and is bounded by a request timeout, so a slow GitHub never freezes the application.
- `get_github_polling_state` answers from memory and disk alone, so the Git panel and Project settings render at once.
- Polling state outside the pending claims lives in memory for the polling session and is never written to disk.
