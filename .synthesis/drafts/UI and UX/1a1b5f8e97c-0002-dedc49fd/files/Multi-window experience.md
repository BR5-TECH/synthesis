## Intent

The user can have multiple projects open at once as project tabs, aligned with the macOS multi-tab experience. Each project tab is one OS window. On macOS the windows group as native window tabs. On Windows and Linux an in-app tab strip lists the project windows.

## User journey

1. The user opens project A. The window looks as it does today. No project tab strip shows.
2. The user opens the project switcher and selects project B. A dialog asks: **This tab** or **New tab**.
   - **This tab**: project B replaces project A in the current project tab (same teardown as today).
   - **New tab**: project B opens in a new project tab (a new window). Project A stays open and keeps its state.
3. Two project tabs now exist. On macOS the native tab bar shows. On Windows and Linux an in-app tab strip shows in the top chrome. Each tab shows the full project name. The user clicks a tab to switch project. Each project keeps its own file tabs, panels, editor state, conversations and runs. Nothing from one project shows in another.
4. A project in the background keeps working. When something needs the user (agent turn ends, graduation run needs input), its tab shows a needs-attention dot (on macOS: a dot before the tab title), and an OS notification names the project. No toast shows for a background project. A click on the notification activates that project tab.
5. The user closes a project tab with its × (Windows and Linux), with **Close Project**, or with ⌘W (Ctrl+W on Windows and Linux). When a run is active, a confirmation appears first.
6. The user closes the last project tab. The Project picker shows.
7. The user quits and starts the app again. The Project picker shows. No project tab returns.
8. The user presses ⌘T (Ctrl+T on Windows and Linux). The Project picker opens as a modal over the window. The user selects a project. The **This tab** / **New tab** dialog shows.

## Requirements

- **Project tab**: one open project with its own isolated workspace (file tabs, panels, editor state and undo history, conversations, worktree selection, runs, file watching, indexing). One project is one project tab and one OS window. No limit on the number of project tabs. Do not confuse with file tabs (TAB) in the main viewport.
- **Window model**: each project tab is its own OS window on every platform. The Project picker window and the Settings window never join a tab group.
- **macOS tabs**: project windows share one native tab group. The system draws the tab bar. It hides with one project open and shows with two or more. Each tab title is the full project name. A tab that needs attention has its title prefixed with a dot (●). The app draws no in-app strip on macOS.
- **Windows and Linux tabs**: each project window draws an in-app tab strip in the top chrome, next to the agent selector and theme selector.
  - Hide it while one project is open.
  - Every window lists all open project tabs in the same order (order of opening).
  - Each tab shows the full project name (also its accessible name), a clear active marking, a × to close, and a needs-attention dot.
  - A click on a tab focuses that project's window.
  - The strip scrolls horizontally and keeps the active tab in view. It must not push the agent or theme selector out.
  - Use tablist/tab roles and full keyboard support.
- **Commands** (menu items and shortcuts): **New Tab** ⌘T (Ctrl+T) opens the Project picker modal, then the dialog. **Close Project** ⌘W (Ctrl+W) closes the active project tab. Next and previous tab: Ctrl+Tab and Ctrl+Shift+Tab. On macOS the Window menu also holds the system commands Show/Hide Tab Bar, Move Tab to New Window and Merge All Windows. Remove any existing binding of ⌘T and ⌘W that conflicts.
- **Switcher** (SNV-FR-17..22): keep the control, list and ordering. Selecting an entry other than the current project opens the dialog **This tab** / **New tab**.
  - **This tab**: replace the current project in the active project tab. Teardown follows OVW-FR-11, including the write of pending changes first and cancel on a failed write.
  - **New tab**: open the project in a new project window and activate it (macOS: as a native tab next to the current one).
  - If the project is already open in a tab, do not open it twice: activate that tab (for both choices). The current project stays non-actionable.
