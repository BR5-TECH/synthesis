# Map delivery lens

**Spec code:** `MDL`

## Intent
The Delivery lens is the first lens of the Map tab (`SMP-specification-map.md`). It shows the project's one delivery plan on one canvas, from whichever system holds it (`../core/DSR-delivery-source.md`): milestones in roadmap order, the work in them with its state, what is blocked, and which tasks overlap. It is the project's control plane for delivery. The architect reads where the delivery stands, changes the plan in place, and claims a ready task into a graduation from the same surface. When the plan lives on GitHub, each edit goes to GitHub at once, so the team and the architect never work from two copies. When it lives in a Flow file, the lens is the Flow canvas over that file. The plan appears without setup work: a project that selects a GitHub Project gets its delivery from that Project. The author can limit the board to the statuses that matter now, for example hide the done work, and the lens keeps that choice on the machine between sessions. Out of scope: the canvas behaviour itself, which `FLO-flow.md` owns; the links between delivery items and specifications, which `SDL-specification-delivery-links.md` owns; and the delivery settings, which `SET-project-settings.md` presents.

## Functional requirements
1. **MDL-FR-YTAP** The lens resolves the delivery source through `"get delivery source"` when it first renders, and again on `"delivery source changed"`, on `"github polling changed"`, and on `"project tree changed"`.
2. **MDL-FR-XMQU** With the source `none`, the lens renders an empty state that says the project has no delivery, states the reason in words, and offers **Configure delivery**. That action opens the Delivery section of Project settings (per `SET-project-settings.md` SET-FR-WDYJ).
3. **MDL-FR-WAOZ** With a `github` source whose configuration is `invalid`, the lens renders the configuration error text of `../core/GPP-github-polling.md` (GPP-FR-EHRC) with a route to the GitHub Project settings section, and renders no plan. A configuration that is `unchecked` does not stop the lens: the lens loads the plan (MDL-FR-LZIE), and the load checks the configuration (per `../core/GDP-github-delivery-plan.md` GDP-FR-QSHX). The author never opens the settings to show a plan whose configuration was valid before.
4. **MDL-FR-VLQJ** With a `file` source, the lens hosts the canvas of `FLO-flow.md` bound to that Flow file, laid out as the board of `DBR-delivery-board.md`. Every requirement of `FLO-flow.md` applies to it as it applies to a Flow tab, and the Map tab takes the role of the Flow tab in FLO-FR-26, FLO-FR-28, and FLO-FR-30.
5. **MDL-FR-XJBV** With a `github` source, the lens hosts the canvas of `FLO-flow.md` over the plan that `"load github delivery plan (refresh)"` returns, laid out as the board of `DBR-delivery-board.md`. Each edit of FLO-FR-26 goes through `"apply github delivery edit (edit)"` in place of a write. The states and overlaps come with each answer, and FLO-FR-27 to FLO-FR-31, FLO-FR-46, FLO-FR-47, and FLO-FR-STUN do not apply.
6. **MDL-FR-LZIE** With a `github` source, the lens loads with `refresh` false when it first renders, and then with `refresh` true. It loads with `refresh` true on `"github polling changed"` and on **Refresh**, and with `refresh` false on `"drafts changed"` and `"graduation run changed"`. A newer answer replaces an older one.
7. **MDL-FR-NRJM** The lens renders a loading state until the first answer. A load that refuses with no plan to show renders the error text with a **Retry** action.
8. **MDL-FR-LSKL** An answer marked `stale` renders its plan with one notice line above the canvas. The line names the time of the read and the error text, and offers **Retry**.
9. **MDL-FR-KKYH** For a `github` source, the band shows the Project's title as the plan name, the repository as `owner/name`, the time of the last read, and **Refresh**. The plan name and description are read-only. The plan's references are edited as FLO-FR-YDWT describes, through the `set_plan_references` edit.
9. **MDL-FR-BZHY** For a `github` source, the band's `owner/name` is a link with an external-link glyph and the tooltip `Open the GitHub Project`. Activating it invokes `"open tracker link (url)"` with the Project's URL (per `../core/DSR-delivery-source.md` DSR-FR-EDHP). A refusal shows its error text in an Error toast.
9. **MDL-FR-DHHW** For a `file` source, the band shows the Flow's path as a link. Activating it reveals the Flow file in the Project panel (per `LIB-library.md` LIB-FR-18) and opens no tab.
10. **MDL-FR-PUCM** An edit to a GitHub plan renders at once, with a pending mark on each item it touches. The answer of the edit replaces the plan. A refusal restores the plan of the last answer and shows the error text on the item.
11. **MDL-FR-XWGP** While an edit to a GitHub plan is pending, the canvas accepts further edits, and they go to the backend in the order the author made them.
12. **MDL-FR-HNLP** A refusal with `changed_on_tracker` states on the item that it changed on GitHub, and the lens loads with `refresh` true. The refused edit is not applied again.
13. **MDL-FR-XQGD** In a GitHub plan, a removal (FLO-FR-08, FLO-FR-45) opens a dialog that names the item and the count of items under it, and offers **Remove from board**, **Close as not planned**, and **Cancel**. For a milestone it states that the milestone closes. **Cancel** and Escape change nothing.
14. **MDL-FR-IEWL** Where the answer reports unmapped Types, one notice line lists each Type with its issue count, `No Type` standing for issues with no Type, and offers **Map Types**.
15. **MDL-FR-XUJY** **Map Types** opens a dialog with one row for each unmapped Type and a selector of **Epic**, **Feature**, **Task**, **Defect**, and **Leave out**. Confirm writes the choices through `"set delivery settings (settings)"` and the lens loads with `refresh` false. Confirm is disabled until a row holds a choice.
16. **MDL-FR-MWMO** Where the answer reports misplaced items or omitted relations, one notice line states their counts. Activating it lists each one with its issue numbers and the reason in words.
17. **MDL-FR-JPYU** An item with an open external blocker carries the **Blocked** marker of FLO-FR-IKMM. Its tooltip names each external blocker by number and title.
18. **MDL-FR-ECPZ** In a GitHub plan, each issue card shows its `Status` name on its bottom line (per `DBR-delivery-board.md` DBR-FR-HWHO), each group face shows it on its bottom line (DBR-FR-SBTR), and every item carries the tracker tag of FLO-FR-ULEM as its address (DBR-FR-XOOU).
18. **MDL-FR-RILW** In the lens, a card and a container show no excerpt of the item's description (FLO-FR-XCRP), for a `github` source and for a `file` source. Every other part of FLO-FR-CMPK applies. The issue view of a selected item still shows the full description (per `MIV-map-issue-view.md` MIV-FR-LENY).
    - *Why:* On the plan canvas a long issue body hides the structure, and the title is enough to find the item.
