# Flow delivery

**Spec code:** `FDL`

## Intent
The backend that tells the architect how far along a delivery plan is. It reads a Flow document (`FGV-flow-graph-validation.md`) together with the project's drafts and graduation runs, and returns the state of every item, the draft each task is worked in, and which items are blocked by work that is not done. It also answers the reverse question for a draft: which plan items it belongs to. It exists because a plan only gives end-to-end visibility when its state comes from the work itself, not from a status the author must keep up to date by hand. The module only reads. A blocked item is reported, and nothing here starts, stops, or refuses a run. Out of scope: the schema, which is `FGV-flow-graph-validation.md`'s; the overlap of footprints, which is `FOA-flow-overlap-analysis.md`'s; the tracker values, which `FTS-flow-tracker-sync.md` stores in the document and this module only reads; and every graduation decision, which stays `GRD-graduation.md`'s.

## Functional requirements
1. **FDL-FR-AZQM** `load_flow_delivery_state(body)` first validates the body (per `FGV-flow-graph-validation.md` FGV-FR-02). A body with violations returns `flow_invalid` with the report and no state. An empty body returns an empty list.
2. **FDL-FR-TTZZ** A leaf item's **draft** is the draft its `draftId` names. A leaf with no `draftId` and a sync link takes the GitHub-shadow draft whose issue link has the same host, owner, repository, and number (per `DRS-draft-storage.md` DRS-FR-XDWS). Otherwise it has no draft.
3. **FDL-FR-BRAQ** A `draftId` that names no draft in the active worktree gives the item no draft and sets its `draft_missing` flag. The state of FDL-FR-VGZI is then taken as if the item had no `draftId`.
4. **FDL-FR-VGZI** A leaf item's state is `done` when its sync link's base `state` is `closed`. Otherwise it is: `not_started` with no draft; `dropped` when the draft is `archived`; `done` when the draft is `graduated`; `in_progress` when the draft's most recent run is not `completed`, `discarded`, or `failed`; and `drafting` in every other case.
5. **FDL-FR-RMYL** A leaf item with a draft reports the draft's id, name, and status, and the id and state of the draft's most recent graduation run, or no run (per `DRS-draft-storage.md` DRS-FR-18).
6. **FDL-FR-JYKC** A container item's state is `done` when its sync link's base `state` is `closed`. Otherwise it comes from the leaves under it at any depth: `not_started` with no leaves; `dropped` when every leaf is `dropped`; `done` when every leaf is `done` or `dropped`; `in_progress` when any leaf is `drafting`, `in_progress`, or `done`; and `not_started` otherwise.
7. **FDL-FR-IJXW** A container item also reports the count of the leaves under it in each state.
8. **FDL-FR-PXHL** An item is **blocked** when its state is not `done` or `dropped`, and a `blocks` edge whose `to` is the item or one of its ancestors has a `from` whose state is not `done` or `dropped`. Its `blocked_by` lists each such `from` once, in the document order of the edges.
   - *Why:* A dropped item will never be delivered, so work that waited for it waits for nothing.
9. **FDL-FR-PRED** A `relates` edge changes no item's state and blocks nothing.
10. **FDL-FR-PBSD** A call reads the draft list and the run states once, writes nothing, emits no event, and contacts no network. Two calls on the same body with the same drafts and runs return the same answer.
11. **FDL-FR-BDOD** `find_flow_items_for_draft(draft_id)` reads every `flow`-typed artifact of the active worktree from disk and returns each item whose draft (FDL-FR-TTZZ) is that draft. A Flow that fails validation is skipped. Results are in the Flows' path order, then in document order.
12. **FDL-FR-JLXF** Each result of FDL-FR-BDOD carries the Flow's artifact id and name, and the item's id, name, and kind.
13. **FDL-FR-VTOQ** Both operations refuse with `no_project_open` when no project is open, and log the refusal. `load_flow_delivery_state` logs one record per call with the item count and the duration, and no item name or description.

## Contract surface
### Tauri commands
- `"load flow delivery state"` → `load_flow_delivery_state(body)` → `{ items: ItemDeliveryState[] }`, one entry per item in document order. Invoked by `../ui/FLO-flow.md` (FLO-FR-STUN) and in-process by `AGC-agent-conversations.md` (AGC-FR-KQNF).
- `"find flow items for draft"` → `find_flow_items_for_draft(draft_id)` → `FlowItemMatch[]`. Invoked by `../ui/NAW-new-artifact.md` (NAW-FR-TXKN).

```
ItemDeliveryState {
  item_id,
  state,            // "not_started" | "drafting" | "in_progress" | "done" | "dropped"
  draft,            // { id, name, status } | null — leaf items alone
  draft_missing,    // true where draftId names no draft (FDL-FR-BRAQ)
  run,              // { run_id, state } | null — the draft's most recent run
  blocked,          // FDL-FR-PXHL
  blocked_by,       // item ids, in edge document order
  leaf_counts       // { not_started, drafting, in_progress, done, dropped } | null —
                    //   container items alone (FDL-FR-IJXW)
}

FlowItemMatch {
  flow_id,          // the Flow's artifact id (ASC-artifact-scanning.md ASC-FR-13)
  flow_name,
  item_id,
  item_name,
  kind
}
```

Errors are typed: `no_project_open`, and `flow_invalid` carrying the validation report of `FGV-flow-graph-validation.md`.

This module reads drafts through `DRS-draft-storage.md`'s list path, run states through `GRD-graduation.md`, and Flow files through `PST-project-storage.md`. It writes nothing and registers no event.

## Non-functional requirements
- A call on a plan of several hundred items answers fast enough to follow every edit's short rest without the author noticing, so the canvas never shows a state older than the last pause.
- The state is derived on every call and cached nowhere, so it can never disagree with the drafts and runs it was read from.
- No item name, description, or draft prompt is written to a log record.
