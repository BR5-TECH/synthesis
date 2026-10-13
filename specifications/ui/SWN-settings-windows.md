# Settings window

**Spec code:** `SWN`

## Intent
Configuration is not authoring, and it does not look like it. Global settings (`GLS-global-settings.md`) and Project settings (`SET-project-settings.md`) are two groups of **one native child window**, titled Settings, rather than tabs of the main viewport or two windows. The surfaces that change how the application behaves are visually separate from the workspace the author works in, and the author finds both scopes in one place. The window is application-modal to the window that opened it, opens in the centre of the active display, and is the only settings window on screen. It keeps the size the author gave it. It writes every change the author made before it goes, so leaving it never asks them to choose between their edits and their exit. This spec owns the window as a window: how it opens, where it opens, what size it has, what it blocks, how it closes, how its two groups are laid out, and where the application menu and the status bar offer it. What each section presents inside its frame belongs to `GLS-global-settings.md` and `SET-project-settings.md`. Out of scope: any persisted window position, and any maximized or full-screen state.

## User stories
- As an author, I want one Settings window so that I do not choose between two windows to find a setting.
- As an author, I want the global and the project sections clearly apart so that I know which scope a change affects.
- As an author, I want the window to keep the size I gave it so that I do not resize it at every opening.
- As an author, I want the window to keep what I typed when I close it, so that a change I made in a section I moved away from is not lost and I am not asked about it.
- As an author, I want Global settings before I have opened a project, so that I can set up the machine first.

## Wireframes
```
┌─ main window ──────────────────────────────────────────────────┐
│                                                                │
│                ┌─ Settings ─────────────────────┐              │
│                │  GLOBAL                        │              │
│                │  Appearance   │                │  default     │
│                │  Navigation   │   section      │  800 × 600   │
│                │  …            │   content      │  minimum     │
│                │  ───────────  │                │  800 × 600   │
│                │  PROJECT      │                │  resizable   │
│                │  Project      │                │              │
│                │  …            │                │              │
│                └────────────────────────────────┘              │
│   parent window blocked while the child window is open         │
│ ⚙                                            LF ▾   +412 −87   │
└────────────────────────────────────────────────────────────────┘
```
- Layout notes: the child window carries the platform's own title bar and frame; nothing of the main window's chrome — no tab strip, no activity bar, no status bar — is drawn inside it. One side nav lists the Global group, a visible divider, and then the Project group, each under its own label (SWN-FR-QXTV). The window is centred on the display its parent occupies (SWN-FR-04). Its outer size is 800 × 600 px by default, never smaller, and the stored size when the author resized it (SWN-FR-03, SWN-FR-MRBK). The parent window stays visible behind it and accepts no interaction until it closes (SWN-FR-02). While no project is open, the Project group and its divider are absent.

## UI contract boundary
- **Owned by the UI**: the one child window and its creation; the parent it is modal to and the blocking of that parent; the default and the minimum size, the refusal to maximize, the drag-resize, the storing of a resize, and the clamping of a stored size; the centring on each opening and the absence of any persisted window position; the one-window rule and the focus of a window already open; the side nav with its two labelled groups, its divider, its keyboard model, and its accessible names; the presentation of Appearance on a request that names no section; the scope-and-section address of every named-section route; the focus moved into the window and returned to the invoking control; the save-before-close sweep, what counts as a pending change, the awaiting of a write already in flight, and the cancelling of a close when a save fails; the inertness of the close action while a sweep runs; the application-menu entry, its label, its platform placement, and its availability at all times; the status bar button; and the window's title.
- **Delegated to backend (abstract)**: none. The window introduces no operation of its own: every read and write a settings section makes is one `GLS-global-settings.md` and `SET-project-settings.md` already delegate, the size of the window is stored by the window's owner in the user-global store (`../core/GSS-global-settings-storage.md` GSS-FR-WSZK) without a frontend operation, and the menu entry introduces no backend command, exactly as the File and Edit menus do not (per `SNV-shell-navigation.md` SNV-FR-23).

