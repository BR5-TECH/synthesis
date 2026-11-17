# Project windows

**Spec code:** `PWN`

## Intent
The backend registry of open project windows. Each open project has its own OS window, and this module keeps the ordered list of those windows, creates them, activates them, closes them, and publishes the list to every window. It defines the project id, the identity that every command and event serving one project carries, so that the state of one project never reaches another. It owns the needs-attention flag of a window and the macOS tab title. Out of scope: the screens the windows draw, which `../ui/PJT-project-tabs.md` owns; opening, resolving, and storing a project, which `PST-project-storage.md` owns; and any persisted set of open windows, because none is persisted.

## Functional requirements
1. **PWN-FR-MVWL** The **project id** is the project's anchor path (`GSS-global-settings-storage.md` GSS-FR-05): the primary worktree of the owning Git repository, or the project root when the project is outside a Git repository. It is the key of the per-project slot (GSS-FR-18). Every worktree of one repository has one project id.
2. **PWN-FR-EKKX** The module keeps one entry per open project window, in order of opening. An entry holds the project id, the project name, the window label, and the needs-attention flag. The project name is the last segment of the project id. The order changes only when an entry is added or removed.
3. **PWN-FR-TJEN** `list_project_windows()` returns the entries in order. Called with no project window open, it returns an empty list. A project window finds its own entry by its window label.
4. **PWN-FR-NEYF** Each change of the list emits `"project-windows-changed"` to every window of the application. The payload holds the whole list. A change is an entry added, an entry removed, a project replaced in an entry, or a needs-attention flag that changed. A call that changes nothing emits nothing.
5. **PWN-FR-NAQR** `open_project_window(path)` resolves the project id of `path` (PWN-FR-MVWL), creates one project window for it, adds the entry last in the list, and activates the window. It returns the project id, the window label, and whether it created a window. The new window mounts the project itself at startup (`PST-project-storage.md` PST-FR-01): it reads its own entry with `current_project_window()` and opens that project. An error leaves no window and no entry.
6. **PWN-FR-RORD** Two windows never hold one project. When `open_project_window(path)` finds the project id already in the list, it creates no window, mounts nothing, activates the existing window, and returns that entry.
7. **PWN-FR-AYTY** `attach_project_to_window(project_id)` is called by a window after its project is mounted, at the first open in that window and at **This tab**. When the window has an entry, the entry keeps its place in the list and takes the new project id, name, and a cleared flag, and the scope of the project that the window held is dropped. When the window has no entry, the call adds one last in the list. When another window holds `project_id`, the call changes nothing and returns the typed error `project_already_open`. `detach_project_from_window()` removes the entry of the calling window and closes no window. The window then shows the Project picker.
8. **PWN-FR-XUJP** `activate_project_window(project_id)` brings that project's window to the front and gives it OS focus. An unknown project id returns a typed `project_not_open` error. While the Settings window is open, the call focuses the Settings window and changes nothing else.
9. **PWN-FR-BYCL** `request_project_window_close(project_id)` emits `"project-window-close-requested"` with the project id to the window of that project only. It closes nothing itself, because that window must first write its pending changes and ask for confirmation.
10. **PWN-FR-YRXF** `close_project_window(project_id)` stops the runs of the project, tears down only the state of that project (`PST-project-storage.md` PST-FR-14), closes the window, and removes the entry. If the window was in front, the nearest remaining entry in the list is activated. A window not in front leaves the front window as it was.
11. **PWN-FR-QEGI** The first window of the application, with the label `main`, is the Project picker until a project opens in it. It is then a project window. The Project picker is not a window of its own. When `close_project_window` removes the last entry, the window of that entry stays open and shows the Project picker. At launch the list is empty and the first window shows the Project picker.
12. **PWN-FR-CFSW** `set_project_window_attention(project_id, needs_attention)` sets the flag of the entry. A flag that already has that value changes nothing and emits nothing. The flag of a project window that gains OS focus is cleared by the UI through the same call.
13. **PWN-FR-DSLC** The tab title is a pure function `project_window_title(name, needs_attention)`. It returns the full project name, and returns the name prefixed with `● ` (a dot and one space) when the flag is set. It performs no I/O. The title of each project window is set from it on every platform, and each change of the entry sets it again.
14. **PWN-FR-ILXF** On macOS, all project windows are created with one tabbing identifier and join one native tab group (PWN-FR-XGTO). The Project picker window and the Settings window are created with tabbing disabled and never join a group. On Windows and Linux no tab group exists.
14. **PWN-FR-XGTO** On macOS, `open_project_window` adds the new window to the tab group of the calling window, as the tab after it, whatever the system tab preference is. The new window takes the frame of the group. No other window changes its size, position, or full-screen state.
14. **PWN-FR-KQZB** A window that `open_project_window` creates never shows the Project picker or its chrome (`../ui/PPK-project-picker.md` PPK-FR-WCMN), and shows a loading state that names its project while it mounts it. A failed mount shows the error and a **Close this tab** control (`close_project_window`). With no entry, it closes, or shows the Project picker as the only window.
15. **PWN-FR-NXQW** Every command that serves one project takes the `project_id` argument and acts on that project only. A call with a project id that is not open returns a typed `project_not_open` error and changes nothing. No such command reads a "current project" from process state.
16. **PWN-FR-AINZ** Every event that serves one project carries `projectId` in its payload, and is delivered to the window of that project only. An event with no value in its payload becomes an object that holds `projectId` only. Such events are the file watch, scan, worktree context, change set, agent turn, run, progress, and notification events. A window never applies an event whose project id is not its own: the frontend listener drops it.
17. **PWN-FR-XBSI** A project window that is not in front keeps its state mounted: its agent turns, graduation runs, discussions, file watching, and indexing continue. The frontend of such a window is not suspended.
18. **PWN-FR-JFLE** The module persists no list of open windows and no active window. At launch, no project window exists and the Project picker window shows (`../ui/PPK-project-picker.md` PPK-FR-01). The per-project data of `GSS-global-settings-storage.md` GSS-FR-16 and GSS-FR-17 persists as before.
19. **PWN-FR-WVUO** A quit request (`SNV-shell-navigation.md` SNV-FR-26) emits `"menu:exit-requested"` to every project window. Each window writes its pending changes and answers with `finish_exit(proceed)`. When every project window answered with proceed, the application exits. If any window answers that it cannot proceed, the quit is cancelled, nothing exits, and that window is activated. After a cancel, a later answer to the same quit changes nothing. Only a new quit request starts a new round of answers.
20. **PWN-FR-VIYR** While the Settings window is open, the module accepts no activation of another project window. The parent of the Settings window is the project window that opened it, and its Project group edits the project id of that parent (`../ui/SWN-settings-windows.md` SWN-FR-02).
21. **PWN-FR-MMVJ** `open_project_window` and the `resolve_*` commands reject a path that is not a Synthesis project with the typed error of `PST-project-storage.md` PST-FR-01. The caller shows it inline as `../ui/PPK-project-picker.md` PPK-FR-07 requires, and the list does not change.
22. **PWN-FR-ACMV** Each window creation, replacement, activation, close, flag change, and quit request writes one log record through `LGC-logging.md`. A record holds the project name and the window label. It holds no path under the project and no user content.
23. **PWN-FR-SCPE** The state that serves one project is held in a **scope** of the module `project_scope`. A scope holds at most one value of each type. Two scopes never share a value. Dropping a scope releases all its values. The `ProjectState` of the open project is held in a scope.
24. **PWN-FR-PIDA** A Tauri command that serves one project reads the field `projectId` of its invoke payload and uses the scope of that id. A call with no `projectId` uses the **default scope**. The Project picker window uses it, because it holds no project.
25. **PWN-FR-WRAP** The frontend calls the backend only through one `invoke` wrapper (`src/api/invoke.ts`). When the window holds a project id, the wrapper adds `projectId` to the payload of every call. When the window holds none, it sends the payload unchanged.
26. **PWN-FR-CTXT** Code that runs for one project runs in the **context** of that project: the project id. Every command that carries the `project_scoped` attribute sets the context for its whole body. The project id comes from a `Scoped` parameter, or from the `projectId` field of the invoke payload when the command has none. Tasks, threads, and file-watch callbacks that the code starts carry the context on. Code that reads a store of the running project (PWN-FR-ISOL) gets the store of the context. Code that runs with no context gets the one open project, or nothing when two projects are open. It never guesses between two projects.
26. **PWN-FR-UHLU** `close_project` with no `projectId` runs its whole teardown in the default scope (PWN-FR-PIDA), as the context of the default scope. It closes no open project and changes no store of one, also when exactly one project is open. A close that names a project tears down that project alone (`PST-project-storage.md` PST-FR-14).
    - *Why:* With no context, PWN-FR-CTXT gives the one open project, so a close with no id would tear down a project that no caller named.
