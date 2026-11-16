# Delivery board

**Spec code:** `DBR`

## Intent
The delivery board is the layout of the canvas in the Delivery lens of the Map tab (`MDL-map-delivery-lens.md`). It shows the project's delivery the way a team reads a kanban board: each milestone is a column, the epics and features in it are groups, and each task and defect is a card of one fixed size with exactly three lines. The board puts every card in an exact slot of a stack, so cards line up, a long name never makes one card taller than the next, and the plan reads at a glance. The architect plans by moving cards: a drag takes a card or a group from one stack to another stack, in the same milestone or in another, and the stacks open and close around it as it moves, so the architect sees where it lands before the drop. A drop that changes the container is an edit of the plan, which for a GitHub plan goes to GitHub at once (`MDL-map-delivery-lens.md` MDL-FR-PUCM). The order of the cards in one stack is the architect's own arrangement, and it is kept on that machine with the rest of the layout (`../core/DMS-delivery-machine-state.md`). The board applies to a delivery from a GitHub Project and to a delivery from a Flow file. Out of scope: a Flow file that is not the delivery, which keeps the free canvas of `FLO-flow.md` in its Flow tab; swimlanes across milestones; and moving an item with the keyboard.

## Functional requirements
1. **DBR-FR-ERHW** In the Delivery lens, the canvas of `FLO-flow.md` lays the plan out as a **board** for a `github` source and for a `file` source. A Flow tab never shows the board.
2. **DBR-FR-ZGNB** Each top-level milestone is a **column**, and the last column is **No milestone** (per `FLO-flow.md` FLO-FR-SQUS, FLO-FR-IAOQ, and FLO-FR-MCFR). All columns have one width and their tops align. The width is the width that the deepest nesting of groups in the plan needs around a card of the board width (DBR-FR-LNZI).
3. **DBR-FR-IQBN** An epic and a feature are **groups**. A group renders as a header line and, below it, the **stack** of its members. A column also holds a stack. A group fills the inner width of the stack that holds it, and a card fills the inner width of its stack, so every edge of a stack is straight.
4. **DBR-FR-FISH** A stack puts its members one below the other, with one fixed gap between two members, groups and cards mixed in the stack order (DBR-FR-HTCE). No member overlaps another member, and no member has a free position.
5. **DBR-FR-LNZI** A task and a defect are **cards**. Every card of the board has the same height, which holds exactly three lines, whatever the name, the state, or the markers of the item.
6. **DBR-FR-XOOU** The **top line** of a card shows the kind glyph and the kind name (**Task** or **Defect**) and, for an item of a GitHub plan, the **address** `owner/name#number` of its issue, aligned to the trailing edge. The address is the tracker tag of `FLO-flow.md` FLO-FR-ULEM: activating it invokes `"open tracker link (url)"` with the issue's URL. Where the address does not fit, it truncates at its start with an ellipsis, so the number always shows.
7. **DBR-FR-LVBY** The **middle line** of a card shows the item's name on one line. A name that does not fit ends in an ellipsis, and the card's tooltip shows the full name.
8. **DBR-FR-HWHO** The **bottom line** of a card shows the item's status. For an item of a GitHub plan, it is the issue's `Status` name (per `../core/GDP-github-delivery-plan.md` GDP-FR-QGFO), and an issue with no `Status` shows `No status`. For an item of a Flow file, it is the name of its delivery state (per `FLO-flow.md` FLO-FR-KQCE).
9. **DBR-FR-QZPA** The bottom line starts with the **state glyph** of the item's delivery state (FLO-FR-KQCE): a distinct shape and colour for each of **Not started**, **Drafting**, **In progress**, **Done**, and **Dropped**. The glyph's tooltip and accessible name are the state's name. The card's leading edge carries a stripe in the colour of the same state. An item with no state answer yet shows no glyph and no stripe.
10. **DBR-FR-GFGG** The bottom line ends with the item's **marker glyphs**, aligned to the trailing edge, in this order: **Blocked** (per FLO-FR-IKMM), **Overlap** (per FLO-FR-KWFH), and the **draft glyph** where the item has a draft link. Each marker keeps its tooltip and its activation. The draft glyph's tooltip names the draft, and a draft that the delivery state reports as missing shows a warning glyph in its place (per FLO-FR-QZUR). The draft row, the **Unlink** action, and the open action show in the panel of the selected item.
11. **DBR-FR-ZRBD** A card shows no chip, no reference, no excerpt, and no draft row. The panel of a selected item shows all of them (per FLO-FR-13 and FLO-FR-CMPK).
12. **DBR-FR-CJSG** An error that a requirement shows on an item (per `MDL-map-delivery-lens.md` MDL-FR-PUCM and MDL-FR-HNLP, and FLO-FR-NRBV) gives its card or its group header the error outline and an error glyph before the marker glyphs. The glyph's tooltip is the error text, and the panel of the item shows the error text at its top. The card keeps its height.
13. **DBR-FR-SBTR** A group header is one line: the collapse toggle, the kind glyph, the name, which truncates with an ellipsis, the state glyph (DBR-FR-QZPA), and the marker glyphs (DBR-FR-GFGG), and, for an item of a GitHub plan, the address of DBR-FR-XOOU and its `Status` name. A collapsed group renders as its header and one line of the counts of the leaves under it by state (per FLO-FR-ZASL).
14. **DBR-FR-BYQX** A column header keeps the content of `FLO-flow.md` FLO-FR-38: the milestone's number in the milestone order, its glyph, name, due date (FLO-FR-XAPK), state chip, and, in a GitHub plan, its tracker tag. Dragging a column header changes the milestone order (per FLO-FR-OGPF).
15. **DBR-FR-HTCE** The **stack order** of a stack is the order of the stored positions of its members in the layout of the lens's scope (per `../core/DMS-delivery-machine-state.md` DMS-FR-DMWD), from the smallest vertical position to the largest. The members with no stored position follow them, in the automatic order of `FLO-flow.md` FLO-FR-PLUZ: groups before cards, a member after each member it waits for, and plan order for the rest. A new item thus joins the end of its stack.
16. **DBR-FR-MYVI** A press and a move of more than 4 pixels on a card or on a group header starts a **drag** of that item. A press with no such move is a click and selects the item. A drag of a group takes the group with every item in it. A drag never starts from the address, a marker glyph, the collapse toggle, or the connection handle of FLO-FR-32.
17. **DBR-FR-QCAJ** While a drag runs, the item follows the pointer, raised above the board with a shadow, at the size it has in its stack. Its slot in the stack it left closes, and the members below it move up to fill the gap.
18. **DBR-FR-CZEU** While a drag runs, the **target stack** is the innermost stack under the pointer whose holder can hold the item's kind (per `../core/FGV-flow-graph-validation.md` FGV-FR-YHFX): a group, a milestone column, or the No milestone column. A collapsed group under the pointer is a target that takes the item at the end of its stack. The target shows an insertion **placeholder** of the dragged item's height at the slot where the item would land: before the first member whose vertical middle is below the pointer, else at the end. The members after the placeholder move down to make room. The target stack and its holder take the drop-target emphasis.
19. **DBR-FR-DQRX** Each move of the members of a stack, as a slot closes, a placeholder opens, or a placeholder moves, is animated over a short time, so a member slides to its new slot and never jumps. With the system setting for reduced motion, the members take their new slots with no animation.
20. **DBR-FR-EEKD** A stack under the pointer whose holder cannot hold the item's kind shows no placeholder. The pointer shows that a drop is not allowed, and a hint near the pointer states which containers the kind can join (per FLO-FR-OAOZ).
21. **DBR-FR-IQOM** While a drag runs near an edge of the canvas, the view pans toward that edge, faster as the pointer comes nearer the edge, so the item reaches a column or a group that is out of the view.
22. **DBR-FR-KTJS** A release over a placeholder **drops** the item into that slot. The item takes the slot at once, and the board writes the layout of the lens's scope once, through `"save delivery layout (scope, layout)"`, with the slot position of each member of the stacks that the drop changed. A drop that changes the container of the item is also an edit (per `FLO-flow.md` FLO-FR-26 and FLO-FR-42): the `move` edit through `"apply github delivery edit (edit)"` for a GitHub plan (per `MDL-map-delivery-lens.md` MDL-FR-XJBV and MDL-FR-PUCM), and a change of the item's container in the document for a Flow file. A drop in the same stack is the layout alone (per FLO-FR-TDHR).
23. **DBR-FR-BBDZ** A drop that FLO-FR-GZCH or FLO-FR-OAOZ refuses, a release where no placeholder shows, a release outside the canvas, and **Escape** during the drag put the item back in its slot with the animation of DBR-FR-DQRX. They change no plan and no layout. A refused drop gives the feedback of the requirement that refused it.
24. **DBR-FR-CVJY** A drop of a GitHub plan that the backend refuses (per MDL-FR-PUCM) puts the item back in the stack and the slot that the last answer gives it, with the animation of DBR-FR-DQRX, and the item shows the error of DBR-FR-CJSG.
25. **DBR-FR-BKWE** On the board, an item has no free position and a group has no stored size. An item moves only by a drop into a slot, and the board offers no resize of a group: FLO-FR-21, FLO-FR-HBBF, and FLO-FR-44 do not apply, and the stored positions are read as the stack order alone (DBR-FR-HTCE). **Reset layout** (per `MDL-map-delivery-lens.md` MDL-FR-QAJN) gives every stack its automatic order and expands every group.
26. **DBR-FR-CCAS** The edges of the plan render between the cards and the groups of the board on the terms of `FLO-flow.md` FLO-FR-39 and FLO-FR-48, and a connection is drawn on the terms of FLO-FR-32 and FLO-FR-33. While a drag runs, the edges of the dragged item follow it.

