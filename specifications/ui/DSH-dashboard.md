# Dashboard

**Spec code:** `DSH`

## Intent
Project home tab; the initial tab on project open. It is the project's looking glass: every widget shows real data about the project as it stands now, so the author reads the state of the work from one surface instead of opening four. Below the widgets, a full-width Delivery timeline shows the project's milestones and the one the project is working on now. Optimized for "resume what I was doing," not "browse what exists" (browsing belongs in the Project panel). Closable while other work is open, and reopenable from the Home affordance that stands in its place at the head of the tab strip. It is also what the strip falls back to — closing the last remaining tab brings it back — so the main viewport is never empty and the author always lands somewhere that offers a way onward.

## User stories
- As a returning user, I want to see what I was last working on so that I can resume work without searching.
- As a user, I want to see whether an agent run is in flight so that I can jump to its activity surface quickly.
- As a user, I want the drafts I am developing listed so that I can return to one without opening the Drafts panel.
- As a user, I want the work pending in Git counted for me so that I can tell what is uncommitted and what is unpushed at a glance.
- As a user who edits files outside the IDE, I want those edits to move the Dashboard so that it describes the project rather than the application's own history.
- As an architect, I want to see the milestones on a timeline with today marked and the current milestone picked out, so that I know where the delivery stands when I open the project.

## Wireframes
```
┌ RECENTLY EDITED ──────────────┐ ┌ ACTIVE WORKSTREAMS ───────────┐
│ …                             │ │ …                             │
└───────────────────────────────┘ └───────────────────────────────┘
┌ DELIVERY · Payments v2 ──────────────────── 2 of 6 milestones done · 4 blocked ┐
│  ✓ Alpha        ✓ Private beta  │today    ● Beta             ○ GA               │
│  2026-09-30     2026-10-31      │         2027-03-31         2027-06-30         │
│  ▮▮▮▮▮▮ 12/12   ▮▮▮▮▮▮ 8/8      │         ▮▮▮▯▯▯ 7/19        ▯▯▯ 0/9            │
│                                 │         5 in progress · 4 blocked             │
│                                 │         ⊘ 4 blocked                           │
│ ──●───────────────●─────────────┼───────────────●─────────────────●──────────── │
│  Sep            Oct          Nov│ Dec  Jan  Feb  Mar  Apr  May  Jun             │
│ ─────────────────────────────────────────────────────────────────────────────── │
│ NO DATE   ○ v2.1 ─────── ○ v2.2                                                 │
│           ▯▯ 0/3         ▯ 0/1                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘

┌ DELIVERY · Workforce ─────────────────────── 1 of 4 milestones done · 3 blocked ┐
│  ✓ M0 ──────────── ● M1 ──────────── ○ M2 ──────────── ○ M3                     │
│  ▮▮▮▮ 4/4          ▮▯▯▯ 1/4          ▯▯ 0/2            ▯ 0/1                    │
│                    0 in progress · 3 blocked                                    │
│                    ⊘ 3 blocked                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```
- Layout notes: the Delivery widget is the last row of the Dashboard and spans the full width of the content area, under the grid of the other widgets. Its milestone labels sit above the axis. The current milestone uses the `--accent` border and the Today line uses `--accent`. The **No date** track sits under the axis, separated by a 1px `--border-1` rule, and its milestones are joined by a 1px line in plan order. Where no milestone has a due date, the track is the whole content, with no axis and no `No date` label. State glyphs and the leaf bar carry state in shape and in words, not in colour alone.

