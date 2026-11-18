# Map issue view

**Spec code:** `MIV`

## Intent
The issue view is the large view of one item of the Delivery lens (`MDL-map-delivery-lens.md`). It looks like the issue page of a ticket system such as GitHub, Jira, or Linear: the title and the full rich body in a main column, and the fields of the issue in a sidebar. An issue body holds long Markdown, code, Gherkin scenarios, and diagrams, so the view is large enough to read them and renders them as rich text. Every edit of the selected item is available in it, so the architect reads and changes an issue in one place.

## Functional requirements
1. **MIV-FR-SVXH** In the Delivery lens, a click on a card, a group header, or a milestone header (per `DBR-delivery-board.md` DBR-FR-MYVI) selects the item and opens its **issue view**. **Enter** on a focused card or header does the same. This applies to every kind and to milestones, for a `github` source and for a `file` source.
2. **MIV-FR-DYXQ** In the Delivery lens, a card and a header hold no rename field of their own, in place of the in-place rename of `FLO-flow.md` FLO-FR-09. A name is renamed only in the issue view, through **Rename** next to the title (MIV-FR-XMMS).
3. **MIV-FR-TTZU** These open no issue view: a ghost card (per `DBR-delivery-board.md` DBR-FR-WKNI), a selection of more than one item, the activation of a **Blocked** or **Overlap** marker (DBR-FR-ZBVZ), a request to show an item (per `MDL-map-delivery-lens.md` MDL-FR-JIKK, MDL-FR-WTTQ, and MDL-FR-EEDO), and a reference in the review sheet (per `MPV-map-proposal-review.md` MPV-FR-NMYT).
4. **MIV-FR-HULC** The issue view shows only while its item is the single selection of the canvas. A change of the selection to no item or to another item closes it. A drag that starts closes it.
5. **MIV-FR-YPWK** The header of the issue view has a close control with the tooltip `Close`. The close control and **Escape** close the view. The item stays selected, and the focus moves to its card or header.
6. **MIV-FR-SWZY** **Escape** in a field of the issue view acts on that field alone: it cancels a rename, and it leaves the description source. The view stays open.
7. **MIV-FR-BEZE** When the issue view closes, a rename or a description edit that is in progress is committed once.
8. **MIV-FR-HOAE** An issue view that a click or a key opens takes the focus. An issue view that shows again after a lens switch or a tab switch takes no focus.
9. **MIV-FR-RPAU** The open issue view is view state of the Delivery lens (per `SMP-specification-map.md` SMP-FR-RNLH). A lens switch or a tab switch, and a return, shows it again.
10. **MIV-FR-ITSA** The issue view and the review sheet (per `MPV-map-proposal-review.md` MPV-FR-CTTA) never show together. When the review sheet opens, the issue view closes and the item stays selected. While the review sheet shows, a click on an item selects it and opens no issue view.
11. **MIV-FR-HGAQ** The issue view is a region of the Map tab, not a floating overlay (per `SNV-shell-navigation.md` SNV-FR-56). It closes no overlay, and no overlay closes it. The remove dialog (per `MDL-map-delivery-lens.md` MDL-FR-XQGD) shows above it.
12. **MIV-FR-HGGS** The issue view shows over the canvas of the Delivery lens, centred, at 90% of its width and 90% of its height, with no scrim. It is a non-modal dialog. Its accessible name is the address and the name of the item, for example `Issue #41 Card capture`, or `Milestone Beta`.
13. **MIV-FR-POMI** The header shows the kind chip (per `DBR-delivery-board.md` DBR-FR-DZSU) or the milestone flag, the tracker tag (per `FLO-flow.md` FLO-FR-ULEM) where the item has one, the status dot and its word, the pending mark (per MDL-FR-PUCM), **Discuss** (per MDL-FR-XCJN), the claim, retry, or graduate action (per MDL-FR-QEXS and MDL-FR-UKGH), and the close control.
14. **MIV-FR-THGX** The body of the issue view is a main column and a sidebar 280px wide at its trailing edge. Where the view is narrower than 720px, the sidebar goes below the main column.
15. **MIV-FR-XMMS** The main column shows, in this order: the name of the item as the title, with **Rename**; the error text of the item (per `DBR-delivery-board.md` DBR-FR-CJSG); the **Description** section; the **Sub-issues** section (MIV-FR-ZQAX); and **Add existing issue** for a container (per MDL-FR-PUKL).
16. **MIV-FR-LENY** The **Description** section renders the description as rich text (MIV-FR-RNVP). Its **Edit** control shows the source in a plain field on the terms of `FLO-flow.md` FLO-FR-13, and **Done** returns to the rich text.
17. **MIV-FR-VHDP** The sidebar shows these fields, in this order, each under an uppercase label: **Status**, **Assignees**, **Labels**, **Milestone**, **Kind**, **Due**, **Blocked by**, **Draft**, **Footprint**, **References**, and **Specs** (per `SDL-specification-delivery-links.md` SDL-FR-KNOP). A field that does not apply to the item or to the source is absent. **Remove** is at the end, on the danger tone.
18. **MIV-FR-NBHF** **Assignees**, **Labels**, and **Milestone** apply to an issue of a `github` source. **Due** applies to a milestone. **Draft** and **Footprint** apply where `FLO-flow.md` offers them for the kind. **Blocked by** applies where the item has a blocker.
19. **MIV-FR-PGEC** **Blocked by** lists each item of the plan that blocks the item, with its kind chip, its address, and its name. Activating one selects that item, shows it on the canvas, and opens its issue view. Each external blocker (per MDL-FR-JPYU) is a `#number title` link that invokes `"open tracker link (url)"` with its URL.
20. **MIV-FR-HHQR** Every edit that `FLO-flow.md` offers on the selected item is available in the issue view, with the same effect: rename, description, kind, due date, draft actions, footprint, references, and remove. The issue view is inert on the terms of the canvas.
21. **MIV-FR-EDKQ** For an issue of a `github` source, the issue view invokes `"read github plan issue (number)"` once when it opens, and once more each time the `updated_at` of the item changes. A milestone and an item of a `file` source invoke nothing.
22. **MIV-FR-DFVC** While the read runs, **Labels**, **Milestone**, and **Sub-issues** show `Loading…`. A newer answer replaces an older one, and an answer for an item that the view no longer shows is discarded.
23. **MIV-FR-BCOP** An answer marked `stale` shows its milestone and its sub-issues, and **Labels** shows `Labels could not be read`. A refusal shows `Could not be read` in **Labels**, **Milestone**, and **Sub-issues**, and logs a warning with the error code.
24. **MIV-FR-HSRA** **Labels** shows each label name as a neutral badge, and **Milestone** shows the milestone title. Each shows `None` where the answer holds none.
25. **MIV-FR-ZQAX** The **Sub-issues** section lists the sub-issues of the answer, with their count in its label. A sub-issue that is an item of the plan shows its kind chip, its address, its name, and its status dot, and activating it opens its issue view as MIV-FR-PGEC does. Another sub-issue is a `#number` link that invokes `"open tracker link (url)"`.
26. **MIV-FR-TZKJ** The **Sub-issues** section is absent for a milestone, for an item of a `file` source, and where the answer holds no sub-issue.
27. **MIV-FR-RNVP** The description renders as the rich view of `DTV-document-text-viewer.md` DTV-FR-SSQI, with headings, nested lists, tables, emphasis, code, quotes, and image placeholders. It runs no script, renders no raw HTML as elements, loads no remote resource, and a link does not navigate.
28. **MIV-FR-DDAF** A fenced code block keeps each line break and indentation, scrolls horizontally in place of a wrap, sits on the sunken ground in the monospace face, and shows its language as a label at its trailing edge.
29. **MIV-FR-ALPJ** The language of a fenced code block is the first word of its info string, with case ignored and a leading `.`, `{`, or `language-` removed. Where `highlight.js` knows that word as a language or an alias, or the extension map of `ESH-editor-source-files.md` names it, the block is highlighted in that language. Otherwise it is plain.
30. **MIV-FR-IVGV** A fenced code block is never put to autodetection.
31. **MIV-FR-EDFS** A fenced code block with the language `gherkin` or `feature` is highlighted as Gherkin. Its keywords, for example `Feature:`, `Scenario:`, `Given`, `When`, `Then`, `And`, and `But`, take the keyword role, and its tags, strings, and comments take their own roles.
32. **MIV-FR-GZTZ** Token colours in a fenced code block are the theme-aware token roles of `ESH-editor-source-files.md` ESH-FR-YNIV. The block text is the same character for character as the source.
33. **MIV-FR-CTJU** A fenced code block with the language `mermaid` shows its diagram, inert and with no controls (per `EDT-editor.md` EDT-FR-LEUI). While the diagram renders it shows its source. A diagram that fails to render shows its source and one error line.
34. **MIV-FR-IHRB** The issue view logs its open, its close, and the outcome of each detail read with its error code. No record holds a title, a body, a name, or a label.

