# Flow delivery

**Spec code:** `FDL`

## Intent
The backend that tells the architect how far along a delivery plan is. It reads a Flow document (`FGV-flow-graph-validation.md`) together with the project's drafts and graduation runs, and returns the state of every item, the draft each task is worked in, and which items are blocked by work that is not done. For a plan held on GitHub (`GDP-github-delivery-plan.md`) it also reads each item's tracker facts: whether the issue is closed and where it stands on the Project board. It also answers the reverse question for a draft: which plan items it belongs to. It exists because a plan only gives end-to-end visibility when its state comes from the work itself, not from a status the author must keep up to date by hand. The module only reads. A blocked item is reported, and nothing here starts, stops, or refuses a run. Out of scope: the schema, which is `FGV-flow-graph-validation.md`'s; the overlap of footprints, which is `FOA-flow-overlap-analysis.md`'s; the tracker facts, which `GDP-github-delivery-plan.md` reads and this module only uses; and every graduation decision, which stays `GRD-graduation.md`'s.

## Functional requirements
1. **FDL-FR-AZQM** `load_flow_delivery_state(body)` first validates the body (per `FGV-flow-graph-validation.md` FGV-FR-02). A body with violations returns `flow_invalid` with the report and no state. An empty body returns an empty list.
2. **FDL-FR-TTZZ** A leaf item's **draft** is the draft its `draftId` names. A leaf of a GitHub plan with no `draftId` takes the GitHub-shadow draft whose issue link has the host, owner, repository, and number of the leaf's issue (per `DRS-draft-storage.md` DRS-FR-XDWS). Otherwise it has no draft.
3. **FDL-FR-BRAQ** A `draftId` that names no draft in the active worktree gives the item no draft and sets its `draft_missing` flag. The state of FDL-FR-VGZI is then taken as if the item had no `draftId`.
4. **FDL-FR-VGZI** A leaf item's state is: `not_started` with no draft; `dropped` when the draft is `archived`; `done` when the draft is `graduated`; `in_progress` when the draft's most recent run is not `completed`, `discarded`, or `failed`; and `drafting` in every other case.
5. **FDL-FR-RMYL** A leaf item with a draft reports the draft's id, name, and status, and the id and state of the draft's most recent graduation run, or no run (per `DRS-draft-storage.md` DRS-FR-18).
6. **FDL-FR-KWMT** A **work item** is a leaf item, or a container item that holds no item. A container that holds no item takes its state by the rules of a leaf with no draft (FDL-FR-VGZI, FDL-FR-YIAZ, FDL-FR-WPSZ).
   - *Why:* a Feature or an Epic that nobody split into tasks is still work, and a milestone that holds it is not done.
6. **FDL-FR-JYKC** A container item with a work item under it takes its state from the work items under it at any depth: `dropped` when every work item is `dropped`; `done` when every work item is `done` or `dropped`; `in_progress` when any work item is `drafting`, `in_progress`, or `done`; and `not_started` otherwise.
6. **FDL-FR-YIAZ** An item of a GitHub plan that is closed is `dropped` where its close reason is `not_planned`, and `done` otherwise (per `GDP-github-delivery-plan.md` GDP-FR-QGFO). This rule takes the place of FDL-FR-VGZI and FDL-FR-JYKC for that item, except for a container with a work item under it that is neither `done` nor `dropped`: that container takes its state by FDL-FR-JYKC.
6. **FDL-FR-WPSZ** An open work item of a GitHub plan whose state by FDL-FR-VGZI is `not_started` or `drafting` is `in_progress` where its `Status` is the option that the In Progress status resolves to (per `GPP-github-polling.md` GPP-FR-VXSB).
7. **FDL-FR-IJXW** A container item also reports the count of the work items under it (FDL-FR-KWMT) in each state.
8. **FDL-FR-PXHL** An item is **blocked** when its state is not `done` or `dropped`, and a `blocks` edge whose `to` is the item or one of its ancestors has a `from` whose state is not `done` or `dropped`. Its `blocked_by` lists each such `from` once, in the document order of the edges.
   - *Why:* A dropped item will never be delivered, so work that waited for it waits for nothing.
