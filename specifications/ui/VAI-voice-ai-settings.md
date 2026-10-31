# Voice AI settings

**Spec code:** `VAI`

## Intent
Voice AI is the section of the Settings window where the author chooses the speech provider and the Push-to-Talk key. A person who wants to talk with an agent must tell Synthesis which service turns speech into text and text into speech. That service is separate from the AI that answers. So the section has its own keys, and it is not a third level of `AII-ai-integrations.md`. Verification tells the author what a server can do, because two servers with the same address format can offer very different routes. Out of scope: voice activity detection, an OS-wide hotkey, and the voice of an agent, which the persona editor holds (`AGT-agents.md` AGT-FR-DIVM).

## Functional requirements

### The section

1. **VAI-FR-CEJC** The Global group of the Settings window holds a **Voice AI** section (`GLS-global-settings.md` GLS-FR-03). It populates from "list voice ai integrations" when it opens.
2. **VAI-FR-IDIV** The section has one tab per provider, in this order: OpenAI, OpenRouter, Custom. Each tab configures its provider independently.
3. **VAI-FR-UJNQ** A tab holds a base URL field, a key field, an STT model field, a TTS model field, and a default voice field.
4. **VAI-FR-QARI** Each action applies at once, so the section has no dirty state (`GLS-global-settings.md` GLS-FR-10).
5. **VAI-FR-KWOT** Before any provider is verified, the section states that voice mode needs a verified provider.
6. **VAI-FR-BSDN** While the section is open, a `voice-ai-changed` event refreshes each tab that has no candidate edit.

### Keys and verification

7. **VAI-FR-LLAE** The key field is never filled. It shows the masked hint as its placeholder, and an empty field verifies with the stored key.
8. **VAI-FR-ZYEZ** On Custom, the key field is optional, and it says so.
9. **VAI-FR-MJCA** A Verify action calls "verify voice ai integration" with the values of the tab. The action is disabled while the call runs.
10. **VAI-FR-JSNS** The tab states that verification sends short real requests and that a provider can charge for them.
11. **VAI-FR-MFGW** While Custom verification runs, the tab states that it waits for the server, which can load a model first.
12. **VAI-FR-TOLC** Editing the base URL, key, STT model, or TTS model returns the tab to the unverified state and calls no operation.
13. **VAI-FR-HJHX** The status line shows the time of the last verification or the typed failure, in a message for each code of `../core/VOP-voice-ai-providers.md` VOP-FR-DBDI.
14. **VAI-FR-OFNT** After verification, the tab shows one line for the detected capabilities: live or turn-based transcription with its rate, or file transcription, and the speech route.
15. **VAI-FR-LWCD** Where the server refused a model on its Realtime route (`../core/VOP-voice-ai-providers.md` VOP-FR-AVXY), the tab shows the server's message.
16. **VAI-FR-GZOH** An Activate action calls "set active voice ai integration". It is enabled only while the tab is verified.
17. **VAI-FR-AWEA** A Clear action calls "clear voice ai integration".

### Routes and models

18. **VAI-FR-HQCG** The transcription and speech selectors offer only the routes in the detected capabilities. A change calls "set voice ai routes" at once.
19. **VAI-FR-OLNG** Before verification, the route selectors are disabled and state that verification finds the routes.
20. **VAI-FR-PKZY** When the selected transcription route is not live, the tab states that the text appears after recording ends.
21. **VAI-FR-WNVT** The STT and TTS model fields accept free text and suggest the result of "list voice ai models". A failed list leaves them as plain fields.
22. **VAI-FR-ZEZK** The Custom base URL field shows `http://localhost:8080` as its placeholder.
23. **VAI-FR-EEMC** On Custom, an empty TTS model is valid, and the tab states that spoken replies stay off.
24. **VAI-FR-NIWH** When the speech route is a `llamacpp` Realtime route, the tab states that agent voices have no effect on it and that the `/tts` route takes a voice.
25. **VAI-FR-OJBE** The default voice field calls "set voice ai default voice" when it loses focus with a changed value. It offers the provider's voices where a list exists.

### Push-to-Talk

26. **VAI-FR-HVNJ** The section holds a Push-to-Talk behaviour control with two values: **Hold** and **Toggle**. The default is Hold. It saves through `patchAppPreferences`.
27. **VAI-FR-RASA** The section holds a hotkey capture control. The default hotkey is `Ctrl+Shift+Space`. A control to restore the default is present.
28. **VAI-FR-FRJY** A captured hotkey holds at least one modifier, or it is a function key. The control refuses each other chord with an inline message.
29. **VAI-FR-BGBL** The control refuses a chord the application already binds, on Ctrl or Cmd: Z, Shift+Z, X, C, V, A, S, F, R, Q, J, K, Backslash, Enter; and Alt+Up, Alt+Down. It names the reason inline.
30. **VAI-FR-WOCD** With Hold behaviour, the control refuses a chord that holds Cmd. Choosing Hold while the stored chord holds Cmd restores the default hotkey and states this inline.
    - *Why:* macOS does not report `keyup` of a key released while Cmd is held, so a held recording never ends.
31. **VAI-FR-AVBR** A change of behaviour or hotkey reaches the main window through the app-preferences change event, with no restart.

## Wireframes

```
Voice AI
[ OpenAI ] [ OpenRouter ] [ Custom ]

Base URL   [ http://localhost:8080                ]
Key        [ •••• (optional)                      ]
STT model  [ voxtral-mini-4b-realtime          v  ]
TTS model  [ lfm3-audio                        v  ]
Voice      [                                      ]
Verification sends short real requests. A provider can charge for them.
[ Verify ]  [ Activate ]  [ Clear ]
Verified 14:02. Live transcription, 16 kHz. Speech: Realtime.
Transcription [ Realtime v ]   Speech [ Realtime v ]

Push-to-Talk
Behaviour  ( ) Hold  ( ) Toggle
Hotkey     [ Ctrl+Shift+Space ]  [ Restore default ]
```

## UI contract boundary
- **Owned by the UI**: the tabs, candidate field values, the unverified state after an edit, hotkey capture and its refusals, and all messages.
- **Delegated to backend (abstract)**: "list voice ai integrations", "verify voice ai integration", "set voice ai routes", "set voice ai default voice", "set active voice ai integration", "clear voice ai integration", "list voice ai models", "list voice ai voices", each owned by `../core/VOP-voice-ai-providers.md`; "save app preferences", owned by `../core/GSS-global-settings-storage.md`.

## Non-functional requirements
- Each control has an accessible name, and each is operable from the keyboard.
- The hotkey capture control takes keys only while it has focus, and Escape leaves it unchanged.
- No key reaches the webview after the call that submits it.
