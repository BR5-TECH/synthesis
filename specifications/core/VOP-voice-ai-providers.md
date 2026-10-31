# Voice AI providers

**Spec code:** `VOP`

## Intent
The backend home for the speech endpoints that Synthesis calls. It holds one record per Voice AI provider, finds which routes each provider offers, and does the speech-to-text and text-to-speech work of `../ui/VOM-voice-mode.md`. Realtime is the first-class transport: a provider that offers a Realtime route uses it by default, and the application finds that route by testing the server, not from a list of server names. HTTP routes serve the servers that have no Realtime route. Voice AI keys are separate from AI API keys, because a speech endpoint and a conversational endpoint are different services. The module also runs the local detector of a VAD session, which finds each utterance with no provider call and works offline. Out of scope: conversational calls, which belong to `AAP-ai-api-integrations.md`; voice activity detection on a server or a provider; native microphone capture; an OS-wide hotkey; and any storage of audio or of a transcript outside the discussion the transcript posts to.

## Functional requirements

### Providers

1. **VOP-FR-IHLG** Three Voice AI providers exist, each with a stable id: `openai`, `openrouter`, and `custom`. These ids are independent of the AI API provider ids.
2. **VOP-FR-SURK** Each provider has one record. It holds a base URL, the masked hint of its key, an STT model, a TTS model, a transcription route, a speech route, a default voice, the detected capabilities, and the time of its last successful verification.
3. **VOP-FR-CVLB** A provider's descriptor sets its default base URL, STT models, TTS model, and default voice, as the descriptor table of the contract surface states.
4. **VOP-FR-ZSXI** A provider's key is stored at `["voice_ai", "providers", <provider_id>]` in the application secret vault (`ASV-application-secret-vault.md` ASV-FR-03). This module reads no key of the `ai_api` namespace.
5. **VOP-FR-UWUE** `openai` and `openrouter` require a key. `custom` does not require a key, and a `custom` provider with no key sends no authorization header.
6. **VOP-FR-TLVI** A base URL uses `http` or `https` and holds no user information. A `custom` base URL does not end in `/v1`, and such a URL returns `base_url_invalid`.
7. **VOP-FR-KRMY** At most one provider is active. Only a verified provider can become active.
8. **VOP-FR-EBQI** No operation returns a key. A record carries only the masked hint. The function that reads a key for a call is not a Tauri command.
9. **VOP-FR-NWHQ** Each operation that changes a record or the active provider emits `voice-ai-changed` after the write.
   - *Why:* the Settings window and the main window are separate webviews, and the main window enables its voice controls from this state.

### Verification and detection

10. **VOP-FR-GVTD** Verification runs only from "verify voice ai integration". It sends real requests that can cost a very small amount. No other operation runs a probe.
11. **VOP-FR-PGPO** A successful verification writes the key, the record, and the capabilities. A failed verification writes nothing.
12. **VOP-FR-HTZP** An empty `api_key` argument verifies with the stored key.
13. **VOP-FR-GYMG** The same detection runs for every provider, so a provider that adds a Realtime route needs no change to this module.
14. **VOP-FR-CVXJ** Detection derives the Realtime URL from the base URL: `https` becomes `wss`, `http` becomes `ws`, and the path is `/realtime` on a named provider and `/v1/realtime` on `custom`.
15. **VOP-FR-HAXX** Detection opens a session with the query `intent=transcription`. On `custom`, a set STT model is also sent as the query `model`.
    - *Why:* a llama.cpp router selects the model from this query, and the `openai` URL refuses a model together with this intent.
16. **VOP-FR-IQAY** A refused or failed WebSocket handshake means that the provider has no Realtime route. This is not a verification failure.
17. **VOP-FR-AVXY** An `error` event before `session.created` means that the route exists and the model cannot use it. Capabilities record no Realtime transcription, and the record keeps the server's message to show to the author.
18. **VOP-FR-FHDE** A `session.created` event means that Realtime transcription is present. The detected rate is the `session.audio.input.format.rate` of the event, or the dialect's rate when the event has none.
19. **VOP-FR-BTDW** A `session.created` event with `session.type` other than `transcription` starts a turn-based capability. Detection sends `session.update` with `type: "transcription"`, and the capability is `turn`, not `live`.
    - *Why:* such a model gives its text after a commit, so the interface must not promise live text.