## User stories
- As an architect, I want every card on the board to be the same size and to line up in its stack, so that I read the plan as a board and not as a pile of boxes.
- As an architect, I want each card to show its kind, its issue address, its name, and its status on three fixed lines, so that I find any issue at a glance and know where it stands.
- As an architect, I want to drag a task to another milestone or epic and see the stack open where it will land, so that I re-plan in one gesture and trust where the task goes.
- As an architect, I want the order I give the cards in a stack to stay when I open the Map again, so that my priorities read top to bottom.

## Wireframes
```
┌─ 1 ⚑ Beta ─ due 2027-03-31 ─ ◐ In progress ─ #3 ┐     ┌─ 2 ⚑ GA ─ due 2027-06-30 ── ○ #4 ┐     ┌─ No milestone ──────────────┐
│ ▾ ◆ Checkout          ◐ Todo   acme/platform#40 │ ──▶ │ ▾ ▣ Payouts      ⊘ ◐ platform#45 │     │┃☐ Task      acme/platform#47│
│ ┌─────────────────────────────────────────────┐ │     │ ┌──────────────────────────────┐ │     │┃Decline message for a lo…   │
│ │ ▾ ▣ Card capture  ◐ Todo  acme/platform#41  │ │     │ │┃☐ Task   acme/platform#53    │ │     │┃◔ Drafting              ✎   │
│ │ ┌─────────────────────────────────────────┐ │ │     │ │┃Payout schedule              │ │     └─────────────────────────────┘
│ │ │┃☐ Task              acme/platform#42    │ │ │     │ │┃◔ Todo                ⊘ ✎    │ │
│ │ │┃Tokenize card numbers                   │ │ │     │ └──────────────────────────────┘ │
│ │ │┃◐ In Progress                        ✎  │ │ │     │ ▸ ▣ Disputes  ⊘ ○  platform#46   │
│ │ └─────────────────────────────────────────┘ │ │     │   2 not started                  │
│ │ ┌─────────────────────────────────────────┐ │ │     └──────────────────────────────────┘
│ │ │┃☐ Task              acme/platform#43    │ │ │
│ │ │┃Card form                               │ │ │
│ │ │┃◔ Ready                            ⊘ ✎  │ │ │
│ │ └─────────────────────────────────────────┘ │ │
│ └─────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────┘

drag of "3DS challenge" from Card capture into Payouts
┌─ 1 ⚑ Beta ──────────────────┐     ┌─ 2 ⚑ GA ─────────────────────┐
│ ▾ ▣ Card capture            │     │ ▾ ▣ Payouts                  │
│ ┌─────────────────────────┐ │     │ ┌──────────────────────────┐ │
│ │┃☐ Task  acme/platform#42│ │     │ │┃☐ Task  acme/platform#53 │ │
│ │┃Tokenize card numbers   │ │     │ │┃Payout schedule          │ │
│ │┃◐ In Progress        ✎  │ │     │ │┃◔ Todo             ⊘ ✎   │ │
│ └─────────────────────────┘ │     │ └──────────────────────────┘ │
│ ┌─────────────────────────┐ │     │ ┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐ │
│ │┃☐ Task  acme/platform#43│ │     │ ╎        placeholder       ╎ │   ┏━━━━━━━━━━━━━━━━━━━━━━━━━━┓
│ │┃Card form               │ │     │ ╎                          ╎ │◀──┃┃☐ Task  acme/platform#44 ┃
│ │┃◔ Ready            ⊘ ✎  │ │     │ └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘ │   ┃┃3DS challenge for cards… ┃
│ └─────────────────────────┘ │     └──────────────────────────────┘   ┃┃○ Todo               ⧉   ┃
└─────────────────────────────┘                                        ┗━━━━━━━━━━━━━━━━━━━━━━━━━━┛
```
- Layout notes: the card height holds three lines of the body text size and the card's padding, and no content changes it. The stripe on the leading edge of a card is 3px wide. A stack keeps one gap between two members, and a group keeps one inset between its border and its stack on each side. The glyphs on the bottom line are the size of the text. The address uses the monospace face of the tracker tag. The placeholder has a dashed border and the height of the dragged item. A dragged item keeps its size and takes a raised shadow. A drop-target stack takes the accent tint of its holder's border.

## UI contract boundary
- **Owned by the UI**: the board layout, the column width, the stacks and their order, the three-line card and the group header, the drag with its placeholder, its animation, its auto-pan, its refusal, and its return, and the write of the stack order into the layout.
- **Delegated to backend (abstract)**:
  - `"save delivery layout (scope, layout)"` and `"load delivery layout (scope)"` — owned by `../core/DMS-delivery-machine-state.md`.
  - `"apply github delivery edit (edit)"` and `"open tracker link (url)"` — owned by `../core/GDP-github-delivery-plan.md`.

  The canvas, its edges, its selection panel, and its plan rules are owned by `FLO-flow.md`. The lens, its sources, and its pending edits are owned by `MDL-map-delivery-lens.md`.

## Non-functional requirements
- A drag follows the pointer at the frame rate of the display on a plan of several hundred items. A drag reads no backend and writes no layout until the drop.
- A drop writes the layout once and sends at most one edit.
- The board shows no item at a size or a place other than its slot, so a screenshot of a plan shows straight stacks of equal cards.
