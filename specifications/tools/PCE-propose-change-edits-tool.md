# Propose change edits tool

**Spec code:** `PCE`

## Intent
The tool an agent in an OpenSpec change discussion uses to offer the author edits to the change. One call records one proposal of anchored hunks across the files of the change, or updates the open one, and the author decides each hunk in the text it changes (per `../core/CEP-change-edit-proposals.md`). An OpenSpec change must stay coherent across its proposal, its deltas, its design, and its tasks, so the tool takes hunks of several files in one proposal. It is `PPC-propose-prompt-changes-tool.md` for a change: the same hunk rules, the same updates, the same one act of proposal and comment. Out of scope: deciding a hunk, which `DCE-decide-change-edits-tool.md` does on the author's word; and any file outside the change.

## Functional requirements
1. **PCE-FR-QYDD** The tool is a `rig` portable tool named `propose_change_edits` and conforms to `TLC-tool-conventions.md` in full. Its side effect is the one the contract surface declares and nothing else (per TLC-FR-17).
2. **PCE-FR-MRWZ** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema of the contract surface with its parameter descriptions (per TLC-FR-05).
3. **PCE-FR-ABFJ** The tool is constructed per turn. Its constructor binds it to the change of the turn's target, the turn's conversation, and the turn's agent participant. No argument names a stream, a change, a conversation, or a participant.
4. **PCE-FR-FVPF** The tool is attached to the turns whose target is a change and to no other turn (per `../ai/CVL-conversation-loop.md` CVL-FR-EZKH). It is never attached together with `propose_draft_changes` or `propose_prompt_changes`.
5. **PCE-FR-OXMY** A call carries exactly one of `hunks` and `updates`. `hunks` records a new proposal through `record_change_proposal` (CEP-FR-QDJH), and `updates` changes the open one through `update_change_proposal` (CEP-FR-AXEV). A call with both or neither is the retryable `InvalidArgs` refusal.
6. **PCE-FR-PQUD** Each hunk names its `path` relative to the change folder. A hunk is found in its file by the rule of `PPC-propose-prompt-changes-tool.md` PPC-FR-RJCA. A hunk whose path names no file of the change is a new file (CEP-FR-HRCX).
7. **PCE-FR-FEAK** A refusal of the store is the tool refusal of the same code, with the kinds and retry flags of `PPC-propose-prompt-changes-tool.md`'s refusals, and `path_outside_change` as a retryable `InvalidArgs`.
8. **PCE-FR-XEYG** A successful call records the proposal or the update and appends the comment as one act, and its comment body is the `rationale` on the terms of `PPC-propose-prompt-changes-tool.md` PPC-FR-13 and PPC-FR-14.
9. **PCE-FR-OXSO** A successful recording ends the turn, as every proposal tool does (per `../ai/CVL-conversation-loop.md` CVL-FR-15). A refusal does not end the turn.
10. **PCE-FR-CUSY** A resolved conversation is the not retryable `PermissionDenied` refusal, and nothing is recorded.
11. **PCE-FR-IIDO** With no project open the tool gives the shared no-open-project refusal (per TLC-FR-13).
12. **PCE-FR-XBYT** Each call emits under the `ai` and `backend` domains: an `INFO` record naming the tool, the stream id, the change name, and the number of hunks or updates, or a `WARN` record naming the tool and the refusal. No record holds hunk text.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md`. `../ai/CVL-conversation-loop.md` attaches it (CVL-FR-EZKH).

### The tool
`propose_change_edits` — a `rig` portable tool, constructed per turn (per TLC-FR-19).

### The description
> Propose edits to the OpenSpec change under discussion. Use this when you can show the author how the change should read: give the hunks in the files they belong to, across the proposal, the deltas, the design, and the tasks, so that the change stays coherent. Each hunk names its path relative to the change folder, for example `specs/auth/spec.md`. A path that the change does not hold yet creates a new file. The author decides each hunk in the text; nothing is written until they accept. To change the open proposal, call again with `updates`. Your turn ends when the proposal is recorded.

### Arguments
```
ProposeChangeEditsArgs {
  hunks: [
    {
      path:       string,                       // required; change-relative
      kind:       "replace" | "del" | "add",    // optional; the fields decide
      before:     string?, after: string?, after_text: string?,
      lead:       string?, trail: string?       // as PPC's hunk fields
    }
  ]?,
  updates: [
    {
      action: "change" | "add" | "withdraw",
      id:     string?,                          // for "change" and "withdraw"
      path:   string?,                          // for "add": required
      kind, before, after, after_text, lead, trail
    }
  ]?,
  rationale: string                             // required; the comment the author reads
}
```

### Output
```
ProposeChangeEditsOutput { proposal_id: string, hunk_count: integer }
```

## Non-functional requirements
- The tool records and returns. It waits for no decision and holds no state between calls (per TLC-FR-15, TLC-FR-16).
- Nothing in this tool reaches the network.
