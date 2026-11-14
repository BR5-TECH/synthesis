# Delivery machine state

**Spec code:** `DMS`

## Intent
The per-machine home for how the author arranges a delivery canvas, and for the plan data that a GitHub plan has no field for. The **layout** of a plan — where each item sits, how large each container is, and which containers are collapsed — is a preference of one person on one screen, so it is kept on that machine and never committed. The **overlay** of a GitHub plan holds what GitHub cannot: each item's references, footprint, and draft link, the `relates` edges, and the plan's references. Both are kept in the repository store (`RMS-repository-machine-storage.md`), outside every worktree, so every worktree of the repository reads the same records and Git never sees them. Each machine has its own overlay, so two people, and the agents on their two machines, can read different footprints and links for the same issue. Out of scope: the plan itself, which its source holds (`DSR-delivery-source.md`); the read cache of a GitHub plan, which `GDP-github-delivery-plan.md` owns; and the auto-layout, which the canvas computes (per `../ui/FLO-flow.md` FLO-FR-PLUZ).

## Functional requirements
1. **DMS-FR-KITP** The machine state is kept under `delivery/` in the repository store of the open project (per `RMS-repository-machine-storage.md` RMS-FR-ZXHM). No path of it is inside a worktree.
2. **DMS-FR-DMWD** A **layout scope** is `github:<project-node-id>` for the GitHub plan of a Project and `flow:<artifact-id>` for a Flow file. A scope holds, for each item id, an optional position, an optional size for a container item, and a collapsed flag for a container item.
3. **DMS-FR-DPXR** `load_delivery_layout(scope)` returns the layout of the scope. A scope with no stored layout returns an empty layout.
4. **DMS-FR-XAZV** `save_delivery_layout(scope, layout)` replaces the layout of the scope with one atomic whole-file write (per `RMS-repository-machine-storage.md` RMS-FR-PFOB). An empty layout removes the scope's record.
5. **DMS-FR-TJSJ** `save_delivery_layout` refuses a scope that is not of the form of DMS-FR-DMWD, a position or a size that is not finite, and a size that is not positive, with `invalid_layout`, and writes nothing.
6. **DMS-FR-WAUS** The **overlay** of a GitHub plan is kept for each Project node ID. It holds, for each item id, `artifactIds`, `footprint`, and `draftId`. It also holds the `relates` edges, each with its two item ids, the plan's `references`, and the **milestone order**, a list of milestone item ids from first to last (per `../ui/FLO-flow.md` FLO-FR-MCFR).
7. **DMS-FR-QFEA** An overlay entry obeys the rules that `FGV-flow-graph-validation.md` sets for the same field: FGV-FR-RQEM for a footprint, FGV-FR-JDGO for references, FGV-FR-CTFX for draft links, and FGV-FR-11 for `relates` edges. A write that breaks one is refused with `invalid_overlay` and the violations, and writes nothing.
8. **DMS-FR-VEHN** The overlay is read and written in-process only, by `GDP-github-delivery-plan.md` (GDP-FR-CRRO, GDP-FR-YBHD) and `DSR-delivery-source.md` (DSR-FR-PTIT, DSR-FR-RZUH). No command writes it directly.
9. **DMS-FR-RVJQ** A **publish journal** is kept for each Flow artifact id. It maps each item id and each edge id of the Flow to the tracker reference that a publish created for it (per `DSR-delivery-source.md` DSR-FR-ULON). It is removed when the publish completes.
10. **DMS-FR-EKKY** A layout entry or an overlay entry that names an item the plan does not hold is kept. A reader ignores it.
11. **DMS-FR-ETLQ** A record that cannot be parsed reads as empty, and the read logs one record through `LGC-logging.md` with the record kind and no content.
12. **DMS-FR-IHML** Each operation refuses with `no_project_open` when no project is open, and with `store_unavailable` where the repository store is unavailable (per `RMS-repository-machine-storage.md` RMS-FR-HAJC).
13. **DMS-FR-ROMF** No log record of this module carries a path of a footprint, an artifact id, a draft id, or an item name.

## Contract surface
### Tauri commands
- `"load delivery layout (scope)"` → `load_delivery_layout(scope)` → `DeliveryLayout`. Invoked by `../ui/FLO-flow.md` (FLO-FR-PLUZ).
- `"save delivery layout (scope, layout)"` → `save_delivery_layout(scope, layout)` → nothing. Invoked by `../ui/FLO-flow.md` (FLO-FR-21) and `../ui/MDL-map-delivery-lens.md` (MDL-FR-QAJN).

### In-process
- `read_delivery_overlay(project_node_id)` → `DeliveryOverlay` (no UI consumer).
- `write_delivery_overlay(project_node_id, overlay)` → nothing, or `invalid_overlay` (no UI consumer).
- `read_publish_journal(flow_id)`, `write_publish_journal(flow_id, journal)`, and `remove_publish_journal(flow_id)` (no UI consumer).

```
DeliveryLayout {
  items: {
    [item_id]: {
      position,     // { x, y } | absent — measured from the origin of the item's container
      size,         // { width, height } | absent — container items alone
      collapsed     // boolean | absent — container items alone
    }
  }
}

DeliveryOverlay {
  items: { [item_id]: { artifactIds?, footprint?, draftId? } },
  relates: { from, to }[],
  references: string[],
  milestoneOrder: string[]
}
```

The records are `delivery/layout/<scope-digest>.json`, `delivery/overlay/<project-node-id>.json`, and `delivery/publish/<flow-digest>.json`, where a digest is the first 32 lowercase hexadecimal characters of the SHA-256 of the scope or the artifact id.

## Non-functional requirements
- A drag on the canvas writes the layout at most once after the drag ends, so a layout write never follows the pointer.
- The machine state is never staged, committed, pushed, or sent to GitHub.
