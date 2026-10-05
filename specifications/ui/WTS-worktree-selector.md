# Worktree selector

**Spec code:** `WTS`

## Intent
Top-chrome control that makes Git worktrees and branches a first-class dimension of the workspace, sitting beside the project switcher (`SNV-shell-navigation.md` SNV-FR-17) and answering "which checkout am I working in?". A project is a logical boundary — one repository — and inside it the author moves between several worktrees that are, on disk, separate folders. This control is where that movement happens: it lists the repository's worktrees first, then — while the repository's own checkout is the active one — the branches that have none, filters what it shows from a search field at the top, and switches the whole window onto the selection so the Project panel, the Changes panel, and every Git surface read from the checkout the author actually means. What it lists is read from disk, which goes stale the moment a colleague pushes a branch, so a refresh control beside it brings the list back into agreement with the remote on demand. Changing which branch is checked out is an operation on the repository's own checkout: inside a linked worktree the control moves between worktrees and creates new ones, and offers no branch to check out at all. Out of scope: this surface removes no worktree, deletes no branch, and rewrites no history; and it does not choose the comparison target the Changes panel diffs against (`CHG-changes.md` CHG-FR-05), which is a separate, independent choice.

## User stories
- As a user juggling two branches in parallel checkouts, I want to move between them from the app chrome so that I do not close the project and reopen a different folder.
- As a user, I want worktrees listed above plain branches so that the checkouts I already have on disk are the fast path.
- As a user with many branches, I want to type a fragment and see only matching entries so that I do not scroll a long list.
- As a user starting new work, I want to create a worktree for a branch from the same control, with a sensible folder proposed for me, so that I do not drop to a terminal.
- As a user, I want switching to write my pending edits and clear the workspace so that I never carry a stale tab from one checkout into another.
- As a user working inside a linked worktree, I want the control to offer me no branches at all so that I cannot move that checkout off the branch it exists to hold.
- As a user whose colleague just pushed a branch, I want one click beside the selector to pull the branch list up to date so that I can start a worktree for it without dropping to a terminal.
- As a user working on a plane, I want that click to still refresh what is on my disk and tell me plainly that the remote half could not run, rather than failing outright.

## Wireframes

### Resting state in the top chrome
```
┌──────────────────────────────────────────────────────────────────────────┐
│ ◈ Synthesis  [ acme-platform ▾ ] [ ⑂ feature/new-window ▾ ] ⤓  ⌕ search … │
└──────────────────────────────────────────────────────────────────────────┘
                  project switcher    worktree selector       refresh
```

### Refresh whose remote leg failed
```
┌──────────────────────────────────────────────────────────────────────────┐
│ ◈ Synthesis  [ acme-platform ▾ ] [ ⑂ feature/new-window ▾ ] ⤓  ⌕ search … │
│                                    ┌───────────────────────────────────┐ │
│                                    │ ⚠ couldn't reach the remote —     │ │
│                                    │   local branches are up to date ✕ │ │
│                                    └───────────────────────────────────┘ │
└──────────────────────────────────────────────────────────────────────────┘
```

### Dropdown
```
        ┌──────────────────────────────────────────────────┐
        │ ┌──────────────────────────────────────────────┐ │
        │ │ ⌕ filter worktrees and branches              │ │
        │ └──────────────────────────────────────────────┘ │
        │ Worktrees                                        │
        │  ● feature/new-window                  current   │
        │    ~/dev/acme-platform                           │
        │  ○ main                                          │
        │    ~/dev/acme-platform-main                      │
        │  ○ detached at 4f2a10c                           │
        │    ~/dev/acme-platform-spike                     │
        │  ○ release/2.1                         missing   │
        │    ~/dev/acme-platform-release                   │
        │ Branches                                         │
        │  ⑂ fix/editor-scroll                             │
        │  ⑂ chore/deps                                    │
        │  ⑂ origin/experiment                    remote   │
        ├──────────────────────────────────────────────────┤
        │ + New worktree…                                  │
        └──────────────────────────────────────────────────┘
```

### Dropdown inside a linked worktree
```
        ┌──────────────────────────────────────────────────┐
        │ ┌──────────────────────────────────────────────┐ │
        │ │ ⌕ filter worktrees                           │ │
        │ └──────────────────────────────────────────────┘ │
        │ Worktrees                                        │
        │  ● main                                current   │
        │    ~/dev/acme-platform-main                      │
        │  ○ feature/new-window                            │
        │    ~/dev/acme-platform                           │
        ├──────────────────────────────────────────────────┤
        │ + New worktree…                                  │
        └──────────────────────────────────────────────────┘
```