20. **VOP-FR-WUEI** Detection confirms a transcription capability with short silence and a commit. It requires a transcript event or an `input_audio_buffer.committed` event. An `error` event removes the capability.
21. **VOP-FR-USQZ** With a TTS model set, detection opens a second session with no `intent`. Realtime speech is present when that session reports an output rate and one word gives a `response.output_audio.delta`. Detection then sends `response.cancel`.
22. **VOP-FR-OMTM** Detection probes the HTTP routes of the descriptor with a short silent WAV and, with a TTS model set, one word. On `openrouter` it also calls `GET /key`, and a 401 there returns `rejected`.
23. **VOP-FR-KAUL** Verification fails with `no_transcription_route` when no transcription route answers. With a TTS model set, it fails with `no_speech_route` when no speech route answers. A missing Realtime route alone never fails it.
24. **VOP-FR-IKUE** A successful verification selects Realtime live transcription, then Realtime turn-based transcription, then HTTP. It selects Realtime speech, then `openai_speech`, then `llama_tts`. An earlier choice of the author stands while the capabilities still hold it.
25. **VOP-FR-GGBG** A record with an empty TTS model has no speech route, and it is still a verified record.
26. **VOP-FR-YRQL** Each probe has a time limit: 15 s, and 60 s for the first Realtime session of `custom`.
    - *Why:* a llama.cpp router can load a model when a session opens.
27. **VOP-FR-DBDI** Verification distinguishes `base_url_empty`, `base_url_invalid`, `key_missing`, `unreachable`, `tls_untrusted`, `rejected`, `no_transcription_route`, `no_speech_route`, and `keychain_unavailable`.
28. **VOP-FR-MUFH** "list voice ai models" returns the STT and TTS models of a provider: the descriptor list on `openai`, `GET /models?output_modalities=transcription` and `speech` on `openrouter`, and `GET /v1/models` on `custom`. A failed list returns the descriptor list and is not an error.
28. **VOP-FR-TOCX** "list voice ai models" sends a stored key only when the base URL is the provider's saved base URL. A base URL that fails VOP-FR-TLVI gets no request, and the result is the descriptor list.
   - *Why:* the Settings section lists models while the author edits the URL, and a stored key must not go to a partial host.
29. **VOP-FR-XJRF** "list voice ai voices" returns the voices of the named provider, or of the active provider when it names none: the descriptor list on `openai`, and `supported_voices` of the TTS model on `openrouter`. On `custom` it returns no list, and voice names are free text.

### Realtime dialects

30. **VOP-FR-TLGM** Each Realtime session uses one dialect. `openai` uses the `openai` dialect. On other providers, a `session.created` event that holds an audio input rate selects `llamacpp`, and one that holds none selects `vllm`.
31. **VOP-FR-XONB** A Realtime connection sends `Authorization: Bearer <key>` in the handshake. It sends no authorization header when the provider has no key.
32. **VOP-FR-YPEY** A `wss` connection uses the shared TLS configuration (`AAP-ai-api-integrations.md` AAP-FR-WMCX) and offers ALPN `http/1.1` only.
33. **VOP-FR-GDVD** In the `openai` dialect, the client sends `session.update` with `type: "transcription"`, PCM at 24 kHz, the STT model, and `turn_detection: null`.
34. **VOP-FR-CSWH** In the `llamacpp` dialect, the client sends no input rate that differs from the rate of `session.created`.
    - *Why:* the server refuses such a `session.update` with an error.
35. **VOP-FR-YRQN** In the `vllm` dialect, the client sends `session.update` with `model`, sends one `input_audio_buffer.commit` before audio, and sends PCM at 16 kHz. It reads `transcription.delta` and `transcription.done`.
36. **VOP-FR-HOGR** At the end of input, the `openai` dialect sends `input_audio_buffer.commit`. The `llamacpp` and `vllm` dialects send it with `"final": true`.
36. **VOP-FR-QQYG** A capture session ends when its dialect says so: `openai` at the `completed` event of the item that the commit named, `vllm` at `transcription.done`, and `llamacpp` when the socket closes after the final commit. A turn-based session ends when each item is completed.
   - *Why:* after `"final": true`, llama.cpp sends no `committed`, and a `completed` event can precede another item.
37. **VOP-FR-XXAB** Transcript text joins items in commit order, from `item_id` and `previous_item_id`. Events are matched by `item_id`, never by arrival order. A new item that a server starts by itself continues the same transcript on a new line.

### Capture

