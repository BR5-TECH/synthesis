## Intent

The process of draft authoring and the discussion is very focused on text at this moment.

This is a valid framework and it clearly works. However given the support we’ve baked in for voice AI, we need to provide additional voice-first experience for draft authoring first and eventually the whole IDE experience.

Voice mode (`VOM-voice-mode.md`: Push-to-Talk, VAD, spoken part, spoken replies, spoken questions) already exists. This work lets the author use voice end to end on drafts, published prompts, and project documents: talk, hear the answer, and decide the agent's proposed changes by voice. The discussion stays the log of the work, so voice adds no new conversation surface.

The experience must feel like a human collaboration in real time. Anything that can delay the answer to the author (transcription, context gathering, tool calls) runs in the background, off the interaction path. The agent gets the author's speech in sentence chunks while the author still speaks, so its reply is ready soon after the author stops.

## User journey

### Creating a new draft

- User starts a new draft 
- User enables voice mode
- User interactively discusses the intent of the draft, stories there etc. 
- Agent responds to the user helping to shape the document. In background thread agent runs tool requests as needed to improve its context.
- As discussion goes, the documents gets updated filling more and more details iteratively.
- Once user considers the draft is good, he can ask agent to do a full read-through. In this case agent will re-validate the alignment of the new requirements with the existing specifications, asessing the blast radius etc.
- The user starts the draft with the existing action. Starting a draft by voice is later work. The user may switch input to VAD to talk hands-free.
- While voice mode is on, the user may open another draft, tab, or document to look something up. Voice mode stays with the discussion where it started.

### Modifying an existng artifact

- User opens an artifacts
- User enables voice mode.
- User tells the agent what he wants to be improved/changed/modified.
- Agent either propose the changes requested by user, or asks additional questions...
- User says which proposed change to accept, reject, or discuss, for example "accept the second change". The agent decides it with a tool and tells the user.
- User says "Helga, ..." without "at". The IDE sends the comment to that agent.

## Requirements

**Remembered modes**
- The input mode (Push-to-Talk or VAD) and the response mode (Text or Voice) are saved in the app preferences, for the whole app. They survive restarts, and a project or worktree switch. They are the only voice state saved.
- Voice mode itself is not saved. A project or worktree switch, and an app restart, still end it (CVP-FR-49, VOM-FR-KQAI). The rule of VOM-FR-MQHD that the input mode starts as Push-to-Talk is removed.
- Entering voice mode applies the saved modes. With saved VAD, no session opens until the user activates the record control (VOM-FR-PDZR). With saved Voice and no speech route, the effective response mode is Text, and the saved value stays.
- A change of either toggle saves at once.

**One voice discussion, kept while the user navigates**
- At most one discussion is in voice mode (VOM-FR-OSTM stays). Voice mode is bound to that discussion, not to a mounted composer.
- Opening another draft, tab, or document, and closing the tab that owns the discussion, do not end voice mode. A VAD session, the hotkey, spoken replies, and spoken questions keep working for that discussion. A transcript posts into it with no composer mounted, as a typed comment does. A spoken answer to a pending question set still works.
- VOM-FR-GZPA (2 s close) and VOM-FR-EGNX (stop on unmount) no longer apply. VOM-FR-ABPU no longer holds a transcript only because no composer is mounted.
- Voice mode ends on: Stop, the Voice button of its composer, leaving the app's project or worktree (CVP-FR-49), and deletion of the discussion's owner (CVP-FR-51). The Voice button of another composer moves voice mode to it.

**Status bar voice indicator**
- While voice mode is on, the status bar shows a voice indicator in its trailing region, on every tab. It does not displace another region (STB-FR-03).
- It names the discussion and shows the state: Idle, Listening, Hearing speech, Transcribing, Paused, or Speaking. Each state shows by more than colour.
- Activating the indicator opens the discussion (`revealDiscussion`, CVP-FR-54). It does not stop anything.
- A separate **Stop** control ends voice mode: it closes a VAD session, discards a recording, and stops playback.
- Both controls work by keyboard and carry accessible names.

