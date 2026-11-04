# Voice mode

**Spec code:** `VOM`

## Intent
Voice mode lets a person talk in a discussion instead of typing, on a draft, a published prompt, or a project document. A person who shapes or reviews a document often thinks faster aloud, and wants to hear an answer while they look at the work. The author talks, hears the answer, and decides the agent's proposed changes by voice. It lives in the one shared composer, so it exists in every discussion. Speech becomes an ordinary user comment and starts agent turns as a typed comment does, so the discussion stays the log of the work and voice adds no new conversation surface. The experience feels like a real-time collaboration with a human: the agent gets the author's speech in chunks of one or two sentences while the author still speaks, and transcription, context gathering, and tool calls run in the background, off the interaction path, so the reply is ready soon after the author stops. Voice mode belongs to one discussion, not to a visible composer, so the author can open another draft, tab, or document to look something up, and the conversation continues there. A status bar indicator shows where voice mode is and stops it. The input mode and the response mode are app preferences, so the author sets them one time. The agent's text is always there, and speech is an option on top of it. Speech is short, so an answer stays fast to hear: each agent comment carries a spoken part of one or two sentences, and where the written text holds more, that part offers to read it. Voice mode speaks the question text of a question set, one question at a time, and the author answers aloud in free form. A spoken answer also reaches the agent as a comment, so the author can ask about the options, and the agent replies while the set stands. A VAD session makes this hands-free: it opens by itself in VAD mode, a waveform shows that the microphone hears, a local detector in the backend finds each utterance, and each utterance posts with no press. A Mute toggle stops the audio and keeps the session open. Out of scope: a full conversational IDE; starting a draft by voice, and a voice-first entry on any other surface; more than one voice discussion at a time; saving voice mode or an open VAD session across a restart or a project switch; a floating voice control; a control for a read-through and command phrases for decisions; streamed speech of a reply; chunks of a spoken answer; a partial comment, and an edit of a posted comment; voice activity detection on a server or a provider; native microphone capture in the backend; a level meter other than the waveform; near-live text on a route that is not live; an automatic retry of a VAD session that did not open; an OS-wide hotkey; review or edit of a transcript before it posts; a hard limit on the length of the spoken part; a response mode other than Text and Voice; and Accept or Reject controls for the offer to read more.

## Functional requirements

### Entering voice mode

1. **VOM-FR-LAHW** The shared `DiscussionComposer` shows a Voice button in every owner surface (`CVP-conversation-presentation.md` CVP-FR-55).
2. **VOM-FR-UXKL** While no Voice AI provider is active, the Voice button is disabled, and it states why in its accessible description.
3. **VOM-FR-NPLU** Activating the Voice button collapses the text field to the leading edge and leaves a small Expand control. The voice area takes the place of the text field.
4. **VOM-FR-HCMT** The Expand control shows the text field for a time, so the author can type or attach. Collapsing it again keeps the text in the session body.
5. **VOM-FR-JBJI** Activating the Voice button of a composer of the voice discussion leaves voice mode. It ends any recording as VOM-FR-ABPU states, closes a VAD session, and stops playback. Speech of an open chunked utterance no longer reaches an agent: the WebView calls "end speech turn" with no comment id at once (VOM-FR-REJN), a chunk transcript that arrives later steers nothing and starts nothing, and the transcript joins the session body (VOM-FR-ABPU).
6. **VOM-FR-OSTM** At most one discussion is in voice mode, the **voice discussion**. Activating the Voice button in a composer of another discussion leaves voice mode in the first discussion as VOM-FR-JBJI states, then enters it in the second.
   - *Why:* one discussion decides where the hotkey, the transcripts, and the spoken replies go.
7. **VOM-FR-LAQO** While voice mode is on and the text field is collapsed, focus goes to the record control in Push-to-Talk and to the Mute toggle in VAD mode, not to the hidden text field (`CVP-conversation-presentation.md` CVP-FR-42).
8. **VOM-FR-SIEO** Entering voice mode calls no provider. Only a recording or a spoken reply calls one. A VAD session that only listens calls none.
9. **VOM-FR-KEMU** Entering voice mode applies the saved input mode and response mode (`../core/GSS-global-settings-storage.md` GSS-FR-WYHC). A change of either toggle saves at once through "save app preferences". With saved Voice and no speech route, the effective response mode is Text, and the saved value stays. With saved VAD, a VAD session opens at entry with no press (VOM-FR-PDZR).

### The voice discussion

