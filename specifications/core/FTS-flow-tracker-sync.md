# Flow tracker sync

**Spec code:** `FTS`

## Intent
The backend that keeps a delivery plan and the team's tracker the same plan. A Flow (`FGV-flow-graph-validation.md`) that carries a sync section is synced in both directions with the project's publication repository on GitHub (per `GHP-github-publication.md` GHP-FR-YDAN): a deliverable, an epic, a feature, a task, or a defect is an issue with an issue Type, a container is the parent of its members' issues, a milestone is a GitHub milestone, and a `blocks` edge is a "blocked by" relation. It exists because the architect plans in the IDE and the team works in the tracker, and a plan that only one side can change is a plan that one side stops reading. Each sync compares, field by field, the Flow's value, the tracker's value, and the **base** both had at the last sync. A change on one side goes to the other side. A field both sides changed differently is a **conflict**, and the author decides it (`../ui/FSR-flow-sync-review.md`); nothing is overwritten silently. Every tracker call goes through one **adapter** boundary, so another tracker can be added beside GitHub. Out of scope: Jira and every tracker other than GitHub; GitHub Project fields, assignees, labels, and comments; closing or reopening an issue or a milestone, which the team and its CI/CD own (per `GPP-github-polling.md`); `relates` edges, which stay in the Flow; and a sync that runs on a timer, every sync being one the author starts.

