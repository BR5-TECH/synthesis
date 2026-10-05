## Intent

Extend the existing GitHub publication flow so an author can publish a draft either as a root issue or as a sub-issue of an existing issue in the selected publication repository. The author can also set the GitHub issue Type and milestone when GitHub makes those values available, without creating a second token, repository, or publication flow.

## Users and user journey

The user is an author who has finished a draft and has access to the configured GitHub repository and token.

1. The user opens a draft and activates **Publish to GitHub**.
2. The existing remote resolution and remote picker run first. The flow uses the selected publication repository and the existing project-resolved GitHub token.
3. A modal chooser loads:
   - open issues in the selected repository that match the configured parent issue Types;
   - available GitHub issue Types;
   - open repository milestones;
   - the configured sub-issue Type and milestone policy.
4. The chooser offers root publication. If the parent list is empty, no issue matches the configured parent Types, or the parent list cannot be read, it still shows and enables the root option. It does not block publication only because parent choices are unavailable.
5. For a selected parent issue:
   - create the new issue as that issue's sub-issue;
   - use the configured default sub-issue Type, which defaults to `Task`;
   - apply the configured sub-issue milestone policy.
6. For root publication:
   - let the user select an available issue Type, or omit Type when none is available;
   - let the user select an open milestone, or omit the milestone when none is available.
7. The user confirms publication. The issue is created with the selected title, body, parent, Type, and milestone values.
8. If publication fails, the existing retry and recovery surfaces remain available. A retry keeps the original root or parent choice, Type choice, and milestone choice.

## Functional requirements

- Use the existing GitHub REST client, remote resolution, publication marker, attempt store, token resolution, retry flow, and publication history. Do not add a token flow, a repository selector, a second publication flow, or a `gh` CLI dependency.
- Parent choices are open GitHub issues in the selected publication repository. Filter them by the configured set of parent issue Types. The default configured set contains only `Feature`; the setting supports multiple Types.
- A parent row must identify the issue number, title, Type, milestone, and URL. Do not show issues from another repository, closed issues, pull requests, or issues whose Type is not configured.
- Root publication remains supported. The chooser must provide a clear root option, and the root option must remain available when the parent request returns no matches, an empty list, or a typed read failure. A parent-list failure must not prevent root publication.
- Root issue Type is optional. When GitHub returns available Types, show them in the chooser. When the list is empty or the request fails, show an inline non-blocking notice and publish without Type if the user confirms.
- Root milestone is optional. Show open milestones from the selected repository. When the list is empty or the request fails, show an inline non-blocking notice and publish without a milestone if the user confirms.
- Sub-issue Type is controlled by Project settings, not by the publication chooser. The configured default is `Task`. If the configured Type is not available in the selected repository, publish without Type and state that the configured Type is unavailable.
- Configure the sub-issue milestone policy in Project settings with exactly these values:
  - **Inherit parent milestone**: use the parent's current milestone; if the parent has none, omit the milestone.
  - **No milestone**: omit the milestone.
  - **Author-selectable milestone**: show the selected repository's open milestones in the chooser.
- Do not add a separate publication Project selector. Reuse the GitHub Project selected by the current polling settings. Rename the Project settings section from **GitHub polling** to **GitHub Project settings** and keep polling configuration in that section.
- GitHub Project settings must expose:
  - the multiple parent issue Type filter, defaulting to `Feature`;
  - the default sub-issue Type, defaulting to `Task`;
  - the sub-issue milestone policy, defaulting to **Inherit parent milestone**.
  Persist these values with the existing project-public settings store. Preserve saved values when a metadata read fails. If a saved Type is no longer available, show it as unavailable and do not silently replace it.
- The publication chooser must have explicit loading, empty, unavailable, and failed states for parent issues, Types, and milestones. Disable only controls that require unavailable data. Keep root publication enabled when the parent list is unavailable. Never show an empty selector as if it were successfully loaded.
- Save every publication choice in the standing publication attempt before the first GitHub mutation: root or sub-issue, parent repository and issue number when present, selected root Type, resolved sub-issue Type, milestone policy, and selected milestone when present. A retry must reuse these values exactly; it must not reopen the chooser or silently fall back from a missing parent to root.
- Create the issue with the existing title, body, and marker rules plus the selected Type, milestone, and parent. Use the GitHub sub-issue relationship supported by the existing API integration. The parent must be in the selected publication repository.
- Extend idempotent retry and recovery checks to include parent, Type, and milestone. If an issue with the attempt marker already exists and any requested publication metadata differs, return the existing recovery choice instead of creating a duplicate or silently accepting the mismatch. **Update existing** must reconcile the issue to the saved publication choices; **Publish as a new issue** must start a new attempt with a new marker.
- If the saved parent no longer exists, is closed, is in another repository, or cannot be used, return a typed error and keep the attempt recoverable. Do not silently publish a root issue.
- If a GitHub request cannot read Types, milestones, or parent issues, return displayable typed metadata errors to the chooser, but keep the optional root publication path available. A failure to apply the requested publication metadata during issue creation is a publication failure and follows the existing retry rules.
- Keep existing publication history fields and add the saved publication shape needed to identify the parent, Type, milestone, and whether the issue was published as root or sub-issue. Do not expose the token in any view, error, event, log, or stored record.
- Keep the existing draft UI contract: **Publish to GitHub** remains in the action control; the chooser is a modal over the draft; cancellation changes nothing; publication success updates the existing publication tag and history; the issue link opens in the external browser.
- Preserve the existing publication behavior for title, complete prompt body, local-asset refusal, remote eligibility, draft locking, publication history, abandoned attempts, application restart, and issue-marker recovery.

## Scope exclusions

- Do not change GitHub polling eligibility, claim behavior, Project Status values, or GitHub-shadow drafts.
- Do not edit labels, assignees, comments, issue state, issue title, or issue body beyond the existing publication body and marker.
- Do not upload local assets, add a new credential flow, support non-GitHub hosts, or require a GitHub CLI.
- Do not allow the author to override the configured sub-issue Type in the publication chooser.

## Required project specifications and tests

Update these specifications and keep their command names, record shapes, events, and cross-references consistent:

- `specifications/core/GHP-github-publication.md` — publication choices, metadata reads, parent/sub-issue creation, Type and milestone handling, attempt persistence, retry and recovery comparison, typed errors, history records, and Tauri commands.
- `specifications/ui/NAW-new-artifact.md` — chooser states, root and parent selection, Type and milestone controls, loading and failure presentation, retry preservation, cancellation, accessibility, focus, keyboard dismissal, and publication status updates.
- `specifications/ui/SET-project-settings.md` — rename the GitHub polling section to GitHub Project settings and add the parent Type filter, default sub-issue Type, and milestone policy controls.
- `specifications/core/PSS-project-settings-storage.md` — persist the new GitHub Project settings without losing existing polling settings or unrelated project-public data.
- `specifications/core/GPP-github-polling.md` — update references to the renamed shared settings section and preserve polling behavior unchanged.
- Any shared GitHub API, IPC, and test specifications that define issue creation, GitHub Project resolution, token permissions, or typed error propagation.

Add backend, IPC, frontend, retry/recovery, persistence, and end-to-end tests. Cover root publication, sub-issue publication, every milestone policy, configured Type defaults, empty and failed metadata reads, parent-list fallback to root, unavailable saved Types, retry choice preservation, marker-based duplicate recovery, metadata mismatch recovery, invalid or changed parents, cancellation, restart recovery, and unchanged existing root-publication behavior.