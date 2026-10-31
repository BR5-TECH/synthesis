# Voice mode

**Spec code:** `VOM`

## Intent
Voice mode lets a person talk in a discussion instead of typing. A person who reviews a Draft or any other artifact often thinks faster aloud, and wants to hear an answer while they look at the work. It lives in the one shared composer, so it exists in every discussion. Speech becomes an ordinary user comment and starts agent turns as a typed comment does, so nothing about a discussion is special to voice. The agent's text is always there, and speech is an option on top of it. Out of scope: voice activity detection, whose toggle is visible and disabled; an OS-wide hotkey; review or edit of a transcript before it posts; answering a question set by voice; and speaking a question set.

## Functional requirements

### Entering voice mode

1. **VOM-FR-LAHW** The shared `DiscussionComposer` shows a Voice button in every owner surface (`CVP-conversation-presentation.md` CVP-FR-55).
2. **VOM-FR-UXKL** While no Voice AI provider is active, the Voice button is disabled, and it states why in its accessible description.
3. **VOM-FR-NPLU** Activating the Voice button collapses the text field to the leading edge and leaves a small Expand control. The voice area takes the place of the text field.
4. **VOM-FR-HCMT** The Expand control shows the text field for a time, so the author can type or attach. Collapsing it again keeps the text in the session body.
5. **VOM-FR-JBJI** Activating the Voice button again leaves voice mode. It ends any recording as VOM-FR-ABPU states, and it stops playback.
6. **VOM-FR-OSTM** At most one composer is in voice mode. Entering voice mode in a second composer leaves it in the first.
   - *Why:* one owner decides where the hotkey and spoken replies go.
7. **VOM-FR-LAQO** While voice mode is on and the text field is collapsed, focus goes to the record control, not to the hidden text field (`CVP-conversation-presentation.md` CVP-FR-42).
8. **VOM-FR-SIEO** Entering voice mode calls no provider. Only a recording or a spoken reply calls one.

### Voice area

9. **VOM-FR-TISN** The voice area shows the transcript, a record control, and two small toggles at its trailing side: input mode and response mode.
10. **VOM-FR-IKIK** The input mode toggle offers Push-to-Talk and VAD. VAD is visible, disabled, and states that it comes later.
11. **VOM-FR-IAVZ** The response mode toggle offers Text and Voice. The default is Text. Voice is disabled when the active provider has no speech route, and it states why.
12. **VOM-FR-MQHD** Voice mode, the response mode, and the transcript are session state in memory (`CVP-conversation-presentation.md` CVP-FR-47, CVP-FR-48).

### Recording

13. **VOM-FR-HHWL** With Hold behaviour, recording lasts while the record control or the hotkey is held.
14. **VOM-FR-LIFL** With Toggle behaviour, one press of the record control or the hotkey starts recording, and the next press stops it.
15. **VOM-FR-PECJ** On the focused record control, Space and Enter act as a press, and their release acts as a release.
16. **VOM-FR-YIJS** A press calls `getUserMedia` with `echoCancellation`, `noiseSuppression`, and `autoGainControl`. The microphone is released after recording ends.
    - *Why:* on macOS this capture uses the OS voice processing, which lowers the sound of other applications while it runs.
17. **VOM-FR-FTFU** After the release, capture continues for 200 ms so that the last word is complete.
18. **VOM-FR-DGOG** A hold under 300 ms records nothing and posts nothing.
19. **VOM-FR-WSXW** A recording ends at 10 minutes as a release does.
20. **VOM-FR-XQUY** With Hold behaviour, window blur ends the recording as a release does. With Toggle behaviour, blur has no effect.
    - *Why:* a window that lost focus receives no `keyup`.
21. **VOM-FR-VVTY** A press stops playback at once.

### Hotkey

22. **VOM-FR-WPRW** The hotkey of `VAI-voice-ai-settings.md` VAI-FR-RASA acts on the composer in voice mode, while the main window has focus. It is not an OS-wide hotkey.
23. **VOM-FR-HXLV** The hotkey does nothing while focus is in a text field, including the expanded text field, and during an IME composition.
24. **VOM-FR-DXJT** The hotkey does nothing while a floating overlay other than the composer's own surface is mounted (`SNV-shell-navigation.md` SNV-FR-56).
25. **VOM-FR-FHYY** The hotkey does nothing while the composer in voice mode is not mounted.
26. **VOM-FR-SAMO** A repeated `keydown` of a held hotkey is ignored.

### Transcript

27. **VOM-FR-HUXY** On a live route, the voice area shows the transcript as it changes, while the author records.
28. **VOM-FR-SRGN** On a route that is not live, the voice area shows Recording with a timer. After the release it shows Transcribing, and then the text.
29. **VOM-FR-HSKZ** The record control states its phase in its accessible name. The transcript is not an `aria-live` region while recording.
    - *Why:* a screen reader that reads each partial word over the author's own speech makes dictation impossible.

### Posting

