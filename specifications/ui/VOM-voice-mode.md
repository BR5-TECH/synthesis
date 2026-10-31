# Voice mode

**Spec code:** `VOM`

## Intent
Voice mode lets a person talk in a discussion instead of typing. A person who reviews a Draft or any other artifact often thinks faster aloud, and wants to hear an answer while they look at the work. It lives in the one shared composer, so it exists in every discussion. Speech becomes an ordinary user comment and starts agent turns as a typed comment does, so nothing about a discussion is special to voice. The agent's text is always there, and speech is an option on top of it. A VAD session makes this hands-free: the author opens one session, a local detector in the backend finds each utterance, and each utterance posts with no press. Out of scope: voice activity detection on a server or a provider; native microphone capture in the backend; a level meter; a session control separate from the record control; an OS-wide hotkey; review or edit of a transcript before it posts; answering a question set by voice; and speaking a question set.

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
8. **VOM-FR-SIEO** Entering voice mode calls no provider. Only a recording or a spoken reply calls one. A VAD session that only listens calls none.

### Voice area

9. **VOM-FR-TISN** The voice area shows the transcript, a record control, and two small icon toggles at its trailing side: input mode and response mode.
10. **VOM-FR-IKIK** The input mode toggle offers Push-to-Talk and VAD as icon buttons. Each button has an accessible name and a tooltip that state the option, and its selected state shows by more than colour.
11. **VOM-FR-IAVZ** The response mode toggle offers Text and Voice as icon buttons, as VOM-FR-IKIK states. The default is Text. Voice is disabled when the active provider has no speech route, and its tooltip and accessible description state why.
12. **VOM-FR-MQHD** Voice mode, the input mode, the response mode, and the transcript are session state in memory (`CVP-conversation-presentation.md` CVP-FR-47, CVP-FR-48). The input mode starts as Push-to-Talk each time voice mode starts.

### Recording

13. **VOM-FR-HHWL** In Push-to-Talk with Hold behaviour, recording lasts while the record control or the hotkey is held.
14. **VOM-FR-LIFL** In Push-to-Talk with Toggle behaviour, one press of the record control or the hotkey starts recording, and the next press stops it.
15. **VOM-FR-PECJ** On the focused record control, Space and Enter act as a press, and their release acts as a release.
16. **VOM-FR-YIJS** A Push-to-Talk press, or the opening of a VAD session, calls `getUserMedia` with `echoCancellation`, `noiseSuppression`, and `autoGainControl`. The microphone is released after recording ends, or when the VAD session closes.
    - *Why:* on macOS this capture uses the OS voice processing, which lowers the sound of other applications while it runs.
17. **VOM-FR-FTFU** After the release, or after the silence that ends a VAD utterance, capture continues for 200 ms so that the last word is complete.
18. **VOM-FR-DGOG** A hold or VAD speech under 300 ms records nothing and posts nothing.
19. **VOM-FR-WSXW** A recording ends at 10 minutes as a release does. A VAD session stays open after that end.
20. **VOM-FR-XQUY** With Hold behaviour, window blur ends the recording as a release does. With Toggle behaviour or in a VAD session, blur has no effect.
    - *Why:* a window that lost focus receives no `keyup`.
21. **VOM-FR-VVTY** A Push-to-Talk press, or the opening of a VAD session, stops playback at once.

### VAD sessions

21. **VOM-FR-PDZR** In VAD mode, activating the record control by click, Space, or Enter opens a VAD session. Activating it again closes the session. No other control opens a session.
21. **VOM-FR-MMAG** Opening a session opens the microphone (VOM-FR-YIJS), then calls "open voice activity session", then streams the audio with "feed voice activity". The microphone stays open until the session closes.
21. **VOM-FR-CUCN** A denied or missing microphone follows VOM-FR-ZFSA and VOM-FR-JJMY, and no session opens. A failure of "open voice activity session" shows the inline error, opens no session, and releases the microphone.
21. **VOM-FR-BQBI** A session closes when the author activates the record control, selects Push-to-Talk, or leaves voice mode, and when the active provider goes (VOM-FR-WMAP).
21. **VOM-FR-XEIA** Closing during an utterance ends that utterance as a release does, and VOM-FR-ABPU applies. Then the WebView releases the microphone and calls "close voice activity session".
21. **VOM-FR-GZPA** When no composer and no session control of voice mode stays mounted for 2 s, the session closes as VOM-FR-XEIA states.
    - *Why:* the first post of an opening composer moves voice mode to a composer that mounts after the old one unmounts.
21. **VOM-FR-CSXI** A lost microphone track, a refused feed, or a `failed` event with `fatal` true closes the session as VOM-FR-XEIA states, and it shows an inline error.
21. **VOM-FR-VGCE** The VAD option is disabled while a recording, a transcript, or a post is in progress. Push-to-Talk can be selected at any time.
21. **VOM-FR-RTSU** On macOS, while VAD is selected, an info icon near the input mode toggle states in its tooltip and accessible name that other applications can sound lower for the whole session (VOM-FR-YIJS). The icon takes keyboard focus.

