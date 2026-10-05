# Status bar

**Spec code:** `STB`

## Intent
A persistent full-width strip along the bottom edge of the main window that holds the workspace's **service functions** — the controls and indicators that describe how the application is behaving rather than what the open project contains. It gathers three things that otherwise have nowhere coherent to live: the entry points to Global and Project settings, a single place where any long-running backend operation can report that it is working, and the ambient facts about the current editing session — the project's line-ending convention, the active artifact's indentation, and a running total of how far the working tree has moved from `HEAD`. It exists because these facts are peripheral to authoring but constantly consulted, and because a long-running scan, push, or install otherwise runs invisibly. Out of scope: the status bar renders no project content and offers no control that mutates the repository; it is a surface of the main window only, so work performed before a project is open reports nowhere; and its progress display observes operations without starting, cancelling, or retrying any of them. Its one navigation is the activation of an overlay row whose operation names a destination, which opens the surface that owns the work.

## User stories
- As an author, I want a fixed place to glance at while a project indexes or a push runs so that I know the app is working rather than wedged.
- As an author running several things at once, I want to open the progress overlay and see every in-flight activity, newest first, so that I know what the application is doing for me.
- As an author, I want to activate an in-flight activity and land on the surface that owns it, with its target selected, so that I can follow a run, a conversation, or a push without searching for it.
- As an author, I want to see how many lines this session has added and removed without opening a panel, so that I can tell at a glance whether it is time to commit.
- As an author, I want the conventions my saves obey — line endings and indentation — visible and changeable where I can see them, rather than buried in a settings window.

## Wireframes
```
┌────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                    │
│                            main window zones above                                 │
│                                                                                    │
├────────────────────────────────────────────────────────────────────────────────────┤
│ ⚙ ⚙          Indexing project…  ▓▓▓▓▓▓▓░░░       LF ▾   Spaces: 2 ▾    +412 −87    │
└────────────────────────────────────────────────────────────────────────────────────┘
  └ leading ┘  └────────── center ──────────┘      └────────── trailing ──────────┘
```

With no operation in flight, the center region is empty and inert:
```
┌────────────────────────────────────────────────────────────────────────────────────┐
│ ⚙ ⚙                                              LF ▾   Spaces: 2 ▾    +412 −87    │
└────────────────────────────────────────────────────────────────────────────────────┘
```

Clicking the progress bar opens the in-flight operations overlay:
```
                     ┌──────────────────────────────────────┐
                     │ In flight                            │
                     │ ──────────────────────────────────── │
                     │ Indexing project…   ▓▓▓▓▓▓▓░░░       │
                     │ @Helga is thinking about Push button…│
                     │ Pushing feature/x   ▓▓▓▓▓▓▓▓▓▓       │
                     │ Graduating Editor scroll fix…        │
                     │ Installing plugin…  ▓▓░░░░░░░░    ▲▼ │
                     └──────────────────────────────────────┘
┌────────────────────────────────────────────────────────────────────────────────────┐
│ ⚙ ⚙          Indexing project…  ▓▓▓▓▓▓▓░░░       LF ▾   Spaces: 2 ▾    +412 −87    │
└────────────────────────────────────────────────────────────────────────────────────┘
```
- Layout notes: the strip spans the entire window width and sits below every other zone, so the activity bar, vertical panel, bottom panel, and main viewport all terminate above it. Its height is fixed and it carries no splitter. The three regions are anchored independently — leading flush to the leading edge, trailing flush to the trailing edge, center to the strip's center — so an empty center region never causes the trailing controls to drift. Within the trailing region the controls read, in order, line endings, indentation, then the diffstat, with the diffstat closest to the trailing edge. The overlay is anchored above the center region and grows upward. It shows at most five rows at a time; with more than five operations in flight the rows sit in a vertically scrollable list. A row whose operation names a destination is a control; a row without one is plain text.

