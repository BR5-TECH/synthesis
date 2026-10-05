## Intent

Add a **Push** control to the main window's top chrome so authors can publish committed work from the active worktree without opening Git or Changes. The control uses the existing `push_current_branch` operation; it does not create commits, pull, fetch, or change branches. The existing worktree **Refresh** control remains fetch-only.

## User journey

- The author sees **Push** in the top chrome immediately after the existing worktree **Refresh** control and before **Streams**.
- The control is present only while the open project is inside a Git repository.
- It is enabled when the active branch has commits to publish, using the same upstream-state rules as the Changes panel's **Push** action. It is unavailable when there is no remote or no commit to publish, and while any push is running.
- The author activates **Push**. The application pushes the active branch's commits not yet present on its upstream through the existing Git integration.
- Push progress appears in the status bar. Transfer output and failures appear in the Git panel's existing push/pull output area, including when the panel is not open. Do not write these failures to the diagnostic Logs panel.
- A GitHub token-selection error opens the existing token picker; a missing-token error provides the existing route to Global settings → GitHub.


##  Requirements

- Reuse `get_upstream_sync_state` and `push_current_branch`; add no Git command or event. Apply the same push-availability rules as `CHG-changes.md` CHG-FR-37 and the same typed authentication-error handling as CHG-FR-45.
- Treat push as one shared operation across the top chrome, Git panel, and Changes panel. Disable every Push action while a push is in flight, regardless of which surface started it; keep all output in the Git panel's existing transfer area and report progress through the status bar.
- Keep **Refresh** fetch-only. Do not make it run Push or Pull, and do not change the active worktree or branch when Push runs.
- Make the control keyboard-accessible and give it the accessible name **Push**. Its unavailable state must be distinguishable without colour alone; provide the reason when it has no publishable commits, no remote, or a push is already running.
- Update the affected specifications to define the top-chrome order and the shared Push behavior: `specifications/ui/SNV-shell-navigation.md`, `specifications/ui/WTS-worktree-selector.md`, `specifications/ui/WSS-work-stream-selector.md`, `specifications/ui/GIT-git.md`, and `specifications/ui/CHG-changes.md`. Keep `specifications/core/GTC-git.md`'s existing transfer contract unless a contract gap requires a change.
- Add frontend coverage for control placement and visibility, upstream-state enablement, keyboard and accessible states, push progress and output routing, typed token errors, and duplicate-push prevention across all three surfaces.