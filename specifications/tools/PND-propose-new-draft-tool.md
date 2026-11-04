# Propose new draft tool

**Spec code:** `PND`

## Intent
The tool an agent uses when the draft under discussion is too big and the author has decided to **decompose** it: to replace it with two or more new drafts that do the same job one after another. Each call **proposes one new draft**: its name, its full prompt, its position N, and the total M of new drafts that the agent expects at that time. The proposal is recorded beside the original draft and posted into the conversation as one comment of the agent, and the author accepts, rejects, or discusses it on the card that the comment carries (`../ui/DCR-draft-change-review.md`). **Nothing is created until the author accepts**, and the agent opens the next proposal only after the author decided the open one, so each new draft can use the author's decisions and comments on the drafts before it. While a proposal is open, the agent changes or withdraws it with an update, on the update rules of `PDC-propose-draft-changes-tool.md` (PDC-FR-DTNJ). Only the author ends the decomposition (`../core/DCP-draft-change-proposals.md` DCP-FR-VLLP); this tool cannot end it. Out of scope: deciding a proposal, which is the author's on the card or the author's instruction to `DDC-decide-draft-changes-tool.md`; creating the draft, which `../core/DCP-draft-change-proposals.md` does on acceptance; and changing the original draft's prompt, which this tool never does.

## Functional requirements
1. **PND-FR-UXPK** The tool is a `rig` portable tool named `propose_new_draft` and conforms to `TLC-tool-conventions.md` in full. It is one of the tools of TLC-FR-17 that declare a side effect, and its side effect is the one the contract surface declares and nothing else.
2. **PND-FR-XJHL** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema of the contract surface with its parameter descriptions (per TLC-FR-05, TLC-FR-06). Neither varies by draft, conversation, agent, or project.
3. **PND-FR-JNGE** The tool is constructed per turn (per TLC-FR-15). Its constructor binds it to the turn's draft, the origin of the turn's conversation, and the turn's agent participant. It accepts no argument that names a draft, a folder, a conversation, a participant, or a path.
4. **PND-FR-YFES** The tool is attached to the turns whose origin kind is a draft conversation and to no other turn (per `../ai/CVL-conversation-loop.md` CVL-FR-08). A call on any other turn is the **not a draft conversation** refusal: `PermissionDenied`, not retryable. With no project open the call is the shared **no open project** refusal (per TLC-FR-13).
5. **PND-FR-INGY** A call without `update` proposes one new draft. It carries `name`, `body`, `position`, `total`, and `rationale`, and records them through `../core/DCP-draft-change-proposals.md`'s recording path (DCP-FR-TGRN). The tool writes no file and appends no comment itself.
6. **PND-FR-AUFP** A `name` that is blank once trimmed is the **blank name** refusal, and a `body` that is blank once trimmed is the **blank body** refusal. A name is not required to be unique (per `../core/DRS-draft-storage.md` DRS-FR-09).
7. **PND-FR-AFEX** `position` and `total` are whole numbers. A `position` less than 1 and a `total` less than `position` are each the **bad position** refusal. Nothing else about them is checked: an earlier proposal of the same position does not refuse a call.
   - *Why:* After a rejection the agent can propose a replacement at the same position, so a rejected position does not move the numbers of the other drafts.
