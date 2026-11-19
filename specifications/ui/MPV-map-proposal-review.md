# Map proposal review

**Spec code:** `MPV`

## Intent
The surface on which the author decides the work that an agent proposed for the delivery plan in a discussion: a Map thread, or any other discussion of the project. The author reviews a proposal in the tab where the discussion shows, so that an issue that comes up in the discussion of a draft, a file, a note, or a change is filed without a move to the Map tab. The discussion keeps one short summary row for each proposal, so a long proposal never buries the conversation. The review itself opens as a sheet over the Delivery canvas, on its own when the agent proposes or updates, and closes on its own when the author decided the last row: a list of the proposed changes on the left, and the selected change on the right, with its field changes and its description diff typeset as the issue reads, code blocks and Mermaid sources included. The author accepts or rejects each change on its own, and steers the agent from the composer while the sheet shows: an update of the agent redraws the sheet in place. Nothing reaches GitHub, and no draft is created, before the author accepts the change that does it. The surface follows the pattern of `DCR-draft-change-review.md`: a decision is per item, a review bar steps through the items, a refusal shows beside the item it names, and every control works by keyboard alone. Out of scope: the proposal and the application of a row, which are `../core/MPR-map-plan-proposals.md`; and the tool that records a proposal, which is `../tools/PPT-propose-plan-changes-tool.md`.

## Functional requirements

### Operations and statuses
1. **MPV-FR-UXHR** Each row of a proposal has one **operation**: `added` for a `create_issue` row, `withdrawn` for a `modify_issue` row that closes the issue with the reason `not_planned`, `changed` for every other `modify_issue` row, and `draft` for a `new_draft` row.
2. **MPV-FR-JXFA** Each row has one **status**: `applying` while its decision runs, else `pending` for `waiting`, `applied` for `accepted`, `rejected` for `rejected`, and `failed` for `failed`.
3. **MPV-FR-LOSO** The **status of a proposal** is `applying` while any row is `applying`, else `pending` while any row is `pending`, else `failed` while any row is `failed`, else `rejected` when every row is `rejected`, else `applied`.
4. **MPV-FR-FDDK** A status always shows as a dot and its lowercase word together. `pending` uses `--warn`, `applying` uses `--live` with a pulsing dot, `applied` uses `--ok`, and `rejected` and `failed` use `--danger`. The one exception is a list entry (MPV-FR-ENVX), which shows the dot alone with the word as its accessible name.
5. **MPV-FR-WKNB** The **summary** of a proposal counts its rows by operation in the order `changed`, `added`, `withdrawn`, `draft`, and leaves out a count of zero: for example `2 changed, 1 added, 1 withdrawn`. A proposal of one issue row starts with the issue reference: `#18, 1 changed`.

### Summary row in the discussion
6. **MPV-FR-IBSF** A comment that carries a `map_plan_proposal` attachment renders one **summary row** below the comment body, read with `"list map plan proposals (discussion id)"`. The comment body is the agent's rationale. The discussion shows no other part of the proposal.
7. **MPV-FR-QZUD** A comment whose `map_plan_proposal` attachment carries `update` (per `../core/CMS-comments-storage.md` CMS-FR-BSQB) renders the summary row with the counts of the update in place of the summary: `Updated: 1 changed, 1 added, 1 removed`, with a count of zero left out. `removed` counts the rows that the agent withdrew. Its button opens the sheet of the same proposal.
8. **MPV-FR-KCOB** The summary row shows, in one line: the `CHG` chip, the name `Plan proposal`, the summary (MPV-FR-WKNB) in muted text, the status of the proposal, and one button. The summary ends in an ellipsis where the line does not fit. Nothing in the row overflows the Discussion column at its narrowest width.
9. **MPV-FR-GYLV** While the proposal is `pending` and at least one row is decided, the status shows the decided count, for example `1 of 4`. When no row is `pending`, the status shows the time of the proposal in the relative form.
10. **MPV-FR-RJKZ** The button reads **Review** while the proposal is `pending` and the sheet does not show it, **Show review** when no row is `pending` and the sheet does not show it, and **Hide review** while the sheet shows it. It opens or closes the sheet of that proposal.
11. **MPV-FR-FNIS** **Review** or **Show review** on the Specifications lens switches the Map tab to the Delivery lens and opens the sheet there.

