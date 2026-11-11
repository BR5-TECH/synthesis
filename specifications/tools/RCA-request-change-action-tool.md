# Request change action tool

**Spec code:** `RCA`

## Intent
The tool an agent in an OpenSpec change discussion uses to ask the author to **apply** the change or to **rework** it. A run writes the stream and costs time and tokens, so the agent does not start it. The tool **records an action request** and posts it into the conversation as one comment of the agent with a card (per `../core/CAR-change-action-requests.md`). The author confirms or declines the request on the card, or tells an agent to decide it, and only a confirmation starts the run. Out of scope: the runs, which are `../core/OCR-openspec-change-runs.md`'s; the card, which is `../ui/OCW-openspec-change-workspace.md`'s; and the decision by tool, which is `DCE-decide-change-edits-tool.md`'s.

## Functional requirements
1. **RCA-FR-UZJK** The tool is a `rig` portable tool named `request_change_action` and conforms to `TLC-tool-conventions.md` in full. It is one of the tools of TLC-FR-17 that declare a side effect, and its side effect is the one the contract surface declares.
2. **RCA-FR-PTAK** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema of the contract surface with its parameter descriptions (per TLC-FR-05, TLC-FR-06).
3. **RCA-FR-QANG** The tool is constructed per turn. Its constructor binds it to the change of the turn's target, the turn's conversation, and the turn's agent participant. No argument names a stream, a change, a conversation, or a participant.
4. **RCA-FR-KZIJ** The tool is attached to every turn whose target is a change, with or without a fragment, and to no other turn (per `../ai/CVL-conversation-loop.md` CVL-FR-XHKB). An advisor turn is not attached it (per CVL-FR-HNGB).
5. **RCA-FR-BYYI** `action` is `apply` or `rework`, case ignored. Any other value is the **bad action** refusal.
6. **RCA-FR-KTJP** A `rework` call whose `instruction` is absent or blank once trimmed is the **no instruction** refusal. An `apply` call that carries an `instruction` that is not blank is the **apply takes no instruction** refusal.
7. **RCA-FR-XDQZ** A `rationale` that is blank once trimmed is the **blank rationale** refusal. A `rationale` with no spoken part (per `../ai/CVL-conversation-loop.md` CVL-FR-AGNN) is the **no spoken part** refusal.
8. **RCA-FR-CGCE** A call in a conversation whose owner target is not a change is the **not a change conversation** refusal: `PermissionDenied`, not retryable. Nothing is recorded.
9. **RCA-FR-YRTD** A conversation whose folded state is resolved (per `../core/CMS-comments-storage.md` CMS-FR-17) is the **conversation resolved** refusal: `PermissionDenied`, not retryable.
10. **RCA-FR-KKVG** A stream that is not live is the **stream gone** refusal, and a change folder that its working copy does not hold is the **change gone** refusal. Both are `NotFound`, not retryable.
11. **RCA-FR-FRLK** A change that holds a pending request (per `../core/CAR-change-action-requests.md` CAR-FR-MNTP) is the **request pending** refusal: `PermissionDenied`, not retryable. Nothing is recorded.
12. **RCA-FR-QEWH** A call that passes every check records one request through `record_change_action_request` (per CAR-FR-KZOZ), with the action in lowercase, the trimmed instruction of a rework, and the rationale. The comment body is the `rationale` less its citation markers (per `../ai/CVL-conversation-loop.md` CVL-FR-VSSU).
13. **RCA-FR-DJOP** A failure to record the request or to append its comment is the **could not record** refusal: `Other`, retryable. Nothing is recorded.
14. **RCA-FR-IKUP** A successful call returns `{ request_id }` and **ends the turn** (per `../ai/CVL-conversation-loop.md` CVL-FR-15). It starts no run, waits for no decision, and holds no timer. A refusal does not end the turn.
15. **RCA-FR-ABXT** A reply that asks for two calls of this tool dispatches the first. Each other call of the tool in that reply is not dispatched and gets the **one at a time** refusal. One turn records at most one request.
16. **RCA-FR-XXIP** With no project open a call is the shared **no open project** refusal (per TLC-FR-13).
17. **RCA-FR-ZWAF** Each call logs under the `ai` and `backend` domains (per TLC-FR-14): one `INFO` record when a request is recorded, with the tool, the stream id, the change name, and the action, and one `WARN` record when it refuses, with the tool and the reason. No record carries the instruction or the rationale.
18. **RCA-FR-DGRT** The tool reaches no network and writes no file but the request record and the comment. It commits nothing.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached by `../ai/CVL-conversation-loop.md` (CVL-FR-XHKB), and has no Tauri command, event, or frontend reach of its own.