18. **MDL-FR-XKCH** In the lens, the panel of a selected item or milestone is its issue view (per `MIV-map-issue-view.md`). The lens shows no popover beside an item. The header of the issue view offers **Discuss** and, where MDL-FR-QEXS or MDL-FR-UKGH offers one, the claim or graduate action (per MIV-FR-POMI).
18. **MDL-FR-XCJN** **Discuss** in the header of the issue view has the effect of the **Discuss** entry of MDL-FR-HULA for that item or milestone (per MDL-FR-QELI).
19. **MDL-FR-QEXS** A selected task that is eligible for a claim, and that no GitHub-shadow draft or pending claim names, offers **Claim and implement** on the terms of `GIT-git.md` GIT-FR-NQTZ. A pending claim offers **Retry** on the terms of GIT-FR-FTHT.
20. **MDL-FR-UKGH** A selected item whose draft is a GitHub-shadow draft with the status `github_shadow`, and that no graduation run holds, offers **Implement**. It opens the start dialog of `GSD-graduation-start-dialog.md` for that draft.
21. **MDL-FR-PUKL** In a GitHub plan, the issue view of a container item, and the add affordance of FLO-FR-07, offer **Add existing issue**. It takes an issue number and applies the `add_existing_issue` edit into that container or the top level. A refusal shows its error text in place.
22. **MDL-FR-JRGK** For a `file` source while the GitHub configuration is `valid`, the band menu offers **Publish to GitHub**. A confirmation dialog states that GitHub becomes the source and that the file stays on disk unchanged. Confirm writes any pending changes and then invokes `"publish delivery to github"`.
23. **MDL-FR-WMGU** While a publish runs, the lens shows its progress and every edit affordance is inert. A failed publish shows the error text and the links of the items it created, and offers **Retry**, which invokes the publish again.
24. **MDL-FR-QIXG** For a `github` source, the band menu offers **Export to file**. A dialog takes a project-relative path, which ends in `.flow` and is prefilled from the Project title. Confirm invokes `"export delivery to file (path)"`. A refusal shows its error text in the dialog.
25. **MDL-FR-QAJN** The band menu offers **Reset layout**. It invokes `"save delivery layout (scope, layout)"` with an empty layout for the lens's scope, and the canvas lays the plan out again (per `FLO-flow.md` FLO-FR-PLUZ).
26. **MDL-FR-GDUP** The toolbar of the lens holds, after the lens switch of `SMP-specification-map.md` (SMP-FR-TKUS), a 160 × 4px progress track of the plan's work items (per `../core/FDL-flow-delivery.md` FDL-FR-KWMT) with a segment for done, in progress, and blocked items, and the legend `N done`, `N in progress`, `N blocked` over the same work items. A blocked item counts in the blocked segment alone, and each legend entry carries a dot in the colour of its segment.
26. **MDL-FR-MIMF** The toolbar of the lens holds, after the progress track, the **status filter**: a **Status** button that opens a menu. The menu holds one toggle for each board status, `backlog`, `ready`, `in progress`, `in review`, and `done`, each with its dot (per `DBR-delivery-board.md` DBR-FR-QZPA), and a **Blocked only** toggle. A status toggle is checked while the board shows that status.
26. **MDL-FR-WDMP** An item shows where its status is pressed and, while **Blocked only** is pressed, where it is blocked. A group shows where it passes the filter or a descendant shows, and each ancestor of a shown item shows. The milestone columns, the No milestone column, and an item with no status dot always show.
26. **MDL-FR-ZKXX** A proposal ghost card, an item with a proposal mark, each selected item, and the item of the open issue view always show. A request to show an item (MDL-FR-JIKK) selects it, so it shows, and it leaves the filter unchanged.
26. **MDL-FR-ZAGM** The lens reads the view of its layout scope through `"load delivery view (scope)"` when it first renders and when the scope changes, and writes each change of the filter through `"save delivery view (scope, view)"`. A failed read shows every item, and a failed write keeps the filter on screen. Each failure logs its typed error.
26. **MDL-FR-BWEF** While the filter hides an item, the **Status** button shows `N hidden`, the count of hidden cards and groups, and the menu offers **Show all**, which saves the empty view. The progress track, its legend, the group counts, and the readout count the whole plan.
26. **MDL-FR-XHNZ** **Fit** frames only the items that show. The status filter belongs to the Delivery lens alone: a Flow tab has no status filter.
27. **MDL-FR-NTVL** In a GitHub plan, the lens carries no action control and no discussions control. With a `file` source, both are present on the terms of FLO-FR-34 and FLO-FR-35.
28. **MDL-FR-YSMB** Opening the delivery's Flow file from the Project panel, Search, a Dashboard widget, or a plan tag (per `NAW-new-artifact.md` NAW-FR-TXKN) opens the Map tab on the Delivery lens and opens no Flow tab.
28. **MDL-FR-JIKK** A request to show a delivery item, from the Dashboard (per `DSH-dashboard.md` DSH-FR-DFDW) or a plan tag (per `NAW-new-artifact.md` NAW-FR-PHGO), opens the Map tab on the Delivery lens, selects that item, expands each collapsed container that holds it, and centres it. It opens no issue view (per `MIV-map-issue-view.md` MIV-FR-TTZU). An item that the status filter hides shows while it is selected (MDL-FR-ZKXX).
29. **MDL-FR-EFQF** When a Flow file with an open Flow tab becomes the delivery source, that tab writes its pending changes and closes (per `FLO-flow.md` FLO-FR-28).
30. **MDL-FR-EEDO** A request of `SDL-specification-delivery-links.md` to plan work (SDL-FR-CFHH) adds a `task` at the top level with the name and references it gives: through the `create_item` edit and then the `set_references` edit for a GitHub plan, and as an add of FLO-FR-07 for a file source. The new item becomes the selection.
31. **MDL-FR-HULA** A right-click on an item or a milestone of the canvas, and the context-menu key or `Shift+F10` on a focused item or milestone, opens a context menu that holds a **Discuss** entry. It is available for a `github` source and for a `file` source, for every kind of item and for milestones. Its leading icon follows `LCM-library-context-menu.md` LCM-FR-07.
32. **MDL-FR-QELI** **Discuss** hands the node reference of the item or milestone (`lens` `delivery`, `kind` `item` or `milestone`, and the item id) to the Discussion column of `SMP-specification-map.md` (SMP-FR-IHFM). The column shows the latest open discussion of that node, or the opening composer of a new one, and creates none. The menu closes, and the focus moves to the composer of the column.
33. **MDL-FR-JBEL** The menu opens at the item, takes the arrow keys, **Enter**, and **Escape**, and returns the focus to the item when it closes. A right-click does not change the selection of the canvas, and it opens no menu where the canvas holds an edit in progress.
34. **MDL-FR-WTTQ** In a `github` plan, a card of the Discussion column that names a plan item (per `MPV-map-proposal-review.md` MPV-FR-NMYT) selects that item on the canvas and centres it, on the terms of MDL-FR-JIKK.