## UI contract boundary
- **Owned by the UI**: the zone and its fixed height; the three regions and their independent anchoring; the Global settings and Project settings buttons; the choice of which in-flight operation the bar displays and the fallback when it terminates; the empty, inert rendering when nothing is in flight; determinate versus indeterminate bar rendering; the overlay, its anchoring, its ordering, its five-row viewport and its vertical scrolling, whether a row is actionable, the accessible name of a row, the routing of a row's activation to the surface that owns the target, its dismissal, and its mutual exclusion with the other overlays of the main window; the line-ending control and the marking of open Editor tabs dirty when the convention changes; client-side detection of an artifact's indentation and the override control over it; enablement of the indentation control by the active tab's type; formatting of the added/removed totals and the states rendered in their place; and reloading the totals in response to the change event. The UI does not compute diffstats, does not decide which operations report progress, and does not normalize any bytes.
- **Delegated to backend (abstract)**:
  - `"list in-flight operations"` — the operations running at mount time. Owned by `../core/PRG-progress-reporting.md`.
  - The event `"operation progress"` — owned by `../core/PRG-progress-reporting.md`.
  - The `activation` destination each operation may carry — owned by `../core/PRG-progress-reporting.md` PRG-FR-KXQW. The surfaces an activation opens own their targets: the Runs panel's graduation section (`GRU-graduation-runs.md` GRU-FR-PAHN), the `revealDiscussion` route (`CVP-conversation-presentation.md` CVP-FR-06), and the Git panel (`GIT-git.md` GIT-FR-FZMS).
  - `"get uncommitted diff totals"` — the added/removed totals for the active worktree. Owned by `../core/CHC-changes.md`.
  - The event `"changes updated"` — owned by `../core/CHC-changes.md`.
  - `"load project config"` and `"save project config"` — the project's line-ending convention. Owned by `../core/PSS-project-settings-storage.md`.

