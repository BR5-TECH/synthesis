# Draft discussion

**Spec code:** `DDS`

## Intent
The two-column shape of the New Artifact tab, where an author and one or more agents work on one draft together, and of the Editor tab, where they discuss one file of the project. The Editor tab takes the same discussion column, so a conversation about a file reads and works as a conversation about a draft does, and an author learns one discussion surface rather than two. The draft's prompt and the discussion about it stand side by side and both stay visible, and an agent's proposed edits are decided **inside the document** as accept/reject hunks in the prose they change. It exists because a draft is written in conversation: a discussion that covers the draft, or a proposal that hides the discussion, makes the author choose which half of the work to look at. Acceptance is per hunk, so one objection holds up one change rather than the whole proposal, and one change is talked about in a discussion of its own when it needs one. The column header also tells the author how much of the model's context window a turn uses, and warns when the document under discussion is too large to give to the agent whole, because a large document fills the window before the discussion does and only the author can split it. Out of scope: how the context figures are measured and estimated, owned by `../core/AGC-agent-conversations.md`; the size limit setting, owned by `GLS-global-settings.md`; the tab's identity, its creation, its History rail, graduation, and every action a draft affords, all of which are `NAW-new-artifact.md`'s; what a proposal is and what deciding one writes, owned by `../core/DCP-draft-change-proposals.md` for a draft and by `../core/PCP-prompt-change-proposals.md` for a file of the project; the transaction an acceptance is performed as, owned by `../core/DHS-draft-history.md`; how a conversation renders, what posting to one costs, and which discussions an Editor tab lists, owned by `CMT-comments.md`; and the editing surface itself, owned by `EDT-editor.md`.

## Functional requirements
1. **DDS-FR-KTVW** The New Artifact tab and the Editor tab each render one **document column** and one **discussion column** side by side, separated by a splitter. In the New Artifact tab both are visible whenever the tab is (per `NAW-new-artifact.md` NAW-FR-01). In the Editor tab the grid stands below the Editor's toolbar on every surface of the file — the WYSIWYG and the raw-text surface of a Markdown file, and the source surface of a source file (per `CMT-comments.md` CMT-FR-02). Neither tab opens a floating chat window or a review modal for its discussions. The discussion column renders the shared `DiscussionSurface` and `DiscussionComposer` (per `CVP-conversation-presentation.md`), so a discussion looks and behaves here as it does in every other owner surface.
2. **DDS-FR-TGBX** The document column renders the prompt, or the Editor tab's file, on the **page** every document surface uses — an inset sheet with its own edge on a recessed field (per `EDT-editor.md` EDT-FR-63). The page takes the column's measure where the column is narrower than the page's own, and runs to the foot of the column with the same inset there as at its head.
3. **DDS-FR-KPSZ** The discussions of **fragments** — passages of the prompt or of the Editor tab's file — are listed in the discussion column (DDS-FR-QMBC). The document column lends them no margin beside the page and no section below it.
4. **DDS-FR-PNXR** The split holds three presets — document 70, even, and discussion 70 — cycled by one accelerator, and the splitter is also dragged. The chosen ratio is restored per draft in the New Artifact tab and per file in the Editor tab (per `SNV-shell-navigation.md` SNV-FR-08).
5. **DDS-FR-XQMF** One control hides and shows the discussion column, standing in both states. In the New Artifact tab it stands in the tab's action row; in the Editor tab it stands at the trailing end of the Editor's toolbar, with the presets and the accelerator hint of DDS-FR-PNXR leading of it (per `EDT-editor.md` EDT-FR-16). The document column takes the tab's full width while the other is hidden. Whether it is hidden is restored per draft and per file (per `SNV-shell-navigation.md` SNV-FR-08), and a file whose column was never hidden shows it. The presets, the accelerator and the splitter of DDS-FR-PNXR hide neither column, and stand down while one is hidden.
6. **DDS-FR-HGMB** The discussion renders each message with the document's typography, real lists, and inline code, under an author line that names the author and the author's role.
7. **DDS-FR-ZRPT** The **document column** of the New Artifact tab is where a draft proposal is reviewed. It renders the hunks, the review bar, the action chip, and the gutter map that `DCR-draft-change-review.md` defines, and the tab opens no separate surface for them.
7. **DDS-FR-MXHD** The Editor tab's document column carries the in-place review of a published prompt or a project document (per `PCR-prompt-change-review.md` PCR-FR-QHZN): the hunks, the review bar, the action chip, and the gutter map of DDS-FR-ZRPT. Its discussion column carries the hunk discussions, the subject strip, and the anchor bar on the terms of DDS-FR-EHDK.
7. **DDS-FR-EHDK** Hunk discussions work in the Editor tab's discussion column as in the New Artifact tab's: the chooser lists them, **Discuss** opens the composer of DDS-FR-RHKW, the subject strip of DDS-FR-WKTQ and DDS-FR-PBXN names the hunk, the anchor bar of DDS-FR-SVBL stands, a hunk discussion resolves and reopens, and an agent's revision replaces the hunk in place.
8. **DDS-FR-NWRL** A draft's discussions, and an Editor tab's file's discussions, are rendered in this column, the whole-target ones and the fragment ones alike, as the shared surface, and nowhere else. Each is hosted by this column for as long as the tab is open, and the single reveal route of `CVP-conversation-presentation.md` focuses it here rather than opening a second surface. No second rendering of such a discussion exists anywhere.
9. **DDS-FR-DPRJ** The discussion follows an arriving message only while it is scrolled to the tail. Its header reads `following` while it does. The header states the follow state only while the column renders a transcript, and states nothing while it renders none.
10. **DDS-FR-GKMT** Scrolling the discussion away from the tail stops it following. Its header reads `paused · reading history`, an arriving message lands below a divider naming how many are unread, and a control offering the jump to the tail appears. A column that renders no transcript follows nothing and shows none of these three.
11. **DDS-FR-WCHN** An arriving message never scrolls the document column. The document moves for hunk navigation and for the author's own scrolling, and for nothing else.
12. **DDS-FR-WKTQ** While the column renders a hunk discussion (per `DCR-draft-change-review.md` DCR-FR-VBQN) or the composer of a new one, a **subject strip** below its header names the hunk: `Change N of M`, the file, and the hunk's current state, which follows each decision. The strip stands outside the message scroller and holds its place while the transcript scrolls.
    - *Why:* Without it, only the first message names the hunk, and that message scrolls away.