### The tool
`request_change_action` — a `rig` portable tool, constructed per turn (per TLC-FR-15). Its constructor takes the change of the turn's target, the conversation the turn belongs to, and the agent participant the turn answers as.

### The description
The fixed text the model reads (per TLC-FR-05):

> Ask the author to start a run on the OpenSpec change under discussion. Use `apply` when the change is ready to build: an apply run implements its tasks and commits the work on the stream. Use `rework` when the change itself must be revised: a rework run revises the files of the change on your instruction, and its result comes back as a proposal that the author decides. You cannot start a run: this posts a request, and the author confirms or declines it. For `rework`, write in `instruction` what the revision must do; give no `instruction` for `apply`. Tell the author in `rationale` why you ask. Put one `<spoken>…</spoken>` tag in `rationale`: one short sentence that a voice can read aloud. Your turn ends when the request is posted. Make one request at a time.

### Arguments
```text
RequestChangeActionArgs {
  action:      string,       // required; "apply" | "rework"
  instruction: string?,      // rework: required; apply: absent
  rationale:   string        // required; the comment the author reads
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `action` — *"`apply` to implement the tasks of the change, or `rework` to revise the change itself."*
- `instruction` — *"For `rework` alone: what the revision must do, in the words a reviser can act on. Leave it out for `apply`."*
- `rationale` — *"Why you ask, in one or two sentences. It holds one `<spoken>…</spoken>` tag with one short sentence that a voice can read aloud."*

### Output
Data (per TLC-FR-08): `{ request_id }`. The turn ends on the call that recorded the request (per `../ai/CVL-conversation-loop.md` CVL-FR-15).

### The comment
The `rationale`, its spoken part hidden (per `../ui/CMT-comments.md` CMT-FR-BMCI). Beside it the comment carries exactly one `change_action_request` attachment (per `../core/CMS-comments-storage.md` CMS-FR-QVZP).

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **bad action** — kind `InvalidArgs`, retryable: *"`action` is `apply` or `rework`."*
- **no instruction** — kind `InvalidArgs`, retryable: *"A rework needs an instruction. Write in `instruction` what the revision must do."*
- **apply takes no instruction** — kind `InvalidArgs`, retryable: *"An apply run builds the change as it stands and takes no instruction. Leave `instruction` out, or ask for `rework` to change the change first."*
- **blank rationale** — kind `InvalidArgs`, retryable: *"Tell the author in `rationale` why you ask."*
- **no spoken part** — kind `InvalidArgs`, retryable: *"The rationale has no spoken part. Put one short sentence in one `<spoken>…</spoken>` tag in `rationale`."*
- **not a change conversation** — kind `PermissionDenied`, not retryable: *"This conversation is not about an OpenSpec change, so no run can be requested here."*
- **conversation resolved** — kind `PermissionDenied`, not retryable: *"This conversation is resolved and takes no further comments, so nothing was requested. Answer with what you already have; asking again will not succeed."*
- **stream gone** — kind `NotFound`, not retryable: *"The work stream of this change is gone, so no run can be requested."*
- **change gone** — kind `NotFound`, not retryable: *"The change is not on its work stream any more, so no run can be requested."*
- **request pending** — kind `PermissionDenied`, not retryable: *"A request for this change waits for the author. Answer with what you have; the author decides that request first."*
- **could not record** — kind `Other`, retryable: *"The request could not be recorded."*
- **one at a time** — the result of a call that is not dispatched (RCA-FR-ABXT): *"Only one request can be posted in a reply, so this one was not recorded."*

### Side effects
Declared in full (per TLC-FR-17). A successful call produces **exactly one change action request** and **exactly one comment**, through `../core/CAR-change-action-requests.md` CAR-FR-KZOZ. It starts no run, writes no file of the project, and changes no repository state.

## Non-functional requirements
- A call costs one record write and one comment append.
- Nothing in this tool requires network access.
