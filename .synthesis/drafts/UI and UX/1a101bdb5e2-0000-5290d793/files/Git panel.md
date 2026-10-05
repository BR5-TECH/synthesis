## Intent

The user uses the existing bottom Git panel to inspect repository history, branches, and pull requests. The panel is a mock today. Implement the planned behavior in the existing panel and Git integration. Keep the four existing Git sections and their tab-like navigation; do not add or remove a section or a bottom-panel surface.

## Expected behaviors

Keep the four existing Git-panel sections: **Log**, **Branches**, **PRs**, and **Ready tasks**. The first three get the behavior below. Keep Ready tasks and its existing behavior unchanged. Keep all four section controls visible and usable in the left rail at the narrowest supported panel width. Do not clip them or require horizontal scrolling.

### Log section

- Show the latest 100 commits reachable from the active worktree's `HEAD`, newest first. Show the active branch as the history context.
- In the history rail, show each commit's author and email, date, branch context, and commit message.
- When the user selects a commit, show its affected files in the large view, grouped by folder. When the user selects a file, show that commit's diff for the file in the large view. Keep the selected commit and file clear while the user moves between them.

### Branches section

- Show local and remote branches. Place a **Local / Remote** switch at the top of the rail, with **Local** selected by default.
- A branch's right-click menu must also be available by keyboard. **Information** opens an overlay with the branch name and type, upstream, current/worktree or work-stream association where applicable, tip commit, and a newest-first commit list with author, email, date, and message.
- **Delete** opens a confirmation before any deletion. It deletes a local branch and its associated linked worktree. If the branch has an associated remote branch, offer remote deletion with an unchecked checkbox; do not delete the remote branch unless the user selects it.
- Never delete the branch checked out in the repository's primary worktree. Require the user to check out another branch first. Never delete the active worktree. If a branch is checked out in the active linked worktree, require the user to switch away before deletion; do not switch the project automatically. A branch checked out in another, inactive linked worktree may be deleted with that worktree.
- If deletion would discard uncommitted work, the confirmation must warn the user and name or summarize the affected paths. The user may confirm and discard that work; cancellation changes nothing.
- If the branch belongs to a work stream, show a warning and use the existing work-stream deletion flow. Keep its busy and non-terminal-run checks; this Git action must not bypass them. Apply the same explicit uncommitted-work warning and confirmation rule to deletion from the existing Streams overlay.
- Show typed errors in the action that caused them. Keep the panel usable while a request is running, and show clear loading, empty, success, and failure states.

### PRs section

- Show pull requests for the current repository. Provide **Open** and **Closed** filters, with **Open** selected by default.
- Selecting a PR shows its description and the full conversation and activity timeline available through the GitHub API, including comments, reviews, and activities. Use the existing GitHub authentication and error flows. Keep the thread readable and scrollable within the panel.

### Existing panel behavior

Keep the existing user journey and all other Git-panel behavior, including **Ready tasks**. Use the existing Git integration for repository and GitHub access. The panel must not block other application controls while it loads data or runs an operation.

## User journey

- The user opens the existing bottom Git panel and selects **Log**, **Branches**, or **PRs**. The four existing Git sections remain available in the left rail.
- In **Log**, the user selects a commit, reviews its folder-grouped file list, then selects a file to inspect its diff.
- In **Branches**, the user switches between local and remote branches, opens branch information, or starts a confirmed deletion. The user must switch away before deleting a current branch or worktree. A stream branch uses the Streams deletion flow and its safety checks.
- In **PRs**, the user filters open or closed pull requests, opens one, and reads its description, conversation, reviews, and activity timeline.

## Requirements

- Implement the existing Git panel in the Tauri, TypeScript, and React frontend. Keep the current four sections and tab-like navigation. Keep **Ready tasks** unchanged, and make all four section controls fit in the left rail without clipping or horizontal scrolling.
- Use the existing Git integration for Git and GitHub operations. Extend its typed commands and events where the current contract does not support the required commit history, branch information, branch deletion, linked-worktree removal, remote branch deletion, or full PR conversation and activity timeline. Do not call Git or GitHub directly from the UI.
- Keep Log data scoped to the active worktree. Load at most the latest 100 commits reachable from its `HEAD`. Selecting a commit loads its affected-file tree; selecting a file loads that commit's file diff. Handle loading, empty history, no affected files, and typed errors without blocking the rest of the panel.
- Load local and remote branch lists through the Git integration. Default the branch switch to **Local**. Provide keyboard-accessible branch context menus and overlays, with focus and dismissal behavior that follows existing application overlay patterns.
- Before any deletion, re-check the target branch and worktree state in the backend. Refuse deletion of the primary worktree's checked-out branch until another branch is checked out. Refuse deletion of the active worktree until the user switches away. Do not automatically switch branches or worktrees. Delete a non-active associated linked worktree with its local branch.
- For a work-stream branch, call the existing stream deletion flow and preserve its busy and non-terminal-run refusals. Update the Streams overlay to permit deletion with uncommitted changes only after the explicit warning and confirmation described above; never make confirmation bypass stream run-safety checks. On cancel, refusal, or backend failure, preserve the branch and worktree and show the result inline.
- When uncommitted changes would be discarded, show the affected paths or their count before confirmation. A confirmed deletion may discard those changes. Remote deletion is a separate, optional confirmation choice and is unchecked by default. A failed remote deletion must not be presented as a successful remote deletion.
- Show PRs for the current repository with **Open** selected by default and an **Open / Closed** filter. Load and render the full conversation and activity timeline available through the GitHub API. Use the existing token-selection and missing-token flows, and show loading, empty, stale/error, and request-failure states without blocking other controls.
- Preserve current behavior outside the listed changes, including the user journey and **Ready tasks**. Keep state scoped to the open project and active worktree where the existing Git-panel contract requires it; discard stale responses after a project or worktree change.
- Update these specifications to match the feature and resolve the current deletion prohibitions: `specifications/ui/GIT-git.md`, `specifications/core/GTC-git.md`, `specifications/core/WTC-worktree-context.md`, `specifications/core/WKS-work-streams.md`, and `specifications/ui/WSS-work-stream-selector.md`. Define the new backend contracts, current-branch/current-worktree refusals, remote-deletion choice, dirty-worktree confirmation, stream deletion safety, and PR timeline data. Keep the existing worktree selector free of deletion actions.
- Add frontend and backend tests for the 100-commit limit and ordering, file grouping and diff selection, local/remote filtering, branch information, primary/current branch and active-worktree deletion refusals, inactive linked-worktree deletion, stream busy and non-terminal-run refusals, dirty-worktree warning/cancel/confirm behavior in both Git and Streams surfaces, remote deletion opt-in and failure, PR filters and full timeline rendering, loading and error states, keyboard operation, overlay dismissal, and stale-response handling. Run the project's required specification and test checks and report any failure without claiming a pass.
- Look-and-feel of the Git panel and its sections must stay in line with the rest of application panels.