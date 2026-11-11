# Propose new change tool

**Spec code:** `PNC`

## Intent
The tool an agent uses when the OpenSpec change under discussion is too big and the author has decided to **split** it: to move parts of it into two or more new changes on the same work stream. Each call proposes **one new change**: its name, every file of its folder, its position N, and the total M of new changes that the agent expects at that time. The proposal is recorded beside the original change and posted into the conversation as one comment of the agent, and the author accepts, rejects, or discusses it on its card (per `../core/NCP-new-change-proposals.md`). It is `PND-propose-new-draft-tool.md` for a change: nothing is created until the author accepts, the agent opens the next proposal only after the author decided the open one, and only the author ends the split. Out of scope: deciding a proposal, which is the author's or `DCE-decide-change-edits-tool.md`'s on the author's instruction; trimming the original change, which `PCE-propose-change-edits-tool.md` does; and ending the split.

## Functional requirements
1. **PNC-FR-RTSB** The tool is a `rig` portable tool named `propose_new_change` and conforms to `TLC-tool-conventions.md` in full. Its side effect is the one the contract surface declares and nothing else.
2. **PNC-FR-SODJ** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema of the contract surface with its parameter descriptions (per TLC-FR-05).
3. **PNC-FR-IADA** The tool is constructed per turn. Its constructor binds it to the change of the turn's target, the turn's conversation, and the turn's agent participant. No argument names a stream, a change folder of the original, a conversation, or a participant.
4. **PNC-FR-EJAH** The tool is attached to the turns whose target is a change and to no other turn (per `../ai/CVL-conversation-loop.md` CVL-FR-PEFM). A call on any other turn is the **not a change conversation** refusal: `PermissionDenied`, not retryable.
5. **PNC-FR-BPCH** A call without `update` proposes one new change through `../core/NCP-new-change-proposals.md`'s recording path (NCP-FR-VTPG). It carries `name`, `files`, `position`, `total`, and `rationale`.
6. **PNC-FR-RLNW** Each entry of `files` names a `path` relative to the new change folder and its full `text`. The files must hold `proposal.md`. A path that fails CEP-FR-WJKY, two entries of one path, a blank text, and a missing `proposal.md` are each a retryable `InvalidArgs` refusal that names the entry.
7. **PNC-FR-BVYL** The refusals of the store (NCP-FR-RPGN) are the tool refusals of the same codes: `openspec_change_exists` and `invalid_change_name` as retryable `InvalidArgs`; `new_change_proposal_pending` as retryable `PermissionDenied`; `split_confirmed` and a resolved conversation as not retryable `PermissionDenied`.
8. **PNC-FR-LQHP** `position` and `total` follow the rules of `PND-propose-new-draft-tool.md` PND-FR-AFEX, and `rationale` the rules of PND-FR-DDUJ.
9. **PNC-FR-SWWQ** A call with `update` changes or withdraws the open new-change proposal through NCP-FR-SNMA, on the terms of PND-FR-APJP through PND-FR-VHYX, with `files` in the place of `body`. A `change` that carries `files` replaces every file of the proposal.
10. **PNC-FR-JPTJ** A successful call returns `{ proposed: true }` and **ends the turn** (per `../ai/CVL-conversation-loop.md` CVL-FR-15). A refused call ends nothing.
11. **PNC-FR-SUUN** The comment's body is the `rationale`, on the terms of PND-FR-LKHU, and its one attachment is `new_change_proposal` (per `../core/CMS-comments-storage.md` CMS-FR-QYGP).
12. **PNC-FR-VVSC** The tool never changes the original change, never writes into a working copy, never creates a change, and never ends a split. It holds no state between calls.
13. **PNC-FR-HBSW** With no project open the tool gives the shared no-open-project refusal (per TLC-FR-13).
14. **PNC-FR-VHLS** Each call emits under the `ai` and `backend` domains: an `INFO` record naming the tool, the stream id, the change, the proposed name, the number of files, the position, and the total, or a `WARN` record naming the tool and the refusal. No record holds file text or the rationale.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md`. `../ai/CVL-conversation-loop.md` attaches it (CVL-FR-PEFM).

### The tool
`propose_new_change` — a `rig` portable tool, constructed per turn.

### The description
> Propose one new OpenSpec change that takes over a part of the change under discussion, when the author has decided to split this change into two or more changes on the same work stream. Use it only after the author asked for a split and you understand how they want it split; ask them first where you are not sure. Each call proposes **one** change. Give its `name` in kebab-case, every file of its folder in `files` (at least `proposal.md`, and the `specs/`, `design.md`, and `tasks.md` files it needs), its `position` counted from 1, and the `total` number of new changes you expect now. Nothing is created until the author accepts. Remove the moved parts from this change with `propose_change_edits` in a later turn. To change or withdraw your open proposal, call again with `update`. Your turn ends when the proposal is recorded.

### Arguments
```
ProposeNewChangeArgs {
  rationale: string,           // required; why this change is the right next part
  name:      string?,          // the new change name; required without `update`
  files: [ { path: string, text: string } ]?,   // required without `update`
  position:  integer?,         // required without `update`
  total:     integer?,         // required without `update`
  update: { action: "change" | "withdraw" }?
}
```

### Output
`{ proposed: true }`.

## Non-functional requirements
- The description lets a model tell this tool from `propose_change_edits` on the first sentence: that one edits the change under discussion, and this one proposes a new change.
- Nothing in this tool reaches the network.