### Selecting a branch (inline two-way choice)
```
        │ Branches                                         │
        │  ⑂ fix/editor-scroll                             │
        │  ┌────────────────────────────────────────────┐  │
        │  │ [ Check out here ]   [ New worktree… ]     │  │
        │  └────────────────────────────────────────────┘  │
        │  ⑂ chore/deps                                    │
```

### New worktree dialog
```
┌ New worktree ────────────────────────────────┐
│ Branch                                       │
│ ┌──────────────────────────────────────────┐ │
│ │ feature/PROJ-12345/editor-scroll         │ │
│ └──────────────────────────────────────────┘ │
│ Location                        [ Browse… ] │
│ ┌──────────────────────────────────────────┐ │
│ │ ~/dev/acme-platform-editor-scroll        │ │
│ └──────────────────────────────────────────┘ │
│ ✗ a folder already exists at that location   │
│                     [ Cancel ]  [ Create ]   │
└──────────────────────────────────────────────┘
```

Layout notes for the generator:
- The control sits in the top chrome immediately after the project switcher; the refresh control follows it, then the **Push** control (`GIT-git.md` GIT-FR-XXLE), then the work stream selector, and the universal search bar follows those. They occupy the leading cluster in that fixed order.
- The refresh control is an icon-only button sized to the chrome's control height, reading as a sibling of the selector rather than as part of it — it is outside the selector's activation area, so pressing it never opens the dropdown.
- Its icon depicts material arriving from elsewhere — a downward arrow into a line, tray, or cloud — rather than the circular arrows of a generic reload, because what it does is reach the remote and bring back what is there (WTS-FR-37).
- The refresh control's busy state renders in place, within its own footprint, so the leading cluster does not reflow while a refresh runs and the search bar does not shift.
- A remote outcome worth reporting attaches under the leading cluster, anchored to the refresh control, as a dismissible note that overlays the chrome rather than displacing it. When the dropdown is open the same note renders inside the dropdown instead, above the group regions.
- The dropdown is anchored under the control. The filter input is the first focusable element inside it and is always the topmost row. Its label names what is on offer: worktrees and branches from the primary worktree, worktrees alone from a linked one.
- Group headings (**Worktrees**, **Branches**) are non-interactive labels. Worktrees always precede Branches. Inside a linked worktree the **Branches** heading and its region are absent, and the **Worktrees** group runs straight into the divider above **New worktree…**.
- A worktree row is two lines — branch (or detached marker) on the first, absolute path on the second. A branch row is one line.
- Row flags (`current`, `missing`, `remote`) are right-aligned trailing markers, not separate columns.
- The **New worktree…** action sits below a divider, outside the scrolling group regions, and is present from every worktree.
- Errors render inline inside the dropdown (below the row that produced them) or inside the dialog (above its buttons); they never replace the dropdown.
- The New worktree dialog is a centered modal overlay with a fixed width. Both of its fields span the dialog's content width exactly and neither extends past its edge, whether or not it shares a row with a button. The Location field is single-line and truncates from the left when the path overflows. The dropdown is not mounted behind it (WTS-FR-05), so the dialog appears over the workspace rather than over the list it was reached from.

