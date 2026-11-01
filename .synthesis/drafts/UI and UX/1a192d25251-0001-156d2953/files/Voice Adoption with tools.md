## Intent

When voice mode is enabled, user would expect all the responses to be audible. Including questions/answers tool. And user would expect that results are concise and fast.

Every agent message to the user carries a short **spoken part**. With response mode Voice, the UI speaks that part and the agent offers to read more of the written text. The user answers the offer by voice, or ignores it. Question sets are spoken one question at a time and are answered by voice. Tools that only feed the agent with data (`search_*`, `list_*`, `read_*`, `get_*`, `measure_document`, `load_skill`) have no user-facing output, so they get no `audible` field and their result shape does not change (`TLC-FR-08`).

Out of scope: a hard character limit on the spoken part; changes to the tool result shapes; a new response mode; Accept/Reject buttons for the offer.

## User journey

1. The author is in voice mode with response mode Voice. The author asks the agent a question by voice.
2. The agent answers. The UI speaks only the spoken part: one or two short sentences. The comment shows the written text without the spoken part.
3. The spoken part ends with an offer to read more. The author says yes (or asks for more) and the answer posts as a normal comment, so the agent reads more. If the author says no or says nothing, nothing happens.
4. The agent needs several answers and records a question set. The UI speaks question 1, then its options. The author starts a recording (record control, or VAD speech) and says the answer. The transcript fills the own-answer field of question 1 ("Something else - I will say").
5. The UI goes to question 2 and speaks it with its options. After the last answer the set submits. The transcripts are in the history as the submission comment.

## Requirements

### Spoken part

- Every agent comment carries one `<spoken>…</spoken>` tag, anywhere in the body. The tag is mandatory. Its content is one or two short sentences (soft rule, no hard cap). This covers the final answer and the comments of `ask_user_comment`, `propose_draft_changes` and `propose_prompt_changes`.
- The loop always asks the model to write the spoken part, whatever the response mode. The UI decides by response mode whether to speak.
- The stored body keeps the tag. The rendered comment hides it. Speech for a comment is the spoken part only, not the whole text (replaces the text rule of `VOM-FR-TRWN`).
- The spoken part says what matters and, where more detail exists in the written text, offers to read it.
- A final comment with no spoken part makes the loop ask the model again. If it still has none, the turn ends as `empty_reply` (`CVL-FR-13`): same retry, same recoverable failure, nothing appended.
- A comment of `ask_user_comment`, `propose_draft_changes` or `propose_prompt_changes` with no spoken part is a tool refusal. The loop continues (`CVL-FR-14`, `CVL-FR-15`).
- `ask_discussion_questions` appends no comment, so it needs no spoken part.
- The loop prompt (compiled in the binary) and any skill text given to the agent must state the rules: the tag, the length, the offer to read more, the refusal when it is missing.

### Question sets

- With response mode Voice, the UI speaks the pending question set from the record, one question at a time: the question, then its options (labels), then waits.
- A record control press, or in a VAD session an utterance, answers the current question. It auto-selects the own-answer row, and the transcript fills the own-answer field. The agent can then align the answer with the question.
- After a spoken answer, the UI goes to the next question and speaks it. After the last answer the set submits with no extra confirmation. Click answers still work.
- In a VAD session, an utterance while a set stands answers the current question and does not post a comment.

### Scope and accessibility

- All speech applies only with response mode Voice. With response mode Text, nothing is spoken and the author reads the comment.
- The Stop control and the playback rules of `VOM-FR-RKOG`, `VOM-FR-QDWW` apply to question speech. A new question or a new comment replaces the speech at once.
- The question text and options stay visible on screen while spoken.

### Specifications to change

- `specifications/ui/VOM-voice-mode.md`: remove "answering a question set by voice" and "speaking a question set" from Out of scope; change `VOM-FR-TRWN`, `VOM-FR-VSCQ`, and add the question-set speech rules.
- `specifications/ui/DQA-discussion-question-answering.md`: spoken answer, auto-advance and submit.
- `specifications/ai/CVL-conversation-loop.md`: prompt rules, retry for a missing spoken part, `CVL-FR-13`.
- `specifications/tools/AUC-ask-user-comment-tool.md`, `PDC-propose-draft-changes-tool.md`, `PPC-propose-prompt-changes-tool.md`: refusal without spoken part.
- The active draft "Voice Activation": VAD behaviour while a question set stands.