38. **VOP-FR-KBPZ** "start voice capture" copies the active provider's configuration and opens one capture session. A second session while one is open and has not failed returns `voice_session_active`. A start replaces a failed session. Later changes to the record do not affect an open session.
39. **VOP-FR-DXPK** "append voice capture" takes mono PCM16 at the capture rate as a raw request body. It also accepts a JSON array of bytes.
    - *Why:* one failed IPC fetch moves Tauri to JSON bodies for the life of the webview.
40. **VOP-FR-GZOC** Audio is resampled from the capture rate to the rate of the selected route. Detector audio is resampled to 16 kHz.
41. **VOP-FR-GYKP** On a Realtime route, the session opens its socket at start and holds audio until the socket is ready. It then forwards audio as `input_audio_buffer.append` events.
42. **VOP-FR-JAOB** On a live route, each change of the transcript emits `voice-transcript-changed` with the session id and the whole text so far.
43. **VOP-FR-VZDI** "finish voice capture" ends the input, waits for the final transcript, and returns it. The text is trimmed. An empty transcript is a success with empty text.
44. **VOP-FR-EYHP** On the HTTP route, audio is held at 16 kHz. "finish voice capture" sends one WAV as multipart `file` with `response_format=json`, to `<base>/audio/transcriptions` on a named provider and `<base>/v1/audio/transcriptions` on `custom`.
45. **VOP-FR-INCC** Agent nicknames are sent as hints only where a provider documents them: `keywords` on the `openai` Realtime route, `prompt` on the `openai` HTTP route, and `keyterms` on `openrouter`. A nickname that holds `<`, `>`, a carriage return, or a line feed is left out.
46. **VOP-FR-YEFK** A session holds its PCM in memory until a success, the next start, or a cancel. Audio and transcripts are never written to disk, and they never appear in a log. Detector audio stays in memory on the same terms.
47. **VOP-FR-IAFL** "retry voice capture" sends the held PCM again on the same route. When a Realtime socket failed and the capabilities hold the HTTP route, it uses the HTTP route.
48. **VOP-FR-UOYP** A failed capture returns one of `provider_unreachable`, `provider_rejected`, `transcription_failed`, or `voice_session_lost`. The session stays open for "retry voice capture".
49. **VOP-FR-BSNY** An unfinished capture session that receives no audio for 30 s is cancelled, and its socket closes. A VAD session opens a capture only from confirmed speech, so a session that only listens meets no such limit.
    - *Why:* a reloaded webview cannot end its own session, and an open socket can cost money.

### Voice activity

49. **VOP-FR-WYKO** "open voice activity session" takes the capture rate and the keyterms, and it opens one session. It replaces an older session and cancels a capture of that session.
49. **VOP-FR-ADHQ** A model or a runtime that fails to load returns `voice_detector_unavailable`, and no session opens.
49. **VOP-FR-TADX** "feed voice activity" takes mono PCM16 at the capture rate, in the request form of VOP-FR-DXPK. The header `x-voice-session` names the activity session. An unknown id returns `voice_session_lost`.
49. **VOP-FR-XILN** While no capture is open, feed audio goes only to the local detector. A session that only listens makes no provider call.
49. **VOP-FR-SMSX** The detector runs the Silero VAD model through ONNX Runtime on the CPU, on frames of 512 samples at 16 kHz.
49. **VOP-FR-JXQG** The model and the runtime are compiled into the application executable on each platform (`../infra/NLL-native-library-linkage.md` NLL-FR-LTEO). Detection loads no file from the network, and it works offline.
49. **VOP-FR-OORS** `voice_vad_sensitivity` sets the start threshold, and the end threshold is 0.15 under it, as the detector table of the contract surface states.
49. **VOP-FR-DBEE** While the gate reports that a reply plays, the start threshold rises by 0.20, to at most 0.90. The end threshold does not rise.
    - *Why:* the speakers can reach the microphone, and a raised end threshold would cut the author's speech.
49. **VOP-FR-DICU** A frame at or above the start threshold emits `speech_start` at once, when the session is not blocked and no utterance is in progress.
    - *Why:* playback pauses at this event, so it cannot wait for the 300 ms rule.
