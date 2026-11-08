## Intent

Optional advisor mode we’ve added recently should be properly aligned with the Voice mode we have in our IDE. 

## User journey

### Advisor mode or Synthesis by voice

- The author turns on the advisor-mode switch of the Project Agents tab (Multi-persona conversations). The same switch governs text and voice.
- Voice mode is on, response mode is Voice. The author speaks and tags one agent, several agents, or `@all`. Chunks show in the live bubble as today.
- A single tag: Meta starts at the first chunk, as today, and holds its reply while the author speaks. Meta can ask the advisors for advice with a tool. The advice comes when the utterance ends. After the comment posts, the author hears the reply of Meta.
- Several tags or `@all`: no agent starts at the first chunk. When the comment posts, the classifier and the tagged agents start. The author hears nothing yet, and the pending rows show the work. Each tagged agent posts its reply as text. The author hears only the "Synthesis" comment of the driver.
- A later chunk adds a tag to a single-tag utterance: the early Meta turn is cancelled. The several-tag flow runs when the comment posts.

### A long turn

- A turn of the utterance runs longer than 30 s. The author hears one short progress update, for example `Checking existing specifications.`, in the voice of the speaking agent.
- While the turn runs, another update follows every 30 s. After the reply plays, the updates stop.
- The author speaks, presses Stop, or switches to Text. The updates stop at once.

## Requirements

- **Same switch, same flow.** Text and voice use the one switch and one fork-join, classifier, Synthesis, merged-question and merged-proposal flow of Multi-persona conversations. Voice adds no second switch and no voice-only setting.
- **Dispatch timing.** With the switch off, chunk dispatch is as today. With the switch on, the first chunk decides:
  - One tagged agent (Meta): the speech turn starts at the first chunk, and later chunks steer it (VOM-FR-RNBS, VOM-FR-OLFT, VOM-FR-DHUU). Its reply posts after the comment posts.
  - Several tagged agents or `@all`: no speech turn starts and no chunk is sent as steering. The classifier needs the full message. When the comment posts, it dispatches as a typed comment does.
  - Early Meta turn and a final comment that tags several agents (a later chunk added a tag): the early turn is cancelled, and the several-tag flow runs.
  - One tagged agent and no other ready agent: a regular turn with no advisors tool (Multi-persona "Single agent").
- **Advisors tool.** With the switch on and a single tag, the system does not start the advisors by itself. Meta calls an advisors tool, in text and in voice. Meta decides when: its instructions ask for a call at the start of a discussion, and later calls are optional. Meta can skip it. The tool takes Meta's own question. The advisors receive the full message of the author and that question. Advisors run in the fork-join of Multi-persona, and their replies return to Meta as the tool result. In voice, a call made while the utterance is open waits until the comment posts, and then runs. A cancel of the turn ends the wait. The Advisors list and the advisor tool calls show as Multi-persona states.
- **What is spoken.** With the switch on and several tagged agents, only the "Synthesis" comment is spoken. The replies of the tagged agents still post as text and are not spoken. A single tag: only the reply of Meta is spoken. The merged question set is spoken as today, in the voice of the driver. When the classifier fails, or only one tagged agent is ready, no Synthesis exists, and VOM-FR-RKOG applies as today: each reply is spoken and the newest wins.
- **Progress update.** In voice mode with response mode Voice, a turn that the voice discussion runs gets a spoken update 30 s after its utterance ended, and every 30 s after that while it runs. The interval is fixed. It does not depend on the switch, and it does not wait for a change of activity. The update is not a comment and is not saved.
- **Filler.** The filler of VOM-FR-YGAR..UCQN stays as it is. Updates start after it and do not replace it.
- **Text of an update.** A model call writes each update: up to 5 words, no follow-up, no question, and no offer to read more. It is in the `spoken_language` of the turn (AGC-FR-MECW). The input is the current activity only: the active tool calls and, for an advisor call, the advisor name. The call uses the model of the AI API section of Global settings, on the provider that the open project resolves to (as AGC-FR-QEOF does), and no tool. A failed or empty call speaks nothing and shows no error. A log record holds the provider, the model, counts, and the duration, and no text.
- **Voice of an update.** One stream for the whole utterance, in the voice, speed, and style of one agent (VOM-FR-UALP). With the switch on and a driver picked, it is the driver (Meta for a single tag). With no driver, it is the agent of the oldest running turn of the utterance. A running agent never speaks its own update, and no agent speaks more than the one stream.
- **When an update speaks.** It follows VOM-FR-LKXN and VOM-FR-POBZ as the filler does: it waits while the author speaks or an utterance has not settled, and it does not speak while a reply plays or is paused. A due update that cannot speak is dropped, not queued, and the next one is due 30 s later. A barge-in drops a playing update, and it never resumes (VOM-FR-UCQN). A new utterance, Stop, response mode Text, the end of voice mode, and the end or cancel of the last turn of the utterance cancel it. A reply stops a playing update (VOM-FR-RKOG). The indicator shows Speaking while it plays, and Stop stops it.
- **Specifications this work changes.** `VOM-voice-mode.md` (chunk dispatch, spoken replies, long turns); `AGC-agent-conversations.md` (the progress-update call beside "translate voice transcript", and the dispatch of speech turns); `CTA-comment-agent-turns.md` (CTA-FR-QWLB, the agents a speech reaches); `CVL-conversation-loop.md` (speech turns with the switch on; the advisors tool that waits for the end of the speech); `AAP-ai-api-integrations.md` and `AII-ai-integrations.md` (one more reader of the stored AI API model). This draft depends on Multi-persona conversations. That draft must also state the advisors-tool rule above in place of the system start of advisors for a single tag. Also change in `AGC-agent-conversations.md`: the advisors tool.

## Expected deliverables

- Switch on: single tag starts Meta at the first chunk with chunk steering; several tags dispatch at the end of the utterance with no chunk steering; a later tag cancels the early Meta turn.
- Advisors tool for Meta: Meta's question and the full message go to the advisors; a call in an open utterance waits for the comment.
- Voice replies: only Synthesis for several tags, Meta's reply for one tag, and today's rule when no Synthesis exists.
- Progress update: 30 s timer for each utterance, repeating every 30 s, with the cancel and wait rules above.
- Backend call that writes an update of up to 5 words on the AI API model of Global settings, with a log record without text.
- Update speech in the voice of the driver, or of the oldest running agent when no driver exists.
- Specifications updated: all those listed in Requirements.
- Tests for: switch on and off dispatch timing; early Meta for a single tag; cancel of the early turn when a later chunk adds a tag; advisors tool wait during an open utterance and skip by Meta; Synthesis-only speech; classifier failure; the 30 s and repeat timing; the 5-word limit; the drop and cancel cases (author speaks, reply plays, Stop, Text mode, voice off); the filler plus updates; failed model call.