- **Open Another Project…**: opens the existing Project picker as a **modal over the main window**. Reuse the picker implementation; do not build a second one. All open project tabs stay alive. The user dismisses it without a change. When the user opens a project in it, show the same dialog (**This tab** / **New tab**). On success the picker closes.
- **Close**: the tab's ×, **Close Project** (SNV-FR-25) and ⌘W/Ctrl+W close one project tab only.
  - If the tab has a running agent turn or graduation run, ask for confirmation. On confirm, stop its runs, then close.
  - Write pending changes first. A write that cannot proceed safely cancels the close (EDT-FR-32).
  - Close tears down only that tab's state, watcher and retained edit state.
  - Closing a non-active tab keeps the active tab. Closing the active tab activates the nearest remaining tab (macOS: the system's choice).
  - Closing the last tab, or Close Project on the only project, returns to the Project picker as today.
- **Background work**: a project tab that is not in front keeps running (agent turns, graduation runs, discussions, file watching, indexing). Its tab shows the needs-attention marker per NTF (dot on the in-app strip, dot before the title on macOS).
- **Notifications**: every toast and OS notification states the project name. A click on an OS notification activates the project tab, then opens the address as today (NTF-FR-17). The address must identify the project.
  - A raise from a project window that is not in front shows no toast, in that window or in any other. It sets the needs-attention marker and posts an OS notification, subject to the enable switch and the OS permission (NTF-FR-11).
  - The "in front" test of NTF-FR-08 is per project window: a raise is suppressed only when its own project window has OS focus and its address surface is in front. Focus on another project window does not suppress it.
  - A toast shows only in the window of its own project. No toast moves between windows.
  - When the user activates that project tab, the marker clears per NTF-FR-14 and NTF-FR-32.
- **Settings**: stay one Settings child window. It stays application-modal: while it is open, every project window accepts no interaction. Its parent is the project window that opened it. The Project group edits that window's project.
- **Quit**: ⌘Q and Exit (SNV-FR-26) write pending changes of every project window first. A blocked write cancels the quit and focuses that window.
- **Restart**: do not persist or restore the set of project tabs or the active tab. The app starts at the Project picker (PPK-FR-01). Per-project data (active worktree, recent list) persists as today.
- **Isolation**: every backend operation and event that serves one project (file watch, scan, worktree context, agent turns, runs, notifications) must be scoped to its project and its window. No event from one project may update another project's UI. Two windows never hold the same project.
- **Specifications this work changes**: `OVW-overview.md` (OVW-FR-02, OVW-FR-11, OVW-FR-12 wording on one project and teardown), `SNV-shell-navigation.md` (SNV-FR-17..22, SNV-FR-23 and FR-25 Close Project, SNV-FR-26 quit, SNV-FR-27 full-screen exit, plus new requirements for the Windows/Linux tab strip, the commands and the macOS native tab group), `PPK-project-picker.md` (PPK-FR-01, FR-06, FR-09, FR-12: picker as modal over the main window), `NTF-notifications.md` (project name, address, tab marker, no toast for a background project; NTF-FR-08, NTF-FR-15, NTF-FR-17, NTF-FR-QGSV), `SWN-settings-windows.md` (SWN-FR-QXTV, Project group scope), `TAB-tabs.md` (tabs belong to a project tab), `../core/PST-project-storage.md` (PST-FR-14), `../core/WTC-worktree-context.md` (WTC-FR-18, WTC-FR-20) and any backend spec that holds a single open project.

## Expected deliverables

- Project tab model in the main window: several open projects, one active, each with its own isolated workspace state (file tabs, panels, editor state, conversations, worktree selection, runs). No state is shared between project tabs.
- One OS window per project. macOS: native tab group (tabbing identifier), tab title is the project name, dot prefix for needs-attention. Windows and Linux: in-app tab strip in the top chrome, shown from two open projects, full-name labels, active marker, ×, needs-attention dot, horizontal scroll that keeps the active tab in view, keyboard and screen-reader support.
- Menu items and shortcuts: New Tab, Close Project, next and previous tab, with the system Window-menu tab commands on macOS.
- Switcher dialog **This tab** / **New tab**, reused by the **Open Another Project…** picker modal and by New Tab. The existing Project picker implementation is reused, not duplicated.
- Close flow for one project tab: ×, **Close Project** and ⌘W/Ctrl+W, confirmation when a run is active, write of pending changes first, teardown of that tab only, nearest-tab activation, return to the Project picker on the last tab.
- Backend scoping by project for file watching, scanning, indexing, worktree context, agent turns, runs and notifications. Every event carries its project, and the UI routes it to that project tab only.
- Notifications: project name in every toast and OS notification, no toast for a background project, per-window focus test for OS notifications, click on an OS notification activates the project tab, address identifies the project, tab marker.
- Settings Project group follows the active project tab. No persistence or restore of the project tab set.
- Automated tests: tab strip visibility (one, two or more tabs), macOS title with dot prefix, dialog choices, New Tab and Close shortcuts, duplicate-project activation, close with and without a running run, failed write cancels close or replace, closing active and non-active tabs, last tab returns to the picker, event isolation between two projects, background needs-attention dot, notification click activation, no restore after restart.
- Updates to the specifications named in **Specifications this work changes**, including new requirements for the bookmark group and the project tab model, and to any backend specification that holds a single open project.