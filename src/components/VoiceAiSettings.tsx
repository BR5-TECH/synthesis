/**
 * Global settings → **Voice AI** (`VAI-voice-ai-settings.md`).
 *
 * The speech providers, the Push-to-Talk key, and the VAD settings. Each action applies at once, so
 * the section has no dirty state (VAI-FR-QARI). Candidate field values stay
 * here until a verification commits them, and a key never comes back from the
 * backend (VAI-FR-LLAE).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import * as api from "../api";
import { onVoiceAiChanged } from "../events";
import { logInfo, logWarn } from "../logging";
import { loadAppPreferences, patchAppPreferences } from "../state/appPreferences";
import type {
  VoiceAiIntegrations,
  VoiceAiRecord,
  VoiceCapabilities,
  VoiceHotkeyBehaviour,
  VoiceProviderId,
  VoiceSpeechRoute,
  VoiceTranscriptionRoute,
} from "../types";
import { DEFAULT_VOICE_HOTKEY, chordOf, hotkeyProblem, type HotkeyProblem } from "../voice/hotkey";
import { errorCode, voiceMessage } from "../voice/voiceMessages";
import { SETTINGS_TABLIST_STYLE, settingsTabStyle } from "./settingsTabs";
import { VoiceVadSettings } from "./VoiceVadSettings";

const TAB_ORDER: VoiceProviderId[] = ["openai", "openrouter", "custom"];
const TAB_NAMES: Record<VoiceProviderId, string> = {
  openai: "OpenAI",
  openrouter: "OpenRouter",
  custom: "Custom",
};
const CUSTOM_URL_HINT = "http://localhost:8080";

const SPEECH_NAMES: Record<VoiceSpeechRoute, string> = {
  realtime: "Realtime",
  openai_speech: "/audio/speech",
  llama_tts: "/tts",
};

interface Candidate {
  baseUrl: string;
  apiKey: string;
  sttModel: string;
  ttsModel: string;
}

function candidateOf(record: VoiceAiRecord): Candidate {
  return {
    baseUrl: record.baseUrl,
    apiKey: "",
    sttModel: record.sttModel,
    ttsModel: record.ttsModel,
  };
}

/** VAI-FR-OFNT: one line for the detected capabilities. */
export function capabilityLine(record: VoiceAiRecord): string {
  const c = record.capabilities;
  if (!c) return "";
  const rate = c.realtimeInputRate ? `, ${c.realtimeInputRate / 1000} kHz` : "";
  const listening =
    c.realtimeTranscription === "live"
      ? `Live transcription${rate}.`
      : c.realtimeTranscription === "turn"
        ? `Turn-based transcription${rate}.`
        : "File transcription.";
  const speech = record.speechRoute
    ? `Speech: ${SPEECH_NAMES[record.speechRoute]}.`
    : "No spoken replies.";
  return `${listening} ${speech}`;
}

function transcriptionRoutes(c: VoiceCapabilities): VoiceTranscriptionRoute[] {
  const routes: VoiceTranscriptionRoute[] = [];
  if (c.realtimeTranscription) routes.push("realtime");
  if (c.httpTranscription) routes.push("http");
  return routes;
}

function speechRoutes(c: VoiceCapabilities): VoiceSpeechRoute[] {
  const routes: VoiceSpeechRoute[] = [];
  if (c.realtimeSpeech) routes.push("realtime");
  if (c.openaiSpeech) routes.push("openai_speech");
  if (c.llamaTts) routes.push("llama_tts");
  return routes;
}

const HOTKEY_PROBLEMS: Record<HotkeyProblem, string> = {
  no_modifier: "A hotkey needs a modifier, or a function key.",
  reserved: "The application already uses this key.",
  holds_cmd: "Hold behaviour cannot use Cmd, because macOS does not report its release.",
};

