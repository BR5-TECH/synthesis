# Project tabs

**Spec code:** `PJT`

## Intent
The user keeps several projects open at once, each as a **project tab**. Each project tab is one OS window with its own workspace. On macOS the project windows group as native window tabs. On Windows and Linux each window draws an in-app tab strip that lists them. This spec owns the project tab model, both tab bars, the New Tab, Close Project, and tab-switch commands, the This tab / New tab dialog, the close flow, and the quit flow. Out of scope: file tabs of the main viewport (`TAB-tabs.md`), the content of a project window, and any saved set of open project tabs, because none is saved.

## Functional requirements
1. **PJT-FR-HWWO** A **project tab** is one open project with its own isolated workspace: file tabs, panels, editor state and undo history, conversations, worktree selection, runs, file watching, and indexing. A project tab is not a file tab. Where another spec says "the main window", it means the project window of the project it concerns.
2. **PJT-FR-IAQN** One project is one project tab and one OS window. A project window is the main window of its project (`OVW-overview.md` OVW-FR-03). The number of project tabs has no limit. Two windows never hold one project.
3. **PJT-FR-VXQF** The Project picker window and the Settings window are not project tabs. On every platform they never join a tab group, and they appear in no tab strip.
4. **PJT-FR-INEP** A project window applies only the events whose `project_id` is its own, and only the replies to its own commands (`../core/PWN-project-windows.md` PWN-FR-AINZ). An event from project A, delivered while the user works in project B, changes nothing in B. Two windows with the same file path show their own files.
5. **PJT-FR-WVLP** The user switches from project tab A to project tab B and back. Each project keeps its own file tabs, active tab, panel state, editor content, undo history, conversations, and runs. Nothing of one project shows in the other.
6. **PJT-FR-PBQT** On macOS, project windows share one native tab group. The system draws the tab bar. The application draws no in-app project tab strip on macOS (`../core/PWN-project-windows.md` PWN-FR-ILXF).
7. **PJT-FR-WZLQ** On macOS, the native tab bar is hidden while one project is open and shows while two or more are open. The Window menu holds the system commands Show Tab Bar or Hide Tab Bar, Move Tab to New Window, and Merge All Windows.
8. **PJT-FR-UQSG** On macOS, the title of each tab is the full project name. A tab with the needs-attention flag has the title `● <name>`. The title comes from `project_window_title` (`../core/PWN-project-windows.md` PWN-FR-DSLC). A test of the function runs on every platform.
9. **PJT-FR-UHOC** On Windows and Linux, each project window draws an in-app **project tab strip** in the top chrome. The strip is not rendered while one project is open. It is rendered in every window while two or more are open. Closing back to one project removes it.
10. **PJT-FR-XWPP** Every strip lists all open project tabs in the order of opening. Each tab shows the full project name, which is also its accessible name, a clear marking when it is the tab of this window, a close control (x), and the needs-attention dot. A tooltip shows the project id.
11. **PJT-FR-JEGM** A click on a tab, or Enter or Space on a focused tab, invokes `"activate project window"` for that project. The window of that project comes to the front. The strip of the clicked window does not change its own project.
12. **PJT-FR-VRRI** The strip is placed in the top chrome at the leading edge of the trailing cluster, after the universal search bar (`SNV-shell-navigation.md` SNV-FR-LRGV). It scrolls horizontally when the tabs do not fit, and keeps the tab of its window in view. It never pushes the agents control or the theme selector out of the top chrome.
13. **PJT-FR-LGBV** The strip has `role="tablist"` and each tab has `role="tab"` with `aria-selected`. Arrow Left and Arrow Right move focus between tabs without wrapping. Home and End move focus to the first and last tab. Enter or Space activates the focused tab. Delete closes the focused tab. Focus is always visible.
14. **PJT-FR-UXCT** A tab with the needs-attention flag shows a dot before its name, and carries an accessible needs-attention state. The dot comes from the `needs_attention` field of the list (`"project-windows-changed"`). The strip decides nothing about it. A project window that is not in front shows the dot in every strip.
15. **PJT-FR-PILG** **New Tab** opens the Project picker as a modal over the project window (PJT-FR-IOEV). It is a File menu item with the accelerator Cmd+T on macOS and Ctrl+T on Windows and Linux. It is available while a project window is focused. On every platform the accelerator opens the modal, and opens no second modal.
16. **PJT-FR-PCCL** **Close Project** closes the project tab of the focused window (PJT-FR-OJZG). Its accelerator is Cmd+W on macOS and Ctrl+W on Windows and Linux. The close control of the OS window and the x of a strip tab run the same flow. Closing one tab closes no other tab.
17. **PJT-FR-EKKA** **Next Tab** and **Previous Tab** are Window menu items with the accelerators Ctrl+Tab and Ctrl+Shift+Tab on every platform. They activate the next and the previous entry of the list, and wrap at the ends. With one project open they do nothing.
18. **PJT-FR-JGHF** The accelerators of PJT-FR-PILG, PJT-FR-PCCL, and PJT-FR-EKKA replace every other binding of Cmd+T, Cmd+W, Ctrl+T, Ctrl+W, and Ctrl+Tab. While a floating overlay is mounted (`SNV-shell-navigation.md` SNV-FR-56), the overlay holds the keyboard and these accelerators do nothing.
19. **PJT-FR-GASB** Selecting a project other than the current one in the project switcher shows the **open project dialog** with the choices **This tab** and **New tab**, and **Cancel**. Escape cancels. Cancel changes nothing. The dialog names the project. The first focus is on New tab.
20. **PJT-FR-ERZZ** **This tab** replaces the project in the project tab of the window. The window writes its pending changes, discards its file tabs, panel state, retained edit state, and conversation presentations, tears down the project (`"close project"`), mounts the new project, and invokes `"attach project to window"` (`OVW-overview.md` OVW-FR-11). A blocked write cancels the replacement and leaves the project open.
21. **PJT-FR-JLHP** **New tab** invokes `"open project window"` for the project. The new window opens as a fresh open (`OVW-overview.md` OVW-FR-04, OVW-FR-05) and is activated. The project of the current window stays open and keeps its state. On macOS the new window joins the native tab group next to the current tab.
22. **PJT-FR-ZRZD** When the selected project is already open in a project tab, neither choice opens it a second time. Both This tab and New tab activate the existing tab, and the current window keeps its project and its state. The current project stays non-actionable in the switcher (`SNV-shell-navigation.md` SNV-FR-20).
23. **PJT-FR-IOEV** **Open Another Project…** of the switcher (`SNV-shell-navigation.md` SNV-FR-22) and New Tab open the existing Project picker as a modal over the project window (`PPK-project-picker.md` PPK-FR-GMYH). All project tabs stay open. When the user opens a project in the modal, the open project dialog shows over it (PJT-FR-GASB). After a successful choice, the modal closes.
24. **PJT-FR-OJZG** Closing a project tab with a running agent turn or graduation run first shows a confirmation that names the project. Other operations in flight, such as indexing, embeddings jobs, searches, and stream updates, do not count as a run. Cancel keeps the tab open and stops nothing. Confirm stops the runs of that project, then closes it. A tab with no active run closes with no confirmation.
25. **PJT-FR-PEIJ** Before a project tab closes, its window writes its pending changes (`EDT-editor.md` EDT-FR-33, `FLO-flow.md` FLO-FR-29). A write that cannot proceed safely cancels the close, leaves the project open, and runs no stop. Only after the writes does the window invoke `"close project window"`.
26. **PJT-FR-MWLY** Closing a tab that is not the front tab leaves the front tab as it is. Closing the front tab activates the nearest remaining tab in the order of opening, and on macOS the choice of the system. A close that needs a confirmation first activates its tab, so the dialog is in front.
27. **PJT-FR-JSAW** Closing the last project tab, and Close Project on the only project, show the Project picker (`OVW-overview.md` OVW-FR-02). The closing window leaves OS full-screen first (`SNV-shell-navigation.md` SNV-FR-27). No project state of the closed projects stays mounted.
28. **PJT-FR-YVWM** A project tab that is not in front keeps its agent turns, graduation runs, discussions, file watching, and indexing. When something needs the user, its tab shows the needs-attention dot, and an OS notification names the project (`NTF-notifications.md` NTF-FR-ZTGX). No toast shows in any window. The dot clears when the user activates that tab.
29. **PJT-FR-BQTD** The Settings window stays one window and stays application-modal. While it is open, every project window accepts no pointer, keyboard, or menu interaction, and no strip tab or menu item activates another project window. Its parent is the project window that opened it (`SWN-settings-windows.md` SWN-FR-02).
30. **PJT-FR-VLBY** Cmd+Q on macOS and Exit (`SNV-shell-navigation.md` SNV-FR-26) start the quit that `"menu:exit-requested"` announces to every project window. Every project window writes its pending changes and answers with `"finish exit"`. A blocked write cancels the quit, and the window with the blocked write comes to the front.
31. **PJT-FR-YMYN** The application saves no list of project tabs and no active tab. After a restart, the Project picker shows and no project tab returns. The recent list and the active worktree of each project persist as before (`../core/PWN-project-windows.md` PWN-FR-JFLE).
32. **PJT-FR-YCNT** The open project dialog has `role="dialog"` and an accessible name that includes the project name. Tab and Shift+Tab move focus between its three controls inside the dialog. Enter activates the focused control. Focus returns to the control that opened the dialog when it closes.