8. **PND-FR-DDUJ** A `rationale` that is blank once trimmed is the **blank rationale** refusal. A `rationale` with no spoken part (per `../ai/CVL-conversation-loop.md` CVL-FR-AGNN) is the **no spoken part** refusal. Both rules hold for a call with `update` too.
9. **PND-FR-GPNN** A call on a draft that is `archived`, `graduated`, held by a non-terminal graduation run (per `../core/DRS-draft-storage.md` DRS-FR-19), a GitHub-shadow draft, or a draft whose decomposition is confirmed is the refusal named for that case in the contract surface. Each is `PermissionDenied`, not retryable, and nothing is recorded.
10. **PND-FR-BGJI** A call without `update` while a new-draft proposal of the draft is open (per `../core/DCP-draft-change-proposals.md` DCP-FR-OBAW) is the **a proposal is open** refusal: `PermissionDenied`, retryable. Its message tells the agent to wait for the author's decision or to change the open proposal with `update`.
11. **PND-FR-XGNE** An open draft change proposal of the same draft (per `../core/DCP-draft-change-proposals.md` DCP-FR-04) refuses no call of this tool, and an open new-draft proposal refuses no call of `PDC-propose-draft-changes-tool.md` (PDC-FR-WEWV).
12. **PND-FR-BVNO** A conversation whose folded state is resolved (per `../core/CMS-comments-storage.md` CMS-FR-17) is the **conversation resolved** refusal: `PermissionDenied`, not retryable. Neither the proposal nor the comment is produced.
13. **PND-FR-APJP** A call with `update` changes or withdraws the **open** new-draft proposal of the draft through `../core/DCP-draft-change-proposals.md`'s update path (DCP-FR-GDTC). It follows the update rules of `PDC-propose-draft-changes-tool.md` (PDC-FR-DTNJ).
14. **PND-FR-HVWR** A `change` update carries one or more of `name`, `body`, `position`, and `total`, and each field it carries replaces that field of the open proposal. The fields it does not carry stay as they are. The checks of PND-FR-AUFP and PND-FR-AFEX apply to the fields that result.
15. **PND-FR-WQJQ** A `withdraw` update carries no other field. The open proposal becomes `withdrawn`, and no draft is created from it.
16. **PND-FR-OHTJ** A call with `update` while the draft holds no open new-draft proposal is the **no open proposal** refusal: `InvalidArgs`, retryable. Its message tells the agent to propose a new draft without `update`.
17. **PND-FR-FMRO** An `update` with no `action` or an unknown `action`, a `change` that carries none of the four fields, and a `withdraw` that carries one of them are each the **update malformed** refusal. A `change` whose resulting fields equal the open proposal's fields is the **no change proposed** refusal.
18. **PND-FR-VHYX** A call with `update` is all or nothing. A refused call writes no file and appends no comment. A successful call appends exactly one comment (per `../core/DCP-draft-change-proposals.md` DCP-FR-GDTC).
19. **PND-FR-KGRN** A successful call returns the data object of the contract surface and **ends the turn** (per `../ai/CVL-conversation-loop.md` CVL-FR-15). A refused call ends nothing: its refusal reaches the loop as a result, and the agent corrects the call or answers in prose (per CVL-FR-14).
20. **PND-FR-YPCH** A reply that asks for two calls of this tool dispatches the first and leaves the rest undispatched, on the terms of `../ai/CVL-conversation-loop.md` CVL-FR-15. One turn proposes at most one new draft.
21. **PND-FR-LKHU** The comment's body is the `rationale` as the model wrote it, less its citation markers (per `../ai/CVL-conversation-loop.md` CVL-FR-VSSU) and with its spoken part in place. It carries no name, no body, no position, and no identifier. Its one `new_draft_proposal` attachment (per `../core/CMS-comments-storage.md` CMS-FR-YKGI) is what the card renders from.
22. **PND-FR-HXUB** The comment is appended with the turn's agent participant as the registry holds it at the moment of the append (per `../core/CMS-comments-storage.md` CMS-FR-65), the same participant shape a delivered answer is appended with (per `../core/AGC-agent-conversations.md` AGC-FR-16).
23. **PND-FR-SQTD** A failure to record the proposal or to append its comment produces neither (per `../core/DCP-draft-change-proposals.md` DCP-FR-TGRN) and is the **could not record** refusal: `Other`, retryable.
24. **PND-FR-UQCD** The tool never changes the original draft's prompt, its record, or its status, never creates a draft, and never ends a decomposition. It holds no state between calls (per TLC-FR-15).
25. **PND-FR-HGCC** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): one `INFO` record when a proposal is recorded or updated, naming the tool, the draft, the position, the total, and an update's action; and one `WARN` record when it refuses, naming the tool, the draft, and the reason. No record carries the name, the body, or the rationale.
26. **PND-FR-RXRE** The tool reaches no network and touches no credential but the GitHub identity the comment's append resolves (per TLC-FR-18). It commits nothing.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach of its own.

### The tool
`propose_new_draft` — a `rig` portable tool, constructed per turn (per TLC-FR-15). Its constructor takes the draft the turn is about, the origin of the conversation the turn belongs to, and the agent participant the turn answers as.

### The description
The fixed text the model reads (per TLC-FR-05):

> Propose one new draft that replaces a part of the draft under discussion, when the author has decided to decompose this draft into two or more new drafts that do the same job one after another. Use it only after the author asked for a decomposition and you understand how they want it split; ask them first where you are not sure. Each call proposes **one** draft. Give its `name`, its full prompt in `body`, its `position` counted from 1, and the `total` number of new drafts you expect now. The author accepts, rejects, or discusses it, and nothing is created until they accept. Propose the next draft only after the author decided the one before it, and use what they said about it. You can propose a draft again at the same position after a rejection. To change the draft you proposed while it is still open, pass `update` with `action` `change` and the fields to replace. To take it back, pass `update` with `action` `withdraw`. You can not end the decomposition on your own: the author confirms it on the card, or tells you to confirm it, and then you use `decide_draft_changes`. Say why this draft is the right next part, in a sentence or two as you would put it to a colleague. Put one `<spoken>…</spoken>` tag in `rationale`, anywhere in it: one or two short sentences that a voice can read aloud. A rationale with no tag is refused. Your turn ends when the proposal is recorded.

