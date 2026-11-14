# Delivery source

**Spec code:** `DSR`

## Intent
The backend that decides where the project's one delivery plan lives, and that moves the plan from one home to the other. A project has one **delivery**: the plan of milestones, epics, features, tasks, and defects that the Map tab shows first (per `../ui/MDL-map-delivery-lens.md`). Exactly one system holds it at a time. This is the **delivery source**: either the GitHub Project selected for the project (per `GPP-github-polling.md`), which `GDP-github-delivery-plan.md` reads and writes, or one Flow file of the project (per `FGV-flow-graph-validation.md`). With one source, no change needs a merge, and the architect, the agents, and the team read the same plan. The module also holds the mapping between GitHub issue Types and plan kinds, and the two moves between sources: publish a Flow file to GitHub, and export the GitHub plan to a Flow file. Out of scope: each request to GitHub, which belongs to `GDP-github-delivery-plan.md`; the per-machine layout and overlay, which belong to `DMS-delivery-machine-state.md`; the state of an item, which belongs to `FDL-flow-delivery.md`; and every Flow file that is not the delivery, which stays an ordinary planning document in a Flow tab.

## Functional requirements
1. **DSR-FR-HOEW** The **delivery settings** hold the **source choice** and the **Type mapping** (DSR-FR-ZJMC). The source choice is `automatic`, `github`, or `file` with the artifact id of one `flow`-typed file. The settings are project-public, and they are read and written through `PSS-project-settings-storage.md` (PSS-FR-KZGG).
2. **DSR-FR-AECK** `get_delivery_source()` resolves the source. The choice `github` resolves to `github`. The choice `file` resolves to that Flow file. The choice `automatic`, and no stored choice, resolve to `github` where a GitHub Project is selected (per `GPP-github-polling.md` GPP-FR-NLPG), and to `none` otherwise.
3. **DSR-FR-EDHP** A `github` source carries the node ID, title, owner, number, and URL of the selected Project, the polling repository (per `GPP-github-polling.md` GPP-FR-RGNM), and the configuration state of GPP-FR-DEZO. The choice `github` with no selected Project resolves to `none` with the reason `github_project_unset`.
4. **DSR-FR-RHUM** A `file` source carries the Flow's artifact id and path. A stored artifact id that names no file, or a file whose resolved type is not `flow` (per `ASC-artifact-scanning.md` ASC-FR-06), resolves to `none` with the reason `delivery_file_missing` and the stored id.
5. **DSR-FR-ZKGG** A `none` source carries its reason: `no_source` for the choice `automatic` with no selected Project, `github_project_unset`, or `delivery_file_missing`.
6. **DSR-FR-NFFP** `set_delivery_settings(settings)` stores the settings and returns them. It refuses a `file` choice whose artifact is not a `flow`-typed file of the project with `not_a_flow` and writes nothing. A change of the source choice moves no plan data.
7. **DSR-FR-EGFF** Each stored change of the delivery settings, each completed publish (DSR-FR-WEVP), and each completed export (DSR-FR-JWYO) emits `"delivery source changed"` with the resolved source.
8. **DSR-FR-ZJMC** The **Type mapping** holds the **create Type** of each issue kind, which is `Epic`, `Feature`, `Task`, and `Bug` while unset; any number of **read entries**, each a Type name with a kind or `exclude`; and the **untyped choice**, which is a kind, `exclude`, or unset.
9. **DSR-FR-MKIG** An issue's kind is the kind whose create Type equals the issue's Type, else the kind of the read entry whose Type equals it. An issue with no Type takes the untyped choice. Names compare exactly. An issue is **unmapped** when its Type maps to nothing, or when it has no Type and the untyped choice is unset.
10. **DSR-FR-TZDZ** An issue whose Type maps to `exclude`, or that has no Type while the untyped choice is `exclude`, is not part of the plan, and it is not unmapped.
11. **DSR-FR-CPYM** `set_delivery_settings` refuses with `invalid_delivery_settings`, and writes nothing, where a Type name is empty, two kinds share one create Type, two read entries share one Type, or a read entry repeats a create Type. It never checks a Type against GitHub.
12. **DSR-FR-RCLA** `publish_delivery_to_github()` refuses with `not_file_source` where the resolved source is not `file`, and with the error of the GitHub configuration where it is not `valid` (per `GPP-github-polling.md` GPP-FR-DEZO). A refused publish changes nothing.
13. **DSR-FR-GXSE** A publish reads the Flow file and validates it (per `FGV-flow-graph-validation.md` FGV-FR-02). An invalid Flow is refused with `flow_invalid` and its report. A valid Flow is created on GitHub through the `create_item` edit of `GDP-github-delivery-plan.md` (GDP-FR-UDOL), containers before their members, and then each `blocks` edge through the `add_edge` edit.
14. **DSR-FR-PTIT** A publish writes each item's references, footprint, and draft link, the `relates` edges, the Flow's references, the milestone order, and the file's layout into the per-machine state of `DMS-delivery-machine-state.md`, under the ids of the created items. The Flow's name and description stay in the file alone.
15. **DSR-FR-ULON** A publish records each created tracker item in the publish journal (per `DMS-delivery-machine-state.md` DMS-FR-RVJQ) before its next request. A failed publish stops, keeps the journal, and returns the error with the URL of each item created. A later publish of the same Flow reuses each journal entry and creates only the rest.
16. **DSR-FR-WEVP** A publish that completes sets the source choice to `github`, removes the journal, and returns the counts of the milestones, issues, and relations it created. The Flow file stays on disk unchanged.
17. **DSR-FR-OIAS** `export_delivery_to_file(path)` refuses with `not_github_source` where the resolved source is not `github`. It reads the plan through `load_github_delivery_plan(true)` (per `GDP-github-delivery-plan.md` GDP-FR-QSHX) and refuses with the error of that read. It refuses a path that exists with `path_exists`.
18. **DSR-FR-RZUH** An export creates one Flow document of version `3` (per `FGV-flow-graph-validation.md` FGV-FR-07) at the path through `PST-project-storage.md`, with the Project's title as its name and each item, edge, and reference of the plan, its milestone items in milestone order. It writes no tracker reference, and it copies the plan's layout into the layout scope of the new file.
19. **DSR-FR-JWYO** An export that completes sets the source choice to `file` with the new artifact id, and returns that id and the path. It changes nothing on GitHub, and it keeps the overlay of the GitHub plan.
19. **DSR-FR-OJLB** `load_delivery_milestones(refresh)` resolves the source (DSR-FR-AECK). For `none` it answers the source with its reason and no milestone. For `github` it reads the plan through `load_github_delivery_plan(refresh)` (per `GDP-github-delivery-plan.md` GDP-FR-QSHX). For `file` it reads the Flow and its states through `FDL-flow-delivery.md` (FDL-FR-AZQM).
19. **DSR-FR-FQZD** The answer holds the plan's name and each milestone item in milestone order (per `../ui/FLO-flow.md` FLO-FR-MCFR), with its id, name, due date, state, leaf counts (per `FDL-flow-delivery.md` FDL-FR-IJXW), and count of blocked leaves under it. The first milestone whose state is neither `done` nor `dropped` is marked `current`.
19. **DSR-FR-BIYW** The milestone order starts from the document order of a Flow file, and from the order of `GDP-github-delivery-plan.md` GDP-FR-HNEW for a GitHub plan. The answer carries the `read_at` and `stale` of a GitHub plan, and neither for a Flow file.
19. **DSR-FR-SKPX** `load_delivery_milestones` refuses with the error of the read it makes: a configuration or request error of a GitHub plan with no cache, and `flow_invalid` with the report for a Flow file that fails validation. It writes nothing.
20. **DSR-FR-LYMF** Each operation of this module logs its start, its outcome, its counts, and every error, handled ones included, through `LGC-logging.md`. No record carries a token, a Type name, an item name, a description, or a URL query.
21. **DSR-FR-CFQA** Each operation refuses with `no_project_open` when no project is open, and logs the refusal.