12. **DDS-FR-PBXN** The subject strip shows the agent's note on the hunk and the text the hunk removes and adds, in the tones of `DCR-draft-change-review.md` DCR-FR-07. Each text is cut to a few lines and **Show more** shows all of it. While the proposal is open, **Show in document** scrolls the document column to the hunk (per `CMT-comments.md` CMT-FR-48).
12. **DDS-FR-YLRM** A discussion tab, or the control that begins another discussion, leaves the composer of a hunk discussion and shows what the author chose. The text typed into that composer stays in the draft's composer. A decision of the hunk does not close the composer.
12. **DDS-FR-SVBL** While a proposal stands and the discussion is not at the tail, an **anchor bar** below the discussion header names the proposal and offers the return to the hunk under review.
13. **DDS-FR-CLBK** A discussion longer than the column shows the messages above the viewport as one row naming how many they are, which expands in place.
14. **DDS-FR-QPHL** The discussion column's shared composer holds its trailing end clear of the tab's action control, by the measure `ACT-action-control.md` ACT-FR-09 sets. Its **foot** takes that same measure from the column's foot, which is the control's own inset from the tab's, so the two stand on one line.
    - *Why:* The composer grows from one line to five as the author writes, so only its foot is a fixed edge to align on.
15. **DDS-FR-QZAV** A discussion in this column holding a **pending question set** renders that set between this column's message scroller and its composer, and disables that composer while the set stands, on the terms `DQA-discussion-question-answering.md` sets (DQA-FR-NRZB, DQA-FR-PXNC). The block sits outside the scroller, so paging never moves the transcript and an arriving message never moves the block.
16. **DDS-FR-JWNC** The tab's **Discuss** action shows this column where it is hidden and then focuses its composer — the opening composer where the draft or the file has no discussion, the reply composer otherwise — by the same route and with the same persisted result as the control of DDS-FR-XQMF (per `ACT-action-control.md` ACT-FR-QWNP).
17. **DDS-FR-RHKW** **Discuss** on a hunk (per `DCR-draft-change-review.md` DCR-FR-MTFD) shows this column where it is hidden and puts the new hunk discussion's composer in it, with the caret in its text. The subject strip of DDS-FR-WKTQ names its change, and posting creates a whole-target discussion of the draft. In this column a comment's `proposal_hunk` renders below its body as the control of `CMT-comments.md` CMT-FR-48.
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
29. **DDS-FR-FKZL** A message's actions stand at the **trailing end of its author line**. The pointer reveals them on hover and the keyboard reaches them at all times. They are the actions the conversation already offers on a message and this column adds none (per `CMT-comments.md` CMT-FR-12). The discussion's own menu stands in the column header (DDS-FR-MZQJ).
30. **DDS-FR-MZQJ** While the column renders a transcript, its header carries that discussion's overflow menu with the one entry of `CMT-comments.md` CMT-FR-15, reachable by keyboard, whatever part of the transcript is shown. No message of the column carries that menu. A resolved discussion's header states `resolved`, and the column renders it without a composer or a Quote action (per `CMT-comments.md` CMT-FR-16).
30. **DDS-FR-CTXI** The column header carries a **context indicator** at its trailing end, directly before the menu host of DDS-FR-MZQJ, in the New Artifact tab and in the Editor tab. It displaces neither the follow state nor the `resolved` state. It shows the context use of a turn as a whole percent of the model's context window, for example `42%`, beside a small progress ring (DDS-FR-WCET).
30. **DDS-FR-KXWQ** The indicator's value is the input tokens the provider reported for the latest model call of the latest turn of the shown discussion, which `"agent turn state changed"` delivers. Before such a turn exists, and after a reload of the window, the value is the estimated total of `"estimate discussion context"`, and the indicator marks the value as an estimate.
30. **DDS-FR-TSPL** The window is the one of the agent that answered the latest turn of the shown discussion. Before any turn, it is the window of the first agent of the column's agent list. The window is always known (per `../core/AGC-agent-conversations.md` AGC-FR-GMSF), so the indicator always shows a percent.
30. **DDS-FR-WCET** A small circular progress ring stands before the percent, inside the indicator. Its arc is the used share of the window, full at 100% and above. The ring is no taller than the percent text, so the header row keeps its height. Its geometry is the same in both themes, and it does not animate.
30. **DDS-FR-WUCD** The indicator has a **level**: `normal` below 50%, `high` from 50% to below 85%, and `critical` from 85% up. The ring takes the accent tone at `normal`, the warning tone at `high`, and the danger tone at `critical`, each a theme token. The level does not depend on `above_limit`, and the warning of DDS-FR-ZQVE does not depend on the level.
30. **DDS-FR-WRNB** On hover and on keyboard focus, the indicator shows a tooltip with the split of the turn in tokens — instructions, document, discussion history, current comment, and tool results, each labelled as an estimate — the window size, and the level. The accessible name states the percent and the level. The percent text stays visible at every column width.
30. **DDS-FR-ZQVE** While the document under discussion is above the size limit (`above_limit`), a warning icon stands next to the indicator. On hover and on keyboard focus, its tooltip states the document size in estimated tokens and the limit, and says that the author must split or shorten the document. At or below the limit, no warning shows.
    - *Why:* The agent then gets only part of the document, and the author alone can change the document's size.