### Arguments
```
ProposeNewDraftArgs {
  rationale: string,     // required; why this draft is the right next part
  name:      string?,    // the new draft's name; required without `update`
  body:      string?,    // the new draft's full prompt, Markdown; required without `update`
  position:  integer?,   // N, counted from 1; required without `update`
  total:     integer?,   // M, the number of new drafts expected now; required without `update`
  update: {              // present: this call updates the open proposal
    action: "change" | "withdraw"   // required
  }?
}
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `rationale` — *"Why this draft is the right next part, in a sentence or two as you would put it to a colleague. The author reads this as your comment. It holds one `<spoken>…</spoken>` tag with one or two short sentences that a voice can read aloud."*
- `name` — *"The name of the new draft. With `update` and `change`, give it only to replace the name."*
- `body` — *"The full prompt of the new draft, in Markdown, as the author will work on it. With `update` and `change`, give it only to replace the whole prompt."*
- `position` — *"The place of this draft in the sequence, counted from 1."*
- `total` — *"The number of new drafts you expect the decomposition to have now. It can change in a later call."*
- `update` — *"Changes your proposal that is still open. `change` replaces the fields you give. `withdraw` takes the proposal back and gives no other field."*

### Output
Data (per TLC-FR-08): `{ proposed: true }`. The turn ends on the call that proposed (per `../ai/CVL-conversation-loop.md` CVL-FR-15).

### The comment
The `rationale` as the model wrote it, its spoken part hidden (per `../ui/CMT-comments.md` CMT-FR-BMCI), and nothing else. Beside it the comment carries exactly one `new_draft_proposal` attachment (per `../core/CMS-comments-storage.md` CMS-FR-YKGI). A call with `update` posts a comment of the same shape, whose attachment also carries the update's action.

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **not a draft conversation** — kind `PermissionDenied`, not retryable: *"This conversation is not about a draft, so there is no draft here to decompose. Say how you would split it instead; asking again will not succeed."*
- **draft archived** — kind `PermissionDenied`, not retryable: *"This draft is archived, so it can not be decomposed. Tell the author; asking again will not succeed."*
- **draft graduated** — kind `PermissionDenied`, not retryable: *"This draft is graduated, so it can not be decomposed. Tell the author; asking again will not succeed."*
- **draft held by a graduation run** — kind `PermissionDenied`, not retryable: *"A graduation run holds this draft, so it can not be decomposed now. Tell the author; asking again will not succeed."*
- **GitHub task draft** — kind `PermissionDenied`, not retryable: *"This draft mirrors a GitHub task, so it can not be decomposed. Tell the author; asking again will not succeed."*
- **decomposition confirmed** — kind `PermissionDenied`, not retryable: *"The author confirmed the decomposition of this draft, so it takes no new proposals. Tell the author; asking again will not succeed."*
- **a proposal is open** — kind `PermissionDenied`, retryable: *"A new draft you proposed is still waiting on the author. Wait for their decision, or change it with `update`. Nothing was proposed."*
- **conversation resolved** — kind `PermissionDenied`, not retryable: *"This conversation is resolved and takes no further comments, so nothing was proposed. Answer with what you already have; proposing again will not succeed."*
- **blank name** — kind `InvalidArgs`, retryable: *"Give the new draft a name."*
- **blank body** — kind `InvalidArgs`, retryable: *"Give the full prompt of the new draft in `body`."*
- **bad position** — kind `InvalidArgs`, retryable: *"`position` starts at 1 and can not be more than `total`."*
- **blank rationale** — kind `InvalidArgs`, retryable: *"Say why this draft is the right next part. The author reads this beside the proposal."*
- **no spoken part** — kind `InvalidArgs`, retryable: *"The rationale has no spoken part. Put one or two short sentences in one `<spoken>…</spoken>` tag in `rationale`."*
- **no open proposal** — kind `InvalidArgs`, retryable: *"No new draft you proposed is open on this draft, so there is nothing to update. Propose a new draft without `update`."*
- **update malformed** — kind `InvalidArgs`, retryable: *"`update` gives `action`: `change` with at least one of `name`, `body`, `position`, and `total`, or `withdraw` with none of them."*
- **no change proposed** — kind `InvalidArgs`, retryable: *"The fields you gave are the ones the open proposal already has, so there is nothing to decide."*
- **could not record** — kind `Other`, retryable: *"The proposed draft could not be recorded."*

### Side effects
Declared in full (per TLC-FR-17). A successful call without `update` produces **exactly one new-draft proposal** and **exactly one comment**, through `../core/DCP-draft-change-proposals.md`'s recording path (DCP-FR-TGRN). A successful call with `update` produces **no new proposal** and **exactly one comment**, through the update path (DCP-FR-GDTC). No draft is created, the original draft is not changed, no file of the project is touched, and no repository state is changed.

## Non-functional requirements
- The description is written so that a model can tell this tool from `PDC-propose-draft-changes-tool.md`'s on the first sentence: that one offers a rewrite of the draft's own prompt, and this one offers a new draft that replaces a part of it.
- A call costs one file write for the body, one for the record, and one line of I/O through the conversation's own append path.
- The body is written to disk rather than carried in the comment, so a long proposed prompt costs the conversation a line.
- Nothing in this tool requires network access.
