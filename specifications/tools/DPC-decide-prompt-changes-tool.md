# Decide prompt changes tool

**Spec code:** `DPC`

## Intent
The tool an agent uses when the author tells it to decide a change of the open proposal of the published prompt or the project document under discussion. It **accepts**, **rejects**, or **discusses** one or more hunks of that proposal, so the author can decide a proposal to a file by voice, or in a typed comment, as they decide one to a draft (`DDC-decide-draft-changes-tool.md`). Each entry runs the same operation as the matching chip of `../ui/PCR-prompt-change-review.md`, on the same terms, so a decision by tool is the author's decision and not the agent's (`../core/PCP-prompt-change-proposals.md` PCP-FR-VCBX). The agent calls it only on an explicit instruction of the author in the latest comment, and it does not read the decision back and wait for a confirmation. The backend holds the agent to that rule on the terms of `DDC-decide-draft-changes-tool.md`: the newest comment the turn received must be one the author typed or spoke, the call quotes the author's words from it, and one comment authorizes one decision. The call returns what happened to each entry, and the turn continues. Out of scope: deciding on the agent's own judgement; creating, updating, or withdrawing a hunk, which is `PPC-propose-prompt-changes-tool.md`'s; editing a hunk's text; deciding a proposal of a draft, which is `DDC-decide-draft-changes-tool.md`'s; deciding a proposal of any file but the turn's own; and rendering the decision, which is `../ui/PCR-prompt-change-review.md`'s.

## Functional requirements
1. **DPC-FR-AFSM** The tool is a `rig` portable tool named `decide_prompt_changes` and conforms to `TLC-tool-conventions.md` in full. It is one of the tools of TLC-FR-17 that declare a side effect, and its side effect is the one the contract surface declares and nothing else.
2. **DPC-FR-RJBJ** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema of `DDC-decide-draft-changes-tool.md` with `confirm` left out of the `action` enum, and with the parameter descriptions of the contract surface (per TLC-FR-05, TLC-FR-06). Neither varies by artifact, conversation, agent, or project.
3. **DPC-FR-QMZE** The tool is constructed per turn (per TLC-FR-15). Its constructor binds it to the artifact of the turn's target, the origin of the turn's conversation, and the turn's agent participant. It accepts no argument that names an artifact, a path, a proposal, a conversation, or a participant.
4. **DPC-FR-RFEF** The tool is attached to the turns whose origin kind is an artifact conversation and to no other turn (per `../ai/CVL-conversation-loop.md` CVL-FR-08). It is attached in every such turn, typed or spoken, and never together with `decide_draft_changes`.
5. **DPC-FR-JJGY** A call on a turn whose origin kind is not an artifact conversation is the **not an artifact conversation** refusal: `PermissionDenied`, not retryable. With no project open the call is the shared **no open project** refusal (per TLC-FR-13). Neither decides anything.
6. **DPC-FR-RGGP** The arguments, the entry order, the entry independence, and the whole-call refusals are those of `DDC-decide-draft-changes-tool.md` DDC-FR-WRFY, DDC-FR-ZGXF, and DDC-FR-DSTU, with the artifact's `pending` proposal (per `../core/PCP-prompt-change-proposals.md` PCP-FR-04) in place of the draft's open proposal.
7. **DPC-FR-DQQN** An `accept` entry invokes the operation of "accept prompt change hunk" (`../core/PCP-prompt-change-proposals.md` PCP-FR-12) for that hunk, with no feedback. It writes the hunk's text as storage holds it, an author's edit written through PCP-FR-EDTW included, through the file's own write path, and accepts a `rejected` hunk while the proposal is `pending`.
8. **DPC-FR-WNHU** An `accept` that the backend answers with `acceptance_incomplete` is an accepted entry. The tool then invokes "complete prompt change decision" (PCP-FR-14) once. Where that also fails, the row carries `comment_owed: true`, and the review offers the retry of `../ui/PCR-prompt-change-review.md` PCR-FR-15.
   - *Why:* The file is written at the commit point, so the change has landed; only the decision comment is still owed.