27. **PWN-FR-ISOL** Every store that serves one project lives in the scope of that project (PWN-FR-SCPE): the file watch, the content tracker, the indexes, the agent turns, the graduation runs, the work streams, the progress registry, the MCP connections, the polling state, and the filesystem access instances: the IDE instance, the agent instances, and the documents instance (`FSA-filesystem-access.md` FSA-FR-TSHC). Two scopes never share a store. An event that serves one project goes to the window of that project only. An event of the whole application goes to every window. Settings, credentials, voice, the log, the menu, and the list of windows are application events. The pre-project filesystem instance (`FSA-filesystem-access.md` FSA-FR-21) is an application value.
28. **PWN-FR-NTFY** A notification request carries the project id of its project. A click on the notification activates the project tab of that project (PWN-FR-XUJP). The activation event then goes to the window of that project only. A project that no window holds activates no tab.
29. **PWN-FR-VRTN** A routine of the whole application has no window and no project of its own. It goes through every open project and runs its body for each project in the context of that project (PWN-FR-CTXT). The routines are the refresh timer of the Dashboard, the steps of a quit that act on a project (the stop of running graduation runs and the end of the draft-asset housekeeping), the stop of the MCP connections at exit, and the save of the Global embeddings settings. The window that answered a quit last does not decide which project a quit step reaches.
30. **PWN-FR-MSET** A scope makes its stores with their defaults, and the setup that runs once at the start of the application reaches the default scope only. When a project scope mounts its content root, it sets up its own stores: the manager of the MCP connections gets the observer that emits the status event and writes the log, and the embeddings service takes the Global embeddings record before it mounts the project.
31. **PWN-FR-WSTA** A worker thread that the application shares between projects, such as the writer of the statistics logs, carries the context of the project that queued each batch. An event that the batch emits goes to the window of that project only.