10. **VOM-FR-BCRD** Voice mode is bound to the voice discussion, not to a mounted composer. Opening another draft, tab, or document, and closing the tab of the discussion's owner, do not end it. A VAD session, the hotkey, spoken replies, spoken questions, posting, and spoken answers keep working while no composer of that discussion is mounted.
11. **VOM-FR-LSBQ** Voice mode ends only on the Stop control of the status bar voice indicator (`STB-status-bar.md` STB-FR-JHZF), the Voice button of a composer of the voice discussion (VOM-FR-JBJI), the teardown of `CVP-conversation-presentation.md` CVP-FR-49 (VOM-FR-KQAI), the resolution of the voice discussion (VOM-FR-HRFL), and the deletion of the discussion's owner (`CVP-conversation-presentation.md` CVP-FR-51): its draft, its note (`CVP-conversation-presentation.md` CVP-FR-61), or its project file, a project document or a published prompt, deleted from the Library alone or with a folder that holds it (`LCM-library-context-menu.md` LCM-FR-11). The Voice button of another composer moves voice mode (VOM-FR-OSTM).
11. **VOM-FR-HRFL** When the voice discussion becomes resolved, a legacy lock included (`../core/CMS-comments-storage.md` CMS-FR-WKZR), voice mode ends. It discards what is in progress as the deletion of the owner does, and it shows the toast `Voice mode ended: the discussion is resolved` (`NTF-notifications.md`). The graduation of the owner draft does not end voice mode.
12. **VOM-FR-NERG** An opening composer that enters voice mode binds voice mode to its target key, and its first post binds voice mode to the discussion that the post creates. Until that post, its utterances are not chunked and start no early turn.
    - *Why:* a speech turn needs a discussion as its origin, and an opening composer has none.
13. **VOM-FR-XDUY** While voice mode is on, the status bar shows the voice indicator (`STB-status-bar.md` STB-FR-OSOL). Voice mode gives it the display name of the voice discussion's owner and one state: Paused, Hearing speech, Transcribing, Speaking, Listening, or Idle, in that order of precedence.
14. **VOM-FR-UXZT** The Stop control of the status bar voice indicator leaves voice mode. It closes a VAD session with `discard`, discards a recording and its chunks, drops the live bubble, stops playback, and calls "cancel voice speech". VOM-FR-REJN applies to the discarded utterance.

### Voice area

15. **VOM-FR-TISN** The voice area shows the transcript and two small icon toggles at its trailing side: input mode and response mode. In Push-to-Talk it shows the record control. In VAD mode it shows the VAD indicator (VOM-FR-VOUG), the Mute toggle (VOM-FR-QKTE), and the waveform (VOM-FR-WFHN) in place of the record control.
16. **VOM-FR-IKIK** The input mode toggle offers Push-to-Talk and VAD as icon buttons. Each button has an accessible name and a tooltip that state the option, and its selected state shows by more than colour.
17. **VOM-FR-IAVZ** The response mode toggle offers Text and Voice as icon buttons, as VOM-FR-IKIK states. The default is Text. Voice is disabled when the active provider has no speech route, and its tooltip and accessible description state why.
18. **VOM-FR-MQHD** Voice mode, the voice discussion, the live bubble, and the transcript are session state in memory (`CVP-conversation-presentation.md` CVP-FR-47, CVP-FR-48), so an application restart starts with voice mode off. The input mode and the response mode are app preferences of the whole application (`../core/GSS-global-settings-storage.md` GSS-FR-WYHC).

### Recording

19. **VOM-FR-HHWL** In Push-to-Talk with Hold behaviour, recording lasts while the record control or the hotkey is held.
19. **VOM-FR-FZJY** A pointer or key release anywhere in the window, a lost pointer capture, and a record control that becomes disabled or unmounts during a hold end the recording as a release does.
20. **VOM-FR-LIFL** In Push-to-Talk with Toggle behaviour, one press of the record control or the hotkey starts recording, and the next press stops it.
21. **VOM-FR-PECJ** On the focused record control, Space and Enter act as a press, and their release acts as a release.
22. **VOM-FR-YIJS** A Push-to-Talk press, or the opening of a VAD session, calls `getUserMedia` with `echoCancellation`, `noiseSuppression`, and `autoGainControl`. The microphone is released after recording ends, or when the VAD session closes.
22. **VOM-FR-NXEB** A lost microphone track during a Push-to-Talk recording ends the recording as a release does, and the voice area shows the inline error `microphone_unavailable`.
22. **VOM-FR-ONLA** A leave of voice mode, or a move to another discussion, before the microphone and the capture of a press are open discards that recording. No capture starts for it, and the phase of the new state does not change.
22. **VOM-FR-JMMF** A Push-to-Talk press waits until a closing VAD microphone is released before it opens its own microphone.
22. **VOM-FR-LDKK** When the audio worklet does not load or the audio context does not resume within 10 seconds after `getUserMedia` resolves, the open fails with `microphone_unavailable` and the microphone is released.
    - *Why:* on macOS this capture uses the OS voice processing, which lowers the sound of other applications while it runs.
