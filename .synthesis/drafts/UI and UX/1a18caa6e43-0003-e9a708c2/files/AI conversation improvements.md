## Intent

### Change 1

Today we have “@agent thinking…” messaging when there’s a turn. we need to show more details inside expandable list. so user should be able to click on the row, and it would expand to show what were the tools called, what were the documents/artifacts retrieved, what was searched in the web.

The intent is to get user a clear view of what the agent is doing.

### Change 2.

When agent turn starts, it might take some time.  Under the text area we should show a progress bar that kind of shows the time till timeout. I.e. if we allow 300s - we should be showing how the time goes showing the progress. but it should be reverse: going from 100% to 0%. from right to left.

### Change 3.

when during agentic turn user adds some message - this message should be added to the agent’s information in-between the calls, if possible, so agent takes the new input into account. so we steer the conversation.

## User journey

- **Watch the agent work.** The author asks an agent a question. The row `@agent thinking…` appears. The author clicks the row. The row expands and lists each tool the agent called, with its key input. The author sees what the agent reads and searches.
- **See the time left.** While the agent works, the author looks under the text area. A bar shows the time left before the turn times out. The bar empties from right to left.
- **Steer the agent.** The agent is working. The author posts a new message that addresses the same agent. The agent reads the message before its next model call and changes course. The bar fills again.
- **Review after the turn.** The agent delivers its comment. The expandable row stays at the foot of the discussion until the next turn starts in that discussion.

## Requirements

### Change 1: Expandable tool list

- The `@agent thinking…` row (and its status text per CTA-FR-IWOJ…TFGR) is expandable by click and by keyboard (Enter/Space), with `aria-expanded`. It is collapsed by default.
- The expanded list shows one entry per tool call of the turn, in activation order (AGC-FR-33). Each entry shows the **tool name** and its **key input**, and its state (running, done, failed). Web searches, web fetches, and artifact/specification/document reads are all entries. No tool result, no result title, and no model text is shown.
- The key input is the one argument that identifies the call: the query for a web search, the address for a web fetch, the path or id for a read. The backend picks it per tool. It is cut at **200 characters**, with an ellipsis. The UI shows it as plain text, wrapped, never as Markdown or HTML.
- The list lives in the session store only (CVP-FR-47). It is not persisted, and it is lost on application restart. A turn with no tool call shows no expand control.
- While a turn runs, the list is part of its pending row. When the turn ends, the list moves to a row at the **foot of the discussion**. It stays there until the next turn starts in that discussion, then it is replaced. Failed and cancelled turns keep their list on the same terms.
- Several turns that run at once each show their own row and list.
- The key input is for the UI only. Log records still carry no argument, no query, and no result (CVL-FR-28, CVL-FR-34, TLC-FR-14).

### Change 2: Timeout bar

- A thin progress bar shows directly under the text area (reply field) of the discussion column. It shows only while at least one turn of the shown discussion is running.
- The bar shows the **whole-turn deadline** (CVL-FR-16). It starts when the turn holds its slot (AGC-FR-24, AGC-FR-25). A turn that waits for a slot shows no bar. The bar goes from 100% to 0%, and it empties from right to left.
- With several running turns, the bar shows the turn with the **most time left**.
- The backend sends the turn's start time and its deadline in `"agent turn state changed"` (or in the turn record). The UI draws the bar from these values on a local clock. The backend sends no tick events. A new deadline replaces the old one at once.
- The bar has `role="progressbar"` with an accessible name and values. It does not announce each tick. It uses no animation beyond the width change when the user prefers reduced motion. The bar blocks nothing.
- The bar goes away when no turn of the discussion is running.

### Change 3: Steering

- A human comment posted while a turn is running, and addressed to the agent of that turn (by the routing rules of CTA-FR-QUXJ…QNBS), is **injected into that running turn** and does **not** dispatch a new turn for that agent. Other agents it addresses are dispatched as today. A comment never goes into a turn of another agent.
- The loop adds the message to the exchange between model calls, so the next `complete` call sees it. A turn that waits for a slot reads it before its first call.
- The comment is stored and shown as an ordinary comment, in the same way as today.
- If the turn cannot take the message (it is on its last model call, or it ended first), the message **falls back to a new turn** for that agent, as today. The author sees no error.
- A taken steering message **restarts the full whole-turn deadline** (CVL-FR-16). The turn sends the new deadline to the UI, and the bar fills to 100%.
- Cancel, Retry, and the rules for `awaiting_reply` turns do not change. A turn in `awaiting_reply` is not running, so a reply to it dispatches a new turn as today.

### Specifications to change

- `specifications/ai/CVL-conversation-loop.md`: CVL-FR-26, CVL-FR-27, CVL-FR-33 (a tool-call report carries the key input, cut at 200 characters, to the discussion surface); CVL-FR-34 and CVL-FR-28 stay unchanged (logs); CVL-FR-12 and CVL-FR-16 (injection between calls, deadline restart, deadline in the turn state).
- `specifications/ui/CTA-comment-agent-turns.md`: CTA-FR-VRJU (status text stays fixed; the list is the new surface), new requirements for the expandable row and its place at the discussion foot, and the dispatch rule that starts a new turn for a comment posted to a running turn (CTA-FR-ZVKL and the rules around it).
- `specifications/core/AGC-agent-conversations.md`: AGC-FR-01 (a turn may take up more than one message), AGC-FR-03 (a message to a running turn is injected, not dispatched), AGC-FR-33/34 (tool events carry the key input), the turn state event (start time and deadline).
- `specifications/ui/DDS-draft-discussion.md` and `specifications/ui/CVP-conversation-presentation.md`: the bar under the reply field, and the tool list in the session store (CVP-FR-47) for every owner surface (CTA-FR-MWPE).