30. **DDS-FR-HJMF** The indicator and the warning block nothing: posting, dispatching a turn, and deciding a proposal stay available above the limit. Both refresh when a turn reports its context, when the shown discussion changes, and when the document under discussion is saved.
30. **DDS-FR-PWDG** Hover on a row or a card darkens that surface and deepens its border one stop, and tints it with the accent in no state. Keyboard focus draws a ring, and a focused field also takes an accent border.
31. **DDS-FR-HQTX** The column's **reply field** is one bounded field that carries the placeholder and the hint for the post accelerator (per `CMT-comments.md` CMT-FR-70). Focus moves its border to the accent tone and draws the ring of DDS-FR-PWDG.
31. **DDS-FR-ZUXP** The column's reply field carries the timeout bar of `CVP-conversation-presentation.md` CVP-FR-JSHF directly under the field, inside the field's card. The bar keeps the composer's foot on the line of DDS-FR-QPHL.
31. **DDS-FR-MCDG** The column's opening composer and its reply composer are **one field of one geometry**: the same height, padding, control spacing, type size, and placeholder tone. When the first message of a discussion posts, the empty reply field keeps the size and the control positions the opening composer had.
31. **DDS-FR-JKPC** The voice area of `VOM-voice-mode.md` VOM-FR-TISN has the geometry of DDS-FR-MCDG, and its trailing toggles hold the clearance of DDS-FR-QPHL.
32. **DDS-FR-RJEV** The column's **geometry is the same in both themes**. A theme changes token values alone, and changes no size, no spacing, and no radius here.
33. **DDS-FR-QMBC** A selection in the draft prompt, or on the WYSIWYG or source surface of an Editor tab (per `CMT-comments.md` CMT-FR-05), offers **Comment**. Activating it shows this column where it is hidden and opens the shared opening composer with the selection as a **fragment** of the tab's target: the prompt's draft-relative path or the file's project-relative path, the character offsets, and the exact quote. The posted discussion is a fragment discussion of the draft or the file. While it is open, the text shows and highlights the fragment with a style that is not carried by colour alone, activating the fragment quote in this column scrolls the fragment into view, and activating the fragment focuses the discussion in this column. A fragment that cannot be restored in the text leaves the discussion kept and shown as unavailable in this column.
34. **DDS-FR-VHZN** The column opens a **whole-target** discussion of the draft or the file from its opening composer, which stands where the statement of DDS-FR-ZMXQ's empty states stands. A whole-target discussion carries no fragment. The column lists whole-target and fragment discussions in the order `CMT-comments.md` sets, and every one renders through the shared surface.
35. **DDS-FR-LPSC** With the column's opening composer or reply composer focused, **Ctrl+Enter** on Windows and Linux and **Cmd+Enter** on macOS posts it, on the terms `ACT-action-control.md` ACT-FR-28 sets for every composer. The column's composer text, pending attachments, scroll position, and unread state are held in session stores keyed by discussion id, so they survive the column being hidden and the tab being closed and opened again.