23. **VOM-FR-FTFU** After the release, or after the silence that ends a VAD utterance, capture continues for 200 ms so that the last word is complete.
24. **VOM-FR-DGOG** A hold or VAD speech under 300 ms records nothing and posts nothing.
25. **VOM-FR-WSXW** A recording ends at 10 minutes as a release does. A VAD session stays open after that end.
26. **VOM-FR-XQUY** With Hold behaviour, window blur ends the recording as a release does. With Toggle behaviour or in a VAD session, blur has no effect.
    - *Why:* a window that lost focus receives no `keyup`.
27. **VOM-FR-VVTY** A Push-to-Talk press, or the opening of a VAD session, stops playback at once.

### VAD sessions

28. **VOM-FR-PDZR** A VAD session opens with no press when voice mode is entered with saved VAD, and when the author selects VAD while voice mode is on. The Open again control of VOM-FR-CUCN also opens one. Nothing else opens a session. VAD mode has no record control.
29. **VOM-FR-MMAG** Opening a session opens the microphone (VOM-FR-YIJS), then calls "open voice activity session", then streams the audio with "feed voice activity". The microphone stays open until the session closes.
30. **VOM-FR-CUCN** A denied or missing microphone follows VOM-FR-ZFSA and VOM-FR-JJMY, and no session opens. A failure of "open voice activity session" shows the inline error, opens no session, and releases the microphone. While no session is open in VAD mode, the VAD indicator shows an Open again control. Nothing retries by itself.
31. **VOM-FR-BQBI** A session closes when the author selects Push-to-Talk or leaves voice mode, on the Stop control of the status bar voice indicator (VOM-FR-UXZT), when the active provider goes (VOM-FR-WMAP), and on a fatal failure (VOM-FR-CSXI). Nothing else closes it.
32. **VOM-FR-XEIA** Closing during an utterance ends that utterance as a release does, and VOM-FR-ABPU applies. Then the WebView releases the microphone and calls "close voice activity session".
33. **VOM-FR-CSXI** A lost microphone track, a refused feed, or a `failed` event with `fatal` true closes the session as VOM-FR-XEIA states, and it shows an inline error.
34. **VOM-FR-VGCE** The VAD option is disabled while a recording, a transcript, or a post is in progress. Selecting VAD opens a session at once (VOM-FR-PDZR). Push-to-Talk can be selected at any time.
35. **VOM-FR-RTSU** On macOS, while VAD is selected, an info icon near the input mode toggle states in its tooltip and accessible name that other applications can sound lower for the whole session (VOM-FR-YIJS). The icon takes keyboard focus.

### Mute and waveform

36. **VOM-FR-QKTE** In VAD mode, a **Mute toggle** stands beside the VAD indicator. It is a toggle button with a pressed state. Its icon changes with the state, and its accessible name and tooltip state the action, Mute microphone or Unmute microphone. Click, Space, and Enter toggle it.
37. **VOM-FR-MUZB** While muted, the WebView feeds silence to the session in place of the microphone audio, and the gate reports `blocked`, so no utterance starts. The session and the microphone stay open. A Mute during an utterance takes effect when that utterance ends.
    - *Why:* a session that receives no feed for 30 s ends (`../core/VOP-voice-ai-providers.md` VOP-FR-KUGV), so a long Mute must still feed.
38. **VOM-FR-RBWX** Unmute feeds audio to the session again and clears `blocked`. A session opens unmuted. The muted state is session state in memory and is not saved.
39. **VOM-FR-WFHN** In VAD mode, a **waveform** shows the live level of the microphone. Its level comes from the WebView microphone stream of the session. It calls no provider and no backend operation.
40. **VOM-FR-ZDLY** The waveform is flat while muted and while no session is open.
41. **VOM-FR-PJOC** The waveform is decorative: it is hidden from assistive technology and is not an `aria-live` region. With a reduced-motion setting, it is not animated and changes at most two times each second.

### VAD utterances