## UI contract boundary
- **Owned by the UI**: grid layout, widget rendering, drag-to-reorder of widgets, hiding empty widgets, click-through routing per widget, per-project layout persistence, each widget's loading, stale, and error presentation, and the re-invocation of a loader when that loader's refresh event arrives.
- **Delegated to backend (abstract)**: "list recently edited artifacts", "list active drafts", "list recent agent runs", "list pending git activity", "list due/upcoming reminders", "list project health signals", together with the refresh events "dashboard recently edited changed", "dashboard active drafts changed", "dashboard agent activity changed", "dashboard pending git changed", and the failure event "dashboard refresh failed". All eleven are owned by `../core/PST-project-storage.md`. The Delivery widget uses these:
  - `"load delivery milestones (refresh)"` and the event `"delivery source changed"` — owned by `../core/DSR-delivery-source.md`.
  - The event `"github delivery plan changed"` — owned by `../core/GDP-github-delivery-plan.md`.
  - The events `"github polling changed"` (owned by `../core/GPP-github-polling.md`), `"drafts changed"` (owned by `../core/DRS-draft-storage.md`), and `"graduation run changed"` (owned by `../core/GRD-graduation.md`). The first four loaders and the reminders loader serve real project data; "list project health signals" returns canned data during the walking skeleton.

## Functional requirements
1. **DSH-FR-01** Dashboard is a tab in the main viewport, opened automatically when a project is opened.
2. **DSH-FR-02** Dashboard occupies the leading position of the tab strip whenever it is open, and its close control is active only while at least one other tab is open; closing it then leaves the Home affordance in its place, which reopens it, and while it is the only open tab the control is greyed-out and closes nothing (per `SNV-shell-navigation.md` SNV-FR-09, SNV-FR-41 and `TAB-tabs.md` TAB-FR-08, TAB-FR-16).
3. **DSH-FR-03** Dashboard renders a grid of widgets. The v1 widget set is fixed: Recently edited artifacts, Active workstreams, Last/current agent activity, Pending Git activity, Reminders, Project health signals, and Delivery. **Active workstreams** is the widget's name and its items are the project's drafts (DSH-FR-11); the name is what the author already knows the widget by, and it names no workstream entity.
4. **DSH-FR-04** A widget that has no content is hidden from the grid (no empty-state placeholders are rendered). The Recently edited artifacts widget is the exception: while the Dashboard shows any content it stays visible and renders its own empty state when its list is empty (DSH-FR-09).
5. **DSH-FR-05** Widgets can be reordered by drag-and-drop; the resulting order is persisted per project.
6. **DSH-FR-06** Each widget item is clickable and routes to where its target entity lives:
   - Recently edited artifact of kind `markdown` → open in the Editor.
   - Recently edited artifact of kind `flow` → open the Flow tab for that Flow, or the Map tab on its Delivery lens where that Flow is the project's delivery (per `MDL-map-delivery-lens.md` MDL-FR-YSMB).
   - Active workstreams item → open that draft in a New Artifact tab (DSH-FR-12).
   - Run item (last/current agent activity) → open and focus the Runs bottom panel.
   - Pending Git count → open and focus the Git bottom panel.
   - Reminder item → open the entity the reminder is attached to in its natural surface.
   - Project health signal item → open the offending entity in its natural surface.