## User stories
- As an author, I want the discussion to be as readable as the draft, so that I hold a long design conversation without opening another window.
- As an author, I want a proposal to appear as hunks in the document, so that I see each change in the text it changes.
- As an author, I want to accept part of a proposal and keep discussing the rest, so that one objection does not block every change.
- As an author discussing one change, I want to see that change above the conversation, so that I always know what the discussion is about.
- As an author reading back through the history, I want new messages to stop moving the view, so that I finish reading and then return to the hunk under review.
- As an author discussing a large document, I want to see how much of the model's context window a turn uses and be warned when the document is too large, so that I split or shorten it before the agent works from part of it.
- As an author reading a draft rather than discussing it, I want to hide the discussion column, so that the whole window holds the document and the conversation stays one control away.

## Wireframes

At rest — both columns scroll on their own, the discussion pinned to the tail:

```
┌─ ✦ Draft discussion · Backend comms.md   ● 3 proposed changes   [doc70|50/50|chat70] ⌘\ ✕ ┐
├───────────────────────────────────────────┬─┬───────────────────────────────────────────┤
│ B I ⟨⟩ │ H1 H2 H3 H4 │ • 1.               │ │ DISCUSSION           [following]  ◔ 38% ⋯ │
├───────────────────────────────────────────┤ ├───────────────────────────────────────────┤
│  ## Ontology                            ▍ │ │ ──── 14 earlier messages ────             │
│  This prompt is structured around…      ▍ │ │ raver119                          6h ago  │
│                                         ▍ │ │ we're only putting the ontology in…       │
│    ┌─[ 1/3  [Accept] [Reject] Discuss ]───┐│ │                                          │
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

A hunk discussion — the subject strip under the header, outside the scroller:

```
│ DISCUSSION                          [following]  ◔ 42% ⚠ ⋯ │
├─────────────────────────────────────────────────────────────┤
│ ⟨diff⟩ Change 2 of 3 · Backend comms.md · ● undecided        │
│ Names the remote client as a second kind of user.           │
│ − A user is a person who uses the IDE.                      │
│ + A USER is a person who uses the IDE and the remote…       │
│ Show more                                 Show in document  │
├─────────────────────────────────────────────────────────────┤
│ ▌ You                                          2m ago       │
│ ▌ @Olaf why does the remote client count as a user?         │
│ + Olaf     Backend engineer                    just now     │
│ It owns projects of its own, so…                            │
└─────────────────────────────────────────────────────────────┘
```

The context indicator and the warning in the header, each with its tooltip open:

```
│ DISCUSSION                          [following]  ◔ 42% ⚠ ⋯ │
│                         ┌──────────────────────────────────┐ │
│                         │ Context  84,210 of 200,000       │ │
│                         │ Instructions       ~2,100 est.   │ │
│                         │ Document          ~60,000 est.   │ │
│                         │ Discussion history ~9,400 est.   │ │
│                         │ Current comment      ~120 est.   │ │
│                         │ Tool results      ~12,500 est.   │ │
│                         │ Window 200,000 tokens. Normal.   │ │
│                         └──────────────────────────────────┘ │
│                            ┌───────────────────────────────┐ │
│                            │ Document ~152,000 tokens est. │ │
│                            │ Limit 60,000 (30% of window)  │ │
│                            │ Split or shorten the document │ │
│                            └───────────────────────────────┘ │
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