49. **VOP-FR-QFRF** Before a capture opens, 3 frames in a row under the end threshold emit `misfire`. No capture opens, and no provider call is made.
49. **VOP-FR-GZPY** At 10 frames after `speech_start`, or at the first later frame at or over the end threshold, the session opens a capture as VOP-FR-KBPZ states, with the active provider's configuration at that time. It emits `speech_confirmed` with the capture id and `live`.
49. **VOP-FR-FYQX** A `voice_session_active` refusal of that capture emits `misfire`. Another refusal emits `failed` with its code, and the session keeps listening.
49. **VOP-FR-FAQA** The capture receives the audio from 320 ms before `speech_start`, then the live audio, in order and at the capture rate, as "append voice capture" does.
49. **VOP-FR-OFDY** In a capture, a frame under the end threshold starts the silence count. Only a frame at or above the start threshold clears it. At the silence timeout plus 200 ms, the session emits `speech_end`.
    - *Why:* a noise floor between the two thresholds must not hold an utterance open.
49. **VOP-FR-GWZP** An utterance emits `speech_end` at 10 minutes, and the session stays open.
49. **VOP-FR-FMRV** After `speech_end`, no utterance starts until the gate names that capture as settled.
49. **VOP-FR-DAZW** After a failed capture open, no `speech_start` comes until a frame under the end threshold.
    - *Why:* the author still speaks, and a new start every 320 ms would pause and resume a reply again and again.
49. **VOP-FR-NDEH** After a `misfire` while a reply plays, no `speech_start` comes for 16 frames.
    - *Why:* a reply that reaches the microphone would otherwise pause and resume itself.
49. **VOP-FR-QZCE** "set voice activity gate" carries `seq`, `blocked`, `playing`, `settled`, and `keyterms`. A `seq` under the last applied `seq` is ignored.
    - *Why:* Tauri runs async commands in parallel, so a whole record with an order cannot leave a wrong value.
49. **VOP-FR-KYFC** While `blocked` is true, no utterance starts. An utterance in progress continues.
49. **VOP-FR-DSNZ** The session reads `voice_vad_silence_ms` and `voice_vad_sensitivity` from app preferences while it waits for speech, so a change applies from the next utterance. It clamps the silence to 500 through 5000 ms.
49. **VOP-FR-FAAB** "close voice activity session" ends the session. With `discard`, it cancels a capture that has not settled and returns null. Otherwise it returns the id of that capture and does not cancel it, for "finish voice capture". Unconfirmed speech is dropped.
    - *Why:* the reply and a `speech_end` event travel apart, so a reply can come first.
49. **VOP-FR-KUGV** A session that receives no feed for 30 s cancels its capture, emits `failed` with `fatal` true, and ends.
    - *Why:* a reloaded webview cannot close its session, and a lost microphone sends nothing.
49. **VOP-FR-BTBM** A detector failure emits `failed` with `voice_detector_failed` and `fatal` true. A `voice_session_lost` append emits `failed` with `fatal` false, and it ends the utterance.

### Speech

50. **VOP-FR-MOSY** "speak voice segment" takes one text segment, a voice, and a `Channel`. It sends audio frames on the channel, then one end frame, and it resolves when the segment ends. A segment over 4096 characters returns `voice_segment_too_long`. A failed segment sends no end frame.
    - *Why:* a large frame reaches the WebView after the command resolves, so only the end frame shows that all audio arrived.
51. **VOP-FR-XZET** A frame is raw bytes. Byte 0 is the kind: `0` for PCM16 mono, `1` for one encoded audio file, `2` for the end of a segment. Bytes 1 to 4 are the sample rate as little-endian `u32`, and `0` for a file or an end frame. The rest is the payload, which an end frame does not have.
52. **VOP-FR-TZDV** On a Realtime speech route, each `response.output_audio.delta` becomes one PCM frame at once, so audio starts to play before the segment ends.
53. **VOP-FR-UPRG** In the `openai` dialect, each segment opens a new session with its voice. The segment is a `response.create` with `conversation: "none"`, `input: []`, `output_modalities: ["audio"]`, and instructions that say to speak the text exactly.
    - *Why:* the voice of a session cannot change after its first audio, and no socket stays open while voice mode is idle.
54. **VOP-FR-TOFP** In the `llamacpp` dialect, each segment opens a new session. It sends `session.update` with the instructions and `output_modalities: ["audio"]`, then `conversation.item.create` with `input_text`, then `response.create`. The output rate comes from `session.created`.
    - *Why:* this server fixes `instructions` after the first message and has no `conversation: "none"`.
55. **VOP-FR-QTZJ** On `openrouter`, speech is `POST <base>/audio/speech` with `response_format: "mp3"`. On a 400, it sends the request once more with `pcm`, at the rate of the `Content-Type` or else 24 kHz.
56. **VOP-FR-TSVV** On `custom`, `openai_speech` is `POST <base>/v1/audio/speech` with `response_format: "wav"`, and `llama_tts` is `POST <base>/tts` with `input`. The module rewrites the RIFF and `data` sizes to the real length, then sends one file frame.
    - *Why:* Ollama writes maximum values in those fields, and a decoder can refuse them.
