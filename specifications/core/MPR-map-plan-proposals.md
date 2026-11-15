# Map plan proposals

**Spec code:** `MPR`

## Intent
The backend of the planning conversation of the Map tab. An agent in a Map discussion cannot write the delivery plan. It records a **plan proposal**: one batch of **rows**, each row one new issue, one change to a plan issue, or one new draft. The author decides each row on its own. Nothing reaches GitHub, and no draft is created, before the author accepts the row that does it. This module stores the proposals, keeps the state of each row, and applies an accepted row through the GitHub delivery plan path of `GDP-github-delivery-plan.md`. It also owns the Map discussions that the proposals belong to. Out of scope: the tools that record a proposal, which are `../tools/PPT-propose-plan-changes-tool.md` and `../tools/PIR-plan-issue-read-tools.md`; the card that the author decides on, which is `../ui/MPV-map-proposal-review.md`; and the storage of the discussion log, which is `CMS-comments-storage.md`.

## Functional requirements
1. **MPR-FR-TQVZ** A **plan proposal** belongs to one Map discussion and to one agent comment in it. It holds ordered rows. Each row has a `key` that is unique in the proposal, a kind (`create_issue`, `modify_issue`, or `new_draft`), a state, and the fields of its kind. Only the state, error, and result of a row change later.
2. **MPR-FR-QUYY** A row is in one of four states: `waiting`, `accepted`, `rejected`, or `failed`. A new row is `waiting`. A decision moves a `waiting` row to `accepted`, `rejected`, or `failed`, and a decided row never changes state again. A decision on a decided row is refused with `row_decided` and changes nothing.
3. **MPR-FR-RVRN** A proposal is stored in the Map owner's folder of the comments store (per `CMS-comments-storage.md`), beside the Map discussion log, and lives as long as that log lives. It is stored through `FSA-filesystem-access.md`. Recording a proposal and appending its comment are one step: a failure of either produces neither.
4. **MPR-FR-MQEQ** A decision applies to one row only. A rejected, failed, or accepted row leaves every other row of the proposal as it was. A failed row does not stop a later decision on another row.
5. **MPR-FR-YCHW** Accepting a `create_issue` row creates the issue through `create_item` of `GDP-github-delivery-plan.md` (GDP-FR-UDOL) with its title, body, Type, milestone, parent, and labels, and adds it to the selected Project. The row becomes `accepted` and holds the number and URL of the new issue.
6. **MPR-FR-FBSH** Accepting a `modify_issue` row applies each changed field through the edits of `GDP-github-delivery-plan.md`: title, body, Type, milestone, parent, labels, Status, and close. Each field is written in that order, with close last. The first failed write stops the row, and the row becomes `failed` with the error of that write. A write that happened stays on GitHub.
7. **MPR-FR-XPXP** Accepting a `new_draft` row creates one draft with the name and the prompt of the row, through the draft storage of `DRS-draft-storage.md`. It makes no request to GitHub. The row becomes `accepted` and holds the id of the new draft.
8. **MPR-FR-RTGH** A `modify_issue` row records the `updated_at` of the issue at the proposal. Accepting it passes that value to the edit (per `GDP-github-delivery-plan.md` GDP-FR-GBDF). Where the issue changed on GitHub since, the row becomes `failed` with `changed_on_tracker`, nothing is written, and no other row changes.
9. **MPR-FR-YLKL** A `create_issue` row can name another `create_issue` row of the same proposal as its parent, by `key`. Such a row is **blocked** while that parent row is `waiting`, `rejected`, or `failed`. Accepting a blocked row is refused with `parent_not_accepted` and a reason of `waiting`, `rejected`, or `failed`, and nothing changes.
10. **MPR-FR-EQAX** When the parent row is `accepted`, accepting the child creates the issue with the real number of the parent issue as its parent. A parent that is rejected or failed leaves the child blocked for good. The author can still reject the child.
11. **MPR-FR-NGSK** A label change of a row holds an `add` list and a `remove` list. Only labels that exist in the repository are allowed. The check runs at the proposal and again at the accept. A missing name fails with `label_unknown`. 
12. **MPR-FR-WBKZ** A `modify_issue` row can close the issue with the reason `completed` or `not_planned`. The reason is part of the row. Accepting the row closes the issue with that reason through the edit set of `GDP-github-delivery-plan.md`.
13. **MPR-FR-GOHN** A `modify_issue` row can set the Project Status of the issue to one option of the Status field of the selected Project. A name that is not an option of the field fails with `status_unknown`.
14. **MPR-FR-GKOL** A `modify_issue` row carries, for each changed field, the value before and the value after, read at the time of the proposal. For labels it carries the added and the removed names. A field whose new value equals its value before is not part of the row.
15. **MPR-FR-SBYA** A Map discussion has an **open row** while any row of any of its proposals is `waiting`. `record_map_plan_proposal` for a discussion that has an open row is refused with `proposal_open`, and records nothing. A row that is accepted, rejected, or failed is not an open row.
16. **MPR-FR-SKLA** Rejecting a row sets its state to `rejected`. It writes nothing to GitHub, creates no draft, and needs no network. A blocked row can be rejected.
17. **MPR-FR-VMXC** Accepting a `create_issue` or `modify_issue` row refuses with `not_github_source` where the delivery source is not a GitHub Project (per `DSR-delivery-source.md` DSR-FR-AECK), and with the configuration error of `GPP-github-polling.md` where the configuration is not `valid`. A `new_draft` row needs neither.
18. **MPR-FR-HKYH** At most one decision runs at a time for one open project. Decisions apply in the order they arrive, and a second decision on the same row waits and then finds it decided.
19. **MPR-FR-SGYA** After a row that wrote to GitHub is applied, the module updates the read cache of the plan (per `GDP-github-delivery-plan.md` GDP-FR-VNNU). Each change of a proposal emits `"map plan proposals changed"` with the discussion id.
20. **MPR-FR-NRNZ** The Map owner holds the **Map thread**, a discussion with no node reference, and any number of **node threads**, each a discussion with one node reference. A project has at most one Map thread, and at most one thread for each node reference. The storage of these discussions follows `CMS-comments-storage.md`.
21. **MPR-FR-CEKS** A **node reference** is `{ lens, kind, id, label }`. `lens` is `delivery` or `specifications`. `kind` is `item`, `milestone`, `index`, or `spec`. `id` is a plan item id (for example `issue-12`), an index node id, or a spec code. `label` is the name shown to the author.
22. **MPR-FR-MYLA** `find_map_discussion(map_node)` returns the thread of the node reference, or the Map thread where the reference is absent, or nothing where the thread does not exist. It creates nothing. A node reference matches on `lens`, `kind`, and `id`.
23. **MPR-FR-MAXH** `get_or_create_map_discussion(map_node, body, attachments)` returns the thread of the node reference. Where it exists, the call appends the comment to it. Where it does not exist, the call creates the thread and its first comment as one step. The call is a human write command for a Map thread (per `CMS-comments-storage.md` CMS-FR-VRZA).
24. **MPR-FR-PMLX** A comment can carry one `map_node` attachment: a node reference as the **focus** of the turn it triggers. The focus of a turn is the node reference of the latest human comment that carries one, else the node reference of the thread, else the whole map.
25. **MPR-FR-ZCAM** The plan tools of `../tools/PIR-plan-issue-read-tools.md` and `../tools/PPT-propose-plan-changes-tool.md` are attached to a turn of a Map discussion only while the delivery source is a GitHub Project with a `valid` configuration (per `GPP-github-polling.md` GPP-FR-NLPG), checked at the start of the turn. Other discussions never get them.
26. **MPR-FR-NXDX** The input of a Map turn holds the focus, a short summary of the plan (the milestones, the issue counts, and the unmapped Types), and each row of the proposals of the discussion with its state, so the agent knows what the author decided.
27. **MPR-FR-KZRN** Each recording, decision, and refusal logs its operation, its outcome, the row kind, and the typed error through `LGC-logging.md`. No record carries a title, a body, a label name, a draft name, or a token.
28. **MPR-FR-RVTT** Each operation refuses with `no_project_open` when no project is open, and logs the refusal.