## User stories
- As an architect, I want the delivery plan of my GitHub Project to appear on the map without building it by hand, so that the plan the team works from is the plan I read.
- As an architect, I want each change I make on the canvas to reach GitHub at once, so that nobody works from a stale copy and nothing needs a merge.
- As an architect, I want to claim a ready task from the plan where I see it, so that I start a graduation without going to another panel.
- As an architect who plans alone, I want the delivery to live in a Flow file, and to publish it to GitHub when the team joins.

## Wireframes
```
┌ ◈ Map ● ───────────────────────────────────────────────────────────────────────────────────────────┐
│ [Delivery|Specifications] │ ━━━━──── ● 3 done ● 5 in progress ● 8 blocked │ [Status · 3 hidden ▾]  │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ Payments v2  Description  [DOC payments-hld.md ↗] +        acme/payments ↗  read 2 min ago [Refresh] ⋯ │
│ ⚠ 5 issues have no kind: Spike (3), No Type (2)                                         [Map Types] │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ ┌ ▾ ⚑ Beta  in progress ── 1/3 ┐ ──▶ ┌ ▾ ⚑ GA  not started ─ 0/1 ┐                                 │
│ │ ┌───────────────────────────┐ │     │ ┌───────────────────────┐ │                                │
│ │ │ FTR #41               1/2 ▾│ │     │ │ TSK #53               │ │                               │
│ │ │ Card capture              │ │     │ │ Payout schedule       │ │                                │
│ │ │ ● in progress             │ │     │ │ ● ready     [blocked] │ │                                │
│ │ ├───────────────────────────┤ │     │ └───────────────────────┘ │                                │
│ │ │ ┌───────────────────────┐ │ │     └───────────────────────────┘                                │
│ │ │ │ TSK #51               │ │ │                                                                  │
│ │ │ │ Tokenize card numbers │ │ │                                                                  │
│ │ │ │ ● done                │ │ │                                                                  │
│ │ │ └───────────────────────┘ │ │                                                                  │
│ │ └───────────────────────────┘ │                                                                  │
│ └───────────────────────────────┘                                                                  │
├────────────────────────────────────────────────────────────────────────────────────────────────────┤
│ 16 items · 4 edges · 8 blocked · 1 overlap                                       − 100% + ⤢        │
└────────────────────────────────────────────────────────────────────────────────────────────────────┘

remove in a GitHub plan
┌ Remove "Refunds" ──────────────────────────┐
│ 3 items are under it.                      │
│ [Remove from board] [Close as not planned] │
│                               [Cancel]     │
└────────────────────────────────────────────┘
```
- Layout notes: the toolbar is the Map tab's toolbar (per `SMP-specification-map.md` SMP-FR-CVIB) and keeps its 38px height. The **Status** button is a small default button with a chevron, and it never wraps the toolbar. While the filter hides an item, the button carries `N hidden` in the monospace face. Its menu opens below the button, holds the status toggles as checkbox items with their dots and names, a separator, **Blocked only**, and **Show all**, and stays open while the author toggles. Escape and a click outside close it and give the focus back to the button. Below a tab width of 900px, the button shows `N hidden` and its chevron alone, and the legend of the progress track shows its numbers and dots without their words. Both keep the words in their accessible names. The menu never extends past the edge of the window. Its lens switch is the segmented control of the design system. The band is 44px high and sits between the toolbar and the canvas: the plan name in the title size, the description, the reference chips with their add affordance, and then, at the trailing edge, the source and the time of the read in the monospace face, **Refresh** as a small default button, and the band menu. The band menu items are in sentence case. The notice lines sit below the band, one line each, and never cover the canvas. Below the canvas, a 28px strip on the panel ground holds the element readout in the monospace face at its leading edge, and the action control, the discussions control, and the view controls at its trailing edge. The canvas and its add affordance are those of `FLO-flow.md`. The board inside the canvas, its cards, and its groups are those of `DBR-delivery-board.md`. The issue view of MDL-FR-XKCH covers the canvas as `MIV-map-issue-view.md` shows.

