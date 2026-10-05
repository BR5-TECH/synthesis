# Draft discussion

**Spec code:** `DDS`

## Intent
The two-column shape of the New Artifact tab, where an author and one or more agents work on one draft together. The draft's prompt and the discussion about it stand side by side and both stay visible, and an agent's proposed edits are decided **inside the document** as accept/reject hunks in the prose they change. It exists because a draft is written in conversation: a discussion that covers the draft, or a proposal that hides the discussion, makes the author choose which half of the work to look at. Acceptance is per hunk, so one objection holds up one change rather than the whole proposal. Out of scope: the tab's identity, its creation, its History rail, graduation, and every action a draft affords, all of which are `NAW-new-artifact.md`'s; what a proposal is and what deciding one writes, owned by `../core/DCP-draft-change-proposals.md`; the transaction an acceptance is performed as, owned by `../core/DHS-draft-history.md`; how a conversation renders and what posting to one costs, owned by `CMT-comments.md`; and the editing surface itself, owned by `EDT-editor.md`.

## Functional requirements
1. **DDS-FR-KTVW** The New Artifact tab renders one **document column** and one **discussion column** side by side, separated by a splitter. Both are visible whenever the tab is (per `NAW-new-artifact.md` NAW-FR-01). The tab opens no floating chat window and no review modal. The discussion column renders the shared `DiscussionSurface` and `DiscussionComposer` (per `CVP-conversation-presentation.md`), so a draft's discussion looks and behaves as a discussion does in every other owner surface.
2. **DDS-FR-TGBX** The document column renders the prompt on the **page** every document surface uses — an inset sheet with its own edge on a recessed field (per `EDT-editor.md` EDT-FR-63). The page takes the column's measure where the column is narrower than the page's own, and runs to the foot of the column with the same inset there as at its head.
3. **DDS-FR-KPSZ** The discussions of **prompt fragments** render in a column **below** the page, on the arrangement `CMT-comments.md` CMT-FR-64 defines. The document column lends them no margin beside the page.
4. **DDS-FR-PNXR** The split holds three presets — document 70, even, and discussion 70 — cycled by one accelerator, and the splitter is also dragged. The chosen ratio is restored per draft (per `SNV-shell-navigation.md` SNV-FR-08).
5. **DDS-FR-XQMF** One control in the tab's action row hides and shows the discussion column, standing in both states. The document column takes the tab's full width while the other is hidden. Whether it is hidden is restored per draft (per `SNV-shell-navigation.md` SNV-FR-08). The presets, the accelerator and the splitter of DDS-FR-PNXR hide neither column, and stand down while one is hidden.
6. **DDS-FR-HGMB** The discussion renders each message with the document's typography, real lists, and inline code, under an author line that names the author and the author's role.
7. **DDS-FR-ZRPT** The **document column** is where a proposal is reviewed. It renders the hunks, the review bar, the action chip, and the gutter map that `DCR-draft-change-review.md` defines, and the tab opens no separate surface for them.
8. **DDS-FR-NWRL** A draft's discussions are rendered in this column, the whole-target ones and the fragment ones alike, as the shared surface, and nowhere else. Each is hosted by this column for as long as the tab is open, and the single reveal route of `CVP-conversation-presentation.md` focuses it here rather than opening a second surface. No second rendering of a draft discussion exists anywhere.
9. **DDS-FR-DPRJ** The discussion follows an arriving message only while it is scrolled to the tail. Its header reads `following` while it does. The header states the follow state only while the column renders a transcript, and states nothing while it renders none.
10. **DDS-FR-GKMT** Scrolling the discussion away from the tail stops it following. Its header reads `paused · reading history`, an arriving message lands below a divider naming how many are unread, and a control offering the jump to the tail appears. A column that renders no transcript follows nothing and shows none of these three.
11. **DDS-FR-WCHN** An arriving message never scrolls the document column. The document moves for hunk navigation and for the author's own scrolling, and for nothing else.
12. **DDS-FR-SVBL** While a proposal stands and the discussion is not at the tail, an **anchor bar** below the discussion header names the proposal and offers the return to the hunk under review.
13. **DDS-FR-CLBK** A discussion longer than the column shows the messages above the viewport as one row naming how many they are, which expands in place.
14. **DDS-FR-QPHL** The discussion column's shared composer holds its trailing end clear of the tab's action control, by the measure `ACT-action-control.md` ACT-FR-09 sets. Its **foot** takes that same measure from the column's foot, which is the control's own inset from the tab's, so the two stand on one line.
    - *Why:* The composer grows from one line to five as the author writes, so only its foot is a fixed edge to align on.