## Contract surface
All operations below are Tauri commands. The quoted name is the canonical operation name used in `../ui/PJT-project-tabs.md`. The snake_case identifier is the Rust function name.

### Tauri commands
- `"list project windows"` → `list_project_windows()` → `[ProjectWindow]`, in order of opening.
- `"open project window"` → `open_project_window(path)` → `{ project_id, window_label, created }`. `created` is false when the project was already open.
- `"current project window"` → `current_project_window()` → the entry of the calling window, or nothing.
- `"attach project to window"` → `attach_project_to_window(project_id)` → nothing, or `project_already_open`.
- `"detach project from window"` → `detach_project_from_window()` → nothing.
- `"resolve project at path"` → `resolve_project_at_path(path)`, `"resolve project from git url"` → `resolve_project_from_git_url(url)`, and `"resolve new project"` → `resolve_new_project(name, mode, target_path)` → the project handle. They mount nothing, so the choice of **This tab** or **New tab** comes before any mount.
- `"activate project window"` → `activate_project_window(project_id)` → nothing, or `project_not_open`.
- `"request project window close"` → `request_project_window_close(project_id)` → nothing, or `project_not_open`.
- `"close project window"` → `close_project_window(project_id)` → nothing, or `project_not_open`.
- `"set project window attention"` → `set_project_window_attention(project_id, needs_attention)` → nothing, or `project_not_open`.

### Events (Tauri event bus)
- `"project-windows-changed"` — emitted to every window. Payload: `{ windows: [ProjectWindow] }`.
- `"project-window-close-requested"` — emitted to one project window. Payload: `{ project_id }`.

### Payload shapes
```
ProjectWindow {
  project_id,        // the anchor path of PWN-FR-MVWL
  name,              // the full project name
  window_label,      // the label of the OS window
  needs_attention    // boolean
}
```

### Internal (Rust API)
- `project_window_title(name, needs_attention)` — the pure tab title function of PWN-FR-DSLC. It is no Tauri command.

## Non-functional requirements
- The list is held in memory. Nothing of it is written to disk.
- A change of the list reaches every window without a polling loop.
- The title function runs on every platform, so tests verify it on every platform.