**Agent name**
- A transcript that starts with a bare roster nickname ("Helga, check this") becomes a tag for that agent. The forms `at <nickname>`, `hey <nickname>`, and `at all` stay.
- Matching ignores case and uses the discussion's roster only. The first word is matched exactly, then by a fuzzy match (for example edit distance or phonetic match) on the same roster. A fuzzy match counts only when one nickname is clearly the closest. A tie or a weak match gives no tag, and the text stays as spoken. Nicknames of 3 characters or less match exactly only.
- The tag replaces the spoken name. The posted comment shows the tag, so the user sees which agent was reached. A spoken answer to a question set is not tagged (VOM-FR-FUFE).

**Real-time speech to the agent**
- In Push-to-Talk and VAD, an utterance splits into **chunks** of one or two sentences. A chunk ends at a short pause that follows a sentence end. The short pause is shorter than the VAD silence timeout, which still ends the whole utterance. Each chunk is transcribed when it ends, in the background.
- Listening does not wait for transcription. The next chunk and the next utterance record while earlier chunks transcribe. Order is kept. VOM-FR-KHGC no longer applies.
- A spoken answer to a pending question set is not chunked. It records as one utterance and posts at its end (VOM-FR-PINR). It shows no live bubble and starts no agent turn.
- **Live bubble:** while chunks arrive, the discussion shows one local bubble at the end of the log. It joins the chunk transcripts as they come. It is not a comment, is not saved, and is shown in the discussion that owns voice mode. At the end of the utterance the bubble is replaced by one ordinary comment with the full transcript (VOM-FR-WYIE, agent name rule above). Comments stay immutable (CMT-FR-08).
- **Early turn:** the first transcribed chunk starts the agent turn. The agent is chosen as for a typed comment of that text, with the agent name rule applied to the first chunk. If that agent has a running turn in the discussion, the chunk steers it (AGC-FR-ISKZ) and starts no new turn. The author's speech during a running turn never cancels it.
- **Ephemeral chunk input:** each later chunk reaches the turn as a steering message that is not a comment. A fixed line marks it as partial speech of an author who still speaks. It is held in memory only and is never saved. At the end of the utterance, the posted comment reaches the turn as the final steering message. It states that it replaces the chunks.
- **Held reply:** while the author speaks, or chunks wait for transcription, the turn does not deliver a reply. The loop runs again with the final speech (CVL-FR-XXKL). The reply posts after speech ends, and the spoken part plays after it posts. Speech output is not streamed (VOM stays as it is). When no comment posts, a held reply posts as it stands.
- **Barge-in:** when the author starts to speak, in either input mode, playback of a spoken reply stops at once. The resume and drop rules of VOM stay.
- **Failed chunk:** a chunk that fails to transcribe is skipped. The bubble shows an inline error at its place and later chunks continue. The comment holds the chunks that succeeded. When no chunk succeeds, VOM-FR-WWZX applies.
- **No comment posts** (misfire, Stop, discarded recording, no transcript): a turn that the chunks started is cancelled and its output is dropped. A turn that already ran before the first chunk is not cancelled, and the steering messages of the chunks are dropped.
- The status bar indicator and the record control show the state during chunking (Hearing speech, Transcribing). A chunk in transcription shows Transcribing while the microphone stays open.

**Decisions by tool**
- Two new agent tools let an agent **accept**, **reject**, or **discuss** a hunk of the open proposal: one for a draft's prompt (bound to that draft, like `PDC-propose-draft-changes-tool.md`) and one for a published prompt or a project document (bound like `PPC-propose-prompt-changes-tool.md`). Each has its own tool specification. They are available in every discussion, typed or spoken.
- A call carries a list of entries: a hunk id and an action. Each entry runs on its own, in order, with the same operation as the matching chip of `DCR-draft-change-review.md`: accept writes the hunk's text as it stands, including the author's edit, and records one version; reject writes nothing; discuss does what the Discuss chip does. The result says what happened to each entry.
- The tool refuses an entry for an unknown hunk, a `lost` hunk, an accepted hunk, a hunk already in the asked state, and any call where no proposal is open. The refusal is retryable and names the entry by its place.
- The agent calls the tool only on an explicit instruction of the author in the latest comment. It does not read back and wait for a confirmation. The tool description states this rule.
- The agent can read each hunk of the open proposal (id, order, state, text), so it can map "the second change" to a hunk. If today's agent input does not hold this, add it.
- The decision comment, the notification retraction, the proposal close, and the statistics line follow the same rules as a decision by chip.
- The tools never create, update, or withdraw a hunk. That belongs to the `Update open proposal` draft.