## User stories
- As an architect, I want to open an issue of the plan in a large view, so that I read its full body without a narrow popover.
- As an architect, I want the issue body to show code, Gherkin scenarios, and diagrams as rich content, so that I read the acceptance criteria as the team wrote them.
- As an architect, I want the labels, the milestone, and the sub-issues of an issue beside its body, so that I do not go to GitHub to see them.
- As an architect, I want to change an issue in the same view where I read it.

## Wireframes
```
┌ Delivery lens canvas ─────────────────────────────────────────────────────────────────────────┐
│  ┌ FTR #41 · ● in progress ·                          [Discuss] [Claim and graduate]   ✕ ┐  │
│  ├──────────────────────────────────────────────────────────────┬────────────────────────┤  │
│  │ Card capture                                        Rename   │ STATUS                 │  │
│  │                                                              │ ● in progress          │  │
│  │ DESCRIPTION                                           Edit   │ ASSIGNEES              │  │
│  │ Capture a card at checkout.                                  │ @mira                  │  │
│  │                                                              │ LABELS                 │  │
│  │ ## Acceptance                                                │ [payments] [p1]        │  │
│  │ ┌──────────────────────────────────────────────── gherkin ┐ │ MILESTONE              │  │
│  │ │ Feature: Card capture                                    │ │ Beta                   │  │
│  │ │   Scenario: A valid card                                 │ │ KIND                   │  │
│  │ │     Given a cart with one item                           │ │ [Feature ▾]            │  │
│  │ │     When the buyer enters a valid card                   │ │ BLOCKED BY             │  │
│  │ │     Then the order is paid                               │ │ TSK #53 Payout schedule│  │
│  │ └──────────────────────────────────────────────────────────┘ │ REFERENCES             │  │
│  │                                                              │ [DOC payments-hld.md]  │  │
│  │ SUB-ISSUES 2                                                 │ SPECS                  │  │
│  │ TSK #51 Tokenize card numbers                         ● done │ PAY Card capture ●     │  │
│  │ #77                                                        ↗ │                        │  │
│  │ ADD EXISTING ISSUE                                           │                        │  │
│  │ [#123                    ] [Add]                             │ [Remove]               │  │
│  └──────────────────────────────────────────────────────────────┴────────────────────────┘  │
└───────────────────────────────────────────────────────────────────────────────────────────────┘
```
- Layout notes: the view has the box of the review sheet: the elevated ground, a 1px border, the medium radius, and the raised shadow. The header is 40px high on the panel ground with a bottom border. The main column is on the canvas ground, scrolls on its own, and holds its content at most 760px wide and centred. The title is 20px with weight 600 and wraps. The sidebar is on the panel ground with a 1px border at its leading edge and scrolls on its own. Its labels are the uppercase micro labels of the design system, and 1px rules divide its fields. A fenced code block takes the full width of the main column.

## UI contract boundary
- **Owned by the UI**: when the issue view opens and closes, its focus, its layout, the rendering of the description and its fenced code blocks, the resolution of a fence language, the Gherkin and other token colouring, the Mermaid rendering, the sidebar fields, the order and discard of detail reads, and every edit control the view holds.
- **Delegated to backend (abstract)**:
  - `"read github plan issue (number)"` — owned by `../core/GDP-github-delivery-plan.md`.
  - `"open tracker link (url)"` — owned by `../core/GDP-github-delivery-plan.md`.

  The edits of the view are those of `MDL-map-delivery-lens.md` and `FLO-flow.md`, and they go through the operations those specs delegate.

## Non-functional requirements
- The issue view opens without a wait for the detail read: the title, the body, and the fields of the plan show at once.
- Highlighting a fenced code block of a few thousand lines does not freeze the view.
- Every control of the issue view is reachable by keyboard. The view does not trap the focus: the Discussion column and its composer stay usable while it shows.
