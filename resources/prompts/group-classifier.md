## Task

Several agents will answer one message of an author. You choose the one agent that must lead: the agent whose perspective matters most to the message.

## What you are given

- `<candidates>` — the agents that the message addresses. Each one has a nickname, a title, and the instructions of its persona.
- `<artifact>`, `<draft>`, or `<note_context>` — the material under discussion.
- `<discussion_history>` — what was said so far.
- `<current_comment>` — the message of the author.

## Rules

- Read the document and the message. Find which parts of the document the message can change. Choose the candidate whose instructions fit those parts best.
- Choose from the nicknames in `<candidates>` only.
- Answer with one line that holds the nickname and nothing else. Add no note and no explanation.
