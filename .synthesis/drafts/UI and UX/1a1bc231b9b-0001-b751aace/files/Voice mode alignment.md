## Intent

Optional advisor mode we’ve added recently should be properly aligned with the Voice mode we have in our IDE. 

## User journey

### Advisor mode or Synthesis by voice

- The author turns on the advisor-mode switch of the Project Agents tab (Multi-persona conversations). The same switch governs text and voice.
- Voice mode is on, response mode is Voice. The author speaks and tags one agent, several agents, or `@all`. Chunks show in the live bubble as today.
- When the utterance ends and its comment posts, the advisors or the classifier start. The author hears nothing yet, and the pending rows show the work.
- A single tag: the tagged agent answers as Meta. The author hears its reply.
- Several tags: each tagged agent posts its reply as text. The author hears only the "Synthesis" comment of the driver.

### A long turn

- A turn of the utterance runs longer than 30 s. The author hears one short progress update, for example `Checking existing specifications.`, in the voice of the speaking agent.
- While the turn runs, another update follows every 30 s. After the reply plays, the updates stop.
- The author speaks, presses Stop, or switches to Text. The updates stop at once.

## Requirements

- **Same switch, same flow.** Text and voice use the one switch and one fork-join, classifier, Synthesis, merged-question and merged-proposal flow of Multi-persona conversations. Voice adds no second switch and no voice-only setting.
- **Dispatch timing.** With the switch on, voice dispatches no speech turn at the first chunk and sends no chunk as steering (VOM-FR-RNBS, VOM-FR-OLFT). Advisors and the classifier need the full message. When the comment of the utterance posts, it dispatches as a typed comment does. This holds for a single dispatched agent too (regular turn, started at the end). With the switch off, chunk dispatch is as today.
- **What is spoken.** With the switch on and several tagged agents, only the "Synthesis" comment is spoken. The replies of the tagged agents still post as text and are not spoken. A single tag: only the reply of Meta is spoken. The merged question set is spoken as today, in the voice of the driver. When the classifier fails, or only one tagged agent is ready, no Synthesis exists, and VOM-FR-RKOG applies as today: each reply is spoken and the newest wins.
- **Progress update.** In voice mode with response mode Voice, a turn that the voice discussion runs gets a spoken update 30 s after its utterance ended, and every 30 s after that while it runs. The interval is fixed. It does not depend on the switch, and it does not wait for a change of activity. The update is not a comment and is not saved.
- **Filler.** The filler of VOM-FR-YGAR..UCQN stays as it is. Updates start after it and do not replace it.
- **Text of an update.** A model call writes each update: up to 5 words, no follow-up, no question, and no offer to read more. It is in the `spoken_language` of the turn (AGC-FR-MECW). The input is the current activity only: the active tool calls and, for an advisor call, the advisor name. The call uses the model of the AI API section of Global settings, on the provider that the open project resolves to (as AGC-FR-QEOF does), and no tool. A failed or empty call speaks nothing and shows no error. A log record holds the provider, the model, counts, and the duration, and no text.
- **Voice of an update.** One stream for the whole utterance, in the voice, speed, and style of one agent (VOM-FR-UALP). With the switch on and a driver picked, it is the driver (Meta for a single tag). With no driver, it is the agent of the oldest running turn of the utterance. A running agent never speaks its own update, and no agent speaks more than the one stream.
- **When an update speaks.** It follows VOM-FR-LKXN and VOM-FR-POBZ as the filler does: it waits while the author speaks or an utterance has not settled, and it does not speak while a reply plays or is paused. A due update that cannot speak is dropped, not queued, and the next one is due 30 s later. A barge-in drops a playing update, and it never resumes (VOM-FR-UCQN). A new utterance, Stop, response mode Text, the end of voice mode, and the end or cancel of the last turn of the utterance cancel it. A reply stops a playing update (VOM-FR-RKOG). The indicator shows Speaking while it plays, and Stop stops it.
- **Specifications this work changes.** `VOM-voice-mode.md` (chunk dispatch, spoken replies, long turns); `AGC-agent-conversations.md` (the progress-update call beside "translate voice transcript", and the dispatch of speech turns); `CTA-comment-agent-turns.md` (CTA-FR-QWLB, the agents a speech reaches); `CVL-conversation-loop.md` (speech turns with the switch on); `AAP-ai-api-integrations.md` and `AII-ai-integrations.md` (one more reader of the stored AI API model). This draft depends on Multi-persona conversations.

## Expected deliverables

- Switch on: voice dispatch at the end of the utterance, no chunk steering.
- Voice replies: only Synthesis for several tags, Meta's reply for one tag, and today's rule when no Synthesis exists.
- Progress update: 30 s timer for each utterance, repeating every 30 s, with the cancel and wait rules above.
- Backend call that writes an update of up to 5 words on the AI API model of Global settings, with a log record without text.
- Update speech in the voice of the driver, or of the oldest running agent when no driver exists.
- Specifications updated: all those listed in Requirements.
- Tests for: switch on and off dispatch timing; Synthesis-only speech; classifier failure; the 30 s and repeat timing; the 5-word limit; the drop and cancel cases (author speaks, reply plays, Stop, Text mode, voice off); the filler plus updates; failed model call.