### Review sheet
12. **MPV-FR-UWQL** When a comment with a `map_plan_proposal` attachment arrives in a discussion that a tab shows (MPV-FR-HGLN), and that proposal has a `pending` row, the sheet opens that proposal without a click. On the Specifications lens, the Map tab first switches to the Delivery lens. A comment that the discussion held when the tab showed it opens nothing.
13. **MPV-FR-KHDP** A sheet that opens on its own (MPV-FR-UWQL) takes no focus. The focus stays where it was, for example in the composer.
   - *Why:* the author often types in the composer to steer the agent, and a sheet that takes the focus breaks that typing.
14. **MPV-FR-CTTA** The **review sheet** shows over the canvas of the Delivery lens, centred, 90% of its width and 90% of its height. It covers the canvas only. The Discussion column and its composer stay usable while the sheet shows, and no scrim covers them. At most one sheet shows at a time, and the sheet never shows together with the issue view (per `MIV-map-issue-view.md` MIV-FR-ITSA). Outside the Map tab, the sheet shows on the terms of MPV-FR-ZQRE.
14. **MPV-FR-HGLN** The summary row and the sheet work in every tab that shows a discussion, and not in the Map tab alone: the New Artifact tab and the Editor tab with the Discussion column beside the document (per `ACT-action-control.md` ACT-FR-19), the tab of an OpenSpec change, the Flow tab, the Diff tab, and the History detail tab with the floating discussion panel (per ACT-FR-20), and a conversation tab (per `TAB-tabs.md` TAB-FR-QXMV). A discussion with a fragment target and a discussion with none alike show it. Each tab holds its own sheet and its own state of the proposals (MPV-FR-CZDO).
14. **MPV-FR-ZQRE** Outside the Map tab, the sheet shows over the content of the tab: the draft, the file, the change, the Flow, or the diff, centred, 90% of its width and 90% of its height. The Discussion column, the floating discussion panel, and the composer stay usable while the sheet shows, and no scrim covers them. In a conversation tab, the messages of the discussion are the content, so the sheet covers them and leaves the composer usable. At most one sheet shows in a tab at a time.
14. **MPV-FR-CUWB** Outside the Map tab, the sheet shows no Map lens and selects nothing on a canvas. **Open draft** of an accepted `new_draft` row opens the draft in the New Artifact tab. The link of an applied issue row names the repository of the delivery source. The kind chip of a `create_issue` row shows the kind that its Type maps to (per `../core/DSR-delivery-source.md` DSR-FR-ZJMC), and a `modify_issue` row shows no kind chip.
15. **MPV-FR-PBAD** The sheet header holds the layers icon, `Plan proposal`, the count of rows in monospace (`4 issues`, `1 issue`), the summary in monospace at the trailing edge, and a close control. The close control and **Escape** close the sheet and put the focus back on the button of the summary row.
16. **MPV-FR-ENVX** A list of 248px at the left holds one **entry** per row, in proposal order. An entry shows the operation badge, the kind chip, the reference (`#18`, or `new` for an added issue), the status dot, and the title on at most two lines. The selected entry has the active ground.
17. **MPV-FR-SNCR** The operation badge of `changed` uses the accent tone, `added` the ok tone, `withdrawn` the danger tone, and `draft` the neutral tone. The kind chip is the kind chip of the Delivery board (per `DBR-delivery-board.md`). A row whose Type the plan does not name shows no kind chip.
18. **MPV-FR-WQWG** The **detail** at the right shows the selected row: its kind chip, its reference, its operation badge, its status, its title at 15px and weight 600, its field changes (MPV-FR-CCDM), its body diff (MPV-FR-ZFDO), and its footer (MPV-FR-PIQU). Its content is at most 720px wide and centred.
19. **MPV-FR-NMYT** The reference of a `changed` or `withdrawn` row in the detail selects that item on the Delivery lens canvas (per `MDL-map-delivery-lens.md`) and opens no issue view. Where the item is not on the canvas, and in every tab other than the Map tab, the reference is plain text.
20. **MPV-FR-CAQL** When the sheet opens, it selects the first `pending` row, else the first row. After a decision on the selected row, the sheet selects the next `pending` row after it, else the first `pending` row, else keeps the selection.
21. **MPV-FR-GRTP** An update of the proposal that the sheet shows redraws the sheet in place. A changed row shows its new fields and its new status. An added row takes its place in the list. A withdrawn row leaves the list. The selection stays on its row while that row is in the list, else moves as MPV-FR-CAQL states.
22. **MPV-FR-BBQY** After a decision made in the sheet leaves no row `pending`, the sheet closes on its own, and the focus goes to the button of the summary row. Where that decision failed, the sheet stays open and selects the failed row.
23. **MPV-FR-VXHC** **Accept all** and **Reject all** close the sheet when they end with no row `pending` and no row that their run failed. Where a row of the run failed, the sheet stays open and selects the first failed row. A decision made in another window does not close the sheet.

