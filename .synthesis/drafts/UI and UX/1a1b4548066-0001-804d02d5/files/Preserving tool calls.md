## Intent

Discussion now shows the tool calls agent does during his current session.

This feature should be extended. User should be able to see the tool calls of a delivered agent answer in the chat history, in the same way it is visualized now, also after an application restart. The tool list is saved with the agent's answer comment and shown as its own row after that comment.

This is a follow-up to "Multi-persona conversations". It does not need that feature: with its switch off, every delivered answer still keeps its tool list, under the name of the agent that wrote it. When Multi-persona is on, the same list also keeps the advisor name of each advisor call.

## User journey

- User asks an agent a question in a discussion. While the turn runs, the tool list is available in the pending row (unchanged).
- The agent delivers an answer. The comment appears, and the row `@agent called N tools` appears right after it. It is collapsed.
- User expands the row and sees every tool call: tool name, key input, state.
- User restarts the application and opens the discussion. The comment and its row are still there, collapsed.
- User scrolls the history. Each earlier answer keeps its own row, whatever turns came after it.
- Multi-persona on, one tag: the reply of the tagged agent (Meta) has its collapsed "Advisors" list. Its tool row follows the comment and lists Meta's calls and the advisors' calls. Each advisor call names the advisor.
- Multi-persona on, several tags: each tagged agent's delivered reply has its own row. The driver's "Synthesis" comment has its own row, with the driver's calls only.

## Requirements

- Look and feel must stay the same as is.
- **Scope: delivered answers only.** The tool list is saved on the agent comment that a turn appends as its answer (AGC-FR-16). Comments that a turn ends on by a question (`ask_user_comment`) or by a proposal carry no tool list. A turn that appends no comment (failed, cancelled, `ask_discussion_questions` set, AGC-FR-18) saves nothing. Those turns keep today's session-only behaviour: the row at the foot of the discussion, removed when the next turn starts.
- **Storage.** The tool list is one optional field on the agent comment line, written in the same append as the comment. It is never written later and never changed (CMS-FR-04). A comment without the field (every comment written before this feature, and every comment named above) shows no row, no note, and no change.
- **Content saved.** For each call, in activation order (AGC-FR-33): tool name, key input as shown now (plain text, cut at 200 characters), and the final state (done or failed). For a call made by an advisor, also the advisor's nickname (plain text). A call made by the answering agent itself has no advisor name. No result, no result title, no model text. No call is saved as running: a call still in flight when its advisor is skipped or cancelled is saved as failed. The turn has ended when the comment is written.
- **Display.** A saved list shows as an activity row directly after its comment, with the same text (`@arch called 3 tools`, `called 1 tool`), disclosure control, expanded list, keyboard use, and accessibility as today (CTA-FR-QMWO, CTA-FR-GGYJ, CTA-FR-BJWN, CTA-FR-FPNP). It is collapsed when first shown. It has no timestamp, no cancel, no Retry, and no Quote action. A turn with no tool call gets no field and no row.
- **No duplicate.** For a delivered turn the saved row replaces the session-only foot row. The same list never shows twice. A new turn starting in the discussion removes foot rows only. It never removes a saved row.
- **Not part of the conversation text.** A saved tool list is never given to a model as context, never read aloud by voice mode, and never counted as a comment (unread count, reply routing, resolve rules).
- **Privacy.** Key inputs (search queries, URLs, paths) now reach the disk in the discussion log, and the author has accepted this. Log records of the model loop, the tools, and the statistics log still carry no argument, no query, and no result (CVL-FR-28, CVL-FR-34, TLC-FR-14, DSS).
- **Same in every owner surface** (CTA-FR-MWPE).
- **Multi-persona.** The tool list is a separate optional field on the agent comment line. It is not merged with the "Advisors" list or the "Synthesis" label of Multi-persona. All fields of a comment are written in one append. The row text names the agent that wrote the comment. The "Advisors" list stays part of the comment, and the tool row follows it. Calls of one turn are never saved on another agent's comment: in a multi-agent conversation each delivered comment holds only its own turn's calls. A turn that ends on the merged question set or the merged proposal saves nothing (see Scope). With the switch off, nothing changes except the saved list.

## Specifications this work must change

- `specifications/ui/CTA-comment-agent-turns.md`: CTA-FR-ZBIB, CTA-FR-GCKO, CTA-FR-MNPS (and CTA-FR-EZNI, CTA-FR-XKRY where they say an activity row is no part of the conversation). A delivered turn's row is saved with the comment and is not session-only.
- `specifications/core/AGC-agent-conversations.md`: AGC-FR-16 (the delivered comment carries the tool list), and the statements that no operational part of a turn is stored.
- `specifications/core/CMS-comments-storage.md`: the new optional field on an agent comment line, kept as appended by the fold, with no change to existing lines.
- `specifications/ui/CMT-comments.md` and `specifications/ui/CVP-conversation-presentation.md`: the row after the comment, and session-only wording for tool lists.
- `AGC-agent-conversations.md`, `CTA-comment-agent-turns.md`, `CVP-conversation-presentation.md` and `CMS-comments-storage.md` are also changed by "Multi-persona conversations". Build on its wording: the tool-call record carries the advisor name, and the comment line holds the "Advisors" list and the "Synthesis" label as separate fields. Do not duplicate or contradict its edits.