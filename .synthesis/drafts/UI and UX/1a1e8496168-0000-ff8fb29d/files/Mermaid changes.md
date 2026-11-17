## Intent

When Author is working on documents with Mermaid diagram, agent proposing changes to mermaid diagram should have a dedicated tool suited for diagram changes.

This tool should be connected to the dedicated new side-by-side comparison render inside the editor, so the diff allows user to compare two diagrams side by side and not the text.

## User journey

- Author discusses a document with Mermaid diagrams (a draft prompt or a project document) with an agent.
- The agent calls the new diagram tool. A diagram hunk joins the one open proposal of that file, or opens the proposal when none is open.
- Author sees the fence in the document show the **new** diagram, with a chip. The chip carries Accept, Reject, and Discuss, as every hunk does, and a button that opens the comparison pop-up.
- Author opens the pop-up. It shows the old diagram and the new diagram side by side. Author reads them and accepts or rejects the hunk from the pop-up, or closes it and decides later.
- Accept writes the new diagram source into the file. Reject leaves the file as it was and the fence shows the old diagram again.
## Requirements

### Agent tool

- The new tool serves the draft's prompt and project files (documents). It does not serve OpenSpec changes. Like the existing propose tools, it is bound to the artifact of the conversation, accepts no other target, and ends the agent's turn.
- A diagram hunk is a **new hunk kind of the same proposal**. It does not make a second proposal, so the one-undecided-proposal-per-artifact rule holds. The tool adds hunks to the open proposal, and the agent can change and withdraw its diagram hunks with the update rules of the existing tools.
- One diagram hunk can **change**, **add**, or **remove** one diagram:
  - Change: the argument names the fence by its **current source as anchor** (fences carry no id) and gives the **full new source**.
  - Remove: the argument names the fence by its current source.
  - Add: the argument gives the full new source and the text it follows, as an insertion hunk does.
  - An anchor that matches no fence, or more than one fence, is refused with a typed error, as for text hunks.
- The tool **refuses** a new source that is not valid Mermaid. The refusal carries the parse error, so the agent corrects the source and calls again. The check uses the same bundled Mermaid parser as the Editor (EDT-FR-JIBV). No proposal is recorded on a refusal.
- The Conversation Agent prompt must tell the agent: a change to a Mermaid diagram never goes in a text hunk, and never in the same call as text changes. Diagram changes and text changes are separate tool calls.

### Review in the document

- A diagram hunk shows in the document as the fence with the **new** diagram and a chip. A removed diagram shows in place, marked as removed. The chip has the hunk actions of the review (accept, reject, discuss) and a **Compare** button.
- Decisions are per hunk, on the terms of the existing review. Accept all and Reject all, and the decide tool of the agent, work on diagram hunks as on text hunks.
- A diagram hunk's proposed source is **not editable**. The author decides only: accept, reject, or discuss. The hunk-edit rules of DCR-FR-24 to DCR-FR-26 do not apply to it.
- Accept replaces, inserts, or deletes the fence in the file through the same write path and the same transaction as a text hunk (one version for a draft).

### Comparison pop-up

- The Compare button opens a pop-up with the old diagram on the left and the new diagram on the right, both rendered diagrams and not text. For an add, the old side shows an empty state. For a remove, the new side shows an empty state.
- The pop-up has Accept and Reject for that hunk, and a visible Close control. After a decision the pop-up closes and the review moves on by its existing rule. Escape and Close decide nothing and change no file.
- The pop-up follows the diagram overlay rules of the Editor (EDT-FR-SJGJ): modal with a scrim, focus into the overlay on open and back to the Compare button on close, modal dialog role with an accessible name, keyboard operable, and a floating overlay as the other diagram overlays are.
- Both diagrams use the bundled Mermaid library, work offline, use the strict security level, and follow the active theme (EDT-FR-JIBV, EDT-FR-JBZU). A diagram that fails to render shows its source and the error message, as EDT-FR-XTEB states.
- Opening and closing the pop-up is view state: no buffer change, no dirty mark, no undo step.
- When the hunk is decided elsewhere (another window, or the decide tool), the pop-up closes.

## Expected deliverables

- New diagram tool for the conversational agent, with its specification.
- Diagram hunk kind in the draft and file proposals.
- Rich editor review: diagram hunk rendering, chip with Compare button, and the side-by-side comparison pop-up with decisions.
- Conversation Agent prompt update.

## Specification impact

This work changes, and must update in the same change:
- `specifications/tools/`: a new specification for the diagram tool, in the shape of `PDC-propose-draft-changes-tool.md`. `PDC-propose-draft-changes-tool.md` and `PPC-propose-prompt-changes-tool.md` state that a diagram change goes to the new tool.
- `specifications/core/DCP-draft-change-proposals.md` and `specifications/core/PCP-prompt-change-proposals.md`: the diagram hunk kind (change, add, remove), its storage, and its acceptance.
- `specifications/ui/DCR-draft-change-review.md`: the diagram hunk rendering, the chip with Compare, the rule that its text is not editable, and the pop-up.
- `specifications/ui/PCR-prompt-change-review.md`: the same for files. Its statement "This surface opens no modal, no scrim" must allow the comparison pop-up.
- `specifications/ui/EDT-editor.md`: the comparison pop-up reuses the diagram overlay rules.
- `specifications/ui/CMT-comments.md`: a diagram hunk reference in a proposal comment, if the `proposal_hunk` attachment needs a new kind.