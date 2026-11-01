# Decide draft changes tool

**Spec code:** `DDC`

## Intent
The tool an agent uses when the author tells it to decide a change of the open proposal of the draft under discussion: "accept the second change", "reject the last one", "let us discuss the first". The author can then decide a proposal by voice, or in a typed comment, without a hand on the review. The tool **accepts**, **rejects**, or **discusses** one or more hunks of that proposal. Each entry runs the same operation as the matching chip of `../ui/DCR-draft-change-review.md` (DCR-FR-12), on the same terms, so a decision by tool is the author's decision and not the agent's (`../core/DCP-draft-change-proposals.md` DCP-FR-CIPW). The agent calls it only on an explicit instruction of the author in the latest comment, and it does not read the decision back and wait for a confirmation: the instruction is the confirmation. The call returns what happened to each entry, and the turn continues, so the agent tells the author in its reply. Out of scope: deciding on the agent's own judgement, which the description forbids; creating, updating, or withdrawing a hunk, which is `PDC-propose-draft-changes-tool.md`'s; editing a hunk's text, which is the author's on the review (DCR-FR-24); deciding a proposal of a published prompt or a project document, which is `DPC-decide-prompt-changes-tool.md`'s; deciding a proposal of any draft but the turn's own; and rendering the decision, which is `../ui/DCR-draft-change-review.md`'s.

## Functional requirements
1. **DDC-FR-SQYQ** The tool is a `rig` portable tool named `decide_draft_changes` and conforms to `TLC-tool-conventions.md` in full. It is one of the tools of TLC-FR-17 that declare a side effect, and its side effect is the one the contract surface declares and nothing else.
2. **DDC-FR-VAYO** Its `description()` returns the fixed text of the contract surface, and its `parameters()` returns the JSON Schema of the contract surface with its parameter descriptions (per TLC-FR-05, TLC-FR-06). Neither varies by draft, conversation, agent, or project.
3. **DDC-FR-QRIY** The tool is constructed per turn (per TLC-FR-15). Its constructor binds it to the turn's draft, the origin of the turn's conversation, and the turn's agent participant. It accepts no argument that names a draft, a proposal, a conversation, a participant, or a path.
4. **DDC-FR-UOLP** The tool is attached to the turns whose origin kind is a draft conversation and to no other turn (per `../ai/CVL-conversation-loop.md` CVL-FR-08). It is attached in every such turn, typed or spoken, and never together with `decide_prompt_changes`.
5. **DDC-FR-OEJB** A call on a turn whose origin kind is not a draft conversation is the **not a draft conversation** refusal: `PermissionDenied`, not retryable. With no project open the call is the shared **no open project** refusal (per TLC-FR-13). Neither decides anything.
6. **DDC-FR-WRFY** A call carries `decisions`, a list of 1 to 50 entries, each with an `id` and an `action`. An empty list, a list of more than 50 entries, and an entry with no `id` or no `action` are the shared **malformed arguments** refusal, and nothing is decided.
7. **DDC-FR-ZGXF** A call while the draft holds no open proposal (per `../core/DCP-draft-change-proposals.md` DCP-FR-PWSF) is the **no open proposal** refusal of the whole call: `InvalidArgs`, retryable. Nothing is decided, and the result names no entry.
8. **DDC-FR-DSTU** The entries run one at a time, in the order of the call. Each entry reads the proposal as it stands after the entries before it, and a refused entry stops no other entry.
   - *Why:* The author says "accept the first and the third"; a refusal of the first must not cost the author the third.