9. **DPC-FR-TVKX** An accept by tool writes the file while an Editor tab may hold it open. The tool does not wait for that tab. The open session follows `../ui/PCR-prompt-change-review.md` PCR-FR-LAGZ, which applies the external change rules of `../ui/EXC-editor-external-change.md`.
10. **DPC-FR-QVKH** A `reject` entry invokes the operation of "reject prompt change hunk" (`../core/PCP-prompt-change-proposals.md` PCP-FR-RJHN) for that hunk, with no feedback, and writes nothing into the project.
11. **DPC-FR-WCBF** A `discuss` entry opens a **new** hunk discussion of the artifact, on the terms of the chip's **Discuss** (`../ui/PCR-prompt-change-review.md` PCR-FR-HDMC). Its opening comment carries one `proposal_hunk` attachment that names the hunk and the artifact (per `../core/CMS-comments-storage.md` CMS-FR-TQVB, CMS-FR-AHDQ).
12. **DPC-FR-AFVA** The tool appends that opening comment as the turn's agent participant, through `../core/CMS-comments-storage.md`'s `append_as` (CMS-FR-26, CMS-FR-41). Its body is the fixed line of `DDC-decide-draft-changes-tool.md` and nothing else.
13. **DPC-FR-QPNB** After the hunk discussion opens, a `discuss` entry invokes the operation of "hold prompt change hunk" (PCP-FR-HLDQ) for a `pending` hunk. A `rejected` hunk stays `rejected`. The entry dispatches no agent turn into the new discussion.
14. **DPC-FR-NXZF** An entry is refused on the codes, the order, and the places of `DDC-decide-draft-changes-tool.md` DDC-FR-JCSV, DDC-FR-EDMT, DDC-FR-XBQV, DDC-FR-LYTA, and DDC-FR-SLBB. A hunk discussion is one whose opening comment carries a `proposal_hunk` that names the hunk and the artifact.
15. **DPC-FR-JEXJ** `decision_failed` is retryable for `write_failed` and `acceptance_in_progress`, and not retryable for `artifact_not_found`, `not_a_prompt_artifact`, and every other error.
16. **DPC-FR-UEXT** A call that passes the whole-call refusals returns the data object of `DDC-decide-draft-changes-tool.md`, with `comment_owed` on an accepted row, also when every entry is refused.
17. **DPC-FR-IHKP** The decision comment (PCP-FR-RSLV), the `"prompt change proposals changed"` event (PCP-FR-17), the proposal close (PCP-FR-HKLD), and the notification retraction (`../ui/PCR-prompt-change-review.md` PCR-FR-26) follow from each entry as they follow from the same decision by chip.
18. **DPC-FR-QTIW** A close that the tool causes dispatches no agent turn, and a call does not end the turn, on the terms of `DDC-decide-draft-changes-tool.md` DDC-FR-KQZN and DDC-FR-PWTE.
19. **DPC-FR-SNEB** The tool never creates, updates, edits, or withdraws a hunk, and never invokes "decline prompt change proposal". It never creates a file (per `../core/PCP-prompt-change-proposals.md` PCP-FR-08).
20. **DPC-FR-HWOT** The description tells the agent to call the tool only on an explicit instruction of the author in the latest comment, to copy the words of that instruction into `instruction`, not to read the decision back and wait for a confirmation, and to map a change named by its place as `DDC-decide-draft-changes-tool.md` DDC-FR-MDCX states.
21. **DPC-FR-VJIG** Every call follows the authority rules of `DDC-decide-draft-changes-tool.md` DDC-FR-PZBH, DDC-FR-ELAE, DDC-FR-ZYOL, DDC-FR-NKAB, DDC-FR-VONX, DDC-FR-CMRE, and DDC-FR-VYLJ, with its refusals and their messages, and DPC-FR-JJGY in place of DDC-FR-OEJB. The decision comment that a call causes ends with the instruction line of `DDC-decide-draft-changes-tool.md` (PCP-FR-QKCT). An entry with the action `confirm` is refused with `invalid_action`.
22. **DPC-FR-CIRL** Each call logs as `DDC-decide-draft-changes-tool.md` DDC-FR-TBVA states, with the artifact's project-relative path in place of the draft, and no record carries the `instruction`. The tool holds no state between calls, reaches no network, and touches no credential but the identity the decision comment resolves (PCP-FR-27).

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach of its own.

### The tool
`decide_prompt_changes` — a `rig` portable tool, constructed per turn (per TLC-FR-15). Its constructor takes the artifact of the turn's target, the origin of the conversation the turn belongs to, and the agent participant the turn answers as.

### The description
The fixed text the model reads (per TLC-FR-05):

