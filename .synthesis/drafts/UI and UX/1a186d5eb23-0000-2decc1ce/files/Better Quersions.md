## Intent

During discussion Agent can ask questions and propose options to answer.

We have a tool for it, and this tool has a UI integration.

Now we need to improve the answering experience. Each proposed option (not the author's own-answer row) gets a short **label** and a separate **description**: pros/cons, trade-offs, impacts. The description is extra context that helps the author decide. The label becomes only a few words. The detail moves to the description.

The answering block shows the labels on the left and the description of one option on the right.

## User journey

- User starts a discussion.
- Agent asks few questions using the existing tool.
- The answering block above the composer always shows 2 vertical rails. The author chooses an option in the left rail. The right rail shows a description.
- The right rail shows the description of the hovered option. If no option is hovered, it shows the description of the selected option. If there is none, it shows a placeholder that says the description appears once an option is selected. The own-answer row has no description, so the right rail shows the placeholder for it.
- Where the width is short (draft discussion column, floating panel), the rails stack under a width threshold: the description shows below the options in one column.
- The author submits. The history shows each question comment with labels only. An icon button on the comment expands and collapses the descriptions. The default is collapsed.

## Requirements

- Look and feel of the discussion must stay aligned witht the current state of it.
- Descriptions are not part of the comment body. The body keeps the labels only, as a numbered list, with no machine-readable marker. A new structured field on the question comment stores the description of each option. The history expand button reads this field. Later agent turns see labels only. The turn builder does not add descriptions to the conversation text.
- The pending question set also stores the description of each option. A set or a question comment stored before this change has no description. It must still load and render in one column, with no expand button.
- The tool requires a description for every option. A call where an option has a blank or missing description is refused with a retryable `InvalidArgs`. The tool enforces these limits and refuses a call that breaks one with a retryable `InvalidArgs`: label at most 5 words and 40 characters; description at most 300 characters.
- The agent must write short labels (a couple of words) and concise descriptions that stay inside the limits above.
- After these changes the conversational agent must be capable of handling the conversation as it does today.

## Expected Deliverables

- The compiled discussion prompt (`CVL-conversation-loop`) and the model-facing description of the question tool are updated to tell the agent to give short labels and concise descriptions within the limits. No new skill is created.
- The tool, the storage and the UI are updated as described above.
- Specifications this work must change: `tools/ADQ-ask-discussion-questions-tool.md` (option shape, limits, refusals, comment body and structured field), `core/CMS-comments-storage.md` (`PendingQuestionSet` option and question comment field), `ui/DQA-discussion-question-answering.md` (two rails, placeholder, hover, stacking), `ui/CMT-comments.md` (expand control in history), `ai/CVL-conversation-loop.md` (compiled discussion prompt).