57. **VOP-FR-IAEN** `openai` has no HTTP speech route. Its TTS model is `gpt-realtime-2.1-mini`.
    - *Why:* OpenAI shuts down every model of `/v1/audio/speech` on 2027-01-06, and the replacement is available only through Realtime.
58. **VOP-FR-RABZ** The module speaks with the requested voice. A voice outside the voices of the active provider becomes the default voice. On `custom`, a 400 for a request with a voice repeats once with the default voice.
59. **VOP-FR-INDT** A `llamacpp` Realtime speech route takes no voice. The requested voice has no effect on that route.
60. **VOP-FR-DWSB** "cancel voice speech" ends the segment at once. It sends `response.cancel` or aborts the HTTP request, and it sends no more frames.
61. **VOP-FR-GQJS** A failed segment returns one of `provider_unreachable`, `provider_rejected`, or `speech_failed`.

### Platform

62. **VOP-FR-XMKU** The macOS bundle declares `NSMicrophoneUsageDescription` in `src-tauri/Info.plist`, and it holds the entitlement `com.apple.security.device.audio-input`.
    - *Why:* WebKit hides `navigator.mediaDevices` without the usage string, and the hardened runtime blocks the microphone without the entitlement.
63. **VOP-FR-MNTN** On Linux, the main webview enables media streams and grants an audio `permission-request` from the application's own page. It refuses each other request of that kind.
    - *Why:* the webview layer has no handler, so WebKitGTK denies each capture request by default.

## Contract surface
The module owns the `voice_ai.providers` namespace of the application secret vault (`ASV-application-secret-vault.md` ASV-FR-03) and the Voice AI provider registry in the user-global store (`GSS-global-settings-storage.md` GSS-FR-CKQG).

### The provider descriptor

| | `openai` | `openrouter` | `custom` |
|---|---|---|---|
| Default base URL | `https://api.openai.com/v1` | `https://openrouter.ai/api/v1` | none |
| Key | required | required | optional |
| STT models | `gpt-live-transcribe` (Realtime), `gpt-transcribe` (HTTP) | `openai/gpt-transcribe` | none |
| TTS model | `gpt-realtime-2.1-mini` | `elevenlabs/eleven-v4-turbo` | none |
| Default voice | `marin` | `george` | none |
| Voices | `alloy`, `ash`, `ballad`, `coral`, `echo`, `fable`, `nova`, `onyx`, `sage`, `shimmer`, `verse`, `marin`, `cedar` | `supported_voices` of the model | free text |
| HTTP transcription | yes | yes | detected |
| HTTP speech | none | `/audio/speech` | detected: `/v1/audio/speech`, `/tts` |

### The Realtime dialects

| | `openai` | `llamacpp` | `vllm` |
|---|---|---|---|
| Session | `?intent=transcription`, no model in the URL | `?intent=transcription`, optional `model` | `session.update` with `model` |
| Input rate | 24 kHz, set by the client | from `session.created` | 16 kHz |
| End of input | `commit` | `commit` with `"final": true` | `commit` with `"final": true` |
| Live text | `conversation.item.input_audio_transcription.delta` | same, items restart after 15 to 30 s | `transcription.delta` |
| Final text | `conversation.item.input_audio_transcription.completed` | same | `transcription.done` |
| Speech | one session per segment | one session per segment | none |

### Records

```
VoiceAiRecord {
  provider:              "openai" | "openrouter" | "custom",
  base_url:              string,
  masked_hint:           string | null,
  stt_model:             string,
  tts_model:             string,                         // empty: no speech
  transcription_route:   "realtime" | "http",
  speech_route:          "realtime" | "openai_speech" | "llama_tts" | null,
  default_voice:         string,
  capabilities:          VoiceCapabilities | null,
  capability_note:       string | null,                  // the server's refusal message (VOP-FR-AVXY)
  verified_at:           string | null,
}

VoiceCapabilities {
  dialect:               "openai" | "llamacpp" | "vllm" | null,
  realtime_transcription: "live" | "turn" | null,
  realtime_input_rate:   number | null,
  http_transcription:    bool,
  realtime_speech:       bool,
  openai_speech:         bool,
  llama_tts:             bool,
}

VoiceAiIntegrations {
  active:                "openai" | "openrouter" | "custom" | null,
  records:               VoiceAiRecord[],
}

VoiceCaptureStart {
  session_id:            string,
  route:                 "realtime" | "http",
  live:                  bool,
}

VoiceActivityChanged {
  activity_id:           string,
  state:                 "speech_start" | "speech_confirmed" | "speech_end" | "misfire" | "failed",
  capture_id:            string | null,                  // speech_confirmed, speech_end
  live:                  bool | null,                    // speech_confirmed
  code:                  string | null,                  // failed
  fatal:                 bool | null,                    // failed
}
```

