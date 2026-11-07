## Intent

Discussion now shows the tool calls agent does during his current session.

This feature should be extended. User should be able to see the tool calls of a delivered agent answer in the chat history, in the same way it is visualized now, also after an application restart. The tool list is saved with the agent's answer comment and shown as its own row after that comment.

## User journey

- User asks an agent a question in a discussion. While the turn runs, the tool list is available in the pending row (unchanged).
- The agent delivers an answer. The comment appears, and the row `@agent called N tools` appears right after it. It is collapsed.
- User expands the row and sees every tool call: tool name, key input, state.
- User restarts the application and opens the discussion. The comment and its row are still there, collapsed.
- User scrolls the history. Each earlier answer keeps its own row, whatever turns came after it.

## Requirements

- Look and feel must stay the same as is.
- **Scope: delivered answers only.** The tool list is saved on the agent comment that a turn appends as its answer (AGC-FR-16). Comments that a turn ends on by a question (`ask_user_comment`) or by a proposal carry no tool list. A turn that appends no comment (failed, cancelled, `ask_discussion_questions` set, AGC-FR-18) saves nothing. Those turns keep today's session-only behaviour: the row at the foot of the discussion, removed when the next turn starts.
- **Storage.** The tool list is one optional field on the agent comment line, written in the same append as the comment. It is never written later and never changed (CMS-FR-04). A comment without the field (every comment written before this feature, and every comment named above) shows no row, no note, and no change.
- **Content saved.** For each call, in activation order (AGC-FR-33): tool name, key input as shown now (plain text, cut at 200 characters), and the final state (done or failed). No result, no result title, no model text. No call is saved as running. The turn has ended when the comment is written.
- **Display.** A saved list shows as an activity row directly after its comment, with the same text (`@arch called 3 tools`, `called 1 tool`), disclosure control, expanded list, keyboard use, and accessibility as today (CTA-FR-QMWO, CTA-FR-GGYJ, CTA-FR-BJWN, CTA-FR-FPNP). It is collapsed when first shown. It has no timestamp, no cancel, no Retry, and no Quote action. A turn with no tool call gets no field and no row.
- **No duplicate.** For a delivered turn the saved row replaces the session-only foot row. The same list never shows twice. A new turn starting in the discussion removes foot rows only. It never removes a saved row.
- **Not part of the conversation text.** A saved tool list is never given to a model as context, never read aloud by voice mode, and never counted as a comment (unread count, reply routing, resolve rules).
- **Privacy.** Key inputs (search queries, URLs, paths) now reach the disk in the discussion log, and the author has accepted this. Log records of the model loop, the tools, and the statistics log still carry no argument, no query, and no result (CVL-FR-28, CVL-FR-34, TLC-FR-14, DSS).
- **Same in every owner surface** (CTA-FR-MWPE).

## Specifications this work must change

- `specifications/ui/CTA-comment-agent-turns.md`: CTA-FR-ZBIB, CTA-FR-GCKO, CTA-FR-MNPS (and CTA-FR-EZNI, CTA-FR-XKRY where they say an activity row is no part of the conversation). A delivered turn's row is saved with the comment and is not session-only.
- `specifications/core/AGC-agent-conversations.md`: AGC-FR-16 (the delivered comment carries the tool list), and the statements that no operational part of a turn is stored.
- `specifications/core/CMS-comments-storage.md`: the new optional field on an agent comment line, kept as appended by the fold, with no change to existing lines.
- `specifications/ui/CMT-comments.md` and `specifications/ui/CVP-conversation-presentation.md`: the row after the comment, and session-only wording for tool lists.