9. **DDC-FR-VUJQ** An `accept` entry invokes the operation of "accept draft change hunk" (`../core/DCP-draft-change-proposals.md` DCP-FR-11) for that hunk, with no feedback. It writes the hunk's text as storage holds it, an author's edit written through DCP-FR-25 included, and records one version in the draft's History, as the chip's **Accept** does (`../ui/DCR-draft-change-review.md` DCR-FR-13).
10. **DDC-FR-YKPP** An `accept` entry accepts a hunk that is `pending`, `discussing`, or `rejected`, so a rejected hunk is accepted while the proposal is open (per `../core/DCP-draft-change-proposals.md` DCP-FR-11).
11. **DDC-FR-QFNQ** A `reject` entry invokes the operation of "reject draft change hunk" (`../core/DCP-draft-change-proposals.md` DCP-FR-14) for that hunk, with no feedback. It writes nothing into the draft, as the chip's **Reject** does (`../ui/DCR-draft-change-review.md` DCR-FR-14).
12. **DDC-FR-EEAZ** A `discuss` entry opens a **new** hunk discussion of the draft, on the terms of the chip's **Discuss** (`../ui/DCR-draft-change-review.md` DCR-FR-MTFD). Its opening comment carries one `proposal_hunk` attachment that names the hunk and the draft (per `../core/CMS-comments-storage.md` CMS-FR-TQVB).
13. **DDC-FR-ILCB** The tool appends that opening comment as the turn's agent participant, through `../core/CMS-comments-storage.md`'s `append_as` (CMS-FR-26, CMS-FR-41). Its body is the fixed line of the contract surface and nothing else.
14. **DDC-FR-NKIA** After the hunk discussion opens, a `discuss` entry invokes the operation of "hold draft change hunk" for a `pending` hunk, as `../ui/DCR-draft-change-review.md` DCR-FR-JTXE does. A `rejected` hunk stays `rejected`.
15. **DDC-FR-NLDN** A `discuss` entry dispatches no agent turn into the new discussion. The calling turn continues in its own conversation.
16. **DDC-FR-JCSV** An entry is refused, and decides nothing, with the code that applies first: `invalid_action` for an action other than `accept`, `reject`, or `discuss`; `unknown_hunk` for an id that names no hunk of the open proposal; `hunk_withdrawn`; `hunk_accepted`; and `hunk_lost` for a hunk that resolves nowhere in the prompt (DCP-FR-BMLX).
17. **DDC-FR-EDMT** A `reject` of a `rejected` hunk, and a `discuss` of a hunk that already has a hunk discussion (a resolved one included, per `../ui/DCR-draft-change-review.md` DCR-FR-VBQN), are refused with `already_in_state`.
18. **DDC-FR-XBQV** An entry that runs after an earlier entry of the same call closed the proposal is refused with `no_open_proposal`.
19. **DDC-FR-LYTA** An entry that the backend operation refuses for another reason is refused with `decision_failed`, which names the backend's typed error. It is retryable for `write_failed`, `acceptance_in_progress`, and `history_recovery_failed`, and not retryable for every other error.
20. **DDC-FR-SLBB** Every entry refusal names the entry by its place in the call, counted from 1 — *Decision 2 of 3* — and carries a message that says what to correct. Every refusal of DDC-FR-JCSV, DDC-FR-EDMT, and DDC-FR-XBQV is retryable.
21. **DDC-FR-WSIA** A call that passes DDC-FR-OEJB, DDC-FR-WRFY, and DDC-FR-ZGXF returns the data object of the contract surface, also when every entry is refused. The result has one row per entry, in call order.
22. **DDC-FR-IBYT** The decision comment (DCP-FR-15), the `"draft change proposals changed"` event (DCP-FR-16), the proposal close (DCP-FR-PWSF), and the statistics line (DCP-FR-WTKA) follow from each entry as they follow from the same decision by chip.
23. **DDC-FR-KQZN** A close that the tool causes dispatches no agent turn. The calling turn is running and reads the close in the result.
   - *Why:* A chip decision dispatches a turn because no turn runs (`../ui/DCR-draft-change-review.md` DCR-FR-15); here one already runs, and a second turn would answer the same decision twice.