### VAD utterances

21. **VOM-FR-NZXN** The backend finds each utterance (`../core/VOP-voice-ai-providers.md` VOP-FR-DICU). The WebView drives the control state, the playback pause, and the posting from `voice-activity-changed` events. It drops an event of another session.
21. **VOM-FR-WRVR** At `speech_confirmed`, the utterance is a recording. The transcript shows as VOM-FR-HUXY and VOM-FR-SRGN state, and a held recording is discarded.
21. **VOM-FR-OTRF** At `speech_end`, the WebView calls "finish voice capture" with the capture id, and the control shows Transcribing. The transcript then follows VOM-FR-WYIE, VOM-FR-RENV, VOM-FR-ABPU, VOM-FR-TAQJ, and VOM-FR-BVNX. The session stays open.
21. **VOM-FR-XWPZ** A `misfire` event records nothing, posts nothing, and shows no note. A held recording stays.
21. **VOM-FR-WWZX** A failed utterance shows the inline error, and VOM-FR-WICV applies when a recording is held. The session stays open.
21. **VOM-FR-KHGC** No utterance starts until the last utterance settles: its transcript posted, joined the session body, came back empty, or failed. Speech in that time is not recorded.
21. **VOM-FR-GTAQ** While the composer is disabled or a question set stands, an open session stays open and starts no utterance. When the block ends, listening continues with no press.
21. **VOM-FR-VGLT** While a question set stands in the discussion of an open session, the surface shows a session control near the line of `DQA-discussion-question-answering.md` DQA-FR-PXNC. It holds the record control with its state, a line that says that the microphone is open and speech is not recorded, the Stop control, and the inline feedback.
    - *Why:* the reply composer unmounts while a set stands, and an open microphone must stay visible and closable.
