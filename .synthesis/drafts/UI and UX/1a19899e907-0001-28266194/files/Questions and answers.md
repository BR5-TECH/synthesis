## Intent

We recently added voice-first experience. few changes required.

### Change 1.

When Agent asks questions using the tools, in voice mode only question should be read. No need to read answers, since user will answer in a free form. at the same time, user should be able to ask for better explanation if needed, and then agent should be able to add more details without breaking the questions...

### Change 2.

I do not see the feature I’ve requested: I do not see live visualization of the transcript when I’m (as the User) talking in voice mode. I only see those on short pauses when I talk. Real-time behavior should be improved.

### Change 3.

I think that with with VAD mode, we shouldn’t need the button “Press to start listening”… Once voice mode enabled - recording/transcription starts automatically. and we visualize this recording (maybe some nice waveform visualization?).

## User journey

1. The author has VAD saved and enters voice mode. A VAD session opens by itself, and a waveform shows that the microphone hears. The author does not press anything.
2. The agent asks a question set. Voice mode speaks the question text only. The options stay on screen.
3. The author answers aloud in free form. The transcript fills the own answer, the block moves to the next question, and the last answer submits the set.
4. The author says "explain option 2". The same speech also goes to the agent as a comment. The agent replies with a comment, and its spoken part is spoken. The question set stays, and the question is not read again.
5. While the author talks, the live text of the open chunk shows at once, not only at short pauses.
6. The author mutes the microphone with the Mute toggle. The session stays open, and the waveform shows the muted state.

## Requirements

### Change 1: question sets by voice

- With response mode Voice, question speech is the question text only. It holds no option label. Citation markers stay removed. Specs: `VOM-voice-mode.md` (VOM-FR-HBLE, VOM-FR-NWGL).
- A spoken answer while a question set stands does both:
  - It answers the shown question as today: own-answer row, move to the next unanswered question, and auto-submit when the last question is answered (DQA-FR-SUUU, DQA-FR-ITCZ, DQA-FR-YMGD stay). Auto-submit by a clarifying question is accepted for now.
  - It posts as one normal user comment through the path of VOM-FR-WYIE, with tagging (VOM-FR-TAQJ, VOM-FR-SBSS) and ordinary routing (`CTA-comment-agent-turns.md` CTA-FR-PXJB). The spoken answer stays not chunked and shows no live bubble.
- An empty or failed spoken answer posts nothing and changes no answer (VOM-FR-MAVW stays).
- The agent turn that this comment starts must reply with a comment. It must not record a second question set, and the standing set stays. The agent instructions must say this.
- The reply speaks its spoken part in response mode Voice, on the terms of VOM-FR-ZPOD and barge-in. The question does not speak again after the reply, because the block did not move (VOM-FR-UALK).
- Specs to change: `VOM-voice-mode.md` (VOM-FR-PINR, VOM-FR-FUFE, VOM-FR-UXYH, VOM-FR-WYIE, VOM-FR-HBLE, VOM-FR-NWGL, Intent); `DQA-discussion-question-answering.md` (Intent: the block takes no other contribution, except the comment of a spoken answer; DQA-FR-SUUU); `../tools/ADQ-ask-discussion-questions-tool.md` and the discussion agent instructions in `../ai/CVL-conversation-loop.md` (no second set while one stands).

### Change 2: live transcript

- On a live route, the live text of the open chunk shows in the live bubble while the author speaks, and it updates as the provider sends text. It shows after the chunk transcripts (VOM-FR-HUXY). It is not limited to short pauses.
- A spoken answer shows its live text in the voice area or the session control (VOM-FR-HUXY).
- A route that is not live is unchanged (VOM-FR-SRGN). Near-live text on such routes is out of scope.
- The live text is not an `aria-live` region (VOM-FR-HSKZ stays).
- This is a defect against VOM-FR-HUXY. Find why the live text does not reach the bubble between chunk ends, and fix it. The spec changes only if it is unclear.

### Change 3: VAD starts by itself

- A VAD session opens with no press in two cases: voice mode is entered with VAD saved, and the author switches to VAD in voice mode. It does not open in other cases. Push-to-Talk is unchanged.
- In VAD mode, the record control ("Press to start listening") is removed. In its place, the voice area and the session control (VOM-FR-VGLT) show a state indicator and a **Mute toggle**. The state indicator keeps the states of VOM-FR-VOUG (by icon, not only colour, and in the accessible name). The status bar indicator is unchanged.
- Mute stops feeding audio to the session and keeps the session open. No utterance starts while muted. The muted state shows by more than colour and in the accessible name, and Mute works with the keyboard. Unmute feeds audio again.
- A **waveform** shows the live microphone level in the voice area and in the session control. It is a level signal from the WebView microphone stream and calls no provider. It is flat while muted and while no session is open. It is decorative: no `aria-live`, and a reduced-motion setting gives a static or a slower display.
- A session closes on: leaving voice mode, selecting Push-to-Talk, the Stop control, a lost provider, and a fatal failure (VOM-FR-BQBI, VOM-FR-CSXI, VOM-FR-XEIA). The record control no longer closes it.
- A denied or missing microphone, or a failed open, shows the inline error and opens no session (VOM-FR-CUCN). A control in the indicator lets the author try to open the session again. No automatic retry loop.
- The waveform needs a level signal. Specs to change: `VOM-voice-mode.md` (Intent: remove "a level meter" from out of scope for this waveform; VOM-FR-KEMU, VOM-FR-PDZR, VOM-FR-BQBI, VOM-FR-VGCE, VOM-FR-GPNK, VOM-FR-VGLT, VOM-FR-VOUG, VOM-FR-TISN, VOM-FR-RTSU as they refer to the record control in VAD); `DQA-discussion-question-answering.md` (DQA-FR-GGKI: focus is not on a record control in VAD); `VAI-voice-ai-settings.md` only if its text mentions the press to start.