30. **VOM-FR-WYIE** When recording ends, the composer posts the transcript as a normal user comment, through the same path as a typed comment. It dispatches agent turns on the same terms (`CTA-comment-agent-turns.md` CTA-FR-HCMJ). No review step exists.
31. **VOM-FR-RFWC** The comment body is the unsent session body, then the transcript. Pending attachments go with it.
32. **VOM-FR-RENV** An empty transcript posts nothing, and the voice area states that it heard no speech.
33. **VOM-FR-ABPU** A transcript that cannot post now joins the session body and is not posted. The voice area states this. This applies when no composer is mounted, when a post is in flight, when voice mode ended, and when a question set arrived.
    - *Why:* the speech is kept, and the author does not post into a discussion state they did not see.
34. **VOM-FR-TAQJ** At the start of a transcript, `at <nickname>`, `hey <nickname>`, and `at all` become a tag. Matching ignores case and uses the discussion's roster only. Other text stays as spoken.
35. **VOM-FR-BVNX** When a posted transcript reaches no agent, the voice area states this.
36. **VOM-FR-LNUH** A recording sends the roster's nicknames as hints (`../core/VOP-voice-ai-providers.md` VOP-FR-INCC).
37. **VOM-FR-ZUWW** While the composer is disabled, or a question set stands, the record control and the hotkey do nothing (`DQA-discussion-question-answering.md` DQA-FR-PXNC).

### Failure

38. **VOM-FR-ZFSA** A denied or missing microphone, an unreachable provider, or a failed transcription shows an inline error in the composer and posts nothing.
39. **VOM-FR-WICV** After a failure of a provider, the error shows a Retry action. Retry calls "retry voice capture", and a success posts as VOM-FR-WYIE states.
40. **VOM-FR-FASJ** The next press, Retry's success, or leaving voice mode discards the held recording.
41. **VOM-FR-JJMY** A denied microphone message tells the author where to allow it in the operating system settings.

### Spoken replies

42. **VOM-FR-ZLAY** With response mode Voice, a new agent comment in the discussion of the composer in voice mode is spoken.
43. **VOM-FR-ILJW** The set of comment ids at voice mode entry is the baseline. A comment in that set is never spoken. An opening composer takes its baseline from the discussion that its first post creates.
44. **VOM-FR-VSCQ** Only comments with an Agent author are spoken. Comments that a question submission creates are not spoken.
45. **VOM-FR-TRWN** Speech is the text of the comment's Markdown, with citation markers removed. A code block or a table is replaced by a short spoken phrase. A comment with no speakable text is not spoken.
46. **VOM-FR-QOXX** Text is split at sentence ends into segments under 4096 characters, and the first segment is short.
47. **VOM-FR-UALP** Each segment uses the voice of the agent that wrote the comment (`AGR-agent-registry.md` AGR-FR-ABCF), or else the provider's default voice.
48. **VOM-FR-RKOG** A new agent comment stops the current playback at once, and its speech replaces it. No reply queues behind another.
49. **VOM-FR-QDWW** A Stop control is visible while audio plays. It stops playback at once and calls "cancel voice speech".
50. **VOM-FR-SJYH** Frames of one reply play in order and with no gap between them.
51. **VOM-FR-EGNX** When the composer in voice mode unmounts, playback stops, and a new comment is not spoken.
    - *Why:* speech belongs to the visible composer, so a Stop control is always there while audio plays.
52. **VOM-FR-QPFQ** A failed segment stops playback and shows an inline error. The text of the comment is not affected.

### Lifecycle

53. **VOM-FR-KQAI** The surface teardown of `CVP-conversation-presentation.md` CVP-FR-49 ends voice mode, discards any recording, and stops playback.
54. **VOM-FR-WMAP** A `voice-ai-changed` event updates the enabled state of the voice controls in the main window. A recording that has started keeps its configuration.

## Wireframes

```
Text composer
+----------------------------------------------------------+
| [+]  Ask an agent...                        [Voice] [Post] |
+----------------------------------------------------------+

Voice mode, recording on a live route
+----------------------------------------------------------+
| [>] [ (o) ] Check the second paragraph...  [PTT|VAD] [Text|Voice] |
+----------------------------------------------------------+

Voice mode, audio playing
+----------------------------------------------------------+
| [>] [ (o) ] Press to talk  [Stop]  [PTT|VAD] [Text|Voice] |
+----------------------------------------------------------+
```

- Layout notes: the voice area has the geometry of the text field, in all owner surfaces. Its parts stand on one line, and they wrap only where the column is too narrow for that line, as a growing text field does. Its trailing end holds the clearance of `DDS-draft-discussion.md` DDS-FR-QPHL.

## UI contract boundary
- **Owned by the UI**: the voice area, the toggles, the single composer rule, capture through the WebView, the press and hotkey logic, the spoken tag conversion, the conversion of Markdown to speech text, the split into segments, playback, reply detection, and all messages.
- **Delegated to backend (abstract)**: "start voice capture", "append voice capture", "finish voice capture", "retry voice capture", "cancel voice capture", "speak voice segment", "cancel voice speech", "list voice ai integrations", each owned by `../core/VOP-voice-ai-providers.md`.

## Non-functional requirements
- Each voice control is operable from the keyboard and has an accessible name.
- One `AudioContext` serves playback. It is created by the activation of the Voice button, and it resumes before each segment.
- Audio chunks reach the backend in order. One chunk is in flight at a time, and a backlog merges into the next chunk.
- On non-macOS platforms, the standard WebView capture constraints apply.
- Voice mode writes no audio and no transcript to storage, and it logs neither.
