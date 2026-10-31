## Intent

Add hands-free voice input to voice mode: voice activity detection (VAD). The author opens a VAD session once. The application then detects when the author starts and stops speaking, and it posts each utterance as a normal user comment. No press or hold is needed per utterance. It changes no other voice behaviour, and it adds no new provider call. A local detector in the backend finds the speech. The WebView only captures the microphone and shows the state.

Out of scope: server-side or provider Realtime VAD, native microphone capture in the backend, a level meter, a Start/End session control separate from the record control, an OS-wide hotkey, and every other voice UX item except the two cosmetic changes in "Cosmetic changes" below.

## User journey

1. The author is in voice mode (`VOM-voice-mode.md`) and selects **VAD** in the input mode toggle. It is no longer disabled.
2. The author activates the record control. A VAD session opens, and the microphone stays open. The control shows **Listening**.
3. The author speaks. The control shows **Hearing speech**, and the transcript shows as on any recording (VOM-FR-HUXY, VOM-FR-SRGN).
4. The author stops. After the silence timeout, the utterance ends. The control shows **Transcribing**, then the transcript posts as a normal user comment (VOM-FR-WYIE). The control returns to **Listening**. The session stays open for the next utterance.
5. An agent reply plays (response mode Voice). The author speaks. Playback pauses at once. When the author stops: if the utterance posted, the paused reply is dropped and the next agent reply plays; if nothing posted, the paused reply resumes.
6. The author activates the record control again. The session closes, and the microphone is released.

## Requirements

### Session

- The input mode toggle offers Push-to-Talk and VAD, and VAD is enabled. The input mode is session state in memory (VOM-FR-MQHD). It starts as Push-to-Talk each time voice mode starts.
- In VAD mode, activating the record control (click, Space, Enter) opens the session. Activating it again closes the session. It is the only control that opens or closes a session.
- Opening a session calls `getUserMedia` with the constraints of VOM-FR-YIJS. The microphone stays open until the session closes. A denied or missing microphone follows VOM-FR-ZFSA and VOM-FR-JJMY, and no session opens. After the microphone opens, the WebView calls "open voice activity session" and streams the audio to the backend (see "Detection").
- A session closes when: the author activates the record control; the author selects Push-to-Talk; the author leaves voice mode; the composer unmounts; or the surface teardown of CVP-FR-49 runs. Closing during an utterance ends that utterance as a release does (VOM-FR-ABPU applies). Then the WebView calls "close voice activity session" and releases the microphone.
- Window blur does not close a session and does not end an utterance. This differs from VOM-FR-XQUY on purpose.
- Opening a session stops playback, as VOM-FR-VVTY states for a press.
- On macOS, OS voice processing can lower the sound of other applications while the microphone is open (VOM-FR-YIJS). In VAD mode this lasts for the whole session. While VAD is selected on macOS, a short hint near the input mode toggle states this.

### Detection

- Speech detection runs in the backend, on audio that the WebView sends. The WebView captures the microphone and streams mono PCM16 at the capture rate, in short frames, with the new operation "feed voice activity". It uses the same request form as VOP-FR-DXPK. The backend resamples to 16 kHz (VOP-FR-GZOC). The stream goes only to the local detector. No provider call is made while the session only listens (VOM-FR-SIEO).
- The backend runs the Silero VAD model through ONNX Runtime (for example the `ort` crate). The model file and the runtime ship inside the application on each platform. Detection loads no file from the network and works offline. On macOS the runtime is linked statically (NLL-FR-VMBF, NLL-FR-AVJK). The session uses the one microphone stream of the session, and it opens no second stream. Detector audio stays in memory. It is never written to disk and never logged (VOP-FR-YEFK).
- Silero gives a speech probability for each audio frame. The Sensitivity levels set the speech start threshold on that probability (Low is the highest threshold, High is the lowest). The Silence timeout sets the silence length that ends an utterance. The 300 ms minimum speech length and the pre-roll map to the matching Silero parameters. The backend reads both settings from the app preferences at the start of each utterance. The implementing agent picks the numbers and records them in `VOP-voice-ai-providers.md`.
- The playback margin of "Barge-in" raises the Silero start threshold while a reply plays. The WebView tells the backend whether a reply plays with "set voice activity playback".
- The backend owns each utterance: the pre-roll, the start, and the end. It sends the event `voice-activity-changed` to the WebView with one of these states: `speech_start` (carries the capture session id), `speech_end` (after the tail and the 300 ms rule; carries the id), or `misfire` (speech under 300 ms; nothing recorded). The WebView drives the control state, the playback pause, and the posting from these events. A failure to load the model or the runtime makes "open voice activity session" return an error. The WebView shows the inline error, opens no session, and releases the microphone.
- An utterance starts when detected speech passes the sensitivity level. The utterance includes a short pre-roll of audio from before detection, so the first word is complete.
- An utterance ends when silence lasts for the silence timeout. Then the 200 ms tail of VOM-FR-FTFU applies.
- Speech shorter than 300 ms records and posts nothing (VOM-FR-DGOG). An utterance ends at 10 minutes (VOM-FR-WSXW), and the session stays open.
- Each utterance is one capture session. At `speech_start` the backend runs "start voice capture". It then sends the pre-roll and the live audio to that session, in order, as "append voice capture" does, so the live transcript works as in VOM-FR-HUXY. At `speech_end` the WebView calls "finish voice capture" with the id. Captures never overlap: the backend starts no utterance while a capture awaits its finish, and speech in that time is not recorded. A capture is open only from speech start, so the 30 s no-audio timeout of VOP-FR-BSNY does not meet a listening session. Posting, empty transcript, tag conversion, and failure follow VOM-FR-WYIE, VOM-FR-RENV, VOM-FR-TAQJ, VOM-FR-BVNX, and VOM-FR-ZFSA through VOM-FR-FASJ.
- A failed utterance shows the inline error and Retry. The session stays open. The next utterance discards the held recording (VOM-FR-FASJ).
- While the composer is disabled or a question set stands, the session stays open but starts no utterance (VOM-FR-ZUWW).