8. **FDL-FR-UEPM** An item of a GitHub plan that is not `done` or `dropped` is also blocked where it has an open external blocker (per `GDP-github-delivery-plan.md` GDP-FR-URZL). An external blocker is not listed in `blocked_by`.
8. **FDL-FR-SBLQ** An item of a GitHub plan that is open and not `done` or `dropped` is also blocked where its `Status` is the option that the Blocked status resolves to (per `GPP-github-polling.md` GPP-FR-BLVX). Such an item lists nothing in `blocked_by` for its Status.
9. **FDL-FR-PRED** A `relates` edge changes no item's state and blocks nothing.
10. **FDL-FR-PBSD** A call reads the draft list and the run states once, writes nothing, emits no event, and contacts no network. Two calls on the same body with the same drafts and runs return the same answer.
11. **FDL-FR-BDOD** `find_flow_items_for_draft(draft_id)` reads every `flow`-typed artifact of the active worktree from disk and returns each item whose draft (FDL-FR-TTZZ) is that draft. A Flow that fails validation is skipped. Results are in the Flows' path order, then in document order.
11. **FDL-FR-JLUJ** Where the delivery source is `github` (per `DSR-delivery-source.md` DSR-FR-AECK), `find_flow_items_for_draft` also searches the plan of the read cache of `GDP-github-delivery-plan.md` (GDP-FR-YOSF) and makes no network request. Its results come after those of the Flow files.
12. **FDL-FR-JLXF** Each result of FDL-FR-BDOD carries the Flow's artifact id and name, the item's id, name, and kind, and whether the plan is the project's delivery. A result from a GitHub plan carries no artifact id, and the Project's title as its name.
13. **FDL-FR-VTOQ** Both operations refuse with `no_project_open` when no project is open, and log the refusal. `load_flow_delivery_state` logs one record per call with the item count and the duration, and no item name or description.

## Contract surface
### Tauri commands
- `"load flow delivery state"` → `load_flow_delivery_state(body)` → `{ items: ItemDeliveryState[] }`, one entry per item in document order. Invoked by `../ui/FLO-flow.md` (FLO-FR-STUN) and in-process by `AGC-agent-conversations.md` (AGC-FR-KQNF).
- In-process: `compute_delivery_state(document, tracker_facts)` → `{ items: ItemDeliveryState[] }`, the same answer for a GitHub plan with the rules FDL-FR-TTZZ, FDL-FR-YIAZ, FDL-FR-WPSZ, FDL-FR-SBLQ, and FDL-FR-UEPM applied. Invoked by `GDP-github-delivery-plan.md` (GDP-FR-XWES) (no UI consumer).
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
                    //   container items alone, counts of work items (FDL-FR-IJXW)
}

FlowItemMatch {
  flow_id,          // the Flow's artifact id (ASC-artifact-scanning.md ASC-FR-13), or null for a GitHub plan
  flow_name,
  delivery,         // true where the plan is the project's delivery (DSR-delivery-source.md)
  item_id,
  item_name,
  kind
}
```

Errors are typed: `no_project_open`, and `flow_invalid` carrying the validation report of `FGV-flow-graph-validation.md`.

This module reads drafts through `DRS-draft-storage.md`'s list path, run states through `GRD-graduation.md`, Flow files through `PST-project-storage.md`, the delivery source through `DSR-delivery-source.md`, and the read cache of a GitHub plan through `GDP-github-delivery-plan.md`. It writes nothing and registers no event.

## Non-functional requirements
- A call on a plan of several hundred items answers fast enough to follow every edit's short rest without the author noticing, so the canvas never shows a state older than the last pause.
- The state is derived on every call and cached nowhere, so it can never disagree with the drafts and runs it was read from.
- No item name, description, or draft prompt is written to a log record.