24. **DDC-FR-PWTE** A call does not end the turn. The loop continues after the result, so the agent can tell the author what it decided. The tool is not one of the tools that end a turn (per `../ai/CVL-conversation-loop.md` CVL-FR-15).
25. **DDC-FR-HUJM** The tool never creates, updates, edits, or withdraws a hunk, and never declines the whole proposal through "decline draft change proposal". An author who asks to reject all is answered with one `reject` entry for each undecided hunk.
26. **DDC-FR-ROLG** The description tells the agent to call the tool only on an explicit instruction of the author in the latest comment, and not to read the decision back and wait for a confirmation.
27. **DDC-FR-MDCX** The description tells the agent to map a change named by its place to the hunk at that place in proposal order, with withdrawn hunks skipped, as the review counter counts (per `../ui/DCR-draft-change-review.md` DCR-FR-11). The agent reads each hunk's id, place, state, and text in its input (per `../core/AGC-agent-conversations.md` AGC-FR-EPNB).
28. **DDC-FR-TBVA** Each call emits under the `ai` and `backend` domains (per TLC-FR-14): one `INFO` record that names the tool, the draft, and the number of entries of each outcome, and one `WARN` record for each refused entry or call, with its code and the entry's place. No record carries hunk text, comment text, or the agent's input.
29. **DDC-FR-GXEW** The tool holds no state between calls (per TLC-FR-15), reaches no network, and touches no credential but the GitHub identity that the decision comment's append resolves (per `../core/DCP-draft-change-proposals.md` DCP-FR-23).

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach of its own.

### The tool
`decide_draft_changes` — a `rig` portable tool, constructed per turn (per TLC-FR-15). Its constructor takes the draft the turn is about, the origin of the conversation the turn belongs to, and the agent participant the turn answers as.

### The description
The fixed text the model reads (per TLC-FR-05):

> Decide changes of the open proposal of the draft under discussion, when the author tells you to. Call it only when the author's latest comment tells you plainly to accept, reject, or discuss a change, for example "accept the second change". Never call it on your own judgement, on an earlier comment, or to tidy up a proposal. Do not read the decision back and wait for a confirmation: the author's instruction is the confirmation. Pass `decisions`, one entry for each change the author named, in the order the author named them. Each entry gives the `id` of a change and an `action`. `accept` writes the change into the prompt as it stands, with any edit the author made to it. `reject` writes nothing. `discuss` opens a new discussion about that change, with a first comment from you that says the author asked to discuss it; it does not decide the change. A rejected change can still be accepted while the proposal is open. Your input lists each change of the open proposal with its id, its place, its state, and its text. When the author names a change by its place, count the changes in their order and skip the withdrawn ones, as the review does. Each entry runs on its own: a refused entry stops no other, and the result tells you what happened to each. Tell the author in your reply what you did. This tool never makes, changes, or withdraws a change; use `propose_draft_changes` for that.

### Arguments
```
DecideDraftChangesArgs {
  decisions: [            // required; 1 to 50 entries, run in this order
    {
      id:     string,     // required; the id of a hunk of the open proposal
      action: string      // required; "accept" | "reject" | "discuss"
    }
  ]
}
```

The JSON Schema:

```json
{
  "type": "object",
  "properties": {
    "decisions": {
      "type": "array",
      "minItems": 1,
      "maxItems": 50,
      "items": {
        "type": "object",
        "properties": {
          "id": { "type": "string" },
          "action": { "type": "string", "enum": ["accept", "reject", "discuss"] }
        },
        "required": ["id", "action"]
      }
    }
  },
  "required": ["decisions"]
}
```

`action` is decoded as a string, so a value outside the enum is the per-entry `invalid_action` refusal and not a refusal of the whole call (DDC-FR-JCSV).

Their descriptions in the parameter schema (per TLC-FR-06):

- `decisions` — *"The decisions the author asked for in the latest comment, in the order the author gave them. Each entry names one change by its `id` and gives one `action`."*
- `id` — *"The id of a change of the open proposal, as your input lists it."*
- `action` — *"`accept` to write the change into the prompt, `reject` to decline it, or `discuss` to open a new discussion about it."*