> Decide changes of the open proposal of the prompt or project document under discussion, when the author tells you to. Call it only when the author's latest comment tells you plainly to accept, reject, or discuss a change, for example "accept the second change". Never call it on your own judgement, on an earlier comment, or to tidy up a proposal. Do not read the decision back and wait for a confirmation: the author's instruction is the confirmation. Pass `decisions`, one entry for each change the author named, in the order the author named them. Each entry gives the `id` of a change and an `action`. `accept` writes the change into the file as it stands, with any edit the author made to it. `reject` writes nothing. `discuss` opens a new discussion about that change, with a first comment from you that says the author asked to discuss it; it does not decide the change. A rejected change can still be accepted while the proposal is open. Your input lists each change of the open proposal with its id, its place, its state, and its text. When the author names a change by its place, count the changes in their order and skip the withdrawn ones, as the review does. Each entry runs on its own: a refused entry stops no other, and the result tells you what happened to each. Tell the author in your reply what you did. Copy into `instruction` the words of the author's latest comment that ask for these decisions, exactly as the author wrote or said them. A call is refused when the latest comment is not the author's, when the words are not in it, or when that comment already gave a decision. While the author is still speaking, wait until the comment of the speech arrives. This tool never makes, changes, or withdraws a change; use `propose_prompt_changes` for that.

### Arguments
`DecidePromptChangesArgs` has the shape and the JSON Schema of `DecideDraftChangesArgs` (`DDC-decide-draft-changes-tool.md`), with `"accept" | "reject" | "discuss"` as the values of `action`. Their descriptions in the parameter schema (per TLC-FR-06):

- `instruction` — the description of `DDC-decide-draft-changes-tool.md`.

- `decisions` — *"The decisions the author asked for in the latest comment, in the order the author gave them. Each entry names one change by its `id` and gives one `action`."*
- `id` — *"The id of a change of the open proposal, as your input lists it."*
- `action` — *"`accept` to write the change into the file, `reject` to decline it, or `discuss` to open a new discussion about it."*

### Output
Data (per TLC-FR-08): `DecidePromptChangesOutput`, the shape of `DecideDraftChangesOutput` (`DDC-decide-draft-changes-tool.md`), with one more field on each row:

```
comment_owed: boolean?   // on an "accepted" row: true where the decision comment
                         //   is still owed after "complete prompt change decision"
```

`decision_failed` names the typed errors of `../core/PCP-prompt-change-proposals.md`.

### The discussion comment
The fixed line of `DDC-decide-draft-changes-tool.md`, with one `proposal_hunk` attachment that names the hunk and the artifact.

### Refusals
Of the whole call, as `ToolExecutionError`:
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: `NotFound`, not retryable.
- **not an artifact conversation** — `PermissionDenied`, not retryable: *"This conversation is not about a prompt or a project document, so there is no proposal here to decide. Asking again will not succeed."*
- **malformed arguments** — the shared refusal of `TLC-tool-conventions.md`: `InvalidArgs`, retryable.
- **speech not posted**, **not authorized**, **instruction not found**, and **instruction used** — those of `DDC-decide-draft-changes-tool.md`, with their kinds and messages.
- **no open proposal** — `InvalidArgs`, retryable: *"No proposal is open on this file, so there is no change to decide. Tell the author that nothing is waiting for a decision."*

Of one entry: the codes and the messages of `DDC-decide-draft-changes-tool.md`, with "the file" in place of "the prompt".

### Side effects
Declared in full (per TLC-FR-17). Each `accept` entry that succeeds writes the artifact's file through `../core/PST-project-storage.md`'s write path (PST-FR-09), inside the transaction of `../core/PCP-prompt-change-proposals.md` PCP-FR-14. Each `reject` entry that succeeds writes the proposal record alone. Each `discuss` entry that succeeds appends one new discussion of the artifact and, for a `pending` hunk, writes the proposal record. The entry that closes the proposal appends the decision comment. A call that decides an entry spends its authorizing comment, in memory alone (`../core/AGC-agent-conversations.md` AGC-FR-QZVW). A refused entry writes nothing. No file is created.

## Non-functional requirements
- A call costs one decision operation for each entry, at most one "complete prompt change decision" for each accepted entry, and nothing more.
- A decision by tool reaches the review and the Editor tab through the `"prompt change proposals changed"` event.
- The description is written so that a model can tell this tool from `PPC-propose-prompt-changes-tool.md` on its first sentence, and from `DDC-decide-draft-changes-tool.md` by the thing it decides.
