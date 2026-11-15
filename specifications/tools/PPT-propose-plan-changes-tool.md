# Propose plan changes tool

**Spec code:** `PPT`

## Intent
The tool an agent in a Map discussion uses to propose work on the delivery plan. One call carries a batch of **rows**: a new issue, a change to a plan issue, or a new draft. The call records the batch as one plan proposal and posts one comment of the agent, which carries one review card (`../ui/MPV-map-proposal-review.md`). The tool only proposes. **Nothing reaches GitHub, and no draft is created, until the author accepts the row that does it** (`../core/MPR-map-plan-proposals.md`). The agent does not need to propose all work at once: it keeps the discussion focused and proposes in steps, and it proposes again only after the author decided every row of the card. Out of scope: reading the plan, which is `PIR-plan-issue-read-tools.md`; deciding a row, which is the author's on the card; and the application of an accepted row, which is `../core/MPR-map-plan-proposals.md`.

## Functional requirements
1. **PPT-FR-STDD** The tool is a `rig` portable tool named `propose_plan_changes` and conforms to `TLC-tool-conventions.md` in full. It is one of the tools of TLC-FR-17 that declare a side effect, and its side effect is the one the contract surface declares and nothing else.
2. **PPT-FR-FFML** `description()` returns the fixed text of the contract surface and `parameters()` returns its JSON Schema with the parameter descriptions (per TLC-FR-05, TLC-FR-06). Neither varies by conversation, agent, or project.
3. **PPT-FR-VFOX** The tool is constructed per turn (per TLC-FR-15). Its constructor binds it to the turn's Map discussion and the turn's agent participant. It accepts no argument that names a discussion, a repository, a conversation, or a path.
4. **PPT-FR-DDSX** The tool is attached to the turns of a Map discussion while the delivery source is a GitHub Project with a `valid` configuration, and to no other turn (per `../core/MPR-map-plan-proposals.md` MPR-FR-ZCAM). A call from another turn is the **not a Map discussion** refusal. With no project open the call is the shared **no open project** refusal (per TLC-FR-13).
5. **PPT-FR-PMQL** A call carries a `rationale` and 1 to 20 `rows`. Each row has a `key` that is unique in the call and an `op` of `create_issue`, `modify_issue`, or `new_draft`. A call with no row, with more than 20, or with a repeated or blank `key` is the **bad rows** refusal. One call records one proposal.
6. **PPT-FR-OVKT** A `create_issue` row carries a non-blank `title`, and optionally `body`, `type`, `milestone`, `parent_issue` or `parent_row`, and `labels`. A blank title is the **blank title** refusal. A `type` that the repository does not offer is the **unknown type** refusal. A `milestone` that names no milestone of the plan, by exact title, is the **unknown milestone** refusal.
7. **PPT-FR-KVFB** A `modify_issue` row carries `number` and at least one of `title`, `body`, `type`, `milestone`, `parent_issue`, `labels_add`, `labels_remove`, `close_reason`, and `status`. A number that is not a plan issue is the **not a plan issue** refusal. A row that changes nothing against the issue as it stands is the **no change proposed** refusal.
8. **PPT-FR-DPQP** `parent_row` is for `create_issue` rows only. It names the `key` of an earlier `create_issue` row of the same call. A key that names no earlier `create_issue` row is the **bad parent row** refusal. `parent_row` on a `modify_issue` row is also the **bad parent row** refusal. `parent_issue` names a plan issue, and `null` removes the parent of an existing issue. A row gives `parent_issue` or `parent_row`, never both.
9. **PPT-FR-XOMU** `labels` of a new issue, and `labels_add` and `labels_remove` of a change, name labels that already exist in the repository. A name that does not exist is the **unknown label** refusal that names it.  A label in both `labels_add` and `labels_remove` is the **bad rows** refusal.
10. **PPT-FR-JPFE** `close_reason` is `completed` or `not_planned`, and it closes the issue. Any other value is the **bad close reason** refusal. A close of an issue that is closed already is the **no change proposed** refusal.
11. **PPT-FR-YIFZ** `status` names one option of the Status field of the selected Project. A name that is not an option is the **unknown status** refusal.
12. **PPT-FR-QCTM** A `new_draft` row carries a non-blank `name` and a non-blank `body`, the full prompt of the draft. A blank one is the **blank name** or **blank body** refusal. The row creates no issue, and it needs no GitHub access when accepted.
13. **PPT-FR-AYQT** A call while a row of any proposal of the discussion is `waiting` is the **a card is open** refusal: `PermissionDenied`, retryable. Its message tells the agent to wait for the decisions of the author. A call is accepted again when every row is accepted, rejected, or failed.
14. **PPT-FR-NXPZ** A `rationale` that is blank once trimmed is the **blank rationale** refusal. A `rationale` with no spoken part (per `../ai/CVL-conversation-loop.md` CVL-FR-AGNN) is the **no spoken part** refusal.
15. **PPT-FR-BSSO** A conversation whose folded state is resolved (per `../core/CMS-comments-storage.md` CMS-FR-17) is the **conversation resolved** refusal: `PermissionDenied`, not retryable.
16. **PPT-FR-UZEK** A valid call records the proposal through `record_map_plan_proposal` of `../core/MPR-map-plan-proposals.md` and appends one comment. Each modify row records its `updated_at` and the value before of each changed field. A failure to record or to append produces neither, and is the **could not record** refusal.
17. **PPT-FR-CXFG** The comment's body is the `rationale` as the model wrote it, less its citation markers, with its spoken part in place. It carries no row. Its one `map_plan_proposal` attachment (per `../core/CMS-comments-storage.md`) names the proposal, and the card renders from it. The comment is appended with the turn's agent participant (per `../core/CMS-comments-storage.md` CMS-FR-65).
18. **PPT-FR-PRPX** A successful call returns the data object of the contract surface and **ends the turn** (per `../ai/CVL-conversation-loop.md` CVL-FR-15). A refused call ends nothing, and the agent corrects the call or answers in prose (per CVL-FR-14). A reply that asks for two calls dispatches the first only.
19. **PPT-FR-WBYJ** The tool never writes to GitHub, never creates a draft, never creates a label, and never changes the plan's read cache. It holds no state between calls (per TLC-FR-15).
20. **PPT-FR-DGAA** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): one `INFO` record at a recording, naming the tool, the discussion, and the count of rows of each kind; and one `WARN` record at a refusal, naming the tool and the reason. No record carries a title, a body, a label name, a draft name, or the rationale.
21. **PPT-FR-XMZF** The tool uses the token, host, and read path of `../core/GDP-github-delivery-plan.md` to check the Types, the milestones, the labels, and the Status options, and nothing else. It adds no token flow and no MCP server.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached by `../ai/CVL-conversation-loop.md` (CVL-FR-LCSN) and has no Tauri command, event, or frontend reach of its own.