### Output
Data (per TLC-FR-08):

```
DecideDraftChangesOutput {
  proposal_state: "pending" | "accepted" | "rejected",   // after the last entry
  closed:         boolean,       // true where an entry of this call closed the proposal
  decisions: [                   // one row per entry, in call order
    {
      place:         number,     // the entry's place in the call, from 1
      id:            string,     // the id as the call gave it
      action:        string,     // the action as the call gave it
      outcome:       "accepted" | "rejected" | "discussion_opened" | "refused",
      hunk_state:    HunkState?, // the hunk's state after the entry; absent for unknown_hunk
      discussion_id: string?,    // the new hunk discussion, for "discussion_opened"
      refusal: {                 // for "refused" only
        code:      "unknown_hunk" | "hunk_lost" | "hunk_accepted" | "hunk_withdrawn"
                 | "already_in_state" | "invalid_action" | "no_open_proposal"
                 | "decision_failed",
        error:     string?,      // the backend's typed error, for "decision_failed"
        message:   string,       // names the place: "Decision 2 of 3: ..."
        retryable: boolean
      }?
    }
  ]
}
```

`HunkState` is the shape of `../core/DCP-draft-change-proposals.md`.

### The discussion comment
The opening comment of a hunk discussion that a `discuss` entry opens. Its body is this fixed line and nothing else:

```
The author asked to discuss this change.
```

It carries one `proposal_hunk` attachment that names the hunk and the draft.

### Refusals
Of the whole call, as `ToolExecutionError`:
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: `NotFound`, not retryable.
- **not a draft conversation** — `PermissionDenied`, not retryable: *"This conversation is not about a draft, so there is no proposal here to decide. Asking again will not succeed."*
- **malformed arguments** — the shared refusal of `TLC-tool-conventions.md`: `InvalidArgs`, retryable.
- **no open proposal** — `InvalidArgs`, retryable: *"No proposal is open on this draft, so there is no change to decide. Tell the author that nothing is waiting for a decision."*

Of one entry, as a `refused` row of the output:
- `unknown_hunk` — *"Decision N of M: the id you gave is not a change of the open proposal. Use an id from your input."*
- `hunk_lost` — *"Decision N of M: the text this change replaces is no longer in the prompt, so it can not be decided here. Tell the author to clear it in the review."*
- `hunk_accepted` — *"Decision N of M: this change is already accepted, and an accepted change is final."*
- `hunk_withdrawn` — *"Decision N of M: this change was withdrawn, so there is nothing to decide."*
- `already_in_state` — *"Decision N of M: this change is already rejected"*, or *"already has a discussion"*, *"so nothing changed."*
- `invalid_action` — *"Decision N of M: the action must be `accept`, `reject`, or `discuss`."*
- `no_open_proposal` — *"Decision N of M: an earlier decision of this call closed the proposal, so this one was not made."*
- `decision_failed` — *"Decision N of M: the decision could not be made"*, with the backend's typed error.

### Side effects
Declared in full (per TLC-FR-17). Each `accept` entry that succeeds writes the draft's prompt and records one version, through `../core/DCP-draft-change-proposals.md` DCP-FR-11. Each `reject` entry that succeeds writes the proposal record alone (DCP-FR-14). Each `discuss` entry that succeeds appends one new discussion of the draft and, for a `pending` hunk, writes the proposal record (DCP-FR-FHWN). The entry that closes the proposal appends the decision comment and the statistics line (DCP-FR-15, DCP-FR-WTKA). A refused entry writes nothing.

## Non-functional requirements
- A call costs one decision operation for each entry and nothing more. The tool reads the hunk discussions of the draft only for a `discuss` entry.
- A decision by tool reaches the review through the `"draft change proposals changed"` event, so an open review shows it in the same moment as a decision from another window.
- The description is written so that a model can tell this tool from `PDC-propose-draft-changes-tool.md` on its first sentence: this one decides changes that stand, and that one offers them.