## UI contract boundary
- **Owned by the UI**: the chrome control and its resting label; the refresh control beside it, its busy state, and the client-side rule that it is present under exactly the condition the selector is; the routing of a refresh's remote failure to either the token picker or the Global settings GitHub section; dropdown rendering, anchoring, and dismissal; the filter input and the matching it performs; the groups, their fixed order, their headings, their row rendering including the `current` / `missing` / `remote` flags, and the decision to render the **Branches** group at all — which follows from whether the active worktree is the repository's primary one; the inline two-way choice presented for a branch row; the New worktree dialog, its two inputs, the re-derivation of the proposed location while the branch is edited, and client-side enablement of its Create action; inline rendering of typed errors; the ordering of the switch transition (flush, then close every tab, then invoke, then reopen the Dashboard); and mutual exclusion with the other overlays of the main window. The UI does not enumerate worktrees or branches, does not decide which branches have a worktree, does not compute the proposed location, does not contact a remote, and does not perform any Git operation itself.
- **Delegated to backend (abstract)**:
  - `"list worktrees and branches"` — the dropdown's contents. Owned by `../core/WTC-worktree-context.md`.
  - `"refresh worktrees and branches"` — the refresh control's action. Owned by `../core/WTC-worktree-context.md`.
  - `"get active worktree"` — the control's resting label. Owned by `../core/WTC-worktree-context.md`.
  - `"activate worktree (path)"` — switching to an existing worktree. Owned by `../core/WTC-worktree-context.md`.
  - `"check out branch in active worktree (branch)"` — checking a branch out in place. Owned by `../core/WTC-worktree-context.md`.
  - `"propose worktree path (branch)"` — the pre-filled location in the New worktree dialog. Owned by `../core/WTC-worktree-context.md`.
  - `"create worktree (branch, path)"` — creating and switching to a new worktree. Owned by `../core/WTC-worktree-context.md`.
  - `"browse for folder"` — the dialog's Browse action. Owned by `../core/FSA-filesystem-access.md` FSA-FR-01.
  - The event `"worktree context changed"` — owned by `../core/WTC-worktree-context.md`.
  - The event `"branches changed"` — owned by `../core/WTC-worktree-context.md`.
  - `"set project github token binding"` — written on confirmation of the token picker the refresh control opens. Owned by `../core/GTS-github-token-storage.md`.