### The tool
`propose_plan_changes` — a `rig` portable tool, constructed per turn (per TLC-FR-15). Its constructor takes the Map discussion the turn is about and the agent participant the turn answers as.

### The description
The fixed text the model reads (per TLC-FR-05):

> Propose work on the delivery plan: new issues, changes to plan issues, and new drafts. One call carries a batch of rows. Each row is `create_issue`, `modify_issue`, or `new_draft`. The author accepts or rejects each row on its own, and nothing is written to GitHub and no draft is created until they accept. A new issue can set title, body, Type, milestone, parent, and labels. A change can set title, body, Type, milestone, parent, labels to add and to remove, a close reason, and the Status. Use only labels, Types, milestones, and Status options that exist: read them with `list_plan_issues`. A parent can be a plan issue (`parent_issue`) or an earlier new issue of the same call (`parent_row`). Where a feature exists but is not fully built, propose a `new_draft` that finishes it, in place of an issue. Do not propose all work at once. Keep the discussion on one topic, propose one step, and propose again after the author decided every row. Say what you propose and why, in a sentence or two as you would put it to a colleague. Put one `<spoken>…</spoken>` tag in `rationale`, anywhere in it: one or two short sentences that a voice can read aloud. A rationale with no tag is refused. Your turn ends when the proposal is recorded.

