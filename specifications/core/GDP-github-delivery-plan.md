# GitHub delivery plan

**Spec code:** `GDP`

## Intent
The backend that reads and writes the project's delivery plan when its source is a GitHub Project (per `DSR-delivery-source.md`). The plan is the issues on the selected Project board, with their sub-issue hierarchy, their milestones, their blocked-by relations, and the Project's `Status` field. GitHub is the plan's one source of truth: each edit the architect makes on the canvas goes to GitHub at once, and no local copy of the plan is ever written back. The module keeps a per-machine read cache so that the plan opens fast and offline. All tracker access goes through one tracker adapter, so another tracker can take GitHub's place later. Out of scope: which source is active, which belongs to `DSR-delivery-source.md`; the rules of an item's state, which belong to `FDL-flow-delivery.md`; the overlaps, which belong to `FOA-flow-overlap-analysis.md`; the per-machine layout and overlay, which belong to `DMS-delivery-machine-state.md`; the claim of a ready task, which belongs to `GPP-github-polling.md`; and Project fields other than `Status`, assignees, labels, and comments.

## Functional requirements
1. **GDP-FR-QSHX** `load_github_delivery_plan(refresh)` refuses with `not_github_source` where `DSR-delivery-source.md` resolves another source (DSR-FR-AECK). It refuses with the configuration error of `GPP-github-polling.md` where the configuration is not `valid` (GPP-FR-DEZO).
2. **GDP-FR-ZGJB** Every request goes to the host of the polling repository (per `GPP-github-polling.md` GPP-FR-RGNM and GPP-FR-HSTD), with the secret of `GTS-github-token-storage.md` GTS-FR-13, through the GraphQL and REST bases of GTS-FR-PDWB.
3. **GDP-FR-BGIT** A read takes every item of the selected Project. A **plan issue** is an item that is an issue of the polling repository whose kind is mapped (per `DSR-delivery-source.md` DSR-FR-MKIG). A pull request, a draft item, an issue of another repository, an excluded issue, and an unmapped issue are not plan issues.
4. **GDP-FR-MYLM** The plan's milestones are every open milestone of the polling repository and every closed milestone that a plan issue names. Each one becomes a `milestone` item with the milestone's title as its name, its description, and its due date.
5. **GDP-FR-NFWA** A plan issue becomes an item of its kind, with the issue's title as its name and its body as its description. An issue item's id is `issue-<number>`, and a milestone item's id is `milestone-<number>`.
6. **GDP-FR-KBWK** An item's container is its parent issue where that parent is a plan issue whose kind can hold the item (per `FGV-flow-graph-validation.md` FGV-FR-YHFX). Otherwise it is the item of its milestone, and otherwise the top level.
7. **GDP-FR-JCAR** An item whose parent issue is a plan issue that cannot hold it is reported as **misplaced**, with the item and its parent.
8. **GDP-FR-SYSZ** A blocked-by relation between two plan items becomes a `blocks` edge from the blocking item to the blocked item. A relation that joins an item to its ancestor or descendant, or that closes a dependency cycle (per FGV-FR-KBLT and FGV-FR-PMTY), becomes no edge and is reported as **omitted**. Relations are taken in Project order.
9. **GDP-FR-URZL** A blocked-by relation to an open issue that is not a plan issue is an **external blocker** of the blocked item. It carries the issue's number, title, and URL.
10. **GDP-FR-QGFO** Each item carries its **tracker facts**: its reference (`issue` or `milestone`, number, URL), its state (`open` or `closed`), its close reason (`completed`, `not_planned`, or none), its `updated_at`, and, for an issue, its `Status` name and whether it is eligible for a claim (per `GPP-github-polling.md` GPP-FR-BOQX).
11. **GDP-FR-CRRO** A read adds the overlay of `DMS-delivery-machine-state.md` (DMS-FR-WAUS): each item's references, footprint, and draft link, the `relates` edges, the plan's references, and the milestone order. An overlay entry that names no plan item is kept and is not part of the plan.
12. **GDP-FR-HNEW** An answer holds the plan as a Flow document of version `3` (per `FGV-flow-graph-validation.md` FGV-FR-01), with the Project's title as its name and the Project's short description as its description. Milestones come first, in the order of GDP-FR-XJDE. Issues follow in Project order.
12. **GDP-FR-XJDE** The milestones of an answer follow the milestone order of the overlay (DMS-FR-WAUS). The milestones it does not name come after them, by due date with undated ones last, then by number.
13. **GDP-FR-XWES** An answer also holds the tracker facts, the delivery state of each item (per `FDL-flow-delivery.md` FDL-FR-YIAZ), the overlaps (per `FOA-flow-overlap-analysis.md` FOA-FR-RSBH), and the **notices**: each unmapped Type with its issue count, the misplaced items, the omitted relations, and the count of skipped Project items.
14. **GDP-FR-YOSF** A successful GitHub read stores what it read, and nothing it derived, as the **read cache** of the selected Project in the repository store (per `RMS-repository-machine-storage.md` RMS-FR-OCPB). It replaces the previous cache of that Project.
14. **GDP-FR-URZY** Each change of a read cache, by a read or by an edit, emits `"github delivery plan changed"` with the Project node ID.
15. **GDP-FR-LKXA** With `refresh` false, a load answers from the read cache of the selected Project where one exists, and computes the states from the current drafts and runs. Where no cache exists, it reads GitHub. With `refresh` true, it reads GitHub.
16. **GDP-FR-GCJO** A GitHub read that fails answers from the read cache, marked `stale`, with the typed error. Where no cache of the Project exists, the load refuses with the error.
17. **GDP-FR-WEFV** Each answer carries `read_at`, the instant of the GitHub read that its tracker data comes from.
18. **GDP-FR-RJFS** At most one GitHub read runs at a time for one open project. A load that asks for a read while one runs waits for it and returns its answer.
19. **GDP-FR-VNNU** `apply_github_delivery_edit(edit)` applies one edit (GDP-FR-UDOL to GDP-FR-YBHD). It then reads each changed item again, updates the read cache, and returns the plan as a load with `refresh` false returns it.
20. **GDP-FR-GBDF** Before a tracker write, an edit reads each tracker item that it changes. Where an item's `updated_at` differs from the value the edit names, it refuses with `changed_on_tracker`, writes nothing, and the read cache takes the values it read.
21. **GDP-FR-UDOL** `create_item` creates a repository milestone for the kind `milestone`. For another kind it creates an issue with the create Type of that kind (per `DSR-delivery-source.md` DSR-FR-ZJMC), its parent and its milestone on the terms of GDP-FR-NJJZ, and adds the issue to the selected Project. It returns the new item's id.
22. **GDP-FR-CNVO** `create_item` refuses with `issue_type_unavailable` where the repository offers Types but not the create Type of the kind. A repository that offers no Type gets an issue with no Type, and only for the kind of the untyped choice; another kind is refused with `issue_type_unavailable`.
23. **GDP-FR-DTPB** `rename` writes an item's name as the issue or milestone title. `set_description` writes it as the body or the milestone description. `set_due` writes a milestone's due date, or removes it.
24. **GDP-FR-EJTS** `set_kind` writes the create Type of the new kind to the issue. A change to or from the kind `milestone` is refused with `kind_change_unsupported`.
25. **GDP-FR-NJJZ** `move` sets an issue's parent to its nearest container that is an issue, or removes the parent where there is none. It sets the issue's milestone to its nearest container that is a milestone, or removes the milestone where there is none. A milestone item has no container, and a `move` of one is refused with `kind_change_unsupported`.
26. **GDP-FR-YGXA** `add_edge` with the type `blocks` adds a blocked-by relation to the blocked issue, and `remove_edge` removes it. A `blocks` edge with a milestone at either end is refused with `milestone_edge_unsupported`.
27. **GDP-FR-WAKG** `remove_item` with the mode `remove_from_board` removes the issue, and each plan issue under it, from the selected Project. The mode `close_not_planned` closes each of them with the reason not planned. For a milestone item, either mode also closes the milestone.
28. **GDP-FR-HULQ** `add_existing_issue` adds an issue of the polling repository to the selected Project, then moves it to the container the edit names. It refuses with `tracker_item_not_found` for no such issue or a pull request, with `already_in_plan`, and with `type_unmapped`.
29. **GDP-FR-YBHD** The overlay edits `set_references`, `set_footprint`, `link_draft`, `unlink_draft`, `add_edge` and `remove_edge` with the type `relates`, `set_plan_references`, and `set_milestone_order` write to `DMS-delivery-machine-state.md` (DMS-FR-VEHN). They make no GitHub request.
30. **GDP-FR-MSVS** An edit that needs several tracker writes stops at the first failure and refuses with `partial_edit` and the error. The writes it made stay on GitHub, and the plan it returns shows them.
31. **GDP-FR-IRDD** At most one edit runs at a time for one open project. Edits apply in the order they arrive.
32. **GDP-FR-PIAQ** `open_tracker_link(url)` opens the OS default browser at the URL, and opens no page inside the application. It refuses with `tracker_url_refused` a URL that is not an HTTPS issue or milestone address of the polling repository, or the address of the selected Project.
33. **GDP-FR-UPXR** All tracker access goes through one **tracker adapter** interface: read the Project's items with their fields, read the milestones, create and update an issue, set its parent and its milestone, add and remove a blocked-by relation, add and remove a Project item, create, update, and close a milestone, and close an issue with a reason. `github` is the one adapter.
34. **GDP-FR-QHQR** Each read and each edit logs its operation, its outcome, its counts, and its duration, and every error, handled ones included, through `LGC-logging.md`. No record carries a token, a title, a body, a Type name, or a URL query.
35. **GDP-FR-ZFQH** Each operation refuses with `no_project_open` when no project is open, and logs the refusal.

