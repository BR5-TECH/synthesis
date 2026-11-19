# Propose plan changes tool

**Spec code:** `PPT`

## Intent
The tool an agent in a Map discussion uses to propose work on the delivery plan. One call carries a batch of **rows**: a new issue, a change to a plan issue, or a new draft. The call records the batch as one plan proposal and posts one comment of the agent, which carries one review card (`../ui/MPV-map-proposal-review.md`). The tool only proposes. **Nothing reaches GitHub, and no draft is created, until the author accepts the row that does it** (`../core/MPR-map-plan-proposals.md`). The agent keeps the discussion on one topic and proposes one complete step in one call: every row that the step needs, up to 20. It proposes a new batch only after the author decided every row of the card. In one turn the agent can call the tool several times: the first successful call posts the card and its comment, and later calls update that card in place, with reads between them. While a row of its card waits, a later turn **updates** that card in place when the discussion asks for it. This is how the author steers the proposal. Out of scope: reading the plan, which is `PIR-plan-issue-read-tools.md`; deciding a row, which is the author's on the card; and the application of an accepted row, which is `../core/MPR-map-plan-proposals.md`.

## Functional requirements
1. **PPT-FR-STDD** The tool is a `rig` portable tool named `propose_plan_changes` and conforms to `TLC-tool-conventions.md` in full. It is one of the tools of TLC-FR-17 that declare a side effect, and its side effect is the one the contract surface declares and nothing else.
2. **PPT-FR-FFML** `description()` returns the fixed text of the contract surface and `parameters()` returns its JSON Schema with the parameter descriptions (per TLC-FR-05, TLC-FR-06). Neither varies by conversation, agent, or project.
3. **PPT-FR-VFOX** The tool is constructed per turn (per TLC-FR-15). Its constructor binds it to the turn's Map discussion and the turn's agent participant. It accepts no argument that names a discussion, a repository, a conversation, or a path.
4. **PPT-FR-DDSX** The tool is attached to the turns of a Map discussion while the delivery source is a GitHub Project with a `valid` configuration, and to no other turn (per `../core/MPR-map-plan-proposals.md` MPR-FR-ZCAM). A call from another turn is the **not a Map discussion** refusal. With no project open the call is the shared **no open project** refusal (per TLC-FR-13).
5. **PPT-FR-PMQL** A call carries a `rationale` and 1 to 20 `rows` or `updates`. Each row has a `key` that is unique in the call and an `op` of `create_issue`, `modify_issue`, or `new_draft`. A call with no row, with more than 20, or with a repeated or blank `key` is the **bad rows** refusal. One call records one proposal.
6. **PPT-FR-OVKT** A `create_issue` row carries a non-blank `title`, and optionally `body`, `type`, `milestone`, `parent_issue` or `parent_row`, and `labels`. A blank title is the **blank title** refusal. A `type` that the repository does not offer is the **unknown type** refusal. A `milestone` that names no milestone of the plan, by exact title, is the **unknown milestone** refusal.
7. **PPT-FR-KVFB** A `modify_issue` row carries `number` and at least one of `title`, `body`, `type`, `milestone`, `parent_issue`, `labels_add`, `labels_remove`, `close_reason`, and `status`. A number that is not a plan issue is the **not a plan issue** refusal. A row that changes nothing against the issue as it stands is the **no change proposed** refusal.
8. **PPT-FR-DPQP** `parent_row` names the `key` of an earlier `create_issue` row of the same call, as the parent of a new issue or of a changed issue. Another key is the **bad parent row** refusal. `parent_issue` names a plan issue, and `null` removes the parent of an existing issue. A row gives `parent_issue` or `parent_row`, never both.
9. **PPT-FR-XOMU** `labels` of a new issue, and `labels_add` and `labels_remove` of a change, name labels that already exist in the repository. A name that does not exist is the **unknown label** refusal that names it.  A label in both `labels_add` and `labels_remove` is the **bad rows** refusal.
10. **PPT-FR-JPFE** `close_reason` is `completed` or `not_planned`, and it closes the issue. Any other value is the **bad close reason** refusal. A close of an issue that is closed already is the **no change proposed** refusal.
11. **PPT-FR-YIFZ** `status` names one option of the Status field of the selected Project. A name that is not an option is the **unknown status** refusal.
12. **PPT-FR-QCTM** A `new_draft` row carries a non-blank `name` and a non-blank `body`, the full prompt of the draft. A blank one is the **blank name** or **blank body** refusal. The row creates no issue, and it needs no GitHub access when accepted.
13. **PPT-FR-AYQT** A call with `rows` while a row of any proposal of the discussion is `waiting` is the **a card is open** refusal: `PermissionDenied`, retryable. Its message tells the agent to send `updates` to change the open card, or to wait for the decisions of the author. A call with `rows` is accepted again when every row is accepted, rejected, or failed.
14. **PPT-FR-MYQS** A call carries `rows` or `updates`, never both. A call with both, or with neither, is the **not one form** refusal.
15. **PPT-FR-STLY** A call with `updates` changes the open proposal of the discussion through `update_map_plan_proposal` of `../core/MPR-map-plan-proposals.md` (MPR-FR-TMTI). Each update gives `action`: `change` with the `key` of a row and the full new row, `add` with a new row, or `withdraw` with the `key` of a row. Where no row is `waiting`, the call is the **no open proposal** refusal.
16. **PPT-FR-KOTV** The row of a `change` or an `add` follows every rule of a row of `rows` (PPT-FR-OVKT to PPT-FR-QCTM). A `modify_issue` row reads the issue again (PPT-FR-UZEK). A `parent_row` names a `create_issue` row that the proposal holds after the update. The proposal holds at most 20 rows after the update, else the call is the **bad rows** refusal.
17. **PPT-FR-DPRP** An update with no `action` or an unknown one, a `change` or `add` with no row, and a `withdraw` with row fields are each the **update malformed** refusal. A key that names no row is **no such row**. A key that two updates name is **row named twice**. An `add` with the key of a row is **bad rows**.
18. **PPT-FR-JDUV** A `change` or a `withdraw` of an `accepted` or a `failed` row is the **row already settled** refusal. A `change` with another `op` than the row is the **op changed** refusal. An update that leaves no row is the **no row left** refusal, and its message tells the agent that the author can reject the rows.
19. **PPT-FR-KOCG** A call with `updates` is all or nothing, and it checks every update before it refuses. A refusal about one update names its place in the list, for example *Update 2 of 4*. Several refused updates make one **rows refused** refusal that lists each message. Nothing is written. A refusal about the whole call names no place.
20. **PPT-FR-ISZR** The first successful call of a turn, when it carries `updates`, appends one comment, as PPT-FR-CXFG states. Its `map_plan_proposal` attachment names the open proposal and carries `update`: the number of rows that the call changed, added, and withdrew (per `../core/CMS-comments-storage.md` CMS-FR-BSQB).
21. **PPT-FR-NXPZ** Before the turn has a card (PPT-FR-ZMBA), a `rationale` that is absent or blank once trimmed is the **blank rationale** refusal, and a `rationale` with no spoken part (per `../ai/CVL-conversation-loop.md` CVL-FR-AGNN) is the **no spoken part** refusal. After the turn has a card, the tool does not check `rationale`.
21. **PPT-FR-ZMBA** The first successful call of a turn makes its proposal the **card of the turn**. A later call with `rows` in the same turn is the **one card a turn** refusal: `InvalidArgs`, retryable. A later call with `updates` changes the card of the turn. Where no row of that card is `waiting`, it is the **no open proposal** refusal.
21. **PPT-FR-IRFE** A later successful call of a turn appends no comment and posts no `rationale`. It changes the card of the turn and adds its counts of changed, added, and withdrawn rows to the counts of the turn. The turn reads these counts when it ends (per `../ai/CVL-conversation-loop.md` CVL-FR-KYBE).
22. **PPT-FR-BSSO** A conversation whose folded state is resolved (per `../core/CMS-comments-storage.md` CMS-FR-17) is the **conversation resolved** refusal: `PermissionDenied`, not retryable.
23. **PPT-FR-UZEK** A valid call with `rows` records the proposal through `record_map_plan_proposal` of `../core/MPR-map-plan-proposals.md` and appends one comment. Each modify row records the `updated_at` and the value before of each changed field, read from GitHub in the same call (per `../core/GDP-github-delivery-plan.md` GDP-FR-PBSD). A failure to record or to append produces neither, and is the **could not record** refusal.
24. **PPT-FR-CXFG** The comment of the first successful call of a turn has the `rationale` as its body, as the model wrote it, less its citation markers, with its spoken part in place. It carries no row. Its one `map_plan_proposal` attachment (per `../core/CMS-comments-storage.md`) names the proposal, and the card renders from it. The comment is appended with the turn's agent participant (per `../core/CMS-comments-storage.md` CMS-FR-65).
25. **PPT-FR-PRPX** A successful call returns the data object of the contract surface and does not end the turn. The agent can read, call again with `updates`, and end the turn with a reply (per `../ai/CVL-conversation-loop.md` CVL-FR-KYBE). A refused call ends nothing (per CVL-FR-14). A reply that asks for two calls dispatches both, in the order the model asked for them.
25. **PPT-FR-ELJI** A call with `rows` checks every row before it refuses. Where one row breaks a rule, the refusal is the refusal of that row. Where several rows break rules, the call is one **rows refused** refusal: `InvalidArgs`, retryable. It lists each refused row as `Row N: <message>`, in the order of the rows, and nothing is recorded.
25. **PPT-FR-MSQT** The rules of the call as a whole refuse before any row is checked: no row, more than 20 rows, a repeated or blank `key`, not one form (PPT-FR-MYQS), and the rules of PPT-FR-DDSX, PPT-FR-AYQT, PPT-FR-ZMBA, and PPT-FR-BSSO.
26. **PPT-FR-WBYJ** The tool never writes to GitHub, never creates a draft, and never creates a label. It changes the plan's read cache only through the reads of `../core/GDP-github-delivery-plan.md` GDP-FR-PBSD. It holds no state between calls (per TLC-FR-15), apart from the plan card of its turn (PPT-FR-ZMBA), which the turn owns.
27. **PPT-FR-DGAA** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record at a recording or an update, with the tool, the discussion, and the count of rows of each kind and of updates of each action; a `WARN` record at a refusal, with the tool and the reason, and for a **rows refused** refusal the count of refused rows. No record carries a title, a body, a label name, a draft name, or the rationale.
28. **PPT-FR-XMZF** The tool uses the token, host, and read path of `../core/GDP-github-delivery-plan.md` to check the Types, the milestones, the labels, and the Status options, and nothing else. It adds no token flow and no MCP server.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached by `../ai/CVL-conversation-loop.md` (CVL-FR-LCSN) and has no Tauri command, event, or frontend reach of its own.