## Functional requirements
1. **WTS-FR-01** The worktree selector is a top-chrome control rendered immediately after the project switcher (per `SNV-shell-navigation.md` SNV-FR-32).
2. **WTS-FR-02** The selector is rendered only while the open project's content root sits inside a Git repository; when it does not, no selector appears anywhere in the top chrome and the project switcher occupies the leading cluster alone.
3. **WTS-FR-03** The control's resting label names the branch checked out in the active worktree, taken from `"get active worktree"`; when that worktree's `HEAD` is detached the label names the detached state with the abbreviated commit id instead of a branch.
4. **WTS-FR-04** Activating the control opens a dropdown whose contents come from `"list worktrees and branches"`.
5. **WTS-FR-05** The dropdown and the New worktree dialog are both floating overlays of the main window, and every such overlay is mutually exclusive with every other: opening one closes any other that is open rather than coexisting with it (per `SNV-shell-navigation.md` SNV-FR-56). This holds between these two as much as between either and the rest — opening the dialog closes the dropdown, so the dialog is never presented over the list it was reached from, and dismissing the dialog returns to the chrome rather than to the dropdown.
6. **WTS-FR-06** A filter input is the topmost row of the dropdown and holds keyboard focus from the moment the dropdown opens, so a query can be typed without an additional click.
7. **WTS-FR-07** The filter narrows every group on display at once, matching an entry's branch name, its displayed name, and — for a worktree — its absolute path. An empty filter applies no constraint.
8. **WTS-FR-08** The dropdown presents a **Worktrees** group, and below it a **Branches** group whenever that group is present at all (WTS-FR-12). The order is fixed — Worktrees always precedes Branches — and does not change with the filter, the selection, or the contents.
9. **WTS-FR-09** The **Worktrees** group renders every worktree the backend reports that is not a work stream's working copy, each row showing its checked-out branch — or a detached marker carrying the abbreviated commit id — above its absolute path.
10. **WTS-FR-10** The active worktree's row is flagged current and is non-actionable: clicking it neither re-activates the worktree nor closes any tab.
11. **WTS-FR-11** A worktree the backend reports as missing renders flagged missing and is non-actionable; clicking it invokes nothing.
12. **WTS-FR-12** The **Branches** group is present only while the project's active worktree is the repository's primary one. A linked worktree exists to hold one branch, so moving it onto another is not an operation this application offers (per `../core/WTC-worktree-context.md` WTC-FR-21): from a linked worktree the group is absent entirely rather than rendered inert, and the dropdown offers worktrees alone. Where the group is present it renders every branch the backend reports as having no worktree, with a remote-tracking entry flagged remote and a local entry unflagged.
13. **WTS-FR-13** Selecting a worktree row that is neither current nor missing invokes `"activate worktree (path)"` with that row's path.
14. **WTS-FR-14** Selecting a branch row invokes nothing on its own: it reveals an inline two-way choice on that row offering to check the branch out in the active worktree or to create a worktree for it. Dismissing the choice leaves the branch unselected and the dropdown open.
15. **WTS-FR-15** Choosing to check out in place invokes `"check out branch in active worktree (branch)"` with that branch.
16. **WTS-FR-16** Choosing to create a worktree closes the dropdown (WTS-FR-05) and opens the New worktree dialog with that branch pre-filled in its Branch field.
17. **WTS-FR-17** The dropdown provides a **New worktree…** action rendered outside both groups' scrolling regions; activating it closes the dropdown (WTS-FR-05) and opens the New worktree dialog with an empty Branch field.
18. **WTS-FR-18** The New worktree dialog has exactly two inputs — a branch name and a location — and its location is pre-filled from `"propose worktree path (branch)"` for the current branch value.
19. **WTS-FR-19** While the user has not overridden the location, editing the Branch field re-derives the location from `"propose worktree path (branch)"`; once the user edits the location directly or picks one through Browse, it is no longer re-derived.
20. **WTS-FR-20** The dialog's Browse action invokes `"browse for folder"` and writes the chosen path into the Location field; a cancelled browse leaves the field unchanged.
21. **WTS-FR-21** The dialog's Create action is enabled only when both the branch name and the location are non-empty; activating it invokes `"create worktree (branch, path)"` with the two values.
22. **WTS-FR-22** A successful `"activate worktree (path)"`, `"check out branch in active worktree (branch)"`, or `"create worktree (branch, path)"` makes its target the active worktree and drives the worktree-switch transition of `OVW-overview.md` OVW-FR-12.
23. **WTS-FR-23** Before invoking any of the three switching operations, the UI flushes the pending changes of the artifacts and Flows edited in this session (per `EDT-editor.md` EDT-FR-33 and `FLO-flow.md` FLO-FR-29). A flush that cannot proceed safely cancels the switch (per `EDT-editor.md` EDT-FR-32): no operation is invoked, no tab closes, and the active worktree does not change.
24. **WTS-FR-24** When a switching operation returns a typed error, the error renders inline in the dropdown or dialog that produced it, the project stays on its current active worktree, and the target of the failed operation is not activated.
25. **WTS-FR-25** The selector's label and, when open, its dropdown contents refresh on the `"worktree context changed"` event, so a branch checked out from the Git panel's branches section (per `GIT-git.md` GIT-FR-04) is reflected in the chrome without reopening the dropdown.
26. **WTS-FR-26** Each group's region displays a bounded number of rows and scrolls vertically beyond it. No region of the dropdown scrolls horizontally — a branch name or path wider than the available width is truncated (for example with an ellipsis) rather than introducing a horizontal scrollbar.
27. **WTS-FR-27** No affordance in the selector, the refresh control, or the New worktree dialog removes a worktree, deletes a local branch, discards working-tree content, or rewrites history. Deleting a branch with its linked worktree is offered by the Git panel alone (per `GIT-git.md` GIT-FR-08 and GIT-FR-QYWP), and a stream by the Streams overlay. A refresh drops the remote-tracking refs of branches the remote no longer has (per `../core/GTC-git.md` GTC-FR-13), which is why those branches stop being offered; the local branches that tracked them are left intact.
28. **WTS-FR-28** While a linked worktree is active, nothing in the selector checks a branch out: with the **Branches** group absent (WTS-FR-12) there is no row to reveal the inline choice of WTS-FR-14, and no other control offers one. Moving to another worktree (WTS-FR-13) and creating one (WTS-FR-17) stay available from every worktree, because each changes which checkout the project reads from rather than moving a checkout onto a different branch.
29. **WTS-FR-29** A refresh control is rendered in the top chrome immediately after the worktree selector (per `SNV-shell-navigation.md` SNV-FR-32), under exactly the condition the selector itself is rendered (WTS-FR-02): while the open project's content root sits inside a Git repository both are present, and while it does not neither is. Refreshing a branch list is meaningless outside a repository, so the two share one visibility rule rather than each carrying its own.
30. **WTS-FR-30** Activating the refresh control invokes `"refresh worktrees and branches"`. The control belongs to the chrome rather than to the dropdown: it is activated whether the dropdown is open or closed, and activating it neither opens nor closes it.
31. **WTS-FR-31** While a refresh is in flight the control renders a busy state and is not activatable, so a second refresh cannot be stacked on the first. The backend attributes the work to the status bar's progress region (per `../core/GTC-git.md` GTC-FR-15), so a slow fetch is visible there for as long as it runs without the chrome blocking on it.
32. **WTS-FR-32** On a refresh returning, the selector's label and — when the dropdown is open — its rows re-render from the refreshed context, exactly as they re-render on `"worktree context changed"` (WTS-FR-25). An open dropdown keeps its filter query and its scroll position, and the refreshed rows are filtered by that query. The selector re-renders on `"branches changed"` (per `../core/WTC-worktree-context.md` WTC-FR-25) whatever raised it, so a refresh performed while the dropdown was closed is already reflected the next time it opens.
33. **WTS-FR-33** A refresh never drives the worktree-switch transition of `OVW-overview.md` OVW-FR-12: it flushes nothing, closes no tab, changes no active worktree, and leaves the viewport as it was. It reads what the repository holds rather than moving the project onto a different checkout, so the flush-and-cancel rule of WTS-FR-23 does not apply to it and a refresh is available while an Editor tab holds unsaved changes.
34. **WTS-FR-34** A refresh whose remote leg did not run still applies its locally refreshed listing (per `../core/WTC-worktree-context.md` WTC-FR-23), and reports the remote outcome as a dismissible note anchored to the refresh control — or inside the dropdown while it is open. The note distinguishes the causes the author would act on differently: no remote configured, a remote that could not be reached, and a credential GitHub refused.
35. **WTS-FR-35** A remote leg that failed for want of a credential is routed by which of the two typed causes it carries (per `../core/GTC-git.md` GTC-FR-10). A selection-required failure opens the token picker (`GHA-github-authentication.md` GHA-FR-16), and confirming a token re-invokes `"refresh worktrees and branches"` while cancelling abandons the remote leg (GHA-FR-17); either way the listing refreshed by the local half stays in place. A missing-token failure opens no picker and renders the note of WTS-FR-34 with a route to the Global settings GitHub section (GHA-FR-19), because there is nothing to choose between.
36. **WTS-FR-36** The token picker the refresh control opens is a floating overlay and obeys the mutual exclusion of WTS-FR-05: it closes the dropdown if it is open, and dismissing it returns to the chrome rather than reopening the dropdown.
37. **WTS-FR-37** The refresh control's icon depicts **material arriving from elsewhere** — a downward arrow into a line, a tray, or a cloud — rather than the circular arrows that stand for reloading what is already in hand. What the control does is reach across the network and bring back what the remote holds (WTS-FR-30), and an icon reading as "redraw this" invites the author to press it when the chrome looks stale rather than when a colleague has pushed. The icon does not vary with what a previous refresh reported (WTS-FR-34); the busy state of WTS-FR-31 is the only thing that replaces it, and only while a refresh is in flight.
38. **WTS-FR-ROMD** A worktree the backend flags as a work stream's working copy is absent from the dropdown, from its filter results and from its count, whether or not it is active. The author reaches it through the work stream selector (per `WSS-work-stream-selector.md` WSS-FR-YCAL). A stream's branch stays in the Git panel's branch list.
39. **WTS-FR-BQCI** A switching operation refused with `"direct graduation active"` renders inline on the terms of WTS-FR-24, saying that a graduation run is working in the active worktree and that the switch waits until the run ends.
40. **WTS-FR-GVBD** The **Push** control (`GIT-git.md` GIT-FR-XXLE) follows the refresh control in the top chrome and is a control of its own. Refresh stays fetch-only: activating it never pushes, pulls, or commits, and activating **Push** never refreshes, fetches, switches the active worktree, or changes the branch the active worktree holds. A push that completes leaves the selector's label as it was.

## Non-functional requirements
- The dropdown opens and renders its groups from a single `"list worktrees and branches"` round-trip; it does not chain a second call to resolve a row's flags.
- Filtering is performed client-side over the already-loaded payload, so typing never triggers a backend round-trip and never re-orders the groups.
- The control's resting label has a stable maximum width, so a long branch name does not push the universal search bar out of the top chrome.
- The selector renders and filters without network access: opening the dropdown, listing worktrees and branches, and typing in the filter never contact a remote. The refresh control is the only network act in this surface, it happens only when the author asks for it, and the surface is fully usable without it ever being pressed.
- The dialog's proposed location is a suggestion the user can always replace; nothing about the switch depends on the proposal having been accepted.