### Arguments
```
ProposePlanChangesArgs {
  rationale: string,                // required
  rows: PlanRowArg[]                // required, 1 to 20
}

PlanRowArg {
  key: string,                      // required; unique in the call
  op: "create_issue" | "modify_issue" | "new_draft",   // required
  title?: string, body?: string, type?: string,
  milestone?: string | null,        // the exact title of a milestone of the plan; null removes it
  parent_issue?: integer | null,    // a plan issue number; null removes the parent
  parent_row?: string,              // the key of an earlier create_issue row
  labels?: string[],                // create_issue only
  number?: integer,                 // modify_issue only
  labels_add?: string[], labels_remove?: string[],
  close_reason?: "completed" | "not_planned",
  status?: string,
  name?: string                     // new_draft only; its prompt is `body`
}
```

Their descriptions in the parameter schema (per TLC-FR-06) name the rule of the field in one sentence, for example `labels_add` — *"Labels to add. Each must already exist in the repository."* and `close_reason` — *"Closes the issue with this reason."*

### Output
Data (per TLC-FR-08): `{ proposed: true, rows: integer }`. The turn ends on the call that proposed.

### The comment
The `rationale` as the model wrote it, its spoken part hidden (per `../ui/CMT-comments.md` CMT-FR-BMCI), and nothing else. Beside it the comment carries exactly one `map_plan_proposal` attachment.

### Refusals
- **no open project** — the shared refusal: `NotFound`, not retryable.
- **not a Map discussion** — `PermissionDenied`, not retryable: *"This conversation is not a Map discussion, so there is no plan to change. Say what you would change instead."*
- **plan unavailable** — `NotFound`, not retryable: *"The project has no valid GitHub Project for its plan. Tell the author; asking again will not succeed."*
- **conversation resolved** — `PermissionDenied`, not retryable: *"This conversation is resolved and takes no further comments, so nothing was proposed."*
- **a card is open** — `PermissionDenied`, retryable: *"A proposal of yours still waits for the decisions of the author. Wait for them, then propose again. Nothing was proposed."*
- **bad rows** — `InvalidArgs`, retryable: *"Give 1 to 20 rows, each with its own `key` and an `op`, and no label in both `labels_add` and `labels_remove`."*
- **blank title** / **blank name** / **blank body** — `InvalidArgs`, retryable: *"Give the row a title."* / *"Give the new draft a name."* / *"Give the full prompt of the new draft in `body`."*
- **not a plan issue** — `NotFound`, retryable: *"Issue number N is not part of the delivery plan."*
- **no change proposed** — `InvalidArgs`, retryable: *"Row K changes nothing in issue N as it stands."*
- **bad parent row** — `InvalidArgs`, retryable: *"`parent_row` of row K must name the `key` of an earlier `create_issue` row."*
- **unknown label** — `InvalidArgs`, retryable: *"The label X does not exist in the repository. This tool creates no label. Use an existing label."*
- **unknown type** / **unknown milestone** / **unknown status** — `InvalidArgs`, retryable: *"The Type X is not offered by the repository."* / *"No milestone of the plan has the title X."* / *"The Status X is not an option of the Project."*
- **bad close reason** — `InvalidArgs`, retryable: *"`close_reason` is `completed` or `not_planned`."*
- **blank rationale** — `InvalidArgs`, retryable: *"Say what you propose and why. The author reads this beside the card."*
- **no spoken part** — `InvalidArgs`, retryable: *"The rationale has no spoken part. Put one or two short sentences in one `<spoken>…</spoken>` tag in `rationale`."*
- **could not record** — `Other`, retryable: *"The proposal could not be recorded."*

### Side effects
Declared in full (per TLC-FR-17). A successful call produces **exactly one plan proposal** and **exactly one comment**, through `../core/MPR-map-plan-proposals.md`. It writes nothing to GitHub, creates no draft and no label, and touches no file of the project.

## Non-functional requirements
- The description is written so that a model can tell this tool from `PND-propose-new-draft-tool.md`: that one splits the draft under discussion, and this one proposes work on the delivery plan.
- A call makes three read requests to GitHub at most for the labels, the Types, and the Status options, and one read request for each `modify_issue` row that changes labels, because the read cache holds no labels (per `PIR-plan-issue-read-tools.md` PIR-FR-JPMU).