15. **DDS-FR-QZAV** A draft discussion holding a **pending question set** renders that set between this column's message scroller and its composer, and disables that composer while the set stands, on the terms `DQA-discussion-question-answering.md` sets (DQA-FR-NRZB, DQA-FR-PXNC). The block sits outside the scroller, so paging never moves the transcript and an arriving message never moves the block.
16. **DDS-FR-JWNC** The tab's **Discuss** action shows this column where it is hidden and then focuses its composer — the opening composer where the draft has no discussion, the reply composer otherwise — by the same route and with the same persisted result as the control of DDS-FR-XQMF (per `ACT-action-control.md` ACT-FR-QWNP).
17. **DDS-FR-ZMXQ** The column stacks its messages in **one content column**, centred in the panel. That column is 820 pixels wide where the panel is 860 pixels or wider, and is the panel width less 28 pixels where the panel is narrower. It keeps one gap between its items, and narrows that gap below 620 pixels of panel width.
18. **DDS-FR-CVLM** In this column the accent marks **what the author said or chose**: the rail on the author's own message, the chosen option in a question card, and the selected row of the answering block. No message, row, or card is tinted with it for any other reason. A type chip and a focused field keep their own tones.
19. **DDS-FR-BRHN** The column draws a **card** only for an exchange that carries state: a question card, a change row, the answering block, and the reply field. Plain talk carries no border and no fill, and stands flush on the column's own ground.
    - *Why:* A card around a message that holds no state gives that message the weight of one that does, which is what makes a long transcript unreadable.
20. **DDS-FR-VTKD** A message the author wrote carries a two-pixel **accent rail** on its leading edge and an indent from that rail. It carries no fill, no border, and no alignment of its own, and it takes the content column's full measure.
    - *Why:* An accent fill behind a message would give the accent a second meaning, and the rail is the same device the active tab already uses.
