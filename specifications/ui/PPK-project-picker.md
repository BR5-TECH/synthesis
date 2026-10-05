# Project picker

**Spec code:** `PPK`

## Intent
The pre-project entry surface, shown before any project is open. The only surface that exists before the main window opens. Lets the user reopen a recent project, open one from disk, open one from a Git URL, or create a new one (standalone or co-located).

## User stories
- As a returning user, I want to reopen a recent project with one click so that I do not have to navigate the filesystem each time.
- As a new user, I want to create a new project either as a standalone Synthesis project or co-located inside an existing code repo, so that I can adopt Synthesis without disrupting my repository layout.
- As a collaborator, I want to open a project directly from a Git URL so that I can join a teammate's project without cloning it manually first.

## Wireframes

### Default state (with recent projects)
```
┌──────────────────────────────────────────────────────────────────┐
│  Synthesis                                                       │
│                                                                  │
│  ┌────────────────────────────┐  ┌────────────────────────────┐  │
│  │ Recent projects            │  │ Open                       │  │
│  │                            │  │                            │  │
│  │  ▸ acme-platform           │  │  [ Browse folder... ]      │  │
│  │    ~/dev/acme-platform     │  │                            │  │
│  │    last opened 2d ago      │  │  ── or ──                  │  │
│  │                            │  │                            │  │
│  │  ▸ design-skills           │  │  Open from Git URL         │  │
│  │    ~/dev/design-skills     │  │  ┌──────────────────────┐  │  │
│  │    last opened 5d ago      │  │  │ git@github.com:…     │  │  │
│  │                            │  │  └──────────────────────┘  │  │
│  │  ▸ flow-prototypes         │  │            [ Open ]        │  │
│  │    last opened 3w ago      │  │                            │  │
│  │                            │  └────────────────────────────┘  │
│  │                            │                                  │
│  │                            │  ┌────────────────────────────┐  │
│  │                            │  │ Create new project         │  │
│  │                            │  │                            │  │
│  │                            │  │  Name                      │  │
│  │                            │  │  ┌──────────────────────┐  │  │
│  │                            │  │  │                      │  │  │
│  │                            │  │  └──────────────────────┘  │  │
│  │                            │  │                            │  │
│  │                            │  │  Mode                      │  │
│  │                            │  │  ( ) Standalone            │  │
│  │                            │  │  ( ) Co-located            │  │
│  │                            │  │                            │  │
│  │                            │  │  Target folder             │  │
│  │                            │  │  [ Browse folder... ]      │  │
│  │                            │  │                            │  │
│  │                            │  │           [ Create ]       │  │
│  └────────────────────────────┘  └────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

### Empty recent list state
```
┌──────────────────────────────────────────────────────────────────┐
│  Synthesis                                                       │
│                                                                  │
│  ┌────────────────────────────┐  ┌────────────────────────────┐  │
│  │ Recent projects            │  │ Open                       │  │
│  │                            │  │ Create new project         │  │
│  │  No projects yet.          │  │                            │  │
│  │  Open one or create your   │  │ (same panel as above)      │  │
│  │  first project →           │  │                            │  │
│  │                            │  │                            │  │
│  └────────────────────────────┘  └────────────────────────────┘  │
```

### Error attachment (inline, per failing action)
```
  Open from Git URL
  ┌──────────────────────┐
  │ not-a-url            │
  └──────────────────────┘
            [ Open ]
  ✗ URL is not parsable                ← inline error, scoped to the action
