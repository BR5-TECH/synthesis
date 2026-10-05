# Specification map inspector

**Spec code:** `SMI`

## Intent
The inspector is the reading pane of the specification map. It shows everything the map knows about the selected node — its place in the hierarchy, its summary, its completeness, its children, its dependencies, and what needs attention — and it holds the actions that start work from that node. This spec owns the inspector of the Map tab (`SMP-specification-map.md`), its placement, its content, and its navigation actions. The organization actions it hosts are specified in `SMO-specification-map-organization.md`, and **New draft** in `SMD-specification-map-drafts.md`.

## Functional requirements
1. **SMI-FR-QWOP** At a window width of 1160px or more, the inspector docks at the right of the canvas, 318px wide, on `--bg-panel` with a 1px `--border-1` left border, and it is open by default. Below 1160px it overlays the right edge of the canvas, 300px wide with `--shadow-4`, and it is closed by default.
2. **SMI-FR-BNHI** The close button in the inspector header hides the inspector. While the inspector is hidden, an `inspector` button at the top right of the canvas opens it.
3. **SMI-FR-TRUZ** The inspector header is 28px high. It shows the `INSPECTOR` micro-label, the singular level name of the subject in monospace `--fg-4`, and the close button.
4. **SMI-FR-KDGX** The subject of the inspector is the selected node. While no node is selected, the inspector body shows `Select a node to inspect it.` and the actions of `SMO-specification-map-organization.md` that need no subject.
5. **SMI-FR-MFSA** For a subject, the body shows in order: a breadcrumb of the ancestor labels joined by `→`, or `specifications` for a root node; a badge and the label at 14px weight 600; a summary card with a `SUMMARY` micro-label; a 6px completeness bar; a two-column grid of the four state counts; and a monospace line `R requirements · S scenarios`.
6. **SMI-FR-CEVL** For an index node, a children list follows, headed with the plural name of the next level. Each row shows the child label, a 44 × 4px completeness bar, and the child's spec count. For a spec node, the list is headed `Path` and holds one row that reads `specifications/<path>`. Clicking a child row selects that child.
7. **SMI-FR-YHNT** A **Depends on** list follows. For a spec node it holds the outgoing dependencies, most citations first; each row shows the code badge, the label, and the citation count, in `--danger` when unresolved. For an index node it holds the outgoing links at the depth of that node (per `SME-specification-map-edges.md` SME-FR-FKIW).
8. **SMI-FR-PNGA** Clicking a resolved row of the **Depends on** list selects the node it names. An unresolved row selects nothing.
9. **SMI-FR-ZLOA** A **Needs attention** card on `--danger-soft` follows when the subject has an issue. For a spec node it lists each unresolved outgoing dependency as `cites CODE — identifier resolves to nothing`. For an index node it lists up to four gap specs beneath it as `CODE — label: no coverage yet`. The card is hidden when it has no line.
10. **SMI-FR-GAJD** For a spec node, the primary action is **Open in editor**. It opens the spec file in its Editor tab, or jumps focus to the tab already showing it (per `LIB-library.md` LIB-FR-03, `TAB-tabs.md` TAB-FR-05).
11. **SMI-FR-EUXP** For an index node, the primary action is **Open** followed by the singular level name of the node, for example **Open feature**. It focuses the subtree of that node (per `SMZ-specification-map-zoom.md` SMZ-FR-FOXU).
12. **SMI-FR-RPCO** The secondary action **Zoom to** sets the level at which the subject renders collapsed — its depth for an index node, the last level for a spec node — and centres the subject (per `SMZ-specification-map-zoom.md` SMZ-FR-EPTR).
13. **SMI-FR-FJDW** A click on a canvas node, a click on an inspector row, and a spec row selection of the Project panel (per `LIB-library.md` LIB-FR-SJDC) all set the same selection, and the inspector shows the most recent one.
14. **SMI-FR-PRSL** When the Project panel selects a spec file while the Map tab is active, the map selects the spec node with that path, shows the last level, and centres the node. When no spec node has that path, the selection does not change. A path that arrives before the index loads applies once it loads.
15. **SMI-FR-WDAB** When the subject node is removed from the tree, the selection clears and the inspector shows the state of SMI-FR-KDGX.

## Wireframes
```
┌ INSPECTOR ─────────────── feature  × ┐
│ workspace                            │
│ [17 specs] shell & navigation        │
│ ┌ SUMMARY ─────────────────────────┐ │
│ │ Six zones, one navigation map …  │ │
│ └──────────────────────────────────┘ │
│ ▮▮▮▮▮▮▮▮▮▮▮▮▮▮▮▮▯▯                   │
│ 9 verified        6 built            │
│ 1 drafted         1 gaps             │
│ 312 requirements · 240 scenarios     │
│ GROUPS                               │
│ zones               ▮▮▮▯       4     │
│ panels              ▮▮▯▯       5     │
│ DEPENDS ON                           │
│ [25] agents & conversations     13   │
│ ┌ ● NEEDS ATTENTION ───────────────┐ │
│ │ DSH — dashboard: no coverage yet │ │
│ └──────────────────────────────────┘ │
│ [Open feature] [Zoom to]             │
│ [New group] [Rename] [Delete]        │
│ [New draft]                          │
└──────────────────────────────────────┘
```
- Layout notes: the body scrolls and has 14px padding. The primary action uses `--accent` and `--accent-fg`. **Zoom to** uses `--bg-canvas` with a 1px `--border-2` border. Micro-labels are 11px, uppercase, letter-spacing 0.08em, weight 600, `--fg-3`.

## UI contract boundary
- **Owned by the UI**: inspector placement and visibility, its content model, the selection shared by the canvas, the inspector, and the Project panel, and the navigation actions.
- **Delegated to backend (abstract)**:
  - `"open artifact by id"` — opens the spec file for **Open in editor**. Owned by `../core/PST-project-storage.md`.

## Non-functional requirements
- The inspector reads the same in-memory tree as the canvas, so the two never show different labels, counts, or children for one node.