42. **VOM-FR-NZXN** The backend finds each utterance and each chunk (`../core/VOP-voice-ai-providers.md` VOP-FR-DICU, VOP-FR-JRVF). The WebView drives the control state, the playback pause, the live bubble, and the posting from `voice-activity-changed` and `voice-chunk-changed` events. It drops an event of another session or of a capture that it does not hold.
43. **VOM-FR-WRVR** At `speech_confirmed`, the utterance is a recording, and a held recording is discarded. A chunked utterance shows its text in the live bubble (VOM-FR-NCHC). A spoken answer shows its transcript as VOM-FR-HUXY and VOM-FR-SRGN state.
44. **VOM-FR-OTRF** At `speech_end`, the WebView calls "finish voice capture" with the capture id, and the session listens for the next utterance at once. The result then follows VOM-FR-WYIE, VOM-FR-RENV, VOM-FR-ABPU, VOM-FR-TAQJ, VOM-FR-SBSS, VOM-FR-MVLA, and VOM-FR-BVNX, in the order of VOM-FR-QCZQ.
45. **VOM-FR-XWPZ** A `misfire` event records nothing, posts nothing, and shows no note. A held recording stays.
46. **VOM-FR-WWZX** A failed utterance shows the inline error, and VOM-FR-WICV applies when a recording is held. The session stays open.
47. **VOM-FR-GTAQ** While the composer is disabled and no question set stands, an open session stays open and starts no utterance. While a question set stands, an utterance is a spoken answer (VOM-FR-PINR). When the block ends, listening continues with no press. While the session is muted, no utterance starts (VOM-FR-MUZB).
48. **VOM-FR-VGLT** While a question set stands in the voice discussion, each mounted surface of it shows a session control near the line of `DQA-discussion-question-answering.md` DQA-FR-PXNC. It holds the voice controls of VOM-FR-TISN for the input mode, a line on how to answer aloud and whether the microphone is open or muted, the Stop control, and the inline feedback.
    - *Why:* the reply composer unmounts while a set stands, and the voice controls and an open microphone must stay visible and usable.