## Contract surface
### Tauri commands
- `"get delivery settings"` → `get_delivery_settings()` → `DeliverySettings`, with defaults applied. Invoked by `../ui/SET-project-settings.md` (SET-FR-WDYJ).
- `"set delivery settings (settings)"` → `set_delivery_settings(settings)` → `DeliverySettings`. Invoked by `../ui/SET-project-settings.md` (SET-FR-XUKD) and `../ui/MDL-map-delivery-lens.md` (MDL-FR-XUJY).
- `"get delivery source"` → `get_delivery_source()` → `DeliverySource`. Invoked by `../ui/MDL-map-delivery-lens.md` (MDL-FR-YTAP), `../ui/SMP-specification-map.md` (SMP-FR-TKUS), `../ui/SET-project-settings.md` (SET-FR-WDYJ), and `../ui/LIB-library.md` (LIB-FR-JTXR).
- `"publish delivery to github"` → `publish_delivery_to_github()` → `PublishResult`. Invoked by `../ui/MDL-map-delivery-lens.md` (MDL-FR-JRGK).
- `"export delivery to file (path)"` → `export_delivery_to_file(path)` → `{ flow_id, path }`. Invoked by `../ui/MDL-map-delivery-lens.md` (MDL-FR-QIXG).
- `"load delivery milestones (refresh)"` → `load_delivery_milestones(refresh)` → `DeliveryMilestones`. Invoked by `../ui/DSH-dashboard.md` (DSH-FR-YAZR, DSH-FR-LGJT).