### Field changes
24. **MPV-FR-CCDM** The field changes of a row are a grid with an uppercase label column of 84px and a value column. A changed field shows the value before struck through on the removed-diff tone, an arrow, and the value after on the added-diff tone. The fields are Title, Type, Milestone, Parent, Status, Labels, and State, in that order.
25. **MPV-FR-JBNC** A label change shows each added label as `+ name` on the added-diff tone and each removed label as its name struck through on the removed-diff tone. A Status value carries the status dot of the board. A milestone and a reference use the monospace face.
26. **MPV-FR-LYKS** A close change shows the field State as `open` to `closed as completed` or `open` to `closed as not planned`. The reason is part of the accessible name of the **Accept** control of the row.
27. **MPV-FR-OCYF** An `added` row shows each field it sets as a value after alone, on the added-diff tone: Milestone, Type, Parent, and Labels. A `draft` row shows its name as the title and no field change.

### Body diff
28. **MPV-FR-ZFDO** A row that changes the body, an `added` row with a body, and a `draft` row show a **body diff**: a box with a label strip `Body` and a count at the trailing edge, then the blocks of the body typeset as the issue reads, never as raw Markdown and never as `+` and `-` lines.
29. **MPV-FR-WYTL** The body diff parses the old and the new Markdown into blocks, pairs the blocks that are the same, and marks each other block as added or removed. An added block has a `+` in a 24px gutter on the added-diff tone. A removed block has a `−` in the gutter, struck through on the removed-diff tone.
30. **MPV-FR-PPHV** A removed block directly followed by an added block of the same kind with similar words is one **edited** block. It shows the new block with the removed words in `<del>` and the added words in `<ins>`, inline, and no gutter mark.
31. **MPV-FR-VMYF** A run of unchanged blocks between changes shows as one line, `N unchanged blocks`, that expands in place when activated. An unchanged block next to a change shows in full. The count of the label strip names the added, removed, and edited blocks, for example `3 blocks added, 2 edited`, and an `added` or `draft` row shows `new description`.
32. **MPV-FR-PVXW** The body diff typesets a heading, a paragraph, a quote, a list item, a fenced code block, and inline code, emphasis, and strong text. A block of another kind shows as its plain text.

33. **MPV-FR-ZCWQ** A fenced code block shows as a code block of the issue: the monospace face on the sunken ground, each line break and indentation kept, and a horizontal scroll in place of a wrap. Its language, for example `mermaid`, is a label at its trailing edge. A Mermaid block shows its source, not a diagram.
34. **MPV-FR-JVZN** A removed code block directly followed by an added code block of the same language is one **edited** code block with no gutter mark of its own. Inside it, an added line has `+` on the added-diff tone, and a removed line shows in its place with `−`, struck through on the removed-diff tone. An unchanged line shows plain.
35. **MPV-FR-RQWN** A quote shows its text in the muted tone with a 2px bar in `--border-2` at its leading edge.