21. **VOM-FR-GPNK** Activating the record control of the session control closes the session.
21. **VOM-FR-VOUG** In VAD mode, the record control shows one state: Idle (no session), Listening, Hearing speech, Transcribing, or Paused (playback paused by the author's speech). Each state shows by icon and colour, and in the accessible name.

### Barge-in

21. **VOM-FR-FVRS** When `speech_start` arrives while a reply plays, playback pauses at once. The segment is not stopped and not cancelled.
21. **VOM-FR-KVHG** A `misfire` resumes a paused reply from where it paused.
21. **VOM-FR-UNXR** When an utterance settles, a comment that posted drops the paused reply. If it posted nothing, the paused reply resumes from where it paused.
21. **VOM-FR-ZPOD** An agent reply that arrives from `speech_start` until the utterance settles waits. When the utterance settles, the newest such reply speaks, and it replaces a paused reply.

### Hotkey

22. **VOM-FR-WPRW** In Push-to-Talk, the hotkey of `VAI-voice-ai-settings.md` VAI-FR-RASA acts on the composer in voice mode, while the main window has focus. In VAD mode, it does nothing. It is not an OS-wide hotkey.
23. **VOM-FR-HXLV** The hotkey does nothing while focus is in a text field, including the expanded text field, and during an IME composition.
24. **VOM-FR-DXJT** The hotkey does nothing while a floating overlay other than the composer's own surface is mounted (`SNV-shell-navigation.md` SNV-FR-56).
25. **VOM-FR-FHYY** The hotkey does nothing while the composer in voice mode is not mounted.
26. **VOM-FR-SAMO** A repeated `keydown` of a held hotkey is ignored.

### Transcript

27. **VOM-FR-HUXY** On a live route, the voice area shows the transcript as it changes, while the author records.
28. **VOM-FR-SRGN** On a route that is not live, the voice area shows Recording with a timer. After the release or the end of the utterance it shows Transcribing, and then the text.
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
36. **VOM-FR-LNUH** A recording sends the roster's nicknames as hints (`../core/VOP-voice-ai-providers.md` VOP-FR-INCC). A VAD session sends them when it opens and when the roster changes.
37. **VOM-FR-ZUWW** While the composer is disabled, or a question set stands, the record control and the hotkey start nothing (`DQA-discussion-question-answering.md` DQA-FR-PXNC). The record control can still close an open VAD session.

### Failure

38. **VOM-FR-ZFSA** A denied or missing microphone, an unreachable provider, or a failed transcription shows an inline error in the composer and posts nothing.
39. **VOM-FR-WICV** After a failure of a provider, the error shows a Retry action. Retry calls "retry voice capture", and a success posts as VOM-FR-WYIE states.
40. **VOM-FR-FASJ** The next press, the next VAD utterance, Retry's success, or leaving voice mode discards the held recording.
41. **VOM-FR-JJMY** A denied microphone message tells the author where to allow it in the operating system settings.

### Spoken replies

42. **VOM-FR-ZLAY** With response mode Voice, a new agent comment in the discussion of the composer in voice mode is spoken.
43. **VOM-FR-ILJW** The set of comment ids at voice mode entry is the baseline. A comment in that set is never spoken. An opening composer takes its baseline from the discussion that its first post creates.
44. **VOM-FR-VSCQ** Only comments with an Agent author are spoken. Comments that a question submission creates are not spoken.
45. **VOM-FR-TRWN** Speech is the text of the comment's Markdown, with citation markers removed. A code block or a table is replaced by a short spoken phrase. A comment with no speakable text is not spoken.
46. **VOM-FR-QOXX** Text is split at sentence ends into segments under 4096 characters, and the first segment is short.
47. **VOM-FR-UALP** Each segment uses the voice of the agent that wrote the comment (`AGR-agent-registry.md` AGR-FR-ABCF), or else the provider's default voice.
48. **VOM-FR-RKOG** A new agent comment stops the current playback at once, and its speech replaces it. No reply queues behind another, except as VOM-FR-ZPOD states.
49. **VOM-FR-QDWW** A Stop control is visible while audio plays or a reply is paused. It stops playback at once, drops a paused or waiting reply, and calls "cancel voice speech".
50. **VOM-FR-SJYH** Frames of one reply play in order and with no gap between them. A segment ends at its end frame (`../core/VOP-voice-ai-providers.md` VOP-FR-XZET), so no audio frame is lost.
51. **VOM-FR-EGNX** When the composer in voice mode unmounts, playback stops. A new comment is not spoken while no composer and no session control of voice mode is mounted.
    - *Why:* speech belongs to the visible composer, so a Stop control is always there while audio plays.
52. **VOM-FR-QPFQ** A failed segment stops playback and shows an inline error. The text of the comment is not affected.

### Lifecycle

53. **VOM-FR-KQAI** The surface teardown of `CVP-conversation-presentation.md` CVP-FR-49 ends voice mode, closes a VAD session, discards any recording, and stops playback.
54. **VOM-FR-WMAP** A `voice-ai-changed` event updates the enabled state of the voice controls in the main window. A recording that has started keeps its configuration. An event that leaves no active provider closes a VAD session.

## Wireframes

```
Text composer
+----------------------------------------------------------+
| [+]  Ask an agent...                        [Voice] [Post] |
+----------------------------------------------------------+

Voice mode, recording on a live route
+----------------------------------------------------------+
| [>] [ (o) ] Check the second paragraph...  [m|w] [T|s] |
+----------------------------------------------------------+

Voice mode, audio playing
+----------------------------------------------------------+
| [>] [ (o) ] Press to talk  [Stop]  [m|w] [T|s]           |
+----------------------------------------------------------+

Voice mode, VAD session listening
+----------------------------------------------------------+
| [>] [ ((o)) ] Listening                 (i) [m|w] [T|s]  |
+----------------------------------------------------------+

Question set stands, VAD session open
Answer the questions above to continue the discussion.
+----------------------------------------------------------+
| [ ((o)) ] The microphone is open. Speech is not recorded  |
|           until you answer the questions.                 |
+----------------------------------------------------------+
```

- Toggle legend: `m` is Push-to-Talk, `w` is VAD, `T` is Text, and `s` is Voice. Each is an icon button. `(i)` is the info icon of VOM-FR-RTSU.

- Layout notes: the voice area has the geometry of the text field, in all owner surfaces. Its parts stand on one line, and they wrap only where the column is too narrow for that line, as a growing text field does. Its trailing end holds the clearance of `DDS-draft-discussion.md` DDS-FR-QPHL.

## UI contract boundary
- **Owned by the UI**: the voice area, the toggles, the single composer rule, capture through the WebView, the press and hotkey logic, the VAD session lifecycle, the reaction to `voice-activity-changed` events, the spoken tag conversion, the conversion of Markdown to speech text, the split into segments, playback, reply detection, and all messages.
- **Delegated to backend (abstract)**: "start voice capture", "append voice capture", "finish voice capture", "retry voice capture", "cancel voice capture", "speak voice segment", "cancel voice speech", "open voice activity session", "feed voice activity", "set voice activity gate", "close voice activity session", "list voice ai integrations", each owned by `../core/VOP-voice-ai-providers.md`.

## Non-functional requirements
- Each voice control is operable from the keyboard and has an accessible name.
- One `AudioContext` serves playback. It is created by the activation of the Voice button, and it resumes before each segment, except while a reply is paused.
- Audio chunks reach the backend in order. One chunk is in flight at a time, and a backlog merges into the next chunk. A VAD session sends frames of about 30 ms.
- In a VAD session, the microphone stays open for the whole session, and the detector audio is not stored.
- On non-macOS platforms, the standard WebView capture constraints apply.
- Voice mode writes no audio and no transcript to storage, and it logs neither.