**Read-through**
- No new control and no new tool. The spoken phrase is an ordinary comment, and the agent follows it as it does a typed one.

## Out of Scope

- Full conversational IDE experience.
- Starting a draft by voice, and a voice-first entry on any other surface.
- More than one voice discussion at a time. A single global discussion mechanism is later work.
- Saving voice mode on, or an open VAD session, across restarts or a project switch.
- A floating voice control, a button for the read-through, and new UI command phrases for decisions.
- A change to the hunk review, to hunk states, or to how proposals are created or updated.
- Streaming of spoken replies (text to speech starts only after the full comment posts).
- Chunking of spoken answers to a question set.
- Posting of partial comments, or editing of a posted comment.

## Specification impact

This work changes, and must update in the same change:
- `specifications/ui/VOM-voice-mode.md`: VOM-FR-MQHD, VOM-FR-GZPA, VOM-FR-EGNX, VOM-FR-ABPU, VOM-FR-OSTM, VOM-FR-TAQJ, VOM-FR-KQAI, and the session control rules that assume a mounted composer.
- `specifications/ui/CVP-conversation-presentation.md`: CVP-FR-47 and CVP-FR-48 (the two modes are saved), CVP-FR-49 (teardown stays).
- `specifications/ui/STB-status-bar.md`: the voice indicator.
- `specifications/ui/GLS-global-settings.md` (GLS-FR-14 list of carried fields) and `specifications/core/GSS-global-settings-storage.md`: the two saved modes in the app preferences record.
- `specifications/core/DCP-draft-change-proposals.md` and `specifications/core/PCP-prompt-change-proposals.md`: the Intent and Out of scope rule that nothing decides a proposal on the author's behalf now allows a decision by tool on the author's explicit instruction. Also `specifications/ui/DCR-draft-change-review.md` and `specifications/ui/PCR-prompt-change-review.md` where they name the author as the only decider.
- Two new tool specifications in `specifications/tools/`, and the tool lists of `specifications/ai/CVL-conversation-loop.md` (CVL-FR-08) and `specifications/tools/TLC-tool-conventions.md`.
- `specifications/core/AGC-agent-conversations.md`: the agent input holds each hunk's id and state, if it does not now.
- Align with the drafts `Update open proposal` (hunk states, `updates`) and `Partial rejections` (the Discuss model).
- For real-time speech: `specifications/ui/VOM-voice-mode.md` also changes VOM-FR-KHGC, VOM-FR-NZXN, VOM-FR-WRVR, VOM-FR-OTRF, VOM-FR-WYIE, VOM-FR-PINR, and the playback pause rules. Add the live bubble and the held reply. `specifications/core/VOP-voice-ai-providers.md` changes the utterance events (VOP-FR-DICU) to add chunk end and chunk transcript. `specifications/core/AGC-agent-conversations.md` adds the ephemeral chunk input and the early turn start to the dispatch rules (AGC-FR-ISKZ, AGC-FR-CEKN). `specifications/ai/CVL-conversation-loop.md` adds the ephemeral steering message to CVL-FR-EZKC, CVL-FR-QPEL, and CVL-FR-XXKL. `specifications/ui/CTA-comment-agent-turns.md` and `specifications/ui/CVP-conversation-presentation.md` add the live bubble and the held reply to the discussion surface. The immutability rule CMT-FR-08 does not change.