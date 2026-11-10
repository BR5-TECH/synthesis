# Flow sync review

**Spec code:** `FSR`

## Intent
The panel in which the architect decides what a tracker sync could not decide alone. A sync of a delivery plan (`../core/FTS-flow-tracker-sync.md`) applies every change that only one side made, and returns as **conflicts** the fields that the Flow and GitHub both changed differently, the items removed on one side, and the tracker changes the plan's hierarchy cannot hold. It also returns the issues it **skipped**. This panel lists them, lets the author choose a side for each, and applies the choices in one call. It exists because a two-way sync is only safe when nothing is overwritten without the author seeing it. The panel lives inside the Flow tab, over the canvas, so the author can see the items a conflict is about while deciding it. Out of scope: the sync itself and every rule for what is a conflict, which are `../core/FTS-flow-tracker-sync.md`'s; a merged value that is neither side's, every choice taking one side whole; and any history of past syncs or past decisions.

## Functional requirements
1. **FSR-FR-KGOY** The review opens when a sync returns conflicts or skipped issues (per `FLO-flow.md` FLO-FR-WLHA), and when the author activates the count on the Sync control. It is a panel anchored at the leading side of the Flow tab, over the canvas. It is not a floating overlay of the window and closes none (per `SNV-shell-navigation.md` SNV-FR-56).
2. **FSR-FR-RDZI** The panel lists the conflicts of the last sync or resolve, grouped by item in the Flow's document order, an item that is gone from the Flow last. Each group is headed by the item's name and its tracker tag.
3. **FSR-FR-IFQE** A field conflict row names the field and shows the Flow value and the tracker value side by side, labelled **In this Flow** and **On GitHub**. A long text shows its first lines and expands in place. A parent or a milestone shows the item name and number, and a Type shows its name.
4. **FSR-FR-BRIA** A row of kind `removed_in_flow` reads **Removed in this Flow**, one of kind `removed_on_tracker` reads **Not found on GitHub**, and one of kind `invalid_on_tracker` reads **Not valid in this plan** and names the rule the tracker value breaks.
5. **FSR-FR-QAKW** Each row offers exactly the choices its conflict carries (per `../core/FTS-flow-tracker-sync.md` FTS-FR-PBQC, FTS-FR-SZGF, FTS-FR-BMWI, FTS-FR-VXMN), as one group of mutually exclusive options: `keep_flow` as **Keep Flow**, `take_tracker` as **Take GitHub**, `unlink` as **Unlink**, `restore` as **Restore**, and `remove_item` as **Remove item**. No option is chosen when the row opens.
6. **FSR-FR-CSOT** A **Skipped** section below the conflicts lists each skipped issue with its number, its title, and its reason in words. A skipped issue offers no choice.
7. **FSR-FR-KDMM** Each row whose conflict carries a URL, and each skipped issue, offers a link that invokes `"open tracker link"` with that URL.
8. **FSR-FR-PGBX** Activating an item's name in a group heading selects that item on the canvas and pans it into view. A name of an item that is gone from the Flow selects nothing.
9. **FSR-FR-IDRR** **Apply** is enabled while at least one row has a choice. It first writes the Flow's pending changes (per `FLO-flow.md` FLO-FR-27), then invokes `"resolve flow sync conflicts"` with the chosen rows alone. The tab is inert while the call runs (per `FLO-flow.md` FLO-FR-KHBY).
10. **FSR-FR-AUWG** The result of Apply replaces the list. A conflict the result reports as stale stays in the list with **Changed since the sync** and no choice, and the panel offers **Sync again**, which starts a sync on the terms of `FLO-flow.md` FLO-FR-AKMV.
11. **FSR-FR-UHAM** A result with no conflicts and no skipped issues shows **Everything is in sync** in the panel, and the Sync control loses its count.
12. **FSR-FR-BXJJ** A refused Apply shows its typed text in the panel and keeps every row and every choice as it was.
13. **FSR-FR-ODJL** The panel closes from its close button and from Escape while focus is inside it. Closing discards the choices not yet applied. The conflicts stay, and the next sync reports them again.
14. **FSR-FR-MMVF** Each choice group carries an accessible name that names the item and the field, and the panel is reachable and operable by keyboard alone.

## Wireframes

```
┌─ Sync review ────────────────────────────── × ┐
│ 3 conflicts · 1 skipped                        │
│                                                │
│ ▣ Card capture                        #42 ↗    │
│   Title                                        │
│   In this Flow   Card capture                  │
│   On GitHub      Card capture and 3DS          │
│   ( ) Keep Flow   ( ) Take GitHub              │
│                                                │
│ ☐ Tokenize card numbers               #51 ↗    │
│   Not valid in this plan                       │
│   GitHub moved it under #41, an epic.          │
│   A task must sit in a feature.                │
│   ( ) Keep Flow   ( ) Unlink                   │
│                                                │
│ ☐ Old spike                           #47 ↗    │
│   Removed in this Flow                         │
│   ( ) Unlink      ( ) Restore                  │
│                                                │
│ Skipped                                        │
│   #58 Load test plan ↗   No kind fits its Type │
│                                                │
│                                  [ Apply ]     │
└────────────────────────────────────────────────┘
```

- Layout notes: the panel takes the leading side of the canvas at a bounded width, from below the band to above the bottom control line, and scrolls inside itself. Apply sits at its foot and stays visible while the list scrolls. The canvas stays visible beside it, so a selected item can be seen while its row is decided.

## UI contract boundary
- **Owned by the UI**: the panel, its grouping, its rows and their wording, the choices made and not yet applied, the stale marking, and the selection of an item on the canvas from a row.
- **Delegated to backend (abstract)**:
  - `"resolve flow sync conflicts"` — applies the chosen sides and returns what still stands (owned by `../core/FTS-flow-tracker-sync.md`).
  - `"open tracker link"` — opens a tracker URL in the OS browser (owned by `../core/FTS-flow-tracker-sync.md`).
  - `"save artifact contents"` — the write of the Flow's pending changes before Apply (owned by `../core/PST-project-storage.md` PST-FR-09).

## Non-functional requirements
- The panel holds the last result in memory alone. It is not persisted, and a closed panel or a closed tab loses nothing that the next sync cannot report again.
- A choice is carried in words and in the option's state, never in colour alone.