### Decisions
36. **MPV-FR-PIQU** The footer of the detail depends on the status of the row. `pending`: **Discuss**, **Reject**, and **Accept**. `applying`: the status and `Sending to GitHub`. `applied`: `Applied to GitHub` and the `owner/repo#number` link of the issue, or `Created the draft` and **Open draft**. `rejected`: `Rejected. Nothing was sent to GitHub.` and **Undo**.
37. **MPV-FR-XYDR** **Accept** invokes `"accept map plan row (discussion id, proposal id, row key)"` and **Reject** invokes `"reject map plan row (discussion id, proposal id, row key)"`, for that row alone. **Undo** invokes `"reopen map plan row (discussion id, proposal id, row key)"`. A decision changes no other row.
38. **MPV-FR-PTUT** **Discuss** puts the focus on the composer of the discussion of the proposal. In the Map tab, the item of the row is its focus node, as **Discuss** of the issue view does (per `SMP-specification-map.md` SMP-FR-IHFM). An `added` or `draft` row keeps the focus node of the thread. The sheet stays open.
39. **MPV-FR-WQKV** The footer of a `failed` row shows a one-line strip on the danger-soft ground with the typed error text. A `changed_on_tracker` error says that the issue changed on GitHub after it was read, and that nothing was sent. A failed row offers neither **Accept** nor **Reject**.
40. **MPV-FR-AAFF** Below the strip, a failed row shows `@agent was asked to prepare it again` where the proposal records a re-prepare request (per `../core/MPR-map-plan-proposals.md` MPR-FR-LJRO). Otherwise it shows **Ask @agent to re-prepare**, which invokes `"request map plan reprepare (discussion id, proposal id)"`. The control is disabled while a row is `pending`. `agent` is the handle of the agent that proposed.
40. **MPV-FR-CGOG** A `changed` row whose new parent is an `added` row of the same proposal shows the Parent field change with the title of that row as the value after. The rules of MPV-FR-FUQG and MPV-FR-YEIH apply to it.
41. **MPV-FR-FUQG** A row whose parent is an `added` row of the same proposal shows `Waiting for its parent` with the title of the parent while that parent is `pending`. Its **Accept** is disabled with that reason as its accessible description. When the parent is applied, the row is accepted as any row is.
42. **MPV-FR-YEIH** A row whose parent row is rejected or failed shows that reason in words, for example `Its parent was rejected`, and its **Accept** stays disabled. Its **Reject** stays available while it is `pending`.
43. **MPV-FR-QOPR** While a decision runs, its row is `applying`, and the controls of that row are disabled. The other rows stay operable.
44. **MPV-FR-JHCG** A refused **Undo** shows its typed error text in the footer of the row, and the row stays `rejected`.

### Review bar
45. **MPV-FR-MTSP** While any row is `pending`, a **review bar** on the accent-soft ground ends the sheet: `reviewing change n of N` for the selected row, **Previous**, **Next**, a flexible space, **Reject all**, and **Accept all**. **Previous** is disabled on the first row and **Next** on the last.
46. **MPV-FR-TWSY** **Accept all** accepts each `pending` row that is not blocked, one after the other, in proposal order. **Reject all** rejects each `pending` row the same way. A refused or failed row does not stop the next one. Rows that are not `pending` are not touched.
47. **MPV-FR-IZLJ** When no row is `pending`, the review bar is a neutral line on the panel ground: `All rows decided: A applied, R rejected`, and `, F failed` where any row failed.

### State, keyboard, and feedback
48. **MPV-FR-CZDO** The summary row, the sheet, and the map marks of `DBR-delivery-board.md` read one shared state of the proposals of the tab. That state follows `"map plan proposals changed"` and re-reads the proposals of the discussion. A decision made in another window shows without a reload.
49. **MPV-FR-XAOG** Every list entry is focusable and **Enter** or **Space** selects it. Every control of the detail, the review bar, and the expand line of the body diff is reachable and operable by keyboard alone. A change of the status of a row is announced to assistive technology.
50. **MPV-FR-FESO** The composer of the discussion stays usable while a proposal has a `pending` row.
51. **MPV-FR-WKVL** An applied row that wrote to GitHub or created a draft shows an Info toast, and a failed decision shows an Error toast (per `NTF-notifications.md` NTF-FR-TSRL). The row keeps its own status and error.
52. **MPV-FR-XYXY** A summary row whose rows are all decided stays in the thread as the record of what was decided. Its sheet stays readable, and offers only **Undo** and the re-prepare control.
53. **MPV-FR-KMJX** The surface logs a decision, its outcome, and its typed error through the frontend logging helper. No record carries a title, a body, a label name, or a draft name.

