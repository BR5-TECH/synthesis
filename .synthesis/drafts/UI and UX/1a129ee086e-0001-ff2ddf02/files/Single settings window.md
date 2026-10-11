## Intent

Having 2 separate settings window: Global settings and Project settings isn’t convenient.

It’ll be better to have a unified window, and both will be present there.

## User journey

- User activates the one Settings button in the status bar, or the one Settings entry in the application menu. The Settings window opens on the first Global section (Appearance), every time.
- User sees one side nav with two labelled groups: Global first, then Project. Each group lists its own sections. Sections that exist in both scopes (Plugins, Agents, Docker, GitHub, adapters) appear once in each group.
- User sees clear visual boundaries between global and project level items.
- While no project is open (Project picker only), the user sees the Global group only. The Project group is absent, not disabled.
- User resizes the window. The next time it opens, it has that size.
- User closes the window. Pending changes are saved first, as today.

## Requirements

- `frontend-design` skill must be used for UI parts of the task.
- Look and feel of the new unified settings window must stay algined with the whole application.
- **One window.** Global settings and Project settings become one native child window titled **Settings**. It is application-modal to its parent, as today. There is at most one settings window on screen. A request while it is open focuses it and opens no second one.
- **Layout.** One side nav with two labelled groups: **Global** (its 12 sections), then **Project** (its 10 sections). Exactly one section is presented at a time. The group labels and a visible divider make the boundary clear. Sections with the same name in both groups are told apart by their group. The section content does not change.
- **Opening.** A request with no named section presents Appearance. A request that names a section presents that section, whether the window was closed or already open. Every named-section route (agents roster, GitHub token failure, graduation time limit, notification address) must name the scope and the section. Opening the window never needs a second window to close first.
- **No project open.** The Project group is absent. No route can present a Project section in this state.
- **Entry points.** The status bar's two settings buttons become one **Settings** button. The application menu's two entries become one **Settings** entry. On macOS it sits in the app-name menu. On every other platform it sits in the File menu, between Save All and Close Project. The entry is available at all times, including in the Project picker.
- **Size.** The window has a default size of 800 × 600 px and a minimum size of 800 × 600 px. The user can drag-resize it. Maximize and OS full-screen are not allowed. The window is centred on the parent's display at each opening. Its position is not persisted.
- **Persisted size.** A resize is stored in the user-global (machine-local) persistence, not per project. On open, the stored size is clamped to the current display's work area. If it cannot fit, the window falls back to 800 × 600. The window is at its final size and position before it is interactive.
- **Saving.** One save-before-close sweep covers every section of both groups, not only the section on screen. Every rule on dirty state, in-flight writes, no discard prompt, failed-save cancel (the failed section is presented), inert close during a sweep, and quit stays as it is.
- **Ownership.** The SWN spec owns the window: title, size, opening, modality, menu and status bar entries, save sweep. The GLS and SET specs keep section content only. The specs are not merged.
- **Keyboard and accessibility.** The side nav works from the keyboard alone: arrow keys move between sections, and the presented section is marked as current for assistive technology. Each group label names its group, so same-name sections are told apart without sight of the divider. Focus moves into the window when it opens and returns to the invoking control when it closes.

## Specifications to update

- `specifications/ui/SWN-settings-windows.md`: one window titled Settings; replace SWN-FR-03 (fixed size) with default, minimum and persisted size with clamping; update SWN-FR-01, SWN-FR-02 (parent is the main window or the Project picker), SWN-FR-04, SWN-FR-05, SWN-FR-06 (a focused window presents a named section), SWN-FR-13 (one status bar button, section routes name scope), SWN-FR-14 to SWN-FR-17 (one entry, always available, one title); remove the window-switch order of SWN-FR-07 and the switch wording of SWN-FR-11, SWN-FR-12 and SWN-FR-19 (no switch exists any more); update intent, wireframe and UI contract boundary.
- `specifications/ui/GLS-global-settings.md` and `specifications/ui/SET-project-settings.md`: remove window rules (GLS-FR-01, GLS-FR-02, GLS-FR-11; SET-FR-01, SET-FR-02, SET-FR-09); keep sections as groups of the Settings window; keep the dirty-state rules.
- `specifications/core/GSS-global-settings-storage.md`: persist the Settings window size in user-global storage.
- `specifications/ui/STB-status-bar.md` (STB-FR-04), `specifications/ui/SNV-shell-navigation.md` (SNV-FR-03, SNV-FR-23), `specifications/ui/PPK-project-picker.md` (PPK-FR-15, PPK-project-picker layout note): one Settings button and entry; Project settings absence is now the absent Project group.
- Specs that name a settings section or window: `AGT-agents.md`, `GIT-git.md`, `GRU-graduation-runs.md`, `NTF-notifications.md`, `ABT-about-panel.md` (ABT-FR-NVQT), and any other spec found by searching for "Global settings" and "Project settings".