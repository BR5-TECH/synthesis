# Plan issue read tools

**Spec code:** `PIR`

## Intent
The three tools that an agent in a discussion uses to read the issues of the delivery plan: `list_plan_issues`, `search_plan_issues`, and `read_plan_issue`. They read the issues of the selected GitHub Project that are part of the plan, and no other issue of the repository. They reuse the project token, the selected Project, and the read path of `../core/GDP-github-delivery-plan.md`. They are the agent's eyes for gap analysis between a design document and the plan, and for a check of what the plan holds before the agent proposes an issue from any discussion. They write nothing. Out of scope: proposing a change, which is `PPT-propose-plan-changes-tool.md`; and reading the design document, which `RFT-read-file-tool.md` does.

## Functional requirements
1. **PIR-FR-NBCT** Each of the three is a `rig` portable tool that conforms to `TLC-tool-conventions.md` in full, and is declared read-only (per TLC-FR-17). A call creates, modifies, and deletes no file, mutates no store, and changes nothing on GitHub.
2. **PIR-FR-VMSF** `description()` returns the fixed text of the contract surface and `parameters()` returns the JSON Schema of the contract surface (per TLC-FR-05, TLC-FR-06). Neither varies by conversation, agent, or project.
3. **PIR-FR-HRXN** The tools are attached to the turns of every discussion while the delivery source is a GitHub Project with a `valid` configuration, and to no turn otherwise (per `../core/MPR-map-plan-proposals.md` MPR-FR-ZCAM, `../ai/CVL-conversation-loop.md` CVL-FR-RWQB). A call while the source is any other is the **plan unavailable** refusal. With no project open a call is the shared **no open project** refusal (per TLC-FR-13).
4. **PIR-FR-JEBI** The tools reach **plan issues** only: the issues of the selected Project that are of the polling repository and whose kind is mapped (per `../core/DSR-delivery-source.md` DSR-FR-MKIG, DSR-FR-TZDZ). A pull request, an excluded issue, an unmapped issue, and an issue of another repository are never listed, found, or read.
5. **PIR-FR-JPMU** `list_plan_issues` returns plan issues in Project order, filtered by the optional `state`, `type`, `milestone`, `status`, `label`, and `parent` arguments. It returns at most `limit` issues (default 50, at most 100) from `offset`, and the total that matches. A row holds the number, title, Type, state, close reason, milestone title, parent number, and Status. The read cache holds no labels, so a row holds none, and `read_plan_issue` gives the labels of one issue. The `label` filter checks at most 50 issues, in Project order, against GitHub, with one read request for each.
6. **PIR-FR-DFKG** A first page (`offset` 0) also returns the **catalog** of the plan: its milestones with number, title, and state, the Types the repository offers, the options of the Project Status field, and the names of the repository labels. If the labels cannot be read, the catalog holds `labels: null` and the call still succeeds.
7. **PIR-FR-APBD** `search_plan_issues` takes a `query` and returns the plan issues whose title or body matches it, best first, with the same row as PIR-FR-JPMU and one short snippet of the body. It returns at most `limit` issues (default 20, at most 50). A blank `query` is the **blank query** refusal.
8. **PIR-FR-GYHA** `read_plan_issue` takes an issue `number` and returns the whole issue: title, full body, Type, state, close reason, milestone, parent, sub-issues, labels, Status, blocked-by relations, `updated_at`, and URL. The body is cut at 20000 characters with a flag when it is cut. A number that is not a plan issue is the **not a plan issue** refusal.
9. **PIR-FR-NRLP** `list_plan_issues` and `search_plan_issues` answer from the read cache of the plan (per `../core/GDP-github-delivery-plan.md` GDP-FR-LKXA) and name its `read_at`. `read_plan_issue` reads the issue from GitHub, so the value it returns is current. A failed read falls back to the cache and says so.
10. **PIR-FR-ZVSC** The tools use the token and host of `../core/GDP-github-delivery-plan.md` (GDP-FR-ZGJB). They add no token flow, no second credential, and no MCP server. They never put a token in a result.
11. **PIR-FR-APMQ** The refusals are those of the contract surface. A refusal ends nothing: the agent corrects the call or answers in prose.
12. **PIR-FR-VNGK** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): one `DEBUG` record naming the tool and the count returned, and one `WARN` record on a refusal naming the tool and the reason. No record carries a title, a body, a query, or a token.

## Contract surface
Three tools, conforming to `TLC-tool-conventions.md` in full. They are attached by `../ai/CVL-conversation-loop.md` (CVL-FR-LCSN) and have no Tauri command, event, or frontend reach of their own.

### The tools
`list_plan_issues`, `search_plan_issues`, `read_plan_issue` — `rig` portable tools, constructed per turn (per TLC-FR-15). Each constructor takes the turn's project handle and nothing that names an issue or a repository.

### The descriptions
- `list_plan_issues` — *"List the issues of the delivery plan: the issues of the selected GitHub Project. Filter by state, Type, milestone, Status, label, or parent. The first page also gives the catalog: the milestones, the Types, the Status options, and the labels that exist. Use it to see what the plan holds before you propose a change."*
- `search_plan_issues` — *"Search the issues of the delivery plan by words in the title or the body. Use it to find out whether the plan already holds an issue for a part of a design."*
- `read_plan_issue` — *"Read one issue of the delivery plan in full by its number: body, labels, milestone, parent, sub-issues, Status, and relations. It reads no issue that is not part of the plan."*

### Arguments
```
ListPlanIssuesArgs   { state?: "open"|"closed", type?: string, milestone?: string, status?: string, label?: string, parent?: integer, limit?: integer, offset?: integer }
SearchPlanIssuesArgs { query: string, limit?: integer }
ReadPlanIssueArgs    { number: integer }
```

### Output
Data (per TLC-FR-08):
```
ListPlanIssuesOutput   { total, offset, readAt, stale, issues: PlanIssueRow[], catalog?: { milestones: { number, title, state }[], types: string[], statuses: string[], labels: string[] | null } }
SearchPlanIssuesOutput { total, readAt, stale, issues: (PlanIssueRow & { snippet })[] }
ReadPlanIssueOutput    { number, title, body, bodyCut, type, state, closeReason, milestone, parent, subIssues: number[], labels: string[], status, blockedBy: number[], updatedAt, url }
PlanIssueRow           { number, title, type, state, closeReason, milestone, parent, status }
```

### Refusals
- **no open project** — the shared refusal: kind `NotFound`, not retryable.
- **plan unavailable** — kind `NotFound`, not retryable: *"The project has no GitHub Project selected for its plan, or its configuration is not valid. Tell the author; asking again will not succeed."*
- **blank query** — kind `InvalidArgs`, retryable: *"Give words to search for in `query`."*
- **not a plan issue** — kind `NotFound`, retryable: *"Issue number N is not part of the delivery plan. List the plan issues to find the right number."*
- **could not read** — kind `Other`, retryable: *"The plan could not be read."*

### Side effects
Declared in full (per TLC-FR-17): none. The tools are read-only. `read_plan_issue` makes one read request to GitHub and `list_plan_issues` makes one read request for the labels of the first page, and neither changes anything.

## Non-functional requirements
- A list or a search of a plan of several hundred issues makes no request to GitHub, except the one label read of a first page and the reads of a `label` filter (PIR-FR-JPMU).
- The descriptions say that the tools read the plan only, so a model does not use them to look for other issues of the repository.