21. **DDS-FR-GBWP** A message body is set in the **secondary** text tone. Only the author's own message body and the chosen option of a question card take the primary tone. The quietest tone is used for meta text and for placeholders alone, and never for a message body or an option.
22. **DDS-FR-LWPC** An **author line** is one row that reads the name, then the role where the participant carries one, then the time. The time is monospace meta text, is never emphasised, and is never placed in a corner. The row wraps below 620 pixels of panel width.
23. **DDS-FR-QJFE** Consecutive messages by the **same author** render as one block with one author line and their bodies stacked. The grouping is derived when the column renders, and the conversation records none of it.
24. **DDS-FR-NDSA** Messages written on different days are separated by a **date divider**: a hairline, a lowercase date label, and a hairline, on one row.
25. **DDS-FR-XHRB** A comment carrying a **change reference** renders that reference as one row of the column: the artifact-type chip, the file name, how many changes it holds, the state word, and the time. That row is what opens the review, in place of the control the reference renders in a card (per `CMT-comments.md` CMT-FR-48).
26. **DDS-FR-TGWY** An agent's **pending contribution** renders as an author line and then one row: a six-pixel live dot and the activity status the conversation defines (per `CTA-comment-agent-turns.md` CTA-FR-IWOJ), set in lowercase and without its trailing ellipsis. It carries no skeleton and no spinner. The column shows at most one such row for each agent. When that agent has no other running turn, the message that arrives **replaces that row in place** rather than landing below it.
27. **DDS-FR-YSNB** A state in this column reads as **one lowercase word** beside a six-pixel dot in that state's own tone. The column carries no uppercase status badge, and no symbol stands in place of the word.
28. **DDS-FR-MCUP** A question card and a change row each carry a **three-letter monospace chip** that names the kind of the row. A question card reads `QST` and a change row reads `CHG`. The file name beside the chip is what says which artifact the row is about.
29. **DDS-FR-FKZL** A message's actions stand at the **trailing end of its author line**. The pointer reveals them on hover and the keyboard reaches them at all times. They are the actions the conversation already offers and this column adds none (per `CMT-comments.md` CMT-FR-12, CMT-FR-15, CMT-FR-16).
30. **DDS-FR-PWDG** Hover on a row or a card darkens that surface and deepens its border one stop, and tints it with the accent in no state. Keyboard focus draws a ring, and a focused field also takes an accent border.
31. **DDS-FR-HQTX** The column's **reply field** is one bounded field that carries the placeholder and the hint for the post accelerator (per `CMT-comments.md` CMT-FR-70). Focus moves its border to the accent tone and draws the ring of DDS-FR-PWDG.
32. **DDS-FR-RJEV** The column's **geometry is the same in both themes**. A theme changes token values alone, and changes no size, no spacing, and no radius here.
33. **DDS-FR-QMBC** A selection in the draft prompt offers **Comment**. Activating it opens the shared opening composer with the selection as a **fragment** of the draft target: the prompt's draft-relative path, the character offsets, and the exact quote. The posted discussion is a fragment discussion of the draft. While it is open, the prompt shows and highlights the fragment with a style that is not carried by colour alone, focusing the discussion scrolls the fragment into view, and activating the fragment focuses the discussion in this column. A fragment that cannot be restored in the prompt leaves the discussion kept and shown as unavailable in this column.
34. **DDS-FR-VHZN** The column opens a **whole-target** discussion of the draft from its opening composer, which stands where the statement of DDS-FR-ZMXQ's empty states stands. A whole-target discussion carries no fragment. The column lists whole-target and fragment discussions in the order `CMT-comments.md` sets, and every one renders through the shared surface.
35. **DDS-FR-LPSC** With the column's opening composer or reply composer focused, **Ctrl+Enter** on Windows and Linux and **Cmd+Enter** on macOS posts it, on the terms `ACT-action-control.md` ACT-FR-28 sets for every composer. The column's composer text, pending attachments, scroll position, and unread state are held in session stores keyed by discussion id, so they survive the column being hidden and the tab being closed and opened again.

## User stories
- As an author, I want the discussion to be as readable as the draft, so that I hold a long design conversation without opening another window.
- As an author, I want a proposal to appear as hunks in the document, so that I see each change in the text it changes.
- As an author, I want to accept part of a proposal and keep discussing the rest, so that one objection does not block every change.
- As an author reading back through the history, I want new messages to stop moving the view, so that I finish reading and then return to the hunk under review.
- As an author reading a draft rather than discussing it, I want to hide the discussion column, so that the whole window holds the document and the conversation stays one control away.

## Wireframes

At rest — both columns scroll on their own, the discussion pinned to the tail:

```
┌─ ✦ Draft discussion · Backend comms.md   ● 3 proposed changes   [doc70|50/50|chat70] ⌘\ ✕ ┐
├───────────────────────────────────────────┬─┬───────────────────────────────────────────┤
│ B I ⟨⟩ │ H1 H2 H3 │ • 1.                  │ │ DISCUSSION                    [following] │
├───────────────────────────────────────────┤ ├───────────────────────────────────────────┤
│  ## Ontology                            ▍ │ │ ──── 14 earlier messages ────             │
│  This prompt is structured around…      ▍ │ │ raver119                          6h ago  │
│                                         ▍ │ │ we're only putting the ontology in…       │
│    ┌─[ 1/3  [Accept ⏎] [Reject] Discuss ]─┐│ │                                          │
│  ┃+│ ### User                            ││ │ + Olaf     Backend engineer       6h ago  │
│  ┃ │ A **user** is a person who uses…    ││ │ Agreed. Add `User` as a top-level…        │
│    └─────────────────────────────────────┘│ │ ┌ 3 changes  +38  −4   Review on the left ┐│
│  A **registration** is the one-time…    ▍ │ │ └─────────────────────────────────────────┘│
├───────────────────────────────────────────┤ ├───────────────────────────────────────────┤
│         (page on its recessed field)      │ │ ┌───────────────────────────────────────┐ │
│         ── fragment discussions below ──    │ │ │ Reply to @Olaf…                    ➤  │ │
├───────────────────────────────────────────┤ │ └───────────────────────────────────────┘ │
│ change 1 of 3 [‹ ⌥↑][› ⌥↓]  [Reject all][Accept all] │                                  │
└───────────────────────────────────────────┴─┴─┴───────────────────────────────────────┴─┘
```