## Functional requirements
1. **SWN-FR-01** Global settings and Project settings are two groups of one **native child window** of the application. The window is not a tab of the main viewport (per `TAB-tabs.md` TAB-FR-03), not a surface drawn inside another window, and not a floating overlay of the main window (per `SNV-shell-navigation.md` SNV-FR-56). Opening it closes no overlay.
2. **SWN-FR-02** The window is **application-modal** to its parent. While it is open, the parent renders normally but accepts no pointer, keyboard, or menu interaction. The parent is the **main window** while a project is open, and the **Project picker** while none is (per `PPK-project-picker.md` PPK-FR-15). No project close, project switch, or worktree change can start meanwhile (per `OVW-overview.md` OVW-FR-02, OVW-FR-11, OVW-FR-12).
3. **SWN-FR-03** The window has a default outer size of **800 × 600 px** and a minimum outer size of **800 × 600 px**. The author can drag-resize it to a larger size. It offers no maximize and no OS full-screen.
4. **SWN-FR-04** The window is **centred on the active display** — the display its parent window occupies — each time it opens. It is centred at the size it opens at: the stored size after SWN-FR-CLMP, or the default size when no size is stored. No position of the window is persisted, and a window moved by the author before it closed opens centred again.
5. **SWN-FR-05** At most **one** settings window exists at a time. A request for the window while it is open focuses it and opens no second window.
6. **SWN-FR-06** A request for the window that is **already open** focuses it. The window is not reloaded and its sections keep their state. A request that names a section (SWN-FR-HZWD) presents that section in the window it focuses.
7. **SWN-FR-08** Before the window closes — however the close was requested — it saves **every pending change in every section of both groups**, not only the section on screen. A section holding edits the author never scrolled back to is saved on exactly the terms the visible one is.
8. **SWN-FR-09** A **pending change** is either a section holding a dirty state (per `GLS-global-settings.md` GLS-FR-10 and `SET-project-settings.md` SET-FR-08) or a settings write already **in flight**, such as the Draft template's own write (per `SET-project-settings.md` SET-FR-17). The sweep **awaits** a write in flight and issues no second write for it. The window closes once the write lands.
9. **SWN-FR-10** No discard-unsaved-changes prompt exists for the window. Closing it never asks the author to choose between keeping their edits and leaving, because the close writes them (SWN-FR-08).
10. **SWN-FR-11** A pending save that **fails** cancels the close. The window stays open and presents the failed section, in its group. That section keeps the author's edits unchanged, and shows its own error and its own retry (per `GLS-global-settings.md` GLS-FR-10 and `SET-project-settings.md` SET-FR-08, SET-FR-18). A section saved in the same sweep is not written again.
11. **SWN-FR-12** While a save sweep is running, the close action is **inert**: a second close request and a request for the window each start no further save, duplicate no write already in flight, and open no second window. The sweep either completes and the window closes (SWN-FR-08) or fails and the window stays (SWN-FR-11).
12. **SWN-FR-13** The window opens from three kinds of place: the status bar's **Settings** button (per `STB-status-bar.md` STB-FR-04); the application menu's **Settings** entry (SWN-FR-14); and any surface that routes to a named section. These are the agents roster, which routes to the Project Agents section or to the Personal Agents section (per `AGT-agents.md` AGT-FR-06, AGT-FR-07), a GitHub token failure (per `GIT-git.md` GIT-FR-10), a graduation time limit (per `GRU-graduation-runs.md` GRU-FR-QLRQ), and a notification (per `NTF-notifications.md` NTF-FR-17).
13. **SWN-FR-14** The application menu carries exactly one settings entry, labelled **Settings**. On **macOS** it sits in the **app-name menu**. On **every other platform** it sits in the **File menu**, between Save All and Close Project (per `SNV-shell-navigation.md` SNV-FR-23). Activating it opens the window, or focuses it when it is already open (SWN-FR-06).
14. **SWN-FR-15** The **Settings** entry is available at all times — while a project is open and while the Project picker is the only window (per `PPK-project-picker.md` PPK-FR-15) — because the Global group holds what belongs to the user and the machine rather than to a project.
15. **SWN-FR-16** While no project is open, the **Project group is absent** from the side nav, and is neither present nor disabled. No route presents a Project section in this state. A request that names a Project section is refused and opens nothing.
16. **SWN-FR-17** The window carries the title **Settings**.
17. **SWN-FR-18** The window occupies no place in the main window's tab strip: no settings tab exists, no tab is created, focused, or closed by opening or closing the window, and the never-empty invariant of the strip is untouched (per `TAB-tabs.md` TAB-FR-15). The window takes no needs-attention indication, an indication marking an open tab alone (per `NTF-notifications.md` NTF-FR-27, NTF-FR-28).
18. **SWN-FR-19** Quitting the application writes the pending changes of the open window first, on the terms of SWN-FR-08 and SWN-FR-09. A write that cannot proceed safely cancels the quit and leaves the window open with its failed section presented (SWN-FR-11), exactly as an artifact that cannot be written does (per `SNV-shell-navigation.md` SNV-FR-26).
19. **SWN-FR-QXTV** The window has **one side nav** with two labelled groups. The **Global** group comes first and lists the 12 sections of `GLS-global-settings.md` GLS-FR-03. The **Project** group comes second and lists the 10 sections of `SET-project-settings.md` SET-FR-03. A visible divider separates the groups. Exactly one section is presented at a time. The section content is the content of GLS and SET.
20. **SWN-FR-MRNB** A request that names no section presents the **Appearance** section of the Global group, at every opening, whichever section the window last presented.
21. **SWN-FR-HZWD** Every route that names a section names its **scope** (Global or Project) and its **section**. The window presents that section in that group. A section name that exists in both groups, such as Docker, is therefore never ambiguous. An item within a section, such as one agent's editor, is part of the address (per `AGT-agents.md` AGT-FR-06).
22. **SWN-FR-MRBK** A resize of the window is stored in the user-global persistence (per `../core/GSS-global-settings-storage.md` GSS-FR-WSZK), not per project. The next opening uses the stored size, whatever project is open.
23. **SWN-FR-CLMP** At each opening, the stored size is clamped to the work area of the display the window opens on. When the clamped size is smaller than the minimum size, the window opens at 800 × 600 px. The window has its final size and position before it is interactive.
24. **SWN-FR-KBNV** The side nav works from the keyboard alone: the arrow keys move between sections. The presented section is marked as current for assistive technology. Each group label names its group, so same-name sections are told apart without sight of the divider. Each section row controls one content region. That region is a tab panel. Its accessible name is the group label followed by the section name.
25. **SWN-FR-FCRT** Focus moves into the window when it opens. When it closes, focus returns to the control that invoked it.

## Non-functional requirements
- The window renders, navigates its sections, and saves without network access; a section that reaches the network for its own content says so in its own spec.
- The modality of SWN-FR-02 is the window's own: it is enforced by the platform's window relationship rather than by a scrim, a focus trap, or a listener drawn in the parent, so a parent that is repainted, resized, or moved is blocked throughout.
- The window opens at its final size, centred, before it is interactive, so it is never seen at another size or another position first.
- The save sweep of SWN-FR-08 runs on the sections the window already holds and starts no read of its own, so a close is not delayed by a section reloading what it is about to write.
- The window's frame renders in the current theme and the three typographic roles exactly as the main window does (per `OVW-overview.md` OVW-FR-13, OVW-FR-14); the settings window is a separate window rather than a separately-styled application.