### Events
- `"delivery source changed"` — payload `DeliverySource` (DSR-FR-EGFF).

```
DeliveryKind = "milestone" | "epic" | "feature" | "task" | "defect"
IssueKind = "epic" | "feature" | "task" | "defect"

DeliverySettings {
  source,            // { kind: "automatic" } | { kind: "github" } | { kind: "file", flow_id }
  create_types,      // { epic, feature, task, defect } — Type names (DSR-FR-ZJMC)
  read_entries,      // { type, kind: IssueKind | "exclude" }[]
  untyped            // IssueKind | "exclude" | null
}

DeliverySource =
  | { kind: "github", project: { node_id, title, owner_login, number, url },
      repository: { host, owner, name }, configuration }   // GPP GithubPollingConfiguration
  | { kind: "file", flow_id, path }
  | { kind: "none", reason }   // "no_source" | "github_project_unset" | "delivery_file_missing"

DeliveryMilestones {
  source,            // DeliverySource
  plan_name,         // string | null — null for the source "none"
  milestones: {
    item_id, name,
    due,             // "YYYY-MM-DD" | null
    state,           // ItemDeliveryState.state (FDL-flow-delivery.md)
    leaf_counts,     // { not_started, drafting, in_progress, done, dropped }
    blocked_leaves,  // count of blocked leaves under the milestone
    current          // true on one milestone at most (DSR-FR-FQZD)
  }[],
  read_at,           // GitHub plan alone, else null
  stale              // GitHub plan alone, else false
}

PublishResult {
  milestones, issues, relations,   // counts created
  created_urls                     // every tracker item created by this call
}
```

Errors are typed: `no_project_open`, `not_a_flow`, `invalid_delivery_settings`, `not_file_source`, `not_github_source`, `flow_invalid` with the report of `FGV-flow-graph-validation.md`, `path_exists`, the GitHub configuration errors of `GPP-github-polling.md` GPP-FR-EHRC, and every error of `GDP-github-delivery-plan.md`. A failed publish carries `created_urls`.

This module reads and writes the settings through `PSS-project-settings-storage.md`, Flow files through `PST-project-storage.md`, GitHub through `GDP-github-delivery-plan.md`, and the per-machine state through `DMS-delivery-machine-state.md`.

## Non-functional requirements
- A source change is a change of one setting. It copies, converts, and deletes nothing, so a choice made by mistake is undone by choosing again.
- A publish of a plan of several hundred items is bounded by the GitHub requests it needs, and a failure in the middle of one costs the author a retry, not a second copy of the plan on GitHub.