### The tool
`propose_plan_changes` — a `rig` portable tool, constructed per turn (per TLC-FR-15). Its constructor takes the Map discussion the turn is about and the agent participant the turn answers as.

### The description
The fixed text the model reads (per TLC-FR-05):

> Propose work on the delivery plan: new issues, changes to plan issues, and new drafts. One call carries a batch of rows. Each row is `create_issue`, `modify_issue`, or `new_draft`. The author accepts or rejects each row on its own, and nothing is written to GitHub and no draft is created until they accept. A new issue can set title, body, Type, milestone, parent, and labels. A change can set title, body, Type, milestone, parent, labels to add and to remove, a close reason, and the Status. Use only labels, Types, milestones, and Status options that exist: read them with `list_plan_issues`. A parent can be a plan issue (`parent_issue`), or the new issue of an earlier `create_issue` row of the same call (`parent_row`), for a new issue and for a change. Where a feature exists but is not fully built, propose a `new_draft` that finishes it, in place of an issue. Keep the discussion on one topic. Propose one complete step in one call: every row that the step needs, up to 20. A step of 5 to 10 related rows is normal. Do not split one step into several calls of a few rows, and do not propose the work of later steps before the author decided this one. While a row of your proposal still waits and the discussion asks for a different change, send `updates` in place of `rows`. Each update gives `action`: `change` with the `key` of a row and the full new row, `add` with a new row and a new `key`, or `withdraw` with the `key` of a row you no longer offer. A rejected row can be changed, and the author decides it again. An accepted or failed row can not be updated. Say what you propose and why, in a sentence or two as you would put it to a colleague. Put one `<spoken>…</spoken>` tag in `rationale`, anywhere in it: one or two short sentences that a voice can read aloud. A rationale with no tag is refused. Your turn continues after a successful call: you can read issues and send `updates` to the same proposal. Only the first successful call of a turn posts its rationale. End the turn with a short reply. It is posted only where you changed the proposal after its rationale.

