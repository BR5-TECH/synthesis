## Intent

User working with IDE might want to have voice conversation in discussion instead of typing in chat.

User should be able to discuss the Drafts or any other artifacts as a part of discussion experience.

This build delivers voice input with Push-to-Talk and optional spoken agent replies, in every discussion. VAD is shown but disabled, and comes later.

## User journey

### Configuring a voice chat

- User opens settings panel
- User opens the Voice AI section of the Settings window (global). It is a section of its own, not a third level of the AI integrations.
- User configures a Voice AI provider: OpenAI, OpenRouter, or Custom (Ollama, llama.cpp, or another OpenAI-compatible server). Each provider has its own base URL, key, speech-to-text model, text-to-speech model, and transcription mode (record then transcribe, or streaming). The Voice AI section has its own keys; it does not reuse the AI API credentials. User verifies the provider before it can be used.
- User sets the Push-to-Talk hotkey and chooses hold or toggle behaviour in the same section. 
- User can optionally choose the voice of an agent in the persona editor, for Personal and Project agents. The voice is stored in the agent's own scope: a Personal agent's voice stays on this machine, a Project agent's voice travels with the project.
- Settings are persisted on this machine. Keys are held in the application secret vault, never in a settings file.

### Starting a voice chat

- User opens a Draft or any other Artifact.
- The shared discussion composer has a button to turn voice mode on. While no Voice AI provider is verified, the button is disabled and states why.
- Once the button is clicked, text input area is collapsed to the the left, leaving small button which would allow to expand it back temporarily if the user wants to type something mid-chat or attach an image.
- in place where the text area was, we show two things:
  - small toggles at the right side of the area: input mode (Push-to-Talk, or VAD shown disabled) and response mode (Text or Voice).
  - in-place we show the text that’s being transcribed so user can see how the voice was converted to text.
- IF: Push-to-Talk mode is enabled THEN: user records with the on-screen button or the hotkey. In hold mode, recording lasts while the control is held. In toggle mode, one press starts and the next press stops. When recording ends, the transcript posts as a normal user comment and dispatches agent turns as a typed comment does. There is no review step.
- VAD mode is a later feature. Its toggle is visible and disabled in this build.
- Agent responds with text, using the same Rig and tooling that we have today in the dicussions.
- IF:  responses are set to voice THEN: agent’s response is vocalized using the Voice AI TTS model.
- Playback stops at once when the user starts recording or when a new agent reply arrives. A Stop control is visible while audio plays. A new reply is not queued behind the old one.

## Requirements

- OpenAI-like speech API supported (speech-to-text and text-to-speech).
- Transcription mode is configurable per provider: record, then transcribe on release; or streaming transcription while recording (for example OpenAI realtime, Ollama, or llama.cpp where the server supports it). The UI offers only the modes that the provider type can support.
- OpenRouter must be supported for speech-to-text and text-to-speech, using its own speech models and the interface it offers for them.
- Local model serving (like Ollama) must be supported.
- Voice mode switch toggle must be available in all discussions.
- User can select the input mode with a toggle: Push-to-Talk or VAD. Only Push-to-Talk is enabled in this build.
- User can select the way AI agent will respond: Text or Voice.
- Everything that user says using his voice must be transcribed and visualized in chat, attributing it to the user.
- Push-to-Talk supports non-sticky (hold) and sticky (press to start, press to stop) behaviour. The on-screen button and the hotkey both follow the chosen behaviour.
- The hotkey is configurable by the user and works only while the application has focus. It does not fire while the user types in a text field. It is not a global (OS-wide) hotkey.
- A voice failure (microphone denied or missing, provider unreachable, transcription failed) shows an inline error in the composer and posts nothing. The transcript shows live in the composer while recording.
- All voice controls are operable from the keyboard and have accessible names. 
- On macOS, the application declares the microphone permission, asks for it when the user first starts to record, and uses the OS voice processing (noise suppression and echo cancellation) for capture. On other platforms, the standard WebView capture constraints apply.

## Expected Deliverables

- Voice AI tab in the global settings section
- Push-to-Talk mode in all discussions.
- Voice responses in chats.
- Agent configuration window is updated to allow voice configuration.
- Hotkey and hold/toggle settings in the Voice AI section.
- Stop control for agent speech.
- Microphone permission and voice processing on macOS.

## Out of Scope:

- Voice Activation Detection (VAD) is out of scope for now, and will be added later. Its toggle stays disabled.
- A global (OS-wide) hotkey.
- Review or edit of a transcript before it posts.

## Specifications to change

- `specifications/ui/GLS-global-settings.md`: add the Voice AI section to the Global group.
- `specifications/ui/AII-ai-integrations.md`: AII-FR-01 says the AI has exactly two levels. State that Voice AI is a separate section and not a third level.
- `specifications/core/GSS-global-settings-storage.md`: store the Voice AI settings, the hotkey, and the voice of a Personal agent (GSS-FR-30). Keys go in the application secret vault.
- `specifications/core/AAP-ai-api-integrations.md`: AIC points voice APIs here. Specify the Voice AI providers, their verification, and their speech operations, or add a new voice module and fix that pointer.
- `specifications/ui/AGT-agents.md` (AGT-FR-12: the editor has exactly five controls) and `specifications/core/AGR-agent-registry.md` (AGR-FR-01: the agent fields): add the optional voice.
- `specifications/core/PSS-project-settings-storage.md` (PSS-FR-AGFL): the Project agent file holds the voice.
- `specifications/ui/CVP-conversation-presentation.md` (CVP-FR-55: the composer is one field), `specifications/ui/ACT-action-control.md`, and `specifications/ui/DDS-draft-discussion.md`: the voice button, the collapsed text input, the voice controls, and the Stop control in every composer.