## User stories
- As a user, I want to open a second project in a new tab so that the first project keeps its files and runs.
- As a user, I want a dot on a project tab when its agent needs me so that I can work in another project without missing it.
- As a user, I want one shortcut to close a project tab so that I do not hunt for a close control.

## Wireframes
The strip on Windows and Linux with three projects open, `acme` being the tab of this window and `design-skills` needing attention:

```
┌──────────────────────────────────────────────────────────────────────────────────────┐
│ ◈ acme ▾  main ▾ ⟳  [ search…        ] [ acme ×│● design-skills ×│ flows × ] ✦ 2  ☾ ▾ │
├──┬───────────────────────────────────────────────────────────────────────────────────┤
```

The open project dialog:

```
        ┌────────────────────────────────────────────┐
        │ Open design-skills                         │
        │                                            │
        │ [ This tab ]   [ New tab ]   [ Cancel ]    │
        └────────────────────────────────────────────┘
```

- Layout notes: the strip sits between the search bar and the trailing cluster. It is absent with one project open. It scrolls inside its own region and never moves the trailing cluster. On macOS the system tab bar replaces it.

## UI contract boundary
- **Owned by the UI**: the strip and its tabs, roles, keyboard, scrolling, and dots; the open project dialog; the close confirmation; the Project picker modal; the choice of This tab or New tab; the write of pending changes before a replace, a close, and a quit; the File and Window menu items and their accelerators.
- **Delegated to backend (abstract)**:
  - `"list project windows"` (`list_project_windows`), `"open project window"` (`open_project_window`), `"current project window"` (`current_project_window`), `"attach project to window"` (`attach_project_to_window`), `"detach project from window"` (`detach_project_from_window`), `"resolve project at path"` (`resolve_project_at_path`), `"resolve project from git url"` (`resolve_project_from_git_url`), `"resolve new project"` (`resolve_new_project`), `"activate project window"` (`activate_project_window`), `"request project window close"` (`request_project_window_close`), `"close project window"` (`close_project_window`), `"set project window attention"` (`set_project_window_attention`), and `"finish exit"` (`finish_exit`), all owned by `../core/PWN-project-windows.md`.
  - The events `"project-windows-changed"` and `"project-window-close-requested"`, owned by `../core/PWN-project-windows.md`.

## Non-functional requirements
- The strip updates within one frame of a `"project-windows-changed"` event.
- A project window that is not in front is not throttled by the application.
- The strip and the dialog stay usable at the minimum window size of `SNV-shell-navigation.md`.