### Arguments
```
ProposePlanChangesArgs {
  rationale?: string,               // required before the turn has a card (PPT-FR-NXPZ)
  rows?: PlanRowArg[],              // a new proposal, 1 to 20; exactly one of rows, updates
  updates?: PlanUpdateArg[]         // the updates of the open proposal, 1 to 20, in the order they apply
}

PlanUpdateArg {
  action: "change" | "add" | "withdraw",   // required
  key: string,                      // required; the row to change or withdraw, or the key of the new row
  row?: PlanRowArg                  // change and add only; its `key` is the key of the update
}

PlanRowArg {
  key: string,                      // required; unique in the call
  op: "create_issue" | "modify_issue" | "new_draft",   // required
  title?: string, body?: string, type?: string,
  milestone?: string | null,        // the exact title of a milestone of the plan; null removes it
  parent_issue?: integer | null,    // a plan issue number; null removes the parent
  parent_row?: string,              // the key of an earlier create_issue row, for create_issue and modify_issue
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
Data (per TLC-FR-08): `{ proposed: true, rows: integer }` for a call with `rows`, and `{ updated: true, changed: integer, added: integer, withdrawn: integer }` for a call with `updates`. The turn continues after the call.

### The comment
The comment of the first successful call of a turn: the `rationale` as the model wrote it, its spoken part hidden (per `../ui/CMT-comments.md` CMT-FR-BMCI), and nothing else. Beside it the comment carries exactly one `map_plan_proposal` attachment.

### Refusals
- **no open project** — the shared refusal: `NotFound`, not retryable.
- **not a Map discussion** — `PermissionDenied`, not retryable: *"This conversation is not a Map discussion, so there is no plan to change. Say what you would change instead."*
- **plan unavailable** — `NotFound`, not retryable: *"The project has no valid GitHub Project for its plan. Tell the author; asking again will not succeed."*
- **conversation resolved** — `PermissionDenied`, not retryable: *"This conversation is resolved and takes no further comments, so nothing was proposed."*
- **a card is open** — `PermissionDenied`, retryable: *"A proposal of yours still waits for the decisions of the author. To change it, send `updates` in place of `rows`. Otherwise wait for the decisions, then propose again. Nothing was proposed."*
- **not one form** — `InvalidArgs`, retryable: *"Pass `rows` for a new proposal or `updates` for your open proposal, not both."*
- **one card a turn** — `InvalidArgs`, retryable: *"This turn already posted a plan proposal. Change it with `updates`, or end your turn with a short reply. Nothing was proposed."*
- **no open proposal** — `InvalidArgs`, retryable: *"No row of your proposals waits for the author, so there is nothing to update. Send a new proposal in `rows`."*
- **update malformed** — `InvalidArgs`, retryable: *"Update N of M: give `action` and `key`, and a `row` for `change` and `add`, and no `row` for `withdraw`."*
- **no such row** — `InvalidArgs`, retryable: *"Update N of M: the key K names no row of your open proposal. Name one of its rows, or use `add`."*
- **row named twice** — `InvalidArgs`, retryable: *"Two of your updates name the row K. Put what you want for it in one update."*
- **row already settled** — `InvalidArgs`, retryable: *"Update N of M: the row K is accepted or failed, so it can not be updated. Use `add` for a new row."*
- **op changed** — `InvalidArgs`, retryable: *"Update N of M: a `change` keeps the `op` of the row K. Withdraw it and add a new row instead."*
- **no row left** — `InvalidArgs`, retryable: *"This update withdraws every row. Keep at least one row; the author can reject the rows they do not want."*
- **rows refused** — `InvalidArgs`, retryable: *"N rows were refused. Correct each one and send the call again. Nothing was proposed."*, followed by one line `Row N: <message>` (or `Update N of M: <message>`) for each refused row, in order.
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
Declared in full (per TLC-FR-17). A successful call with `rows` produces **exactly one plan proposal** and **exactly one comment**, through `../core/MPR-map-plan-proposals.md`. A successful call with `updates` produces **no new proposal** and changes the open proposal through the same module. It appends **exactly one comment** where it is the first successful call of its turn, and **no comment** otherwise. A refused call writes nothing and appends no comment. It writes nothing to GitHub, creates no draft and no label, and touches no file of the project.

## Non-functional requirements
- The description is written so that a model can tell this tool from `PND-propose-new-draft-tool.md`: that one splits the draft under discussion, and this one proposes work on the delivery plan.
- A call makes three read requests to GitHub at most for the labels, the Types, and the Status options, and two read requests for each distinct issue of a `modify_issue` row: its full detail, and the issue as the read cache stores it (per `../core/GDP-github-delivery-plan.md` GDP-FR-PBSD).
