# Specification map drafts

**Spec code:** `SMD`

## Intent
The architect plans new work where it belongs in the specification map: a draft started from a node stays visible on the map as a planned chip inside that node, so the plan and the corpus show on one surface. This spec owns the **New draft** action of the inspector (`SMI-specification-map-inspector.md`), the placement of drafts on index nodes, and the planned chip. The draft itself is created and edited exactly as a draft of the Drafts panel is (`DRP-drafts-panel.md`, `NAW-new-artifact.md`). Out of scope: a durable placement, and the graduation of a planned draft into a spec node.

## Functional requirements
1. **SMD-FR-RTMQ** The inspector offers **New draft** for every index node subject. It offers no **New draft** for a spec node or a planned chip.
2. **SMD-FR-HVBE** **New draft** runs the same creation as the pinned create affordance of the Drafts panel: `"create draft (name)"` with no name at the root of the drafts tree. The new draft opens in a New Artifact tab that becomes the active tab (per `DRP-drafts-panel.md` DRP-FR-06, `NAW-new-artifact.md` NAW-FR-03).
3. **SMD-FR-OYLC** After the draft is created, the map attaches the draft to the index node the action ran on and invokes `"attach draft to specification map node"`. The node then shows a planned chip for the draft.
4. **SMD-FR-EKWN** When `"create draft (name)"` fails, the map attaches nothing, opens no tab, and shows a toast that says the draft could not be created.
5. **SMD-FR-CGNW** A planned chip is 96 × 22px with a 1px dashed `--border-2` border. It shows a `draft` badge in `--live` on `--live-soft` and the draft name truncated with an ellipsis. In a leaf group, planned chips follow its spec chips in the same grid (per `SMN-specification-map-nodes.md` SMN-FR-ISBE).
6. **SMD-FR-ULTF** In an expanded root node or branch node, the planned chips render after its children, 3 per row with 6px gaps, and the node grows to hold them. A collapsed index node that holds planned chips beneath it adds `+N planned` to its meta line.
7. **SMD-FR-NQGE** Planned chips add nothing to spec counts, requirement totals, scenario totals, completeness bars, or dependency edges.
8. **SMD-FR-XEPS** A click on a planned chip selects it. The inspector then shows the draft name, the label of the node it is planned under, and the primary action **Open draft**, which opens the draft in its New Artifact tab or jumps focus to the tab already showing it (per `TAB-tabs.md` TAB-FR-17).
9. **SMD-FR-LKVU** A planned chip shows the current name of its draft after a rename of that draft. When the draft is archived, graduated, or deleted, its planned chip is removed from the map. An active or a published draft keeps its planned chip.
10. **SMD-FR-QPAM** A planned chip can be dragged onto any index node, and the drop moves the draft placement to that node (per `SMO-specification-map-organization.md`).
11. **SMD-FR-BWJY** Draft placements live in the map session and are discarded with it (per `SMP-specification-map.md` SMP-FR-MEZK). Discarding a placement never changes the draft.

## Wireframes
```
┌ zones ─────────────────────────── ▮▮▯  4 ┐
│ Layout of the main window …              │
│ ┌──────────────┐┌──────────────┐┌──────── │
│ │OVW UI ove…  ●││SNV shell …  ●││STB sta… │
│ └──────────────┘└──────────────┘└──────── │
│ ┌──────────────┐┏╍╍╍╍╍╍╍╍╍╍╍╍╍╍┓          │
│ │ACT action … ●│╏draft Untitled╏          │
│ └──────────────┘┗╍╍╍╍╍╍╍╍╍╍╍╍╍╍┛          │
└──────────────────────────────────────────┘
```
- Layout notes: the planned chip keeps the size of a spec chip so a grid of mixed chips stays aligned.

## UI contract boundary
- **Owned by the UI**: the **New draft** action, the placement of drafts on index nodes in the map session, the planned chip, and the removal of a planned chip when its draft leaves the active drafts.
- **Delegated to backend (abstract)**:
  - `"create draft (name)"` — owned by `../core/DRS-draft-storage.md`.
  - `"attach draft to specification map node" (stub — no core spec yet)` — takes `{ draftId: string; nodeId: string }` and returns nothing. The stub keeps no state.

## Non-functional requirements
- A draft placement names the draft by its id, so a rename or a move of the draft in the Drafts panel never breaks the placement.