## Functional requirements
1. **STB-FR-01** The status bar is a zone of the main window occupying a single fixed-height strip along its bottom edge, spanning the full window width below every other zone (per `OVW-overview.md` OVW-FR-03 and `SNV-shell-navigation.md` SNV-FR-42); the activity bar, vertical panel, bottom panel, and main viewport all end above it.
2. **STB-FR-02** The status bar is present whenever the main window is mounted and exposes no control to hide, collapse, or resize it. Nothing about it is persisted, so it contributes no field to layout preferences (per `SNV-shell-navigation.md` SNV-FR-08). It is not part of the Project picker, which remains a window with no shell chrome (per `PPK-project-picker.md` PPK-FR-09).
3. **STB-FR-03** The strip has exactly three regions — **leading**, **center**, and **trailing** — each anchored independently to its own edge or to the strip's center, so the content of one region never displaces another.
4. **STB-FR-04** The leading region hosts a Global settings button and a Project settings button, rendered as graphical icon controls. Activating either opens its native settings child window, focuses that window when it is already open, and closes the other settings window first when that one is open (per `GLS-global-settings.md` GLS-FR-01, `SET-project-settings.md` SET-FR-01, and `SWN-settings-windows.md` SWN-FR-05 through SWN-FR-07); neither button creates a tab in the main viewport. These two buttons and the application menu's two settings entries (per `SWN-settings-windows.md` SWN-FR-14) are how the author opens either window from the shell; the activity bar carries no settings control (per `SNV-shell-navigation.md` SNV-FR-03). While a settings window is open the main window takes no interaction, so neither button responds until it closes (per `SWN-settings-windows.md` SWN-FR-02).
5. **STB-FR-05** The center region displays exactly one operation at a time: the most recently started of the operations currently in flight (per `../core/PRG-progress-reporting.md` PRG-FR-02).
6. **STB-FR-06** When the displayed operation reaches a terminal state, the region falls back to the most recently started of the operations still in flight, and continues falling back as each terminates.
7. **STB-FR-07** While no operation is in flight the center region renders nothing at all — no track, no placeholder, no idle label — and is not a click target.
8. **STB-FR-08** An operation reporting a total renders as a determinate bar filled to the proportion completed; an operation reporting no total renders as an indeterminate busy state (per `../core/PRG-progress-reporting.md` PRG-FR-05). An operation that starts indeterminate and later reports a total switches to the determinate rendering in place, without restarting.
9. **STB-FR-09** The center region displays the operation's label beside its bar. An operation whose `kind` the UI does not recognise renders with the same generic treatment as any other (per `../core/PRG-progress-reporting.md` PRG-FR-11).
10. **STB-FR-10** Clicking the center region while at least one operation is in flight opens the in-flight operations overlay.
11. **STB-FR-11** The overlay lists every operation currently in flight, most-recently-started first, each with its own label and its own determinate or indeterminate bar under the rules of STB-FR-08. The operation the center region is displaying is the first row. The overlay lists every in-flight operation whatever its `kind` and whether or not it carries an `activation`.
12. **STB-FR-12** The overlay is a floating overlay of the main window and is mutually exclusive with every other: opening it closes any other open overlay, and opening another closes it (per `SNV-shell-navigation.md` SNV-FR-56).
13. **STB-FR-13** The overlay offers no control to start, cancel, retry, or dismiss an operation; cancelling belongs to the surface that owns the operation, such as the Runs panel's stop control (per `RUN-runs.md` RUN-FR-05). Activating a row (STB-FR-RWPD) only opens a surface and changes nothing about the operation.
14. **STB-FR-14** The overlay is dismissed by pressing Escape, by a pointer-down outside both the overlay and the center region, or by clicking the center region again. It also closes on its own when the last in-flight operation terminates, because the region it is anchored to is no longer rendered (STB-FR-07).
15. **STB-FR-15** On mount the status bar establishes the current set of in-flight operations from `"list in-flight operations"` and keeps it current from `"operation progress"` events thereafter. Both the bar and the overlay update as events arrive without blocking any other surface, and an operation that starts and terminates between two frames is never left rendered.
16. **STB-FR-16** The trailing region hosts a line-ending control displaying the open project's line-ending convention — **LF** or **CRLF** — read from `"load project config"` (per `../core/PSS-project-settings-storage.md` PSS-FR-17).
17. **STB-FR-17** Selecting the other convention persists it immediately via `"save project config"`; from then on every artifact written uses it, whatever the previous on-disk bytes used (per `../core/PST-project-storage.md` PST-FR-22).
18. **STB-FR-18** Changing the convention marks the artifact of every open Editor tab dirty without altering any buffer (per `EDT-editor.md` EDT-FR-41), which schedules a write for each of them on the ordinary terms of `EDT-editor.md` EDT-FR-70, so the conversion is visible in the indicator while it is outstanding and lands shortly after rather than invisibly on some later unrelated edit. An artifact with no open tab carries no dirty marker and is converted whenever it is next written.
19. **STB-FR-19** This control and the line-ending control in the Project settings window's Project section (`SET-project-settings.md` SET-FR-10) read and write one shared project preference; each reflects a change made from the other without a reload.
20. **STB-FR-20** The line-ending control is present and enabled whenever a project is open, whatever the active tab is, because the convention is a property of the project rather than of a tab.
21. **STB-FR-21** The trailing region hosts an indentation control describing the artifact the active tab is editing: whether it is indented with tabs or with spaces and, for spaces, the width (per `EDT-editor.md` EDT-FR-37). It describes that artifact in an **Editor tab** and equally in a **Diff tab**, whose editable target is the same artifact's editing session (per `DFV-diff-viewer.md` DFV-FR-42) — a Tab keypress lands in the target's Source surface there, so a control that named no convention would leave the author unable to see or change what it inserts.
22. **STB-FR-22** When the active tab is editing no artifact — Dashboard, Flow, Search results, History detail — the indentation control renders a neutral disabled state naming no convention, because there is no artifact for it to describe. A Diff tab whose comparison deletes the file, and which therefore holds no target at all until the file is restored (per `DFV-diff-viewer.md` DFV-FR-54), renders that same state.
23. **STB-FR-23** Selecting a different indentation convention changes what a Tab keypress inserts in that artifact's raw-text editing surface from that point on (per `EDT-editor.md` EDT-FR-38). It rewrites no existing line, changes no byte on its own, and does not mark the artifact dirty.
24. **STB-FR-24** An override selected here belongs to the artifact's retained edit state (per `EDT-editor.md` EDT-FR-39), so it survives the tab closing and reopening within the session and is discarded with that state; an artifact carrying no override is described by detection alone.
25. **STB-FR-25** The trailing region displays a diff summary — a total added-line count and a total removed-line count — obtained from `"get uncommitted diff totals"` (per `../core/CHC-changes.md` CHC-FR-21).
26. **STB-FR-26** The summary always describes the uncommitted comparison of the active worktree, regardless of which mode the Changes panel is in (per `CHG-changes.md` CHG-FR-02) and of which target branch that panel is configured against.
27. **STB-FR-27** The summary is reloaded whenever `"changes updated"` fires (per `../core/CHC-changes.md` CHC-FR-16), so it tracks edits made in the app and by external processes alike. It does not depend on the Changes panel having been opened and holds no change set of its own.
28. **STB-FR-28** Binary entries contribute to neither total, because no line counts exist for them (per `../core/CHC-changes.md` CHC-FR-08).
29. **STB-FR-29** When the open project is not inside a Git repository — the condition under which the totals operation returns a typed `"not a git repository"` error (per `../core/CHC-changes.md` CHC-FR-02) — the summary renders nothing in its place rather than zeros or an error message, and the rest of the status bar is unaffected.
30. **STB-FR-30** When the project's active worktree changes (per `OVW-overview.md` OVW-FR-12), the summary discards the totals it was showing and reloads against the new content root.