## Contract surface
### Tauri commands
- `"load github delivery plan (refresh)"` → `load_github_delivery_plan(refresh)` → `GithubDeliveryPlan`. Invoked by `../ui/MDL-map-delivery-lens.md` (MDL-FR-LZIE) and in-process by `DSR-delivery-source.md` (DSR-FR-OIAS) and `FDL-flow-delivery.md`, which reads its read cache (FDL-FR-JLUJ).
- `"apply github delivery edit (edit)"` → `apply_github_delivery_edit(edit)` → `GithubDeliveryPlan`. Invoked by `../ui/MDL-map-delivery-lens.md` (MDL-FR-XJBV) and in-process by `DSR-delivery-source.md` (DSR-FR-GXSE).
- `"open tracker link (url)"` → `open_tracker_link(url)` → nothing. Invoked by `../ui/FLO-flow.md` (FLO-FR-ULEM) and `../ui/MDL-map-delivery-lens.md` (MDL-FR-BZHY).

### Events
- `"github delivery plan changed"` — payload `{ project_node_id }` (GDP-FR-URZY). Consumed by `../ui/DSH-dashboard.md` (DSH-FR-LGJT).

```
GithubDeliveryPlan {
  document,        // a Flow document of version 3 (FGV-FR-01)
  tracker,         // TrackerFacts[], one per item
  states,          // ItemDeliveryState[] (FDL-flow-delivery.md)
  overlaps,        // Overlap[] (FOA-flow-overlap-analysis.md)
  notices: {
    unmapped_types,      // { type: string | null, count }[]
    misplaced,           // { item_id, parent_item_id }[]
    omitted_relations,   // { blocked_item_id, blocking_item_id, reason: "lineage" | "cycle" }[]
    skipped              // count of Project items that are not plan issues
  },
  read_at,
  stale,           // true where a failed read answered from the cache (GDP-FR-GCJO)
  error            // the typed error of that read, or null
}

TrackerFacts {
  item_id,
  ref,             // { kind: "issue" | "milestone", number, url }
  state,           // "open" | "closed"
  close_reason,    // "completed" | "not_planned" | null
  updated_at,
  status,          // the Project Status name, or null; issues alone
  claim_eligible,  // GPP-FR-BOQX; issues alone
  external_blockers   // { number, title, url }[] (GDP-FR-URZL)
}

GithubDeliveryEdit =
  | { kind: "create_item", item_kind, name, container_id, description?, due? }
  | { kind: "rename", item_id, name, updated_at }
  | { kind: "set_description", item_id, description, updated_at }
  | { kind: "set_due", item_id, due, updated_at }
  | { kind: "set_kind", item_id, item_kind, updated_at }
  | { kind: "move", item_id, container_id, updated_at }
  | { kind: "add_edge", from, to, type, updated_at }
  | { kind: "remove_edge", from, to, type, updated_at }
  | { kind: "remove_item", item_id, mode: "remove_from_board" | "close_not_planned", updated_at }
  | { kind: "add_existing_issue", number, container_id }
  | { kind: "set_references", item_id, artifact_ids }
  | { kind: "set_footprint", item_id, footprint }
  | { kind: "link_draft", item_id, draft_id }
  | { kind: "unlink_draft", item_id }
  | { kind: "set_plan_references", references }
  | { kind: "set_milestone_order", milestone_ids }   // milestone item ids, first to last (DMS-FR-WAUS)
```

`container_id` is an item id or `null` for the top level. `updated_at` is the value of the tracker facts that the edit was made against; an edge edit names the blocked item's value.

Errors are typed: `no_project_open`, `not_github_source`, the configuration and request errors of `GPP-github-polling.md` GPP-FR-SUFH, `changed_on_tracker`, `issue_type_unavailable`, `kind_change_unsupported`, `milestone_edge_unsupported`, `tracker_item_not_found`, `already_in_plan`, `type_unmapped`, `partial_edit` with the error that stopped it, `tracker_url_refused`, `invalid_overlay` from `DMS-delivery-machine-state.md`, and `store_unavailable` from `RMS-repository-machine-storage.md`.

The read cache is `delivery/github/<project-node-id>.json` in the repository store. This module reads the delivery settings through `DSR-delivery-source.md`, drafts through `DRS-draft-storage.md`, run states through `GRD-graduation.md`, and the overlay through `DMS-delivery-machine-state.md`.

## Non-functional requirements
- A load with `refresh` false answers from the cache without a network request, so the Map tab opens on the plan before GitHub answers.
- A read of a Project of several hundred issues uses paged GraphQL requests and does not issue one request for each issue.
- The read cache is a copy of what GitHub said. It is never the source of a write, and losing it costs one read.
