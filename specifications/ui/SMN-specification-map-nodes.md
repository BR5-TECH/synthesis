# Specification map nodes

**Spec code:** `SMN`

## Intent
Every node of the specification map shows what it is, what it is for, and how complete it is, at the size its detail level allows. This spec owns the geometry and content of root cards, branch boxes, leaf-group boxes, and spec chips on the canvas of `SMZ-specification-map-zoom.md`, the completeness roll-ups and colours, selection and hover styles, the **gaps only** filter, and the hover card. The node vocabulary is the one of `SMP-specification-map.md` SMP-FR-FEJV.

## Functional requirements
1. **SMN-FR-XCOK** At level 0, root nodes render as 300 × 176px cards in a grid with a 30px gap: 3 columns for a canvas at least 1020px wide, 2 columns from 660px, and 1 column below. The n-th root node takes column `n mod columns`, below the previous root node of that column.
2. **SMN-FR-QHVM** A root card at level 0 shows a 9px hue swatch, the label, and a meta line `N specs · N <plural name of level 1>`. Below them it shows the summary clipped to 54px, a 6px completeness bar, and the counts `n verified`, `n built`, `n drafted`, and a right-aligned `n gaps` that is hidden when zero.
3. **SMN-FR-TNDA** An expanded index node at depth d is `320 + 20 × (G − d)` px wide, G being the leaf-group depth. Children stack 10px inside its left edge, 10px apart under a root node and 8px apart elsewhere, from 46px below a root node's top and 52px below a branch node's top.
4. **SMN-FR-JEQO** Expanded root nodes lay out in the column grid of SMN-FR-XCOK with a 34px gap. An expanded root node hides its summary and its counts row.
5. **SMN-FR-PFYW** A collapsed branch node is 116px high. It shows the label at 13px weight 600, a monospace `N specs` count at 10px, the summary at 11px clipped to 46px, and a 4px completeness bar.
6. **SMN-FR-UJRG** A collapsed leaf group is 64px high. It shows the label at 12px weight 500 in `--fg-2`, a 38 × 4px completeness bar, a monospace spec count at 10px, and the summary at 11px clipped to 32px.
7. **SMN-FR-ISBE** An expanded leaf group is `64 + rows × 28 + 4` px high. Its spec chips are 96 × 22px, 3 per row with 6px gaps, starting 62px below its top and 10px inside its left edge. A chip shows the code badge, the label truncated with an ellipsis, and a 5px dot in the state colour.
8. **SMN-FR-WAGD** A spec code badge takes the colour pair of the top-level folder of its spec under `specifications/`: `ui`, `core`, `ai`, `tools`, `infra`, or `server`. Each folder has one background and foreground pair for the light theme and one for the dark theme.
9. **SMN-FR-CMLN** The state colours are `--ok` for verified, `--accent` for built, `--fg-4` for drafted, and `--danger` for gap. Every completeness bar, status dot, and count of the map uses them.
10. **SMN-FR-OJAY** The spec count, the requirement and scenario totals, and the count per state of an index node are computed from the spec nodes beneath it in the current tree, including the effect of organization edits. Each completeness bar holds four segments in state order, each as wide as its share of those spec nodes.
11. **SMN-FR-EYRV** A click on a node selects it. A selected root card has an `--accent` border and a 3px `--ring` shadow. A selected branch node or leaf group has an `--accent` border. A selected spec chip has an `--accent` border and an `--accent-soft` background.
12. **SMN-FR-KTZB** The active node is the hovered node while the pointer is over a node, and the selected node otherwise. The root node and the branch node that contain the active node, when rendered and not selected, have a `--border-3` border.
13. **SMN-FR-HLDQ** While the **gaps only** toggle is on, a spec chip that is not in the gap state renders at opacity 0.25, and an index node with no gap spec and no drafted spec beneath it renders at opacity 0.3. The toggle is off by default. It uses `--danger-soft` and `--danger` when on, and `--bg-sunken` and `--fg-3` when off.
14. **SMN-FR-MVWA** Hovering a node shows a hover card 280px wide. Its top-left corner sits 18px right of and 14px below the node's top-left corner in screen space, clamped to stay 8px inside the canvas. It shows a badge and the label, the summary, a 5px completeness bar, and a monospace meta line.
15. **SMN-FR-FXRL** The hover card badge is the spec code for a spec node and `N specs` for an index node. The meta line reads `R reqs · S scenarios · cites C` for a spec node, where C is its count of outgoing dependencies, and `R reqs · S scenarios · G gaps` for an index node.
16. **SMN-FR-YSKQ** The hover card ignores the pointer. It hides when the pointer leaves the node, and it does not show while a pan or a drag is in progress.

## Wireframes
```
level 0 root card                      level 3 expanded leaf group
┌───────────────────────────────────┐  ┌───────────────────────────────────┐
│ ■ authoring       26 specs · 3 ft │  │ zones                    ▮▮▯   4  │
│ Everything the architect writes … │  │ Layout of the main window …       │
│ ▮▮▮▮▮▮▮▮▮▮▯▯▯▯▯▯▯▮▮               │  │ ┌─────────────┐┌─────────────┐┌── │
│ 3 verified 13 built 8 drafted  2g │  │ │OVW UI ove… ●││SNV shell …● ││STB│
└───────────────────────────────────┘  │ └─────────────┘└─────────────┘└── │
                                       └───────────────────────────────────┘
```
- Layout notes: a root card has `--bg-canvas`, a 1px `--border-1` border, `--r-md` radius, and `--shadow-2`. A branch box has `--bg-panel`, a 1px `--border-1` border, and `--r-sm` radius. A leaf-group box has `--bg-canvas`, a 1px `--border-1` border, and `--r-sm` radius. A chip has `--bg-canvas`, a 1px `--border-1` border, and `--r-xs` radius. The hover card has `--bg-elevated`, a 1px `--border-1` border, `--r-md` radius, and `--shadow-3`. Summaries use `text-wrap: pretty`. Codes, counts, and meta lines use the monospace font.

## UI contract boundary
- **Owned by the UI**: node geometry and content per level, the folder colour pairs, the state colours, the roll-ups, the selection and hover styles, the **gaps only** filter, and the hover card.
- **Delegated to backend (abstract)**: none. The node data comes from `"load specification map"` of `SMP-specification-map.md`.

## Non-functional requirements
- The roll-ups need no pre-aggregated count from the index.
- No node style uses a coloured shadow or a glow. Focus and selection use the `--ring` token.