- Layout notes: the two columns are one grid of `document | splitter | discussion`, and the grid fills the tab's own bounds — in the Editor tab, the bounds below the Editor's toolbar, which spans the tab and carries the presets, the accelerator hint, and the hide control at its trailing end. The Editor tab's document column holds the file's surface, and the review bar and the gutter map while a proposal is pending. Both columns share their outer edges with the action row above them, and the discussion column reaches the tab's trailing edge, because the shell's own chrome is what holds the tab clear of the window frame. The grid paints no surface of its own, so the page sits on the field the tab already sets. Each column is its own scroll region and neither scrolls the other. The document column stacks a formatting toolbar, the scroller, and the review bar, and only the scroller moves; the gutter map rides the scroller's outer edge, and the page inside the scroller reaches the column's foot. The discussion column stacks its header, the subject strip and the anchor bar where they apply, the message scroller, and the composer, and only the scroller moves — the composer sits outside it, cannot be pushed off, and keeps its trailing end and its foot clear of the tab's action control, which holds the tab's own bottom-trailing corner over this column — the composer's foot and the control standing on one line. The message list sits at the **foot** of its scroller, so a short discussion reads from the bottom where the newest message is. A column holding no discussion yet keeps the composer at that same foot, and stands the statement that nothing has been said **centred** in the space above it, on the measure DDS-FR-ZMXQ sets for the content column: the composer is the one thing the author acts with, and it is in the same place whether the column is empty or long, so reaching for it is never a question of what the column happens to hold; a statement about an empty column is about the column rather than about a message, so it sits in the middle of the space it describes rather than at the foot where the newest message would be; and it takes the measure a message takes, so what stands in the empty column is where the first message will come rather than a paragraph across the column's full width. That statement is one of three, and which one stands says which state the column is in: a draft or a file nobody has spoken about yet, a draft or a file whose discussions are every one resolved, and an author who writes the first message of another discussion. Each statement names the item it is about — the draft in a New Artifact tab, the file in an Editor tab. A column showing no transcript always carries one of them, because a panel that is empty and says nothing reads as a fault rather than as a state. Where every discussion is resolved the statement says so, and the resolved disclosure `CMT-comments.md` CMT-FR-17 pins at the foot of the chooser is where those discussions are read. The action chip is placed over padding the hunk reserves for it, above an insertion and below a deletion, so it never covers a line. The review bar and the discussion header both hold their place while their column scrolls, because each carries the context the column would otherwise lose.
- Header layout notes: the header reads the title, then the follow state or the `resolved` state, then the context indicator with its ring before the percent, then the warning icon where it stands, then the discussion's menu. The title takes the free width, so the other items keep the trailing end. A tooltip opens below its control and stays inside the column. The warning is an icon with a tooltip, so its meaning is not carried by colour alone.
- Message layout notes: inside the scroller the messages stand in one content column, centred, so a wide panel keeps equal air on both sides rather than a gutter on one. One breakpoint governs the column and there is no second layout. Plain talk stands flush on the panel's own ground with no border and no fill; a card is drawn only around a question, a change row, the answering block, and the reply field. The author's own message is marked by a rail and an indent alone. A question and the answer beside it read as one card, with the chosen option marked in the option list and the note in the card's footer, so the transcript never states an answer twice. The header, the answering block, and the reply field all stand outside the scroller: an author reading back through a long discussion must still reach the field they reply in.