## Functional requirements
1. **FTS-FR-LTMW** `sync_flow_with_tracker(flow_id)` reads the Flow's file through `PST-project-storage.md` and validates it (per `FGV-flow-graph-validation.md` FGV-FR-02). An invalid Flow is refused with `flow_invalid`, and a Flow with no sync section with `sync_not_enabled`. Neither makes a tracker request.
2. **FTS-FR-ZLFX** Every request goes to the publication repository that `GHP-github-publication.md` GHP-FR-YDAN resolves, with the secret of `GTS-github-token-storage.md` GTS-FR-13, through the GraphQL and REST bases of GTS-FR-PDWB. A refusal of either resolution refuses the sync with its code, and no request is made.
3. **FTS-FR-TUEJ** At most one sync or resolve runs for one Flow at a time. A second call while one runs is refused with `sync_in_progress`.
4. **FTS-FR-ARMD** All tracker access goes through one **tracker adapter** interface: read items, read sub-issues, create and update an issue, set its parent and its milestone, add and remove a blocked-by relation, and create and update a milestone. `sync.tracker` selects the adapter, and `github` is the one adapter.
5. **FTS-FR-PNMA** An issue link syncs the fields `name` (the title), `description` (the body), `type` (the issue Type of the item's kind, FTS-FR-LMBE), `parent`, `milestone`, and `blockedBy`. A milestone link syncs `name`, `description`, and `due`. Both read `state` from the tracker.
6. **FTS-FR-HLVJ** An item's `parent` is the link of its nearest ancestor that has a link and is not a milestone, or none. Its `milestone` is the number of its nearest milestone ancestor that has a link, or none. Its `blockedBy` is the links of the `from` items of the `blocks` edges whose `to` is the item.
7. **FTS-FR-AIJM** For each field of a link: a field only the Flow changed since the base is written to the tracker; a field only the tracker changed is written to the Flow; a field both changed to the same value only updates the base; a field both changed to different values is a conflict (FTS-FR-VEEG).
8. **FTS-FR-VYOV** `blockedBy` merges per entry: each entry the Flow or the tracker added since the base is added on the other side, and each entry either removed is removed on the other side. It never yields a conflict.
9. **FTS-FR-VRRJ** Descriptions compare with line endings normalized to LF and trailing whitespace at the end of the text removed.
10. **FTS-FR-CYJJ** A sync writes no `state`. It stores the tracker's `state` of each linked item in the base, so a closed issue or milestone reads as done (per `FDL-flow-delivery.md` FDL-FR-VGZI).
11. **FTS-FR-HDRR** A sync creates a tracker item for each item with no link that is not `localOnly`: a milestone for a milestone item, and otherwise an issue with the item's name, description, Type, parent, and milestone. Items are created parents first. Each new link stores the created values as its base.
12. **FTS-FR-DNBY** After the creations, a sync adds the blocked-by relations of the created items on the tracker.
13. **FTS-FR-EGJL** A sync pulls each open or closed sub-issue of a linked issue that no link of the Flow names. The pulled item is placed in the linked milestone item of its milestone, where that item is under the parent's item, and in the parent's item otherwise.
14. **FTS-FR-CJVS** A pulled item takes the first kind, in the order deliverable, epic, feature, task, defect, whose Type in the settings equals the issue's Type and that its container permits (per `FGV-flow-graph-validation.md` FGV-FR-YHFX). An issue for which no kind fits is not pulled and is reported as skipped with `no_kind_fits`.
15. **FTS-FR-LKZO** A pulled item is placed below the last member of its container, and a pulled container kind takes a default size. Its own sub-issues are pulled in the same sync.
16. **FTS-FR-VEEG** A conflict carries an id, the item id, the field, and the base, Flow, and tracker values. Its id is derived from the Flow id, the item id, the field, and the two values, so the same conflict has the same id in every sync.
17. **FTS-FR-AONJ** A sync changes neither side of a conflicting field and keeps its base, so the next sync reports the same conflict until the author resolves it.
18. **FTS-FR-SZGF** A pulled `parent`, `milestone`, or `type` that would give the item a container or a kind its hierarchy refuses is a conflict of kind `invalid_on_tracker`. Its choices are `keep_flow`, which writes the Flow's value to the tracker, and `unlink`.
19. **FTS-FR-BMWI** A link whose item is gone from the Flow is a conflict of kind `removed_in_flow`. Its choices are `unlink`, which removes the link and leaves the tracker item as it is, and `restore`, which adds the item back from the tracker values in the container its parent and milestone give.
20. **FTS-FR-VXMN** A link whose tracker item cannot be found, was deleted, or was transferred to another repository is a conflict of kind `removed_on_tracker`. Its choices are `remove_item`, which removes the item and its edges, and `unlink`, which keeps the item and removes the link.
21. **FTS-FR-PBQC** A field conflict offers two choices: `keep_flow`, which writes the Flow's value to the tracker, and `take_tracker`, which writes the tracker's value to the Flow. Either choice sets the base to the chosen value.
22. **FTS-FR-ARHM** `resolve_flow_sync_conflicts(flow_id, resolutions)` reads the Flow and the tracker again and applies each resolution whose conflict id still matches. A resolution whose id no longer matches is returned as `stale` and applies nothing.
23. **FTS-FR-VFNM** A sync or a resolve makes every tracker write first and then writes the Flow once, through `PST-project-storage.md`'s save path, which validates it (PST-FR-28). A Flow that nothing changed is not written.
24. **FTS-FR-AIDI** A Flow write that fails after tracker writes returns `write_failed` with the URL of each tracker item the call created. The bases stay as they were, so the next sync finds the other written fields equal on both sides and only updates their bases (FTS-FR-AIJM).
25. **FTS-FR-LVTU** A sync returns the counts of tracker items created and updated, of Flow items added and updated, the conflicts, and the skipped issues, each skipped issue with its number, title, and reason.
26. **FTS-FR-WANV** A link whose reference names a repository other than the publication repository is not synced and is reported as skipped with `foreign_repository`.
27. **FTS-FR-LMBE** The **flow tracker settings** hold the issue Type of each issue kind: deliverable, epic, feature, task, and defect. An unset value reads as `Feature`, `Feature`, `Feature`, `Task`, and `Bug`. They are stored through `PSS-project-settings-storage.md` (PSS-FR-VGWF).
28. **FTS-FR-ALOJ** `set_flow_tracker_settings` stores the five Types and returns them. It refuses an empty Type name with `invalid_flow_tracker_settings` and writes nothing. It never checks a Type against GitHub.
29. **FTS-FR-XITX** An issue the sync would create with a Type the repository does not offer is not created. Its item is reported as skipped with `issue_type_unavailable`, and the sync goes on with the other items.
30. **FTS-FR-TPWE** `read_tracker_item(flow_id, kind, number)` reads one issue or milestone of the publication repository and returns its reference and its values on the terms of FTS-FR-PNMA. It refuses with `tracker_item_not_found` for no such item and for a pull request, and with `already_linked` where a link of that Flow names it.
31. **FTS-FR-OBPX** `open_tracker_link(url)` opens the OS default browser at the URL and opens no page inside the application. It refuses with `tracker_url_refused` a URL that is not an HTTPS issue or milestone address of the publication repository.
32. **FTS-FR-FDDV** A sync logs its start, each tracker request with its operation and its outcome, its counts, and every error, handled ones included. No record carries a token, a title, a description, or a URL query.

## Contract surface
### Tauri commands
- `"sync flow with tracker"` → `sync_flow_with_tracker(flow_id)` → `SyncResult`. Invoked by `../ui/FLO-flow.md` (FLO-FR-AKMV).
- `"resolve flow sync conflicts"` → `resolve_flow_sync_conflicts(flow_id, resolutions: ConflictResolution[])` → `SyncResult`, whose `conflicts` are those that still stand. Invoked by `../ui/FSR-flow-sync-review.md` (FSR-FR-IDRR).
- `"read tracker item"` → `read_tracker_item(flow_id, kind, number)` → `TrackerItemSnapshot`. `kind` is `issue` or `milestone`. Invoked by `../ui/FLO-flow.md` (FLO-FR-TYEZ).
- `"open tracker link"` → `open_tracker_link(url)` → nothing. Invoked by `../ui/FLO-flow.md` (FLO-FR-ULEM) and `../ui/FSR-flow-sync-review.md`.
- `"get flow tracker settings"` → `get_flow_tracker_settings()` → `FlowTrackerSettings`, with defaults applied. Invoked by `../ui/SET-project-settings.md` (SET-FR-UFXD).
- `"set flow tracker settings (deliverable type, epic type, feature type, task type, defect type)"` → `set_flow_tracker_settings(deliverable_type, epic_type, feature_type, task_type, defect_type)` → `FlowTrackerSettings`. Invoked by `../ui/SET-project-settings.md` (SET-FR-PQWW).

```
SyncResult {
  created,          // tracker items created
  updated,          // tracker items updated
  added,            // Flow items added by a pull
  changed,          // Flow items changed by a pull
  conflicts: SyncConflict[],
  skipped: SkippedIssue[],
  stale: string[]   // conflict ids of a resolve that no longer matched (FTS-FR-ARHM)
}

SyncConflict {
  id,               // FTS-FR-VEEG
  kind,             // "field" | "invalid_on_tracker" | "removed_in_flow" | "removed_on_tracker"
  item_id,
  item_name,        // the Flow's name for the item, or the tracker title where it is gone
  field,            // "name" | "description" | "type" | "parent" | "milestone" | "due" | null
  base, flow, tracker,  // the values; null where a side holds none
  choices,          // the choices its kind offers (FTS-FR-PBQC, FTS-FR-SZGF,
                    //   FTS-FR-BMWI, FTS-FR-VXMN)
  url               // the tracker item's URL, where it exists
}

ConflictResolution {
  conflict_id,
  choice            // "keep_flow" | "take_tracker" | "unlink" | "restore" | "remove_item"
}

SkippedIssue {
  number, title,
  reason            // "no_kind_fits" | "issue_type_unavailable" | "foreign_repository"
}

TrackerItemSnapshot {
  ref,              // the reference shape of FGV-flow-graph-validation.md
  name, description, state,
  type, parent, milestone, blockedBy,   // issue alone
  due                                    // milestone alone
}

FlowTrackerSettings {
  deliverable_type, epic_type, feature_type, task_type, defect_type
}
```

Errors are typed: `no_project_open`, `flow_not_found`, `flow_invalid`, `sync_not_enabled`, `sync_in_progress`, `tracker_item_not_found`, `already_linked`, `tracker_url_refused`, `invalid_flow_tracker_settings`, `github_unreachable`, `write_failed` (carrying the created URLs, FTS-FR-AIDI), and every refusal code of `GHP-github-publication.md` GHP-FR-YDAN and of `GTS-github-token-storage.md` GTS-FR-13, passed through unchanged. Each carries displayable text that states the exact failure.

### Internal (Rust API, not registered as a Tauri command)
- `TrackerAdapter` — the interface of FTS-FR-ARMD. The GitHub adapter implements it with the GraphQL and REST calls of the publication repository's host.

## Non-functional requirements
- A sync of a Flow of several hundred linked items reads the tracker in batches, so the number of requests grows with the number of linked items and not with the number of fields.
- No token leaves the backend. No secret is written to the Flow, a log record, an error, or a result.
- A sync never deletes an issue or a milestone, never closes or reopens one, and never changes a field outside FTS-FR-PNMA.
- A sync that stops part way leaves both sides in a state the next sync completes without a conflict the author did not cause.