### Barge-in

- When speech starts while a reply plays, playback pauses at once. It does not stop and does not cancel the segment. The backend sends `speech_start` as soon as the start threshold is passed, without waiting for 300 ms. A later `misfire` resumes playback.
- While a reply plays, speech must pass a higher level than the selected sensitivity to start an utterance. This reduces false barge-in when the microphone hears the speakers. The margin is fixed, not a setting. The implementing agent picks it and records it in `VOP-voice-ai-providers.md`.
- When the utterance ends: if it posted a comment, the paused reply is dropped. If it posted nothing (too short, empty transcript, failure, or cannot post), the paused reply resumes from where it paused.
- An agent reply that arrives while the author speaks waits, and it speaks after the utterance ends. It replaces the paused reply (VOM-FR-RKOG).
- The Stop control (VOM-FR-QDWW) works in a session and drops a paused reply.

### Settings

- The Voice AI section holds a **Silence timeout** control. The range is 0.5 s to 5 s, in steps of 0.1 s. The default is 1.5 s.
- The Voice AI section holds a **Sensitivity** control with three levels: Low, Medium, High. The default is Medium.
- Both save through `patchAppPreferences` and reach the main window through the app-preferences change event, with no restart. An open session applies a change from its next utterance. A stored value out of range is clamped.
- The controls apply to VAD only and say so.

### Indicators

- The record control shows the VAD state: **Idle** (no session), **Listening**, **Hearing speech**, **Transcribing**, and **Paused** (playback paused by the author's speech). The state shows by icon and colour, and also in the accessible name (VOM-FR-HSKZ). No colour is the only signal.
- No level meter is shown.

### Hotkey

- While the input mode is VAD, the Push-to-Talk hotkey does nothing. The hotkey control in settings stays, and its label says it applies to Push-to-Talk only.

### Cosmetic changes

- The input mode toggle (Push-to-Talk / VAD) and the response mode toggle (Text / Voice) render as icon-only buttons, so they use less space. Each toggle keeps its two options, its selected state, and its rules (VOM-FR-IKIK, VOM-FR-IAVZ).
- Each icon button has an accessible name and a tooltip that state the option, for example "Push-to-Talk" or "Voice". The selected state shows by more than colour. A disabled option (Voice with no speech route) keeps the reason in its tooltip and accessible description. Each button is reachable and operable by keyboard.
- The post-shortcut hint (Ctrl+Enter on Windows and Linux, Cmd+Enter on macOS) is removed from the reply field and the opening composer of the discussion column. The placeholder stays. The shortcut still posts, on the same terms as before (DDS-FR-LPSC, CMT-FR-70).
- No other behaviour of the toggles or the composer changes.

## Specifications to change

- `specifications/ui/VOM-voice-mode.md`: Intent (VAD no longer out of scope); VOM-FR-TISN and VOM-FR-IKIK (VAD enabled; icon toggles); VOM-FR-IAVZ (icon toggle); VOM-FR-WPRW (hotkey acts only in Push-to-Talk); VOM-FR-VVTY and VOM-FR-XQUY (VAD cases); VOM-FR-ZUWW; VOM-FR-RKOG and VOM-FR-QDWW (pause and resume); VOM-FR-WMAP; the wireframes; new Recording requirements for VAD sessions and barge-in; the UI contract boundary and the non-functional requirements (microphone open for a whole session); VOM-FR-YIJS (macOS volume note for sessions).
- `specifications/ui/VAI-voice-ai-settings.md`: new Silence timeout and Sensitivity controls; the hotkey controls state that they apply to Push-to-Talk only (VAI-FR-RASA); the change event (VAI-FR-AVBR).
- `specifications/ui/GLS-global-settings.md`: GLS-FR-14 (fields that the Voice AI section owns) and GLS-FR-GMTK (the section holds the VAD settings).
- `specifications/core/GSS-global-settings-storage.md`: the app-preferences record and its Tauri commands gain `voice_vad_silence_ms` (number) and `voice_vad_sensitivity` (`"low" | "medium" | "high"`).
- `specifications/core/VOP-voice-ai-providers.md`: the new operations "open voice activity session", "feed voice activity", "set voice activity playback", and "close voice activity session"; the `voice-activity-changed` event; the Silero detector, the bundled model and runtime, and the offline rule; the thresholds, the barge-in margin, and the pre-roll; VOP-FR-KBPZ (the backend starts one capture per utterance, with no overlap); VOP-FR-BSNY (a listening session meets no timeout); VOP-FR-YEFK (detector audio is not stored or logged).
- `specifications/infra/NLL-native-library-linkage.md`: NLL-FR-LTEO lists the ONNX Runtime `-sys` crate (for example `ort-sys`) for static linkage on macOS. Confirm that the macOS build passes the check of NLL-FR-NPMB.
- `specifications/ui/VOM-voice-mode.md` also lists the new operations as delegated to the backend in the UI contract boundary, and it names the events that the WebView reacts to. Check that `src-tauri/tauri.conf.json` ships the model file with the bundle, and that the WebView on each platform (WebKitGTK on Linux) streams microphone audio to the backend.
- `specifications/ui/DDS-draft-discussion.md`: DDS-FR-HQTX (the reply field carries no post-shortcut hint; the placeholder stays); DDS-FR-JKPC and the clearance of DDS-FR-QPHL (icon toggles in the voice area).