function PushToTalkSettings() {
  const [behaviour, setBehaviour] = useState<VoiceHotkeyBehaviour>("hold");
  const [hotkey, setHotkey] = useState(DEFAULT_VOICE_HOTKEY);
  const [capturing, setCapturing] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void loadAppPreferences().then((prefs) => {
      if (cancelled) return;
      setBehaviour(prefs.voiceHotkeyBehaviour ?? "hold");
      setHotkey(prefs.voiceHotkey ?? DEFAULT_VOICE_HOTKEY);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const saveHotkey = (next: string) => {
    setHotkey(next);
    void patchAppPreferences({ voiceHotkey: next });
  };

  const chooseBehaviour = (next: VoiceHotkeyBehaviour) => {
    setBehaviour(next);
    // VAI-FR-WOCD: a stored chord that holds Cmd cannot serve Hold.
    if (hotkeyProblem(hotkey, next) === "holds_cmd") {
      setHotkey(DEFAULT_VOICE_HOTKEY);
      setMessage("Hold behaviour cannot use Cmd, so the default hotkey is restored.");
      void patchAppPreferences({ voiceHotkeyBehaviour: next, voiceHotkey: DEFAULT_VOICE_HOTKEY });
      return;
    }
    setMessage(null);
    void patchAppPreferences({ voiceHotkeyBehaviour: next });
  };

  const onCaptureKey = (event: React.KeyboardEvent) => {
    if (!capturing) return;
    event.preventDefault();
    if (event.key === "Escape") {
      setCapturing(false);
      return;
    }
    const chord = chordOf(event);
    if (chord === null) return;
    const problem = hotkeyProblem(chord, behaviour);
    if (problem) {
      setMessage(HOTKEY_PROBLEMS[problem]);
      return;
    }
    setMessage(null);
    setCapturing(false);
    saveHotkey(chord);
  };

  return (
    <div data-testid="voice-push-to-talk" style={{ marginTop: 20 }}>
      <h3 className="t-h3">Push-to-Talk</h3>
      {/* VAI-FR-HVNJ, VAI-FR-RASA. */}
      <div className="t-ui-sm t-muted" data-testid="voice-ptt-scope" style={{ marginBottom: 10 }}>
        These settings apply to Push-to-Talk only.
      </div>
      <div role="radiogroup" aria-label="Push-to-Talk behaviour" style={{ marginBottom: 10 }}>
        {(["hold", "toggle"] as const).map((value) => (
          <label key={value} style={{ marginRight: 12 }}>
            <input
              type="radio"
              name="voice-behaviour"
              checked={behaviour === value}
              onChange={() => chooseBehaviour(value)}
            />{" "}
            {value === "hold" ? "Hold" : "Toggle"}
          </label>
        ))}
      </div>
      <div className="voice-settings__hotkey">
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          data-testid="voice-hotkey"
          aria-label={`Push-to-Talk hotkey: ${hotkey}`}
          onClick={() => {
            setMessage(null);
            setCapturing(true);
          }}
          onKeyDown={onCaptureKey}
          onBlur={() => setCapturing(false)}
        >
          {capturing ? "Press a key…" : hotkey}
        </button>
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          data-testid="voice-hotkey-default"
          onClick={() => {
            setMessage(null);
            saveHotkey(DEFAULT_VOICE_HOTKEY);
          }}
        >
          Restore default
        </button>
      </div>
      {message && (
        <div className="t-ui-sm t-muted" role="status" data-testid="voice-hotkey-message">
          {message}
        </div>
      )}
    </div>
  );
}

export function VoiceAiSettings() {
  const [integrations, setIntegrations] = useState<VoiceAiIntegrations | null>(null);
  const [provider, setProvider] = useState<VoiceProviderId>("openai");
  const [drafts, setDrafts] = useState<Partial<Record<VoiceProviderId, Candidate>>>({});
  const [dirty, setDirty] = useState<Partial<Record<VoiceProviderId, boolean>>>({});
  const [verifying, setVerifying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [suggestions, setSuggestions] = useState<{ stt: string[]; tts: string[] }>({
    stt: [],
    tts: [],
  });
  /** The URL that the model list is for. It follows a finished edit, not a key press. */
  const [listedUrl, setListedUrl] = useState<string | null>(null);
  const [voices, setVoices] = useState<string[]>([]);

  // A ref, because `apply` reads the marks from an event listener.
  const dirtyRef = useRef(dirty);
  dirtyRef.current = dirty;

  const apply = useCallback((next: VoiceAiIntegrations) => {
    setIntegrations(next);
    // VAI-FR-BSDN: a tab with a candidate edit stays as the author left it.
    setDrafts((held) => {
      const out = { ...held };
      for (const record of next.records) {
        if (!dirtyRef.current[record.provider]) out[record.provider] = candidateOf(record);
      }
      return out;
    });
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void api
      .listVoiceAiIntegrations()
      .then((list) => {
        if (!cancelled) apply(list);
      })
      .catch((e: unknown) => setError(errorCode(e)));
    void onVoiceAiChanged(apply).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [apply]);

  const record = integrations?.records.find((r) => r.provider === provider) ?? null;
  const draft = drafts[provider] ?? (record ? candidateOf(record) : null);
  const unverified = !record?.verifiedAt || dirty[provider] === true;

  // VAI-FR-WNVT: suggestions only, and a failed list leaves plain fields. The list
  // runs for a saved URL and when the URL field loses focus, so that a partial
  // host gets no request (VOP-FR-TOCX).
  const savedUrl = record?.baseUrl ?? "";
  useEffect(() => setListedUrl(null), [provider]);
  const modelsUrl = listedUrl ?? savedUrl;
  useEffect(() => {
    let cancelled = false;
    if (modelsUrl === "") {
      setSuggestions({ stt: [], tts: [] });
      return;
    }
    api
      .listVoiceAiModels(provider, modelsUrl, null)
      .then((models) => {
        if (!cancelled) setSuggestions({ stt: models.sttModels, tts: models.ttsModels });
      })
      .catch(() => {
        if (!cancelled) setSuggestions({ stt: [], tts: [] });
      });
    return () => {
      cancelled = true;
    };
  }, [provider, modelsUrl]);

  // VAI-FR-OJBE: the voices of this tab's provider, where a list exists.
  useEffect(() => {
    let cancelled = false;
    api
      .listVoiceAiVoices(provider)
      .then((list) => {
        if (!cancelled) setVoices(list ?? []);
      })
      .catch(() => {
        if (!cancelled) setVoices([]);
      });
    return () => {
      cancelled = true;
    };
  }, [provider]);

  // VAI-FR-TOLC: an edit changes this tab and calls no operation.
  const edit = (change: Partial<Candidate>) => {
    if (!draft) return;
    setDrafts((held) => ({ ...held, [provider]: { ...draft, ...change } }));
    setDirty((marks) => ({ ...marks, [provider]: true }));
    setError(null);
  };

  const verify = async () => {
    if (!draft) return;
    setVerifying(true);
    setError(null);
    try {
      const verified = await api.verifyVoiceAiIntegration(
        provider,
        draft.baseUrl,
        draft.apiKey === "" ? null : draft.apiKey,
        draft.sttModel,
        draft.ttsModel,
      );
      logInfo(["frontend"], "a voice provider verified", { provider });
      setDirty((marks) => ({ ...marks, [provider]: false }));
      setDrafts((held) => ({ ...held, [provider]: candidateOf(verified) }));
      setIntegrations((held) =>
        held
          ? {
              ...held,
              records: held.records.map((r) => (r.provider === provider ? verified : r)),
            }
          : held,
      );
    } catch (e) {
      const code = errorCode(e);
      logWarn(["frontend"], "a voice provider did not verify", { provider, code });
      setError(code);
    } finally {
      setVerifying(false);
    }
  };

  const act = (operation: Promise<VoiceAiIntegrations>) => {
    operation.then(apply).catch((e: unknown) => setError(errorCode(e)));
  };

  const setRoutes = (
    transcription: VoiceTranscriptionRoute,
    speech: VoiceSpeechRoute | null,
  ) => {
    api
      .setVoiceAiRoutes(provider, transcription, speech)
      .then((updated) =>
        setIntegrations((held) =>
          held
            ? {
                ...held,
                records: held.records.map((r) => (r.provider === provider ? updated : r)),
              }
            : held,
        ),
      )
      .catch((e: unknown) => setError(errorCode(e)));
  };

  const capabilities = record?.capabilities;
  const statusLine = useMemo(() => {
    if (!record?.verifiedAt || unverified) return "Not verified.";
    return `Verified ${new Date(record.verifiedAt).toLocaleString()}.`;
  }, [record, unverified]);

  if (!integrations || !record || !draft) {
    return <div className="t-muted">Loading…</div>;
  }

  const custom = provider === "custom";
  const notLive =
    record.transcriptionRoute === "http" || capabilities?.realtimeTranscription === "turn";
  return (
    <>
      <div style={{ marginBottom: 16 }} data-testid="voice-ai-explanation">
        <p className="t-p" style={{ margin: 0 }}>
          The service that turns speech into text and text into speech. It has its own keys, kept
          in this machine's keychain and never shown back to you. One provider is active at a time.
        </p>
        {/* VAI-FR-KWOT. */}
        {integrations.active === null && (
          <p className="t-p" style={{ margin: "8px 0 0" }} data-testid="voice-ai-empty">
            Voice mode needs a verified, active provider.
          </p>
        )}
      </div>

      <div role="tablist" aria-label="Voice AI" style={SETTINGS_TABLIST_STYLE}>
        {TAB_ORDER.map((id) => (
          <button
            key={id}
            role="tab"
            aria-selected={id === provider}
            className="btn btn--ghost btn--sm"
            data-active={id === provider}
            style={settingsTabStyle(id === provider)}
            onClick={() => {
              setProvider(id);
              setError(null);
            }}
          >
            {TAB_NAMES[id]}
            {integrations.active === id && (
              <span className="badge badge--ok" style={{ marginLeft: 6 }}>
                active
              </span>
            )}
          </button>
        ))}
      </div>

      <div role="tabpanel" aria-label={TAB_NAMES[provider]} data-testid={`voice-panel-${provider}`}>
        <div className="picker-field" style={{ marginBottom: 10 }}>
          <label className="picker-field__label" htmlFor="voice-url">
            Base URL
          </label>
          <input
            id="voice-url"
            className="input input--mono"
            spellCheck={false}
            autoComplete="off"
            placeholder={custom ? CUSTOM_URL_HINT : undefined}
            value={draft.baseUrl}
            onChange={(e) => edit({ baseUrl: e.target.value })}
            onBlur={(e) => setListedUrl(e.target.value)}
          />
        </div>
        <div className="picker-field" style={{ marginBottom: 10 }}>
          <label className="picker-field__label" htmlFor="voice-key">
            {custom ? "Key (optional)" : "Key"}
          </label>
          <input
            id="voice-key"
            type="password"
            className="input input--mono"
            autoComplete="off"
            placeholder={record.maskedHint ? `•••• ${record.maskedHint}` : undefined}
            value={draft.apiKey}
            onChange={(e) => edit({ apiKey: e.target.value })}
          />
        </div>
        <div className="picker-field" style={{ marginBottom: 10 }}>
          <label className="picker-field__label" htmlFor="voice-stt">
            STT model
          </label>
          <input
            id="voice-stt"
            className="input input--mono"
            list="voice-stt-models"
            spellCheck={false}
            value={draft.sttModel}
            onChange={(e) => edit({ sttModel: e.target.value })}
          />
          <datalist id="voice-stt-models">
            {suggestions.stt.map((model) => (
              <option key={model} value={model} />
            ))}
          </datalist>
        </div>
        <div className="picker-field" style={{ marginBottom: 10 }}>
          <label className="picker-field__label" htmlFor="voice-tts">
            TTS model
          </label>
          <input
            id="voice-tts"
            className="input input--mono"
            list="voice-tts-models"
            spellCheck={false}
            value={draft.ttsModel}
            onChange={(e) => edit({ ttsModel: e.target.value })}
          />
          <datalist id="voice-tts-models">
            {suggestions.tts.map((model) => (
              <option key={model} value={model} />
            ))}
          </datalist>
          {/* VAI-FR-EEMC. */}
          {custom && draft.ttsModel.trim() === "" && (
            <div className="t-ui-sm t-muted" data-testid="voice-no-tts">
              With no TTS model, spoken replies stay off.
            </div>
          )}
        </div>
        <div className="picker-field" style={{ marginBottom: 10 }}>
          <label className="picker-field__label" htmlFor="voice-default">
            Default voice
          </label>
          <input
            id="voice-default"
            className="input input--mono"
            list="voice-voices"
            spellCheck={false}
            defaultValue={record.defaultVoice}
            key={`${provider}-${record.defaultVoice}`}
            onBlur={(e) => {
              if (e.target.value === record.defaultVoice) return;
              api
                .setVoiceAiDefaultVoice(provider, e.target.value)
                .catch((err: unknown) => setError(errorCode(err)));
            }}
          />
          <datalist id="voice-voices">
            {voices.map((name) => (
              <option key={name} value={name} />
            ))}
          </datalist>
        </div>

        {/* VAI-FR-JSNS. */}
        <p className="t-ui-sm t-muted" data-testid="voice-cost">
          Verification sends short real requests. A provider can charge for them.
        </p>
        <div style={{ display: "flex", gap: 6, marginBottom: 8 }}>
          <button
            type="button"
            className="btn btn--sm"
            data-testid="voice-verify"
            disabled={verifying}
            onClick={() => void verify()}
          >
            Verify
          </button>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            data-testid="voice-activate"
            disabled={unverified || integrations.active === provider}
            onClick={() => act(api.setActiveVoiceAiIntegration(provider))}
          >
            Activate
          </button>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            data-testid="voice-clear"
            onClick={() => {
              setDirty((marks) => ({ ...marks, [provider]: false }));
              setDrafts((held) => ({ ...held, [provider]: undefined }));
              act(api.clearVoiceAiIntegration(provider));
            }}
          >
            Clear
          </button>
        </div>

        {/* VAI-FR-MFGW. */}
        {verifying && custom && (
          <div className="t-ui-sm t-muted" data-testid="voice-waiting">
            Waiting for the server. It can load a model first.
          </div>
        )}
        <div className="t-ui-sm" data-testid="voice-status">
          {statusLine}
        </div>
        {error && (
          <span className="picker-error" role="alert" data-testid="voice-settings-error">
            ✗ {voiceMessage(error)}
          </span>
        )}
        {/* VAI-FR-OFNT. */}
        {!unverified && (
          <div className="t-ui-sm voice-settings__capabilities" data-testid="voice-capabilities">
            {capabilityLine(record)}
          </div>
        )}
        {/* VAI-FR-LWCD. */}
        {!unverified && record.capabilityNote && (
          <div className="t-ui-sm t-muted" data-testid="voice-capability-note">
            {record.capabilityNote}
          </div>
        )}

        {/* VAI-FR-HQCG, VAI-FR-OLNG. */}
        <div className="voice-settings__routes">
          <label className="t-ui-sm">
            Transcription{" "}
            <select
              className="input"
              data-testid="voice-transcription-route"
              disabled={!capabilities || unverified}
              value={record.transcriptionRoute}
              onChange={(e) =>
                setRoutes(
                  e.target.value as VoiceTranscriptionRoute,
                  record.speechRoute ?? null,
                )
              }
            >
              {(capabilities ? transcriptionRoutes(capabilities) : ["http" as const]).map(
                (route) => (
                  <option key={route} value={route}>
                    {route === "realtime" ? "Realtime" : "File"}
                  </option>
                ),
              )}
            </select>
          </label>
          <label className="t-ui-sm">
            Speech{" "}
            <select
              className="input"
              data-testid="voice-speech-route"
              disabled={!capabilities || unverified || speechRoutes(capabilities).length === 0}
              value={record.speechRoute ?? ""}
              onChange={(e) =>
                setRoutes(record.transcriptionRoute, e.target.value as VoiceSpeechRoute)
              }
            >
              {!record.speechRoute && <option value="">None</option>}
              {(capabilities ? speechRoutes(capabilities) : []).map((route) => (
                <option key={route} value={route}>
                  {SPEECH_NAMES[route]}
                </option>
              ))}
            </select>
          </label>
        </div>
        {!capabilities && (
          <div className="t-ui-sm t-muted" data-testid="voice-routes-hint">
            Verification finds the routes.
          </div>
        )}
        {/* VAI-FR-PKZY. */}
        {capabilities && !unverified && notLive && (
          <div className="t-ui-sm t-muted" data-testid="voice-not-live">
            The text appears after recording ends.
          </div>
        )}
        {/* VAI-FR-NIWH. */}
        {capabilities?.dialect === "llamacpp" && record.speechRoute === "realtime" && (
          <div className="t-ui-sm t-muted" data-testid="voice-no-agent-voice">
            Agent voices have no effect on this route. The /tts route takes a voice.
          </div>
        )}
      </div>

      <PushToTalkSettings />
      <VoiceVadSettings />
    </>
  );
}