## Wireframes
```
Discussion column
┌──────────────────────────────────────────────────────────────────┐
│ CHG  Plan proposal  2 changed, 1 added, 1 withdr…  ● pending  1 of 4 [Hide review] │
└──────────────────────────────────────────────────────────────────┘

Delivery canvas
┌ ◇ Plan proposal  4 issues ─────────────── 2 changed, 1 added, 1 withdrawn  ✕ ┐
│┌ list (248px) ─────────┐┌ detail ──────────────────────────────────────────┐│
││ changed FTR #18     ● ││ FTR #18 changed                       ● pending  ││
││ Model serving …       ││ Model serving integration                        ││
││ changed FTR #11     ● ││ TITLE      ~~old~~                               ││
││ added   FTR new     ● ││            new                                   ││
││ withdrawn FTR #16   ● ││ MILESTONE  ~~M2~~ → M1                           ││
││                       ││ ┌ BODY ───────────────── 3 blocks added, 2 edited ┐││
││                       ││ │   Description                                  │││
││                       ││ │   Text with ~~old~~ new words inline.         │││
││                       ││ │ + Added paragraph                              │││
││                       ││ │ ⇕ 2 unchanged blocks                           │││
││                       ││ │   ┌──────────────────────────────── mermaid ┐ │││
││                       ││ │   │   flowchart LR                          │ │││
││                       ││ │   │ −   A --> B                             │ │││
││                       ││ │   │ +   A --> C                             │ │││
││                       ││ │   └─────────────────────────────────────────┘ │││
││                       ││ └────────────────────────────────────────────────┘││
││                       ││ Discuss →                      [Reject] [Accept] ││
│└───────────────────────┘└──────────────────────────────────────────────────┘│
│ reviewing change 1 of 4  Previous  Next             Reject all [Accept all] │
└─────────────────────────────────────────────────────────────────────────────┘
```
- Layout notes: the summary row has the shape of the `CHG` row of a draft discussion: 36px high, a 1px `--border-1` border, the `--r-md` radius, and the canvas ground. The sheet uses the elevated ground, a 1px `--border-2` border, the `--r-md` radius, and `--shadow-4`, the only shadow of the surface. The host of the sheet is the positioned box of the canvas area. The list uses the panel ground and the detail the canvas ground. Ids, counts, and references use the monospace face. Text is sentence case, with no emoji and no dingbats. All colours, radii, and motion come from the design-system tokens in `src/styles/colors_and_type.css`, and the classes carry the `pp-` prefix.

## UI contract boundary
- **Owned by the UI**: the operation and the status of a row and of a proposal, the summary, the summary row and its update counts, the sheet and its selection, the opening of the sheet when a proposal or an update arrives, its closing when no row is `pending`, the field changes, the body diff, the review bar, the busy state, the keyboard order, the announcements, and the toasts.
- **Delegated to backend (abstract)**:
  - `"list map plan proposals (discussion id)"` — owned by `../core/MPR-map-plan-proposals.md`.
  - `"accept map plan row (discussion id, proposal id, row key)"` — owned by `../core/MPR-map-plan-proposals.md`.
  - `"reject map plan row (discussion id, proposal id, row key)"` — owned by `../core/MPR-map-plan-proposals.md`.
  - `"reopen map plan row (discussion id, proposal id, row key)"` — owned by `../core/MPR-map-plan-proposals.md`.
  - `"request map plan reprepare (discussion id, proposal id)"` — owned by `../core/MPR-map-plan-proposals.md`.
  - `"map plan proposals changed"` — an event of `../core/MPR-map-plan-proposals.md`.

## Non-functional requirements
- A sheet of 20 rows renders without a visible delay, and a body diff of 200 blocks renders without a visible delay.
- The surface draws no raw HTML from a title, a body, or a label.
- Both themes, Paper and Graphite, take every colour from the tokens.