7. **DSH-FR-07** When one of the listed runs is in flight, the Last/current agent activity widget shows a "live" indicator on that run's row and its click-through opens the Runs bottom panel for that live run.
8. **DSH-FR-08** Empty Dashboard (all widgets have no content) renders an empty state inviting the user to start work; no widgets are shown.
9. **DSH-FR-09** The Recently edited artifacts widget lists **at most 5** artifacts, most-recently-edited first, as served by "list recently edited artifacts" (per `../core/PST-project-storage.md` PST-FR-31). The order is the order the loader returns: descending by the filesystem modification time of each artifact's primary source file, so an edit made by an external editor moves an artifact exactly as an edit made in the Editor does. Each entry shows the artifact's name and routes on click by its `kind` per DSH-FR-06. Drafts are not artifacts and never appear here. The widget stays visible whenever the Dashboard has content and renders an empty state (e.g. "No recent edits yet") when its list is empty, rather than being hidden; the empty-state message is centered both horizontally and vertically within the widget's content area. When the entire Dashboard is empty the whole-Dashboard empty state of DSH-FR-08 applies instead.
10. **DSH-FR-10** Dashboard is the tab strip's fallback: whenever the last open tab closes, a Dashboard tab opens in the leading position and takes focus, rendered exactly as it is on project open (per `TAB-tabs.md` TAB-FR-15).
11. **DSH-FR-11** The Active workstreams widget lists **at most 5** drafts, as served by "list active drafts" (per `../core/PST-project-storage.md` PST-FR-32). Every item is a draft whose status is `active`; an archived or graduated draft never appears. The order is the order the loader returns: descending by the filesystem modification time of the draft's one prompt file, so a prompt edited by an external editor moves the draft exactly as a prompt edited in its tab does. Each row shows the draft's stored name, which is the name the application holds for it and is never derived from a file's contents or from any other filesystem metadata.
12. **DSH-FR-12** Activating an Active workstreams item opens that draft in a **New Artifact tab** and focuses it, or jumps focus to the tab already showing it (per `NAW-new-artifact.md` NAW-FR-02, NAW-FR-03 and `TAB-tabs.md` TAB-FR-17). It opens no Project panel filter and no other surface.
13. **DSH-FR-13** The Last/current agent activity widget lists **at most 5** graduation runs, most-recently-updated first, as served by "list recent agent runs" (per `../core/PST-project-storage.md` PST-FR-33). A run the queue still holds in `queued` is listed like any other, so work waiting to start is visible rather than absent. Each row shows the run's source draft, the run's state, and the stage it stands at.
14. **DSH-FR-14** The Pending Git activity widget shows the project's pending work as four counts, as served by "list pending git activity" (per `../core/PST-project-storage.md` PST-FR-34): modified artifacts, modified source files, commits not pushed to the remote, and commits that can be fetched from the remote. A count the backend reports as unavailable — the two commit counts on a branch with no upstream — is rendered as unavailable rather than as zero.
15. **DSH-FR-15** The Last/current agent activity and Pending Git activity widgets refresh **every 5 minutes**, whether or not a Dashboard tab is open. The interval belongs to the backend timer, which dispatches both refreshes on every tick and skips none (per `../core/PST-project-storage.md` PST-FR-36, PST-FR-37); this surface starts no timer of its own and polls nothing. An open Dashboard re-invokes "list recent agent runs" on `"dashboard agent activity changed"` and "list pending git activity" on `"dashboard pending git changed"`, and renders the result.
16. **DSH-FR-16** An open Dashboard re-invokes "list recently edited artifacts" on `"dashboard recently edited changed"` and "list active drafts" on `"dashboard active drafts changed"`, and renders the result. A source file or a draft prompt changed by an external process therefore reaches the Dashboard without the user asking for a refresh, because those events follow the filesystem (per `../core/PST-project-storage.md` PST-FR-35).
17. **DSH-FR-17** Every loader is invoked asynchronously and no widget's result is waited for. Widgets render as their results arrive, in whatever order they arrive; a slow loader leaves its own widget in a loading state while every other widget renders, and nothing about a loading, refreshing, or failed widget blocks the tab strip, the vertical panel, the bottom panel, or any other tab.
18. **DSH-FR-18** A widget whose data could not be read renders an error state in its own content area, naming that the data could not be read, and is not hidden by DSH-FR-04 — an error is content. The state is entered from either direction: a loader this surface invoked returning an error, and `"dashboard refresh failed"` naming that widget, which is how a background refresh the author never asked for still reaches them instead of ending in a log (per `../core/PST-project-storage.md` PST-FR-37). It names one widget and touches no other: the rest of the grid renders, and the tab strip, the vertical panel, and the bottom panel are unaffected. The widget leaves the error state on its next successful result — the refresh event that follows a later successful refresh, or a loader call that returns. A widget refreshing after it has once rendered keeps showing the data it holds until the new result arrives, marked as stale rather than emptied, so neither a refresh nor an error blanks a widget the author is reading.
19. **DSH-FR-19** A loader result this surface no longer wants is **discarded rather than rendered**. Where two invocations of one widget's loader are outstanding at once, the surface renders the result of the later invocation and discards an earlier one that returns after it, so a slow call can never replace fresher data with older; and every outstanding result is discarded when the project or the active worktree changes, rather than being rendered against the new root.
20. **DSH-FR-AHRJ** The **Delivery** widget spans the full width of the Dashboard in one row below the grid of the other widgets. It takes no part in the drag-to-reorder of DSH-FR-05, and its place is fixed.
21. **DSH-FR-YAZR** The Delivery widget reads its data through `"load delivery milestones (refresh)"` with `refresh` false when the Dashboard renders (per `../core/DSR-delivery-source.md` DSR-FR-OJLB). It is hidden on the terms of DSH-FR-04 while the delivery source is `none` or the plan holds no milestone.
22. **DSH-FR-OUPX** The widget's header shows `DELIVERY`, the plan's name, and the count line `N of M milestones done`. Where every milestone is done or dropped, the count line reads `All milestones done`. Where a milestone holds a blocked work item, the count line ends in `· N blocked`, the sum of the milestones' blocked work items.
23. **DSH-FR-PLRL** Each milestone with a due date sits on one horizontal time axis at its due date, with month labels below the axis. The axis runs from the earliest to the latest of the due dates and today, with half a month of margin at each end.
24. **DSH-FR-NZXN** A vertical **Today** line, labelled `today`, marks today's date on the axis. Where no milestone has a due date, the widget shows no axis and no Today line.
25. **DSH-FR-NNWM** Each milestone shows its state glyph, name, due date, a 6px bar of its work-item states in the order of `MDL-map-delivery-lens.md` MDL-FR-GDUP, and `done/total` work items. A milestone with a blocked work item shows `⊘ N blocked`. A due date before today on a milestone that is not done is marked as past (per `FLO-flow.md` FLO-FR-XAPK).
26. **DSH-FR-BBRC** The milestone that the answer marks as **current** (per `../core/DSR-delivery-source.md` DSR-FR-FQZD) takes the `--accent` border and also shows the line `N in progress · N blocked`. No milestone is current where every milestone is done or dropped.
27. **DSH-FR-HVOQ** Milestones without a due date sit in one horizontal **No date** track, in plan order, with the content of DSH-FR-NNWM. The track spans the widget's width under the axis, and is the whole content where no milestone has a due date. A track wider than the widget scrolls horizontally and first shows the current milestone.
28. **DSH-FR-EWPI** The axis gives each dated milestone at least 140px. Where the axis is wider than the widget, the widget scrolls horizontally, and on each render it first shows the current milestone, or today where none is current. Labels that would overlap take a second row.
29. **DSH-FR-DFDW** Activating a milestone opens the Map tab on its Delivery lens with that milestone selected (per `MDL-map-delivery-lens.md` MDL-FR-JIKK). Activating the header opens the Map tab on its Delivery lens.
30. **DSH-FR-LGJT** An open Dashboard re-invokes `"load delivery milestones (refresh)"` with `refresh` false on `"delivery source changed"`, `"github delivery plan changed"`, `"drafts changed"`, `"graduation run changed"`, and `"dashboard recently edited changed"`, and with `refresh` true on `"github polling changed"`.
31. **DSH-FR-YGWR** An answer marked `stale` renders its milestones with the stale marking of DSH-FR-18 and the time of the read. A refused load renders the error state of DSH-FR-18 with the error text.

## Non-functional requirements
- Routing rules for artifacts, runs, reminders, and health signals are shared with Search result click-through (see `SCH-search.md` SCH-FR-09). The draft route of DSH-FR-12 is this surface's own: Search has no draft result group.
- The Dashboard holds no timer, no polling loop, and no cached copy of any widget's data across a project change. What it renders is what its loaders last returned for the open project.
- A widget's five-item limit is applied by the backend loader rather than by this surface, so the surface renders what it is given and two surfaces cannot disagree about what "the five most recent" means.