## UI contract boundary
- **Owned by the UI**: the two-column grid in both tabs, the splitter, the ratio presets and the accelerator that cycles them, and the control that hides and shows the discussion column; the page's placement in the document column and the position the prompt-fragment discussions take below it; the rendering of the transcript and the composer; the scroll position of each column, the auto-follow rule, the unread divider and the jump control; the discussion menu in the column header; the context indicator, its ring, its level and the tone of each level, its tooltip, the warning icon and its tooltip, the choice of which turn and which agent's window they read, and the percent shown; the composer a hunk's Discuss opens; the subject strip; and the anchor bar. Everything the review itself renders inside the document column — the hunks, the review bar, the action chip and the gutter map — is `DCR-draft-change-review.md`'s in the New Artifact tab and `PCR-prompt-change-review.md`'s in the Editor tab, and is delegated no operation here.
- **Delegated to backend (abstract)**:
  - `"load draft file contents"` and `"save draft file contents"` — the prompt, owned by `../core/DRS-draft-storage.md`.
  - `"open discussion (target, fragment, body, attachments)"`, `"add comment to discussion (discussion, body, quotes, attachments)"`, `"set discussion resolution"`, and `"dispatch agent turn"` — the discussion's own operations, invoked exactly as `CMT-comments.md` invokes them (CMT-FR-11, CMT-FR-15, per `CTA-comment-agent-turns.md` CTA-FR-EACI).
  - `"read discussion question set"` and `"submit discussion question answers"`, with the event `"discussion question set changed"` — owned by `../core/CMS-comments-storage.md` (CMS-FR-JWVH, CMS-FR-TXRB, CMS-FR-BQEN) — invoked exactly as `DQA-discussion-question-answering.md` invokes them (DQA-FR-JWEF, DQA-FR-KDVU).
  - The event `"discussion changed"` (per `../core/CMS-comments-storage.md`) keeps the discussion column current. The proposal's own operations and event are `DCR-draft-change-review.md`'s for a draft and `PCR-prompt-change-review.md`'s for a file, and reach this surface through them.
  - `"estimate discussion context"` and the event `"agent turn state changed"`, which carries a turn's context figures — owned by `../core/AGC-agent-conversations.md` — serve the context indicator and the warning (DDS-FR-CTXI through DDS-FR-HJMF). The size limit they compare against is the setting of `GLS-global-settings.md`, applied in the backend.
  - `"load layout preferences"` and `"save layout preferences"` — the persisted ratio and hidden state, per draft and per file, owned by `../core/PSS-project-settings-storage.md`.
  - In the Editor tab, the file's contents are the Editor's (per `EDT-editor.md`), and the discussions are listed, opened, answered, and resolved as `CMT-comments.md` invokes them.

## Non-functional requirements
- Review stays responsive on a prompt of several thousand lines and a proposal of up to 50 hunks.
- Both columns stay usable and legible at the minimum window size the shell supports (≥1280×800). Below the width at which the discussion keeps a readable measure, the columns give way to one column with the discussion in the bottom panel.
- Every colour comes from a token and every state is carried by more than colour, so a proposal is reviewable without colour discrimination.
- The discussion column names no literal colour, size, radius, or duration. Every one is a token of the design system, and a step the system does not yet carry is added to the system rather than written into this surface.
- Every control this surface introduces is reachable and operable by keyboard alone, and each carries an accessible name.
- Only the live marker animates, and it stops under a reduced-motion preference.