## UI contract boundary
- **Owned by the UI**: the resolution of which binding hosts the canvas, the loading, empty, error, and stale states, the band and its menu, the notice lines and the Map Types dialog, the pending marks and the restore after a refusal, the remove dialog, the claim, retry, and graduate actions on an item, the Add existing issue input, the publish and export dialogs and their progress, the roll-up of the toolbar, the status filter and which items it hides, and the routing of the delivery's Flow file to the Map tab.
- **Delegated to backend (abstract)**:
  - `"get delivery source"`, `"set delivery settings (settings)"`, `"publish delivery to github"`, and `"export delivery to file (path)"`, with the event `"delivery source changed"` — owned by `../core/DSR-delivery-source.md`.
  - `"load github delivery plan (refresh)"` and `"apply github delivery edit (edit)"` — owned by `../core/GDP-github-delivery-plan.md`.
  - `"save delivery layout (scope, layout)"`, `"load delivery view (scope)"`, and `"save delivery view (scope, view)"` — owned by `../core/DMS-delivery-machine-state.md`.
  - `"claim github task (issue number)"`, `"retry github claim (issue number)"`, and `"acknowledge github claim (issue number)"`, with the event `"github polling changed"` — owned by `../core/GPP-github-polling.md`.
  - The events `"project tree changed"` (owned by `../core/ASC-artifact-scanning.md`), `"drafts changed"` (owned by `../core/DRS-draft-storage.md`), and `"graduation run changed"` (owned by `../core/GRD-graduation.md`).

  The canvas and its own operations are owned by `FLO-flow.md`. The board layout and its drag and drop are owned by `DBR-delivery-board.md`. The start dialog is owned by `GSD-graduation-start-dialog.md`.

## Non-functional requirements
- The lens shows a cached GitHub plan before the network answers, and a slow or failed GitHub read never blocks reading the plan the cache holds.
- An edit to a GitHub plan never waits for a full read of the Project before it renders.
- A plan of several hundred items stays responsive on the terms of the non-functional requirements of `FLO-flow.md`.