### The detector

| | Low | Medium | High |
|---|---|---|---|
| Start threshold | 0.70 | 0.50 | 0.35 |
| End threshold | 0.55 | 0.35 | 0.20 |
| Start threshold while a reply plays | 0.90 | 0.70 | 0.55 |

| Value | Number |
|---|---|
| Frame | 512 samples at 16 kHz, 32 ms |
| Pre-roll | 320 ms, at the capture rate |
| Frames to confirm | at least 10, 320 ms |
| Frames to a misfire | 3 in a row, 96 ms |
| Frames with no start after a misfire during playback | 16, about 500 ms |
| Tail after the silence timeout | 200 ms |
| Longest utterance | 10 minutes |
| No feed limit | 30 s |

### Operations

- `"list voice ai integrations"` → `list_voice_ai_integrations()` → `VoiceAiIntegrations`
- `"verify voice ai integration"` → `verify_voice_ai_integration(provider, base_url, api_key, stt_model, tts_model)` → `VoiceAiRecord`, or a typed error (VOP-FR-DBDI)
- `"set voice ai routes"` → `set_voice_ai_routes(provider, transcription_route, speech_route)` → `VoiceAiRecord`, or `route_not_detected`
- `"set voice ai default voice"` → `set_voice_ai_default_voice(provider, voice)` → `VoiceAiRecord`
- `"set active voice ai integration"` → `set_active_voice_ai_integration(provider)` → `VoiceAiIntegrations`, or `not_verified`
- `"clear voice ai integration"` → `clear_voice_ai_integration(provider)` → `VoiceAiIntegrations`
- `"list voice ai models"` → `list_voice_ai_models(provider, base_url, api_key)` → `{ stt_models: string[], tts_models: string[] }`
- `"list voice ai voices"` → `list_voice_ai_voices(provider)` → `string[] | null`. `provider` is optional
- `"start voice capture"` → `start_voice_capture(capture_rate, keyterms)` → `VoiceCaptureStart`, or `no_active_provider`, `voice_session_active`
- `"append voice capture"` → `append_voice_capture` with a raw body and the header `x-voice-session`
- `"finish voice capture"` → `finish_voice_capture(session_id)` → `string`, or a typed failure (VOP-FR-UOYP)
- `"retry voice capture"` → `retry_voice_capture(session_id)` → `string`, or a typed failure (VOP-FR-UOYP)
- `"cancel voice capture"` → `cancel_voice_capture(session_id)` → nothing
- `"speak voice segment"` → `speak_voice_segment(speech_id, text, voice, on_frame)` → nothing, or a typed failure (VOP-FR-GQJS)
- `"cancel voice speech"` → `cancel_voice_speech(speech_id)` → nothing
- `"open voice activity session"` → `open_voice_activity_session(capture_rate, keyterms)` → `{ activity_id }`, or `voice_detector_unavailable`, `no_active_provider`
- `"feed voice activity"` → `feed_voice_activity` with a raw body and the header `x-voice-session`
- `"set voice activity gate"` → `set_voice_activity_gate(activity_id, seq, blocked, playing, settled, keyterms)` → nothing
- `"close voice activity session"` → `close_voice_activity_session(activity_id, discard)` → `{ capture_id: string | null }`

### Events

- `voice-ai-changed`: carries `VoiceAiIntegrations`.
- `voice-transcript-changed`: carries `{ session_id, text }`.
- `voice-activity-changed`: carries `VoiceActivityChanged`. The events of one session come in order.

## Non-functional requirements
- No command that waits on the network runs on the main thread.
- A log line names the provider, the route, and a loggable base URL. It holds no key, no audio, and no transcript.
- This module opens no socket and sends no request while voice mode is on but idle, or while a VAD session only listens.
- The detector does not run on the main thread or on an async worker. Application exit ends a VAD session.
- A speech provider receives the transcript audio and the reply text. It receives nothing else from the project.