Reading history — auto-follow paused, the unread divider, the jump control, the anchor bar:

```
│ DISCUSSION                       [paused · reading history] │
├─────────────────────────────────────────────────────────────┤
│ anchor  @Olaf · proposal of 3 changes · 6h    Back to change 2│
├─────────────────────────────────────────────────────────────┤
│ + Helga                                        1d ago       │  ← read, 85% opacity
│ When a mobile device scans the QR code…                     │
│ ─────────────────────── 3 new ──────────────────────────    │
│ + Olaf                                         6h ago       │
│ This records the agreed trust model…                        │
│                                     ( 3 new messages  ⌄ )   │
└─────────────────────────────────────────────────────────────┘
```

The content column, with one of each message form in it:

```
        ┌──────── content column, centred, max 820 ────────┐
        │ ────────────────── today ────────────────────    │
        │                                                  │
        │ Helga   UI/UX developer            17m ago  ⟨⋯⟩  │
        │ Publication metadata is append-only, so retry…   │
        │ Two questions before I touch the requirements.   │
        │                                                  │
        │ ┌──────────────────────────────────────────────┐ │
        │ │ QST  Helga                         17m ago   │ │
        │ │ If the upload succeeds but the issue fails…  │ │
        │ │    1  Delete uploaded assets after failure   │ │
        │ │ ▌  2  Keep and reuse marker-scoped uploads   │ │  ← chosen, marked in place
        │ │    3  Keep orphaned uploads and report them  │ │
        │ ├──────────────────────────────────────────────┤ │
        │ │ YOUR NOTE  Scope the marker per draft.       │ │
        │ └──────────────────────────────────────────────┘ │
        │                                                  │
        │ ▌ You                              17m ago       │  ← accent rail, no fill
        │ ▌ Local assets must not be stored in the repo…   │
        │ ▌ ┌──────────────────────────────────────────┐   │
        │ ▌ │ QUOTING HELGA                            │   │
        │ ▌ │ what should happen to the uploaded files │   │
        │ ▌ └──────────────────────────────────────────┘   │
        │                                                  │
        │ ┌ CHG  Post draft to Github.md  2 changes        │
        │ │                       accepted    16m ago ────┘ │
        │                                                  │
        │ Helga                              just now      │
        │ ● thinking                                       │  ← replaced in place
        └──────────────────────────────────────────────────┘
```

