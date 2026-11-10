# Decide change edits tool

**Spec code:** `DCE`

## Intent
The tool an agent uses when the author tells it to decide hunks of the open proposal of the OpenSpec change under discussion. It accepts, rejects, or discusses one or more hunks, so the author can decide a change's edits by voice or in a comment. It is `DPC-decide-prompt-changes-tool.md` for a change: the same arguments, the same authority rules, and the same refusals, with the change's proposal store (`../core/CEP-change-edit-proposals.md`) in place of a file's. Out of scope: creating, updating, or withdrawing a hunk, which `PCE-propose-change-edits-tool.md` does.

## Functional requirements
1. **DCE-FR-QIKC** The tool is a `rig` portable tool named `decide_change_edits` and conforms to `TLC-tool-conventions.md` in full. Its side effect is the one the contract surface declares and nothing else.
2. **DCE-FR-ILCP** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema of `DPC-decide-prompt-changes-tool.md` (per TLC-FR-05).
3. **DCE-FR-PQAP** The tool is constructed per turn and bound to the change of the turn's target, the turn's conversation, and the turn's agent participant. No argument names a stream, a change, a proposal, or a participant.
4. **DCE-FR-WYJP** The tool is attached to the turns whose target is a change and to no other turn (per `../ai/CVL-conversation-loop.md` CVL-FR-EZKH), and never together with `decide_draft_changes` or `decide_prompt_changes`.
5. **DCE-FR-XVLE** The arguments, the entry order and independence, the whole-call refusals, and the authority rules are those of `DPC-decide-prompt-changes-tool.md` DPC-FR-RGGP and DPC-FR-VJIG, with the change's `pending` proposal (CEP-FR-ZOIG) in place of the artifact's.
6. **DCE-FR-CXZG** An `accept` entry invokes the operation of "accept change edit hunk" (CEP-FR-OPGW), a `reject` entry the operation of "reject change edit hunk", and a `discuss` entry opens a hunk discussion and holds the hunk, on the terms of DPC-FR-WCBF and DPC-FR-QPNB.
7. **DCE-FR-JCHG** An entry refused with `stream_busy` (CEP-FR-ESQR) is a row refused with that code, retryable, and the other entries of the call still run.
8. **DCE-FR-JMQQ** The decision comment, the event, the close, and the commit of the proposal (CEP-FR-RWTQ, CEP-FR-FOJW) follow from each entry as they follow from the same decision by chip. A call does not end the turn.
9. **DCE-FR-RNHP** The tool never creates, updates, edits, or withdraws a hunk, and never invokes "decline change edit proposal".
10. **DCE-FR-GAHP** Each call emits under the `ai` and `backend` domains: an `INFO` record naming the tool, the stream id, the change name, and the count of each decision, or a `WARN` record naming the tool and the refusal.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md`. `../ai/CVL-conversation-loop.md` attaches it (CVL-FR-EZKH).

### The tool
`decide_change_edits` — a `rig` portable tool, constructed per turn (per TLC-FR-19).

### The description
> Decide hunks of the open proposal of the OpenSpec change under discussion, on the author's explicit instruction in the latest comment. Use this only when the author tells you which hunks to accept, reject, or discuss, and copy their words into `instruction`. Name a hunk by its id, or by its place counted from 1. Do not read the decision back and wait for a confirmation. This never creates or changes a hunk.

### Arguments and output
The arguments and the output of `DPC-decide-prompt-changes-tool.md`, with each row naming its hunk's change-relative path.

## Non-functional requirements
- Nothing in this tool reaches the network.