```

Layout notes for the generator:
- Two-column split: left column hosts the recent list, right column stacks Open and Create new project panels.
- Recent list rows are clickable; each row shows project name, full path, last-opened timestamp.
- Mode selector is a single radio group (exactly one of Standalone / Co-located).
- Errors render inline directly below the action that produced them; they never replace the panel.
- Window is the only surface visible — no main shell, no activity bar, no tabs. The Global settings child window may be opened over it from the application menu, in which case the picker is blocked behind it until it closes (PPK-FR-15). The About panel may be opened over it from the application menu, in which case the picker is blocked behind the panel until it closes (PPK-FR-EWQH).
- The picker window has a fixed outer size, is not user-resizable, and is never presented in OS full-screen.
- The right column is sized to fit its panels (Open, Create new project) at their natural dimensions; it never scrolls horizontally or vertically.
- The left column (Recent projects) absorbs all remaining width within the fixed window. The recent list inside it is the only region in the picker permitted to scroll, and only vertically when the list overflows.
- On every launch, the picker window is positioned at the center of the active display (the display containing the mouse cursor at launch time) before it becomes interactive. The picker window's position is never persisted across launches.

## UI contract boundary
- **Owned by the UI**: layout of the picker, recent project list rendering, input fields (folder path, Git URL, new-project name, mode selector), client-side validation of input shape, "open" / "create" button enablement, the About panel's blocking of the picker while it is open (PPK-FR-EWQH).
- **Delegated to backend (abstract)**: "list recent projects", "open project at path", "open project from git url", "create project (mode = standalone | co-located, target path)", "browse for folder". Stubs may return canned data and accept inputs without side effects during the walking skeleton.

## Functional requirements
1. **PPK-FR-01** The Project picker is the application's startup surface and is rendered until a project is successfully opened.
2. **PPK-FR-02** The picker shows a Recent projects list; selecting an entry invokes "open project at path" with that entry.
3. **PPK-FR-RQZV** When the recent-projects list is empty, the picker shows an empty-state message in place of the list, and the Browse, Git URL, and Create actions stay usable.
4. **PPK-FR-03** The picker provides a "Browse for folder" action that invokes "browse for folder" and then "open project at path".
5. **PPK-FR-04** The picker provides a "Open from Git URL" input; submitting a non-empty URL invokes "open project from git url".
6. **PPK-FR-05** The picker provides a "Create new project" flow with two creation modes: standalone and co-located. Mode is chosen at creation time and cannot be changed from the picker afterward.
7. **PPK-FR-06** When the user successfully opens or creates a project, the picker is unmounted and the main window is shown (per `OVW-overview.md` OVW-FR-02 / OVW-FR-04 / OVW-FR-05).
8. **PPK-FR-07** When an open or create operation fails, the picker remains visible and renders an error message attached to the action that failed; no main window is shown.
9. **PPK-FR-08** Inputs are validated client-side for shape (non-empty path, parsable URL) before the corresponding backend operation is invoked.
10. **PPK-FR-09** The picker is presented in a window of fixed outer dimensions and is not user-resizable (no drag-to-resize, no maximize, no OS full-screen).
11. **PPK-FR-10** The right column (Open panel + Create new project panel) is sized to fit its content at natural dimensions and renders no scrollbars — neither vertical nor horizontal — at any window state.
12. **PPK-FR-11** The left column (Recent projects) occupies all width that the fixed window leaves after the right column; when the recent list contains more entries than fit the available height, the list scrolls vertically within the left column. No other region of the picker scrolls.
13. **PPK-FR-12** On every application launch, the Project picker window is positioned at the center of the active display (the display containing the mouse cursor at launch) before it becomes interactive. The picker window's prior position is not persisted and not restored; the centering computation is performed fresh on each launch.
14. **PPK-FR-13** The recent list reflects the ordering and pruning of "list recent projects": pinned entries appear first, and entries whose project no longer exists are not shown (unpinned) or shown flagged as missing (pinned). Management of the list (remove, clear, pin) is performed in Global settings (`GLS-global-settings.md` GLS-FR-07), not in the picker; the picker remains view-and-open only.
15. **PPK-FR-14** The user-global full-screen preference (`../core/GSS-global-settings-storage.md` GSS-FR-19) governs the main window only. The picker neither reads nor writes it and is presented as a normal fixed-size window whatever its value; the preference is applied when the main window mounts (`SNV-shell-navigation.md` SNV-FR-39 / SNV-FR-40), and a main window that left full-screen to show the picker leaves the stored value set (`SNV-shell-navigation.md` SNV-FR-27).
16. **PPK-FR-15** While the picker is the only window, the application menu offers **Global settings**, omits **Project settings** rather than greying it (per `SWN-settings-windows.md` SWN-FR-15, SWN-FR-16), the only route to settings here. Global settings opens application-modal to the picker, centred on its display (SWN-FR-02, SWN-FR-03, SWN-FR-04). The picker renders behind it, takes no interaction, and is unchanged when it closes.
   - *Why:* Project settings is absent rather than disabled because no project is open.
17. **PPK-FR-EWQH** While the picker is the only window, the application menu also offers **About** (per `ABT-about-panel.md` ABT-FR-KMVD), in the position `SNV-shell-navigation.md` SNV-FR-23 gives it. The About panel opens modal over the picker and blocks it. The picker renders behind it, takes no interaction, and is unchanged when the panel closes (per `ABT-about-panel.md` ABT-FR-QZHW, ABT-FR-ZEJM).

## Non-functional requirements
- The recent project list is local-only in v1 (no cross-machine sync).
- The picker renders and accepts input without requiring network connectivity.
