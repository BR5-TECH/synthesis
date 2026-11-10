# Flow overlap analysis

**Spec code:** `FOA`

## Intent
The backend check that finds the tasks of a delivery plan that could run at the same time and are expected to change the same files. Each task or defect of a Flow (`FGV-flow-graph-validation.md`) can carry a **footprint**: the project paths its work is expected to change. Two such items that no `blocks` edge orders, and whose footprints share a path, are an **overlap**. Overlaps are what turn parallel work into merge conflicts, and a plan is cheapest to fix before the work starts. So the architect sees each overlap on the canvas (`../ui/FLO-flow.md` FLO-FR-KWFH), and the agent sees the same report in every Flow discussion (`AGC-agent-conversations.md` AGC-FR-KQNF) and helps to split the work or to order it. Out of scope: writing a footprint, which the author does on the canvas and the agent suggests in conversation; whether a footprint path exists on disk; the state of an item, which is `FDL-flow-delivery.md`'s; and any change to the plan, which this module never makes.

## Functional requirements
1. **FOA-FR-LBTE** `analyze_flow_overlap(body)` first validates the body (per `FGV-flow-graph-validation.md` FGV-FR-02). A body with violations returns `flow_invalid` with the report and no overlaps. An empty body returns no overlaps.
2. **FOA-FR-RSBH** Two leaf items **overlap** when each carries a footprint, an entry of one covers an entry of the other (FOA-FR-XRBA), and neither item is ordered before the other (FOA-FR-XOAR).
3. **FOA-FR-XRBA** An entry ending in `/` names a folder and covers itself and every path below it. Any other entry names a file and covers only itself. Paths compare segment by segment and are case-sensitive.
4. **FOA-FR-XOAR** Item A is **ordered before** item B when B waits for A through one or more `blocks` edges, by the waiting rule of `FGV-flow-graph-validation.md` FGV-FR-PMTY. A `relates` edge orders nothing.
5. **FOA-FR-KANP** An overlap reports its two item ids and its **shared paths**: for each pair of entries where one covers the other, the narrower entry. The shared paths are unique and sorted.
6. **FOA-FR-ZFRA** Each overlapping pair is reported once, with its items in document order. The overlaps are sorted by the document position of their first item, then of their second.
7. **FOA-FR-JVDK** An item with no footprint, and every container item, is in no overlap.
8. **FOA-FR-YUPB** A call reads no file, checks no path on disk, considers no item state, writes nothing, and emits no event. The same body always yields the same report.
9. **FOA-FR-LXAX** A call logs one record with the leaf count, the overlap count, and the duration, and no path or item name.

## Contract surface
### Tauri commands
- `"analyze flow overlap"` → `analyze_flow_overlap(body)` → `{ overlaps: Overlap[] }`. Invoked by `../ui/FLO-flow.md` (FLO-FR-STUN) and in-process by `AGC-agent-conversations.md` (AGC-FR-KQNF).

```
Overlap {
  items,          // [item_id, item_id], in document order
  paths           // the shared paths (FOA-FR-KANP), unique and sorted
}
```

Errors are typed: `flow_invalid`, carrying the validation report of `FGV-flow-graph-validation.md`.

This module invokes nothing but the validation, performs no filesystem access, and contacts no integration.

## Non-functional requirements
- A call on a plan of several hundred leaf items, each with a few footprint entries, answers within the short rest after an edit, so the canvas markers keep up with the author.
- The report is a function of the body alone. It needs no open project and holds no state between calls.
