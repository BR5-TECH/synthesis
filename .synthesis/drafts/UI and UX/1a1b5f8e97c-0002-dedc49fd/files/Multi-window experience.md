## Intent

User should have an ability to have multiple projects open at the same window, I.e. by adding project-level tabs to IDE.

## User journey

1. The user opens project A. The window looks as it does today. No project tab strip shows.
2. The user opens the project switcher and selects project B. A dialog asks: **This window** or **New window**.
   - **This window**: project B replaces project A in the current project tab (same teardown as today).
   - **New window**: project B opens in a new project tab. Project A stays open and keeps its state.
3. Two project tabs now exist. Small capitalised bookmarks show at the right of the top chrome, next to the agent selector and theme selector. The user clicks a bookmark to switch project. Each project keeps its own file tabs, panels, editor state, conversations and runs. Nothing from one project shows in another.
4. A project in the background keeps working. When something needs the user (agent turn ends, graduation run needs input), its bookmark shows a needs-attention dot, and the toast or OS notification names the project. A click on it activates that project tab.
5. The user closes a project tab with the × on its bookmark, or with **Close Project** for the active tab. When a run is active, a confirmation appears first.
6. The user closes the last project tab. The Project picker shows.
7. The user quits and starts the app again. The Project picker shows. No project tab returns.

## Requirements

- **Project tab**: one open project with its own isolated workspace (file tabs, panels, editor state and undo history, conversations, worktree selection, runs, file watching, indexing). One project is one project tab. No limit on the number of project tabs. Do not confuse with file tabs (TAB) in the main viewport.
- **Bookmarks**:
  - Show only while two or more project tabs are open. Hide when one is open.
  - Place them at the right of the top chrome, next to the agent selector and theme selector.
  - Each bookmark shows 2-3 capital letters: the first letters of the project name, in the style of the artifact-type filter in the Project panel. Do not add a digit on collision.
  - Each bookmark has the full project name as its accessible name. The active bookmark is clearly marked.
  - Each bookmark has a × to close the tab.
  - With no limit on tabs, the bookmark group must scroll horizontally and keep the active bookmark in view. It must not push the agent or theme selector out.
- **Switcher** (SNV-FR-17..22): keep the control, list and ordering. Selecting an entry other than the current project opens the dialog **This window** / **New window**. Use clear labels that cannot be mistaken for the Settings child window.
  - **This window**: replace the current project in the active project tab. Teardown follows OVW-FR-11, including the write of pending changes first and cancel on a failed write.
  - **New window**: add the project as a new project tab and activate it.
  - If the project is already open in a tab, do not open it twice: activate that tab (for both choices). The current project stays non-actionable.
- **Open Another Project…**: opens the existing Project picker as a **modal over the main window**. Reuse the picker implementation; do not build a second one. All open project tabs stay alive. The user dismisses it without a change. When the user opens a project in it, show the same dialog (**This window** / **New window**). On success the picker closes.
- **Close**: the × on a bookmark and **Close Project** (SNV-FR-25, applies to the active tab) close one project tab only.
  - If the tab has a running agent turn or graduation run, ask for confirmation. On confirm, stop its runs, then close.
  - Write pending changes first. A write that cannot proceed safely cancels the close (EDT-FR-32).
  - Close tears down only that tab's state, watcher and retained edit state.
  - Closing a non-active tab keeps the active tab. Closing the active tab activates the nearest remaining tab.
  - Closing the last tab, or Close Project on the only project, returns to the Project picker as today.
- **Background work**: a project tab that is not in front keeps running (agent turns, graduation runs, discussions, file watching, indexing). Its bookmark shows a needs-attention dot per NTF.
- **Notifications**: every toast and OS notification states the project name. A click activates the project tab, then opens the address as today (NTF-FR-17). The address must identify the project.
- **Settings**: stay one Settings child window. The Project group edits the active project tab.
- **Restart**: do not persist or restore the set of project tabs or the active tab. The app starts at the Project picker (PPK-FR-01). Per-project data (active worktree, recent list) persists as today.
- **Isolation**: every backend operation and event that serves one project (file watch, scan, worktree context, agent turns, runs, notifications) must be scoped to its project. No event from one project may update another project's UI.
- **Specifications this work changes**: `OVW-overview.md` (OVW-FR-02, OVW-FR-11, OVW-FR-12 wording on one project and teardown), `SNV-shell-navigation.md` (SNV-FR-17..22, SNV-FR-23 and FR-25 Close Project, SNV-FR-27 full-screen exit, plus the new bookmark group), `PPK-project-picker.md` (PPK-FR-01, FR-06, FR-09, FR-12: picker as modal over the main window), `NTF-notifications.md` (project name, address, bookmark marker; NTF-FR-17, NTF-FR-QGSV), `SWN-settings-windows.md` (SWN-FR-QXTV, Project group scope), `TAB-tabs.md` (tabs belong to a project tab), `../core/PST-project-storage.md` (PST-FR-14), `../core/WTC-worktree-context.md` (WTC-FR-18, WTC-FR-20) and any backend spec that holds a single open project.

## Expected deliverables

- Project tab model in the main window: several open projects, one active, each with its own isolated workspace state (file tabs, panels, editor state, conversations, worktree selection, runs). No state is shared between project tabs.
- Bookmark group in the top chrome: shown from two open project tabs, capital-letter labels, active marker, ×, needs-attention dot, horizontal scroll that keeps the active bookmark in view, full project name as accessible name. Keyboard and screen-reader support.
- Switcher dialog **This window** / **New window** (final labels set by the author), reused by the **Open Another Project…** picker modal. The existing Project picker implementation is reused, not duplicated.
- Close flow for one project tab: × and **Close Project**, confirmation when a run is active, write of pending changes first, teardown of that tab only, nearest-tab activation, return to the Project picker on the last tab.
- Backend scoping by project for file watching, scanning, indexing, worktree context, agent turns, runs and notifications. Every event carries its project, and the UI routes it to that project tab only.
- Notifications: project name in every toast and OS notification, click activates the project tab, address identifies the project, bookmark marker.
- Settings Project group follows the active project tab. No persistence or restore of the project tab set.
- Automated tests: bookmark visibility (one, two or more tabs), label derivation, dialog choices, duplicate-project activation, close with and without a running run, failed write cancels close or replace, closing active and non-active tabs, last tab returns to the picker, event isolation between two projects, background needs-attention dot, notification click activation, no restore after restart.
- Updates to the specifications named in **Specifications this work changes**, including new requirements for the bookmark group and the project tab model, and to any backend specification that holds a single open project.