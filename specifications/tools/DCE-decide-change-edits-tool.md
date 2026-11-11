# Decide change edits tool

**Spec code:** `DCE`

## Intent
The tool an agent uses when the author tells it to decide hunks of the open proposal of the OpenSpec change under discussion. It accepts, rejects, or discusses one or more hunks, and it confirms or declines a pending request to apply or rework the change, so the author can decide a change's edits and start its runs by voice or in a comment. It is `DPC-decide-prompt-changes-tool.md` for a change: the same arguments, the same authority rules, and the same refusals, with the change's proposal store (`../core/CEP-change-edit-proposals.md`) in place of a file's. Out of scope: creating, updating, or withdrawing a hunk, which `PCE-propose-change-edits-tool.md` does.

## Functional requirements
1. **DCE-FR-QIKC** The tool is a `rig` portable tool named `decide_change_edits` and conforms to `TLC-tool-conventions.md` in full. Its side effect is the one the contract surface declares and nothing else.
2. **DCE-FR-ILCP** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema of `DPC-decide-prompt-changes-tool.md` (per TLC-FR-05).
3. **DCE-FR-PQAP** The tool is constructed per turn and bound to the change of the turn's target, the turn's conversation, and the turn's agent participant. No argument names a stream, a change, a proposal, or a participant.
4. **DCE-FR-WYJP** The tool is attached to the turns whose target is a change and to no other turn (per `../ai/CVL-conversation-loop.md` CVL-FR-EZKH), and never together with `decide_draft_changes` or `decide_prompt_changes`.
5. **DCE-FR-XVLE** The arguments, the entry order and independence, the whole-call refusals, and the authority rules are those of `DPC-decide-prompt-changes-tool.md` DPC-FR-RGGP and DPC-FR-VJIG, with the change's `pending` proposal (CEP-FR-ZOIG) in place of the artifact's, and with the new-change entries of DCE-FR-UZDN.
6. **DCE-FR-CXZG** An `accept` entry invokes the operation of "accept change edit hunk" (CEP-FR-OPGW), a `reject` entry the operation of "reject change edit hunk", and a `discuss` entry opens a hunk discussion and holds the hunk, on the terms of DPC-FR-WCBF and DPC-FR-QPNB.
7. **DCE-FR-JCHG** An entry refused with `stream_busy` (CEP-FR-ESQR) or `plan_questions_pending` (CEP-FR-KGOL) is a row refused with that code, retryable, and the other entries of the call still run.
8. **DCE-FR-JMQQ** The decision comment, the event, the close, and the commit of the proposal (CEP-FR-RWTQ, CEP-FR-FOJW) follow from each entry as they follow from the same decision by chip. A call does not end the turn.
9. **DCE-FR-RNHP** The tool never creates, updates, edits, or withdraws a hunk, a new-change proposal, or an action request, never invokes "decline change edit proposal", and never undoes a confirmed split.
10. **DCE-FR-GAHP** Each call emits under the `ai` and `backend` domains: an `INFO` record naming the tool, the stream id, the change name, and the count of each decision, or a `WARN` record naming the tool and the refusal.
11. **DCE-FR-UZDN** An entry's `id` names a hunk of the open edit proposal, the pending new-change proposal of the change, the change's split by its `set_id` (per `../core/AGC-agent-conversations.md` AGC-FR-JNOS), or the pending change action request of the change by its request id (per AGC-FR-KRMV). An `id` that names none of them is refused with `unknown_hunk`.
11. **DCE-FR-RAWJ** An `accept` entry for the new-change proposal invokes "accept new change proposal" and a `reject` entry invokes "reject new change proposal" (per `../core/NCP-new-change-proposals.md` NCP-FR-UWOR, NCP-FR-TWDN), both with no feedback. A `stream_busy` refusal of the acceptance is a retryable row, on the terms of DCE-FR-JCHG.
11. **DCE-FR-WMOG** A `discuss` entry for the new-change proposal opens a new discussion of the change, whose opening comment is the fixed line of `DDC-decide-draft-changes-tool.md` with one `new_change_subject` attachment, on the terms of DDC-FR-YDUQ and DDC-FR-ZWKS. It decides nothing.
11. **DCE-FR-WRSI** A `confirm` entry with the split's `set_id` invokes "confirm change split" (NCP-FR-MQCS), and its row has the outcome `confirmed`. A refusal of the confirmation is refused with `decision_failed`, on the terms of DDC-FR-PEJA. A `confirm` entry with an id that names neither the split nor a pending action request is refused with `unknown_hunk`.
11. **DCE-FR-PHSU** A call while the change holds no open edit proposal, no pending new-change proposal, no split that holds an accepted proposal and is not confirmed, and no pending action request is the **no open proposal** refusal of the whole call. The `action` enum of the arguments is that of `DDC-decide-draft-changes-tool.md`, `accept`, `reject`, `discuss`, and `confirm`.
11. **DCE-FR-IZTA** A `confirm` entry with the id of the pending action request invokes "confirm change action request" (per `../core/CAR-change-action-requests.md` CAR-FR-MMPV), and its row has the outcome `started` with the id of the run. A `reject` entry with that id invokes "decline change action request" with no feedback, and its row has the outcome `declined`.
11. **DCE-FR-UVUT** A start that the confirmation refuses is a row refused with `decision_failed` that carries the refusal code. It is retryable for every code but `unknown_stream` and `change_not_found`. An `accept` or a `discuss` entry with the id of an action request is refused with `invalid_action`.
11. **DCE-FR-QUKG** A confirmation by this tool is a decision of the author under the authority rules of DCE-FR-XVLE. The author's comment authorizes it, a comment spoken in voice mode included, and a call with no authorizing author comment confirms nothing and starts no run.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md`. `../ai/CVL-conversation-loop.md` attaches it (CVL-FR-EZKH).

### The tool
`decide_change_edits` — a `rig` portable tool, constructed per turn (per TLC-FR-19).

### The description
> Decide hunks of the open proposal of the OpenSpec change under discussion, the proposed new change of its split, or the pending request to apply or rework the change, on the author's explicit instruction in the latest comment. Use this only when the author tells you which hunks or which new change to accept, reject, or discuss, to confirm the split, or to confirm or reject the request, and copy their words into `instruction`. Name a hunk by its id, or by its place counted from 1; name the new change, the split, and the request by the ids your input lists. Confirming a request starts its run. Do not read the decision back and wait for a confirmation. This never creates or changes a hunk, a proposed change, or a request.

### Arguments and output
The arguments and the output of `DPC-decide-prompt-changes-tool.md`, with each row naming its hunk's change-relative path.

## Non-functional requirements
- Nothing in this tool reaches the network.