31. **STB-FR-HJVM** The overlay shows at most five rows at once. Its row area is as tall as the rows it holds up to five rows. When more than five operations are in flight, the row area is exactly five rows tall and scrolls vertically, so the author reaches the other rows by scrolling. The title does not scroll away with the rows, and the overlay never scrolls horizontally.
32. **STB-FR-RWPD** A row whose operation carries an `activation` is actionable. Activating it closes the overlay and opens the surface that owns the target, with the target selected, by the route named for its `type` (per `../core/PRG-progress-reporting.md` PRG-FR-KXQW):
    - `graduation_run` opens the bottom panel on the Runs surface with its graduation section shown and selects the run with that run id (per `GRU-graduation-runs.md` GRU-FR-PAHN).
    - `discussion` calls `revealDiscussion` with that discussion id (per `CVP-conversation-presentation.md` CVP-FR-06). No second discussion route exists.
    - `git_push` opens the bottom panel on the Git surface with its Logs section shown, and selects the branch the activation names (per `GIT-git.md` GIT-FR-FZMS).
    A `type` this build does not recognise leaves the row non-actionable (STB-FR-DNLC).
33. **STB-FR-DNLC** A row whose operation carries no `activation` is visible and is not actionable. It renders as plain text with its bar, not as a button, not as a disabled button, and not as a link; it takes no focus stop; and activating it by pointer or keyboard opens nothing and closes nothing. The UI does not infer a destination from `kind`, from `label`, or from any other field (per `../core/PRG-progress-reporting.md` PRG-FR-TBZN).
34. **STB-FR-YQFE** An actionable row is a control that the keyboard reaches in the overlay's row order and operates with Enter and Space. Its accessible name states the operation's label and the target it opens, for example `@Helga is thinking about Push button… — open discussion`, `Graduating Editor scroll fix… — open graduation run`, or `Pushing feature/x — open branch feature/x in Git`. The row's progress bar keeps its progressbar semantics. The label is displayed as the producer supplied it, and the UI adds no subject name to it (per `../core/PRG-progress-reporting.md` PRG-FR-HDQS).
35. **STB-FR-LKRN** The overlay stays current while it is open. A row appears when its operation starts, changes in place when the operation updates, and leaves when the operation terminates, on the terms of STB-FR-15, so the list never keeps a completed operation. The order stays most-recently-started first, and an update never moves a row. Rows that stay do not lose the scroll position of the list.

## Non-functional requirements
- The status bar renders on first paint with the main window; the strip never appears after the zones above it have laid out, and its presence never causes a visible reflow of those zones.
- Every region degrades to the minimum window width the shell supports (≥1280×800 per `SNV-shell-navigation.md`) without wrapping to a second line: an over-long operation label truncates rather than growing the strip.
- Progress updates are consumed on the event bus and applied without blocking rendering elsewhere; a high-frequency reporter cannot stall the main viewport, and the coalescing that makes this safe happens upstream (per `../core/PRG-progress-reporting.md` PRG-FR-07).
- The diffstat's rendered width is stable as its counts change, so the trailing controls beside it do not shift horizontally during a reload.
- The status bar renders in full — settings buttons, empty center, editing conventions, and an absent diffstat — with no network access and outside a Git repository.
- The settings buttons open windows rather than tabs, so activating one changes nothing about the tab strip, the panels, or the zones around this strip.