- Layout notes: the two columns are one grid of `document | splitter | discussion`, and the grid fills the tab's own bounds. Both columns share their outer edges with the action row above them, and the discussion column reaches the tab's trailing edge, because the shell's own chrome is what holds the tab clear of the window frame. The grid paints no surface of its own, so the page sits on the field the tab already sets. Each column is its own scroll region and neither scrolls the other. The document column stacks a formatting toolbar, the scroller, and the review bar, and only the scroller moves; the gutter map rides the scroller's outer edge, and the page inside the scroller reaches the column's foot. The discussion column stacks its header, the anchor bar where one applies, the message scroller, and the composer, and only the scroller moves — the composer sits outside it, cannot be pushed off, and keeps its trailing end and its foot clear of the tab's action control, which holds the tab's own bottom-trailing corner over this column — the composer's foot and the control standing on one line. The message list sits at the **foot** of its scroller, so a short discussion reads from the bottom where the newest message is. A column holding no discussion yet keeps the composer at that same foot, and stands the statement that nothing has been said **centred** in the space above it, on the measure DDS-FR-ZMXQ sets for the content column: the composer is the one thing the author acts with, and it is in the same place whether the column is empty or long, so reaching for it is never a question of what the column happens to hold; a statement about an empty column is about the column rather than about a message, so it sits in the middle of the space it describes rather than at the foot where the newest message would be; and it takes the measure a message takes, so what stands in the empty column is where the first message will come rather than a paragraph across the column's full width. That statement is one of three, and which one stands says which state the column is in: a draft nobody has spoken about yet, a draft whose discussions are every one resolved, and an author who writes the first message of another discussion. A column showing no transcript always carries one of them, because a panel that is empty and says nothing reads as a fault rather than as a state. Where every discussion is resolved the statement says so, and the resolved disclosure `CMT-comments.md` CMT-FR-17 pins at the foot of the chooser is where those discussions are read. The action chip is placed over padding the hunk reserves for it, above an insertion and below a deletion, so it never covers a line. The review bar and the discussion header both hold their place while their column scrolls, because each carries the context the column would otherwise lose.
- Message layout notes: inside the scroller the messages stand in one content column, centred, so a wide panel keeps equal air on both sides rather than a gutter on one. One breakpoint governs the column and there is no second layout. Plain talk stands flush on the panel's own ground with no border and no fill; a card is drawn only around a question, a change row, the answering block, and the reply field. The author's own message is marked by a rail and an indent alone. A question and the answer beside it read as one card, with the chosen option marked in the option list and the note in the card's footer, so the transcript never states an answer twice. The header, the answering block, and the reply field all stand outside the scroller: an author reading back through a long discussion must still reach the field they reply in.

## UI contract boundary
- **Owned by the UI**: the two-column grid, the splitter, the ratio presets and the accelerator that cycles them, and the control that hides and shows the discussion column; the page's placement in the document column and the position the prompt-fragment discussions take below it; the rendering of the transcript and the composer; the scroll position of each column, the auto-follow rule, the unread divider and the jump control; and the anchor bar. Everything the review itself renders inside the document column — the hunks, the review bar, the action chip and the gutter map — is `DCR-draft-change-review.md`'s and is delegated no operation here.
- **Delegated to backend (abstract)**:
  - `"load draft file contents"` and `"save draft file contents"` — the prompt, owned by `../core/DRS-draft-storage.md`.
  - `"open discussion (target, fragment, body, attachments)"`, `"add comment to discussion (discussion, body, quotes, attachments)"`, and `"dispatch agent turn"` — the discussion's own operations, invoked exactly as `CMT-comments.md` invokes them (CMT-FR-11, per `CTA-comment-agent-turns.md` CTA-FR-EACI).
  - `"read discussion question set"` and `"submit discussion question answers"`, with the event `"discussion question set changed"` — owned by `../core/CMS-comments-storage.md` (CMS-FR-JWVH, CMS-FR-TXRB, CMS-FR-BQEN) — invoked exactly as `DQA-discussion-question-answering.md` invokes them (DQA-FR-JWEF, DQA-FR-KDVU).
  - The event `"discussion changed"` (per `../core/CMS-comments-storage.md`) keeps the discussion column current. The proposal's own operations and event are `DCR-draft-change-review.md`'s and reach this surface through it.
  - `"load layout preferences"` and `"save layout preferences"` — the persisted ratio and hidden state, owned by `../core/PSS-project-settings-storage.md`.

## Non-functional requirements
- Review stays responsive on a prompt of several thousand lines and a proposal of up to 50 hunks.
- Both columns stay usable and legible at the minimum window size the shell supports (≥1280×800). Below the width at which the discussion keeps a readable measure, the columns give way to one column with the discussion in the bottom panel.
- Every colour comes from a token and every state is carried by more than colour, so a proposal is reviewable without colour discrimination.
- The discussion column names no literal colour, size, radius, or duration. Every one is a token of the design system, and a step the system does not yet carry is added to the system rather than written into this surface.
- Every control this surface introduces is reachable and operable by keyboard alone, and each carries an accessible name.
- Only the live marker animates, and it stops under a reduced-motion preference.