## Contract surface
### Tauri commands
- `"find map discussion (map node)"` → `find_map_discussion(map_node?)` → `Discussion | null`. Invoked by `../ui/SMP-specification-map.md` (SMP-FR-WNLR).
- `"get or create map discussion (map node, body, attachments)"` → `get_or_create_map_discussion(map_node?, body, attachments, spoken_original?)` → `Discussion`.
- `"list map discussions"` → `list_map_discussions()` → `Discussion[]`, the Map thread first, then the node threads by `updated_at` descending.
- `"list map plan proposals (discussion id)"` → `list_map_plan_proposals(discussion_id)` → `MapPlanProposal[]`, oldest first.
- `"accept map plan row (discussion id, proposal id, row key)"` → `accept_map_plan_row(discussion_id, proposal_id, row_key)` → `MapPlanProposal`.
- `"reject map plan row (discussion id, proposal id, row key)"` → `reject_map_plan_row(discussion_id, proposal_id, row_key)` → `MapPlanProposal`.

### In-process
- `record_map_plan_proposal(discussion_id, rows)` → `MapPlanProposal`. Called by `../tools/PPT-propose-plan-changes-tool.md` only. It enforces MPR-FR-SBYA. The tool validates the rows.

### Events
- `"map plan proposals changed"` — payload `{ discussionId }` (MPR-FR-SGYA).

```
MapNodeRef { lens: "delivery" | "specifications", kind: "item" | "milestone" | "index" | "spec", id, label }

MapPlanProposal { id, discussionId, commentId, createdAt, rows: MapPlanRow[] }

MapPlanRow {
  key,
  kind,                 // "create_issue" | "modify_issue" | "new_draft"
  state,                // "waiting" | "accepted" | "rejected" | "failed"
  blockedBy,            // null | { rowKey, reason: "waiting" | "rejected" | "failed" }
  error,                // null | { code, message }
  result,               // null | { issueNumber?, issueUrl?, draftId?, appliedChanges? }
  fields                // CreateIssueFields | ModifyIssueFields | NewDraftFields
}

CreateIssueFields { title, body?, type?, milestone?: { number, title }, parent?: { issue: number } | { row: key }, labels: string[] }
ModifyIssueFields { number, title, baseUpdatedAt, changes: RowChange[] }
RowChange { field: "title" | "body" | "type" | "milestone" | "parent" | "labels" | "close" | "status", before, after, added?, removed?, reason? }
NewDraftFields { name, body }
```

Errors are typed: `no_project_open`, `proposal_not_found`, `row_not_found`, `row_decided`, `parent_not_accepted`, `proposal_open`, `label_unknown`, `status_unknown`, `changed_on_tracker`, `not_github_source`, `issue_type_unavailable`, `partial_edit`, `draft_create_failed`, `discussion_not_found`, `store_unavailable`, and the configuration and request errors of `GPP-github-polling.md` GPP-FR-SUFH.

## Non-functional requirements
- A decision on a `new_draft` row, and a reject, make no network request.
- A proposal of 20 rows is one file write.
- A proposal record holds the text of the rows. It is project content, and no log record copies it.
