# Specification map organization

**Spec code:** `SMO`

## Intent
The architect shapes the specification map to match how they plan the product: they add areas that do not exist yet, name and describe them, move specs and areas where they belong, and remove areas that no longer earn their place. This spec owns the organization actions of the Map tab (`SMP-specification-map.md`): creating an index node, renaming it and editing its summary, moving nodes by drag and drop, and deleting an index node. The tree always keeps its levels: every index node stays at the depth of its level, and spec nodes stay in leaf groups. Out of scope: persisting the organization beyond the map session, editing a spec node, and creating a spec node; new work starts as a draft (`SMD-specification-map-drafts.md`).

## Functional requirements
1. **SMO-FR-UAKE** For an index node subject above the leaf-group depth, the inspector offers **New** followed by the singular name of the next level, for example **New group**. While no node is selected, it offers **New** followed by the singular name of level 0. No such action is offered for a leaf group or a spec node.
2. **SMO-FR-HCQN** Creating and editing an index node use one dialog with a **Name** field and a **Summary** field. The name is trimmed and holds 1 to 60 characters, and the summary holds at most 400 characters. The confirm button is disabled while the name is empty. **Cancel** and Escape close the dialog and change nothing.
3. **SMO-FR-NXDL** A confirmed creation adds the new index node as the last child of the subject, or as the last root node when no node was selected. The new node has no children, becomes the selection, and the map centres it at its own level (per `SMZ-specification-map-zoom.md` SMZ-FR-EPTR).
4. **SMO-FR-PZEI** For an index node subject, the inspector offers **Rename**. It opens the dialog of SMO-FR-HCQN filled with the current name and summary. A confirmed edit changes the label and the summary on the canvas, in the hover card, in the inspector, and in the breadcrumb.
5. **SMO-FR-DKTM** A pointer press on a spec chip, an index node, or a planned chip, followed by a move of 4px or more, starts a drag of that node. A press released before 4px of movement is a click. During the drag a ghost of the node follows the pointer, and the source renders at opacity 0.4.
6. **SMO-FR-WGOF** A spec chip drops only on a leaf group. An index node at depth d drops only on an index node at depth d − 1, and a root node drops only among the root nodes. A planned chip drops on any index node. No node of the dragged subtree is a drop target.
7. **SMO-FR-ARQV** While the pointer is over a valid target, the target shows an `--accent` border and an insertion bar at the drop position. The drop position is the place among the target's children, or among the root nodes, nearest the pointer. Dropping on the node's current parent reorders the node among its siblings.
8. **SMO-FR-JYVB** Escape, a pointer cancel, or a release outside a valid target ends the drag and changes nothing.
9. **SMO-FR-SLNC** The dialog of SMO-FR-PZEI holds a **Move to** list of every valid new parent of SMO-FR-WGOF for the subject. Confirming with a new parent chosen moves the subject to be the last child of that parent.
10. **SMO-FR-IMXW** For an index node subject, the inspector offers **Delete**. It asks for confirmation in a dialog that names the node and its count of direct children. **Cancel** and Escape change nothing.
11. **SMO-FR-OBRF** A confirmed delete removes the index node. Its children and its planned drafts move, in order, to the end of its previous sibling, or to the start of its next sibling when it has no previous sibling. The selection moves to that sibling.
12. **SMO-FR-GQTS** When an index node has no sibling, **Delete** is disabled while the node has children. Its tooltip says that the children must move first. Confirming the delete of such a node without children moves its planned drafts to its parent, or discards their placement when the node is a root node.
13. **SMO-FR-CZLA** Every confirmed creation, edit, move, and delete applies to the map session at once and then invokes `"save specification map organization"` with that one edit. A failure of the invocation is logged and changes nothing on the map.
14. **SMO-FR-MHEU** Organization edits live in the map session and are discarded with it (per `SMP-specification-map.md` SMP-FR-MEZK). A new load of the index shows the index without them.

## Wireframes
```
┌ New group ───────────────────────────────┐
│ Name                                     │
│ [ planning                             ] │
│ Summary                                  │
│ [ Work the team intends to specify …   ] │
│                                          │
│ Move to    [ shell & navigation      ▾ ] │   (Rename only)
│                                          │
│                     [Cancel] [ Create ]  │
└──────────────────────────────────────────┘

drag of a spec chip over a leaf group
┌ panels ────────────────────────────────┐   ← --accent border
│ ┌──────────┐┃┌──────────┐┌──────────┐  │   ┃ = insertion bar
│ │LIB proj ●│┃│LCM cont ●││NTS notes●│  │
│ └──────────┘ └──────────┘└──────────┘  │
└────────────────────────────────────────┘
          ┌──────────┐
          │SCH search│  ghost at the pointer
          └──────────┘
```
- Layout notes: the dialogs are modal windows of the main window and follow its modal styling. The ghost has `--shadow-3` and never captures the pointer.

## UI contract boundary
- **Owned by the UI**: the organization actions and their dialogs, the drag gesture with its hit testing, the validity rules of the tree, and the application of edits to the map session.
- **Delegated to backend (abstract)**:
  - `"save specification map organization" (stub — no core spec yet)` — takes one edit: `{ kind: "create"; parentId: string | null; node: { id: string; label: string; summary: string } }`, `{ kind: "edit"; nodeId: string; label: string; summary: string }`, `{ kind: "move"; nodeId: string; parentId: string | null; index: number }`, or `{ kind: "delete"; nodeId: string }`. It returns nothing. The stub keeps no state.

## Non-functional requirements
- An edit re-renders the canvas, the edges, and the inspector in the same frame, so the map never shows a half-applied edit.
- Log records of an edit carry its kind only, never a label or a summary.