49. **VOM-FR-GPNK** The record control of the session control acts as VOM-FR-HHWL and VOM-FR-LIFL state. Its Mute toggle acts as VOM-FR-QKTE and VOM-FR-MUZB state, and its waveform as VOM-FR-WFHN states.
50. **VOM-FR-VOUG** In VAD mode, the **VAD indicator** shows one state: Idle (no session), Listening, Hearing speech, Transcribing, or Paused (playback paused by the author's speech), with the precedence of VOM-FR-XDUY. Transcribing shows while a chunk or an utterance transcribes. Each state shows by icon and colour, and in the accessible name. The indicator is not a button.
51. **VOM-FR-QTAQ** In Push-to-Talk, the record control shows Recording while it records, then Transcribing while a chunk or an utterance of it transcribes, then Idle. A new press can start a recording while Transcribing shows.

### Real-time speech

52. **VOM-FR-GBCN** An utterance in Push-to-Talk or in a VAD session opens a chunked capture, except a spoken answer (`../core/VOP-voice-ai-providers.md` VOP-FR-ERDS, VOP-FR-JRVF). Listening does not wait for transcription: the next chunk and the next utterance record while earlier chunks transcribe. The chunks of an utterance keep their order.
53. **VOM-FR-QCZQ** An utterance **settles** when its comment posted or failed to post, its transcript joined the session body, it came back empty, it failed, or it was discarded. The comments of utterances post in the order of the utterances: an utterance posts only after each earlier utterance settled.
54. **VOM-FR-NCHC** While a chunked utterance has not settled, the voice discussion shows one live bubble at the end of its log (`CVP-conversation-presentation.md` CVP-FR-TGPV). The bubble joins the chunk transcripts of each such utterance in order, as `voice-chunk-changed` events give them. It is not a comment and is not saved. The posted comment replaces its part.
55. **VOM-FR-RNBS** The first non-empty chunk transcript of an utterance calls "dispatch speech turn" with the capture id as the speech id, for each agent that a typed comment of that text reaches, VOM-FR-TAQJ and VOM-FR-SBSS applied (`../core/AGC-agent-conversations.md` AGC-FR-SJQP). A running turn of that agent in the discussion takes the chunk as steering, and no new turn starts.
56. **VOM-FR-OLFT** Each later non-empty chunk transcript of that utterance calls "steer speech turn" with the speech id and the chunk text (`../core/AGC-agent-conversations.md` AGC-FR-KXOJ). A chunk is never a comment and is never saved.
57. **VOM-FR-DHUU** While the author speaks or a chunk waits for transcription, a speech turn holds its reply (`../ai/CVL-conversation-loop.md` CVL-FR-SZKK), and its pending row says so (`CVP-conversation-presentation.md` CVP-FR-RNYK). The reply posts after the comment of the speech posts, and its spoken part plays after the reply posts. The author's speech never cancels a running turn.
58. **VOM-FR-MBHB** A chunk that fails to transcribe is skipped. The live bubble shows an inline error at its place, and later chunks continue. The comment holds the chunks that succeeded (`../core/VOP-voice-ai-providers.md` VOP-FR-GLDR). When no chunk succeeds, VOM-FR-WWZX and VOM-FR-ZFSA apply.
59. **VOM-FR-REJN** When a chunked utterance settles with no comment posted, the WebView calls "end speech turn" with the speech id and no comment id (`../core/AGC-agent-conversations.md` AGC-FR-NFYK), and the part of the live bubble for that utterance goes.
60. **VOM-FR-MVLA** When the comment of a chunked utterance posts, the WebView calls "end speech turn" with the speech id and the comment id (`../core/AGC-agent-conversations.md` AGC-FR-TCDQ). The comment dispatches turns as a typed comment does, except to an agent whose turn the speech reached (`CTA-comment-agent-turns.md` CTA-FR-SZOC).

### Barge-in

61. **VOM-FR-FVRS** When `speech_start` arrives while a reply plays, playback pauses at once. The segment is not stopped and not cancelled.
62. **VOM-FR-KVHG** A `misfire` resumes a paused reply from where it paused.
63. **VOM-FR-UNXR** When no utterance remains that has not settled, a paused reply is dropped if one of those utterances posted a comment or gave a spoken answer. Otherwise the paused reply resumes from where it paused.
64. **VOM-FR-ZPOD** An agent reply that arrives while an utterance is in progress or has not settled waits. When no utterance remains that has not settled, the newest such reply speaks, and it replaces a paused reply.

### Hotkey

65. **VOM-FR-WPRW** In Push-to-Talk, the hotkey of `VAI-voice-ai-settings.md` VAI-FR-RASA records into the voice discussion while the main window has focus, whether or not a surface of that discussion is mounted. In VAD mode, it does nothing. It is not an OS-wide hotkey.
66. **VOM-FR-HXLV** The hotkey does nothing while focus is in a text field, including the expanded text field, and during an IME composition.
67. **VOM-FR-DXJT** The hotkey does nothing while a floating overlay other than the composer's own surface is mounted (`SNV-shell-navigation.md` SNV-FR-56).
68. **VOM-FR-FHYY** The hotkey does nothing while voice mode is off.
69. **VOM-FR-SAMO** A repeated `keydown` of a held hotkey is ignored.

### Transcript

70. **VOM-FR-HUXY** On a live route, the live text of the open chunk shows after the chunk transcripts in the live bubble while the author speaks. It changes each time `voice-transcript-changed` gives new text. The live text of a spoken answer shows in the voice area or the session control.
71. **VOM-FR-SRGN** On a route that is not live, the record control in Push-to-Talk shows Recording with a timer. Each chunk transcript shows in the live bubble when it arrives. A spoken answer shows Transcribing after the release or the end of the utterance, and then its text.
72. **VOM-FR-SCAM** When a chunk ends, its live text stays at its place in the live bubble until its chunk transcript or its failure comes. The live text of the next chunk shows after it.
73. **VOM-FR-HSKZ** The record control and the VAD indicator state their phase in their accessible names. The transcript and the live bubble are not `aria-live` regions while recording.
    - *Why:* a screen reader that reads each partial word over the author's own speech makes dictation impossible.

### Posting

74. **VOM-FR-WYIE** At the end of an utterance, its transcript posts into the voice discussion as one normal user comment, through the same path as a typed comment, whether or not a composer of that discussion is mounted. It replaces the live bubble part of the utterance. No review step exists. A spoken answer posts as VOM-FR-YTKV states.
75. **VOM-FR-RFWC** The comment body is the unsent session body, then the transcript. Pending attachments go with it.
75. **VOM-FR-QWMK** With a spoken language set (`../core/GSS-global-settings-storage.md` GSS-FR-DNAW), the transcript is translated into English through "translate voice transcript" before it posts. The translation takes the place of the transcript in the comment body, and the comment carries the transcript as its `spoken_original` (`../core/CMS-comments-storage.md` CMS-FR-UYVK). The voice area shows `Translating…` while it waits.
75. **VOM-FR-DLPA** A translation that fails posts the transcript as it stands, with no `spoken_original`, and the voice area shows the note `Not translated. The original text was posted.` A translation equal to the transcript stores no `spoken_original`.
75. **VOM-FR-GZCV** A chunk message and a spoken answer to a question set are not translated.
76. **VOM-FR-RENV** An empty transcript posts nothing, and the voice area states that it heard no speech.
77. **VOM-FR-ABPU** A transcript that cannot post joins the session body of its discussion and is not posted. The voice area states this. This applies when voice mode ended by the Voice button before the transcript arrived, and when a question set arrived after the recording started.
    - *Why:* the speech is kept, and the author does not post into a discussion state they did not see.
78. **VOM-FR-TAQJ** At the start of a transcript, `at <nickname>`, `hey <nickname>`, and `at all` become a tag, and so does a bare nickname (VOM-FR-SBSS). Matching ignores case and uses the discussion's roster only. The tag replaces the spoken words, so the posted comment shows the tag. Other text stays as spoken.
79. **VOM-FR-SBSS** When no form of VOM-FR-TAQJ matches, the first word of the transcript, with trailing punctuation removed, becomes a tag when it matches a roster nickname: exactly, or else by the fuzzy match of VOM-FR-CXXP. So `Helga, check this` posts as `@Helga check this`.
80. **VOM-FR-CXXP** A fuzzy match applies only when a comma follows the first word, and only to nicknames of 4 or more characters: an edit distance of at most 1 for 4 to 6 characters, and at most 2 for 7 or more. The closest nickname must be strictly closer than every other. A tie or no match gives no tag.
    - *Why:* a transcription often misspells a name, and a weak match must not send a comment to the wrong agent.
81. **VOM-FR-BVNX** When a posted transcript reaches no agent, the voice area states this.
82. **VOM-FR-LNUH** A recording sends the roster's nicknames as hints (`../core/VOP-voice-ai-providers.md` VOP-FR-INCC). A VAD session sends them when it opens and when the roster changes.
83. **VOM-FR-ZUWW** While the composer is disabled and no question set stands, the record control and the hotkey start nothing. While a question set stands, they start a spoken answer (`DQA-discussion-question-answering.md` DQA-FR-PXNC).

### Failure

84. **VOM-FR-ZFSA** A denied or missing microphone, an unreachable provider, or a failed transcription of a whole utterance shows an inline error in the voice area and posts nothing. A failed chunk follows VOM-FR-MBHB.
85. **VOM-FR-WICV** After a failure of a provider, the error shows a Retry action. Retry calls "retry voice capture", and a success posts as VOM-FR-WYIE states, or answers as VOM-FR-PINR states.
86. **VOM-FR-FASJ** The next press, the next VAD utterance, Retry's success, or leaving voice mode discards the held recording.
87. **VOM-FR-JJMY** A denied microphone message tells the author where to allow it in the operating system settings.

### Spoken replies

88. **VOM-FR-ZLAY** With response mode Voice, a new agent comment in the voice discussion is spoken, whether or not a surface of that discussion is mounted.
89. **VOM-FR-EWOK** With response mode Text, no comment and no question is spoken.
90. **VOM-FR-ILJW** The set of comment ids at voice mode entry is the baseline. A comment in that set is never spoken. An opening composer takes its baseline from the discussion that its first post creates.
91. **VOM-FR-VSCQ** Only comments with an Agent author and a spoken part are spoken. Comments that a question submission creates carry no spoken part, so they are not spoken.
92. **VOM-FR-TRWN** Speech is the comment's spoken part alone (`../ai/CVL-conversation-loop.md` CVL-FR-AGNN), as the text of its Markdown, with citation markers removed. A code block or a table in it is replaced by a short spoken phrase. A comment with no speakable spoken part is not spoken.
93. **VOM-FR-QOXX** Text is split at sentence ends into segments under 4096 characters, and the first segment is short.
94. **VOM-FR-UALP** Each segment uses the voice, the speed, and the style of the agent that wrote the comment or recorded the question set (`AGR-agent-registry.md` AGR-FR-ABCF, AGR-FR-EPYX). Each value that the agent does not set is the provider's value.
94. **VOM-FR-QUSH** Playback plays the audio of a segment at the rate of its rate frame (`../core/VOP-voice-ai-providers.md` VOP-FR-EMJY). A segment with no rate frame plays at rate 1, and Stop resets the rate.
95. **VOM-FR-RKOG** A new agent comment or a new question stops the current playback at once, and its speech replaces it. No reply queues behind another, except as VOM-FR-ZPOD states.
96. **VOM-FR-QDWW** A Stop control of the voice area is visible while audio plays or a reply is paused. It stops playback at once, drops a paused or waiting reply, and calls "cancel voice speech".
97. **VOM-FR-SJYH** Frames of one reply play in order and with no gap between them. A segment ends at its end frame (`../core/VOP-voice-ai-providers.md` VOP-FR-XZET), so no audio frame is lost.
98. **VOM-FR-QPFQ** A failed segment stops playback and shows an inline error. The text of the comment is not affected.

### Question sets

99. **VOM-FR-HBLE** With response mode Voice, voice mode speaks the pending question set of the voice discussion from the recorded set, one question at a time, whether or not a surface of that discussion is mounted. Question speech is the question text alone. It then waits for an answer.
    - *Why:* the author answers in free form, and the options stay on screen.
100. **VOM-FR-NWGL** Question speech holds no option label and no option description, and it does not name the own-answer row. Citation markers are removed from it (`DQA-discussion-question-answering.md` DQA-FR-JADB).
101. **VOM-FR-UALK** A question speaks when its set arrives and when the block shows another question by any route (`DQA-discussion-question-answering.md` DQA-FR-BXHU). It speaks one time for each arrival and each move. A re-read of the set and a remount of the surface speak nothing.
102. **VOM-FR-LKKN** Question speech is a reply for playback: VOM-FR-QDWW, VOM-FR-SJYH, VOM-FR-QPFQ, VOM-FR-FVRS, VOM-FR-KVHG, VOM-FR-UNXR, and VOM-FR-ZPOD apply to it.
103. **VOM-FR-OSAQ** The Stop control leaves the set, its answers, and the question that shows as they are. The question does not speak again until the block moves.
104. **VOM-FR-PINR** A recording or a VAD utterance that starts while a question set stands is a **spoken answer**, in both response modes. It is not chunked (`../core/VOP-voice-ai-providers.md` VOP-FR-ERDS) and shows no live bubble. Its transcript answers the question that showed when it started (`DQA-discussion-question-answering.md` DQA-FR-SUUU), and it also posts as a comment (VOM-FR-YTKV).
105. **VOM-FR-FUFE** The answer of a spoken answer keeps its transcript as spoken. VOM-FR-TAQJ and VOM-FR-SBSS apply only to its comment. VOM-FR-RFWC does not apply: the comment body is the transcript with its tag alone, it takes no attachment, and the session body stays as it is.
106. **VOM-FR-YTKV** After a spoken answer takes effect in the block, its transcript posts into the voice discussion as one normal user comment through the path of VOM-FR-WYIE, VOM-FR-TAQJ and VOM-FR-SBSS applied. It dispatches turns by the ordinary routing (`CTA-comment-agent-turns.md` CTA-FR-PXJB), and VOM-FR-BVNX applies.
107. **VOM-FR-HNEB** A spoken answer whose comment fails to post keeps its answer in the block. The voice area shows the inline error, and the comment is not posted again.
108. **VOM-FR-MAVW** An empty or a failed spoken answer changes no answer, posts no comment, the question stays, and it does not speak again. VOM-FR-RENV applies to an empty transcript, and VOM-FR-ZFSA and VOM-FR-WICV apply to a failure.
109. **VOM-FR-UXYH** The transcript of a spoken answer whose set no longer stands joins the session body, as VOM-FR-ABPU states, and posts no comment.

### Lifecycle

110. **VOM-FR-KQAI** The surface teardown of `CVP-conversation-presentation.md` CVP-FR-49 ends voice mode, closes a VAD session, discards any recording, drops the live bubble, and stops playback. The saved input mode and response mode stay.
111. **VOM-FR-WMAP** A `voice-ai-changed` event updates the enabled state of the voice controls in the main window. A recording that has started keeps its configuration. An event that leaves no active provider closes a VAD session.

## Wireframes

```
Text composer
+----------------------------------------------------------+
| [+]  Ask an agent...                        [Voice] [Post] |
+----------------------------------------------------------+

Voice mode, recording on a live route
+----------------------------------------------------------+
| [>] [ (o) ] Recording                      [m|w] [T|s]   |
+----------------------------------------------------------+

Live bubble at the end of the discussion log
  .........................................................
  : You (speaking)                                        :
  : Check the second paragraph. I think the scope is too  :
  : wide. [!] One part was not heard.  and the story...   :
  .........................................................

Voice mode, audio playing
+----------------------------------------------------------+
| [>] [ (o) ] Press to talk  [Stop]  [m|w] [T|s]           |
+----------------------------------------------------------+

Voice mode, VAD session listening
+----------------------------------------------------------+
| [>] ((o)) Listening  [Mute] ~^~~^^~~  (i) [m|w] [T|s]    |
+----------------------------------------------------------+

Voice mode, VAD session muted
+----------------------------------------------------------+
| [>] ((o)) Listening  [Unmute] -------  (i) [m|w] [T|s]   |
+----------------------------------------------------------+

Voice mode, VAD session did not open
+----------------------------------------------------------+
| [>] (o) Idle  [Open again]  -------    (i) [m|w] [T|s]   |
| The microphone is not allowed. Allow it in ...           |
+----------------------------------------------------------+

Question set stands, VAD session open
Answer the questions above to continue the discussion.
+----------------------------------------------------------+
| ((o)) [Mute] ~^~~^^~~  The microphone is open. Speak to   |
|                        answer the question above. [Stop]  |
+----------------------------------------------------------+

Question set stands, Push-to-Talk
Answer the questions above to continue the discussion.
+----------------------------------------------------------+
| [ (o) ] Press to answer the question above.       [Stop]  |
+----------------------------------------------------------+

Status bar, trailing region, voice mode on in another tab
+----------------------------------------------------------+
| ...                     [ ((o)) Login draft: Listening ] [Stop] |
+----------------------------------------------------------+
```

- Toggle legend: `m` is Push-to-Talk, `w` is VAD, `T` is Text, and `s` is Voice. Each is an icon button. `(i)` is the info icon of VOM-FR-RTSU. `((o))` without brackets is the VAD indicator, which is not a button. `~^~~^^~~` is the waveform, and `-------` is the flat waveform.

- Layout notes: the voice area has the geometry of the text field, in all owner surfaces. Its parts stand on one line, and they wrap only where the column is too narrow for that line, as a growing text field does. Its trailing end holds the clearance of `DDS-draft-discussion.md` DDS-FR-QPHL.

- Layout notes: the live bubble is the last item of the log, after every comment and pending row. Its dotted border marks it as local. A failed chunk shows its error inline at its place.

- Layout notes: the status bar indicator and its Stop control are two separate controls. The indicator is drawn by `STB-status-bar.md` STB-FR-OSOL.

## UI contract boundary
- **Owned by the UI**: the voice area, the toggles, the single voice discussion rule, the binding of voice mode to a discussion, capture through the WebView, the press and hotkey logic, the VAD session lifecycle, the Mute toggle, the waveform and its level from the microphone stream, the reaction to `voice-activity-changed` and `voice-chunk-changed` events, the live bubble, the order of posts, the spoken tag conversion and the fuzzy nickname match, the reading of the spoken part, the conversion of Markdown to speech text, the split into segments, playback, reply detection, the speech of question sets, the spoken answer and its comment, the state given to the status bar voice indicator, and all messages.
- **Delegated to backend (abstract)**:
  - "start voice capture", "append voice capture", "finish voice capture", "retry voice capture", "cancel voice capture", "speak voice segment", "cancel voice speech", "open voice activity session", "feed voice activity", "set voice activity gate", "close voice activity session", "list voice ai integrations", and the `voice-chunk-changed` event, each owned by `../core/VOP-voice-ai-providers.md`.
  - "dispatch speech turn", "steer speech turn", "end speech turn", and "translate voice transcript", each owned by `../core/AGC-agent-conversations.md`.
  - "save app preferences" and "load app preferences", each owned by `../core/GSS-global-settings-storage.md`.

## Non-functional requirements
- Each voice control is operable from the keyboard and has an accessible name.
- One `AudioContext` serves playback. It is created by the activation of the Voice button, and it resumes before each segment, except while a reply is paused.
- Audio chunks reach the backend in order. One chunk is in flight at a time, and a backlog merges into the next chunk. A VAD session sends frames of about 30 ms.
- In a VAD session, the microphone stays open for the whole session, and the detector audio is not stored.
- Transcription of a chunk, the speech turn calls, and the post of a comment do not block the capture of later audio.
- On non-macOS platforms, the standard WebView capture constraints apply.
- Voice mode writes no audio, no chunk text, and no transcript to storage, and it logs none of them. The only voice state it saves is the input mode and the response mode.