/**
 * The VAD part of the Voice AI section (`VAI-voice-ai-settings.md` VAI-FR-MHHE,
 * VAI-FR-ACFG). Each change saves at once. The backend reads both values from
 * the store, so an open session applies them from its next utterance.
 */
import { useEffect, useState } from "react";

import { logWarn } from "../logging";
import { loadAppPreferences, patchAppPreferences } from "../state/appPreferences";
import type { VoiceVadSensitivity } from "../types";

export const SILENCE_MIN_MS = 500;
export const SILENCE_MAX_MS = 5000;
export const SILENCE_STEP_MS = 100;
export const SILENCE_DEFAULT_MS = 1500;

const LEVELS: { value: VoiceVadSensitivity; name: string }[] = [
  { value: "low", name: "Low" },
  { value: "medium", name: "Medium" },
  { value: "high", name: "High" },
];

/** VAI-FR-WVEV. */
export function clampSilenceMs(stored: number | undefined): number {
  const value = stored ?? SILENCE_DEFAULT_MS;
  return Math.min(SILENCE_MAX_MS, Math.max(SILENCE_MIN_MS, value));
}

function seconds(ms: number): string {
  return `${(ms / 1000).toFixed(1)} s`;
}

export function VoiceVadSettings() {
  const [silence, setSilence] = useState(SILENCE_DEFAULT_MS);
  const [saved, setSaved] = useState(SILENCE_DEFAULT_MS);
  const [sensitivity, setSensitivity] = useState<VoiceVadSensitivity>("medium");

  useEffect(() => {
    let cancelled = false;
    loadAppPreferences()
      .then((prefs) => {
        if (cancelled) return;
        const shown = clampSilenceMs(prefs.voiceVadSilenceMs);
        setSilence(shown);
        setSaved(shown);
        setSensitivity(prefs.voiceVadSensitivity ?? "medium");
      })
      .catch(() => logWarn(["frontend"], "the VAD settings could not be read", {}));
    return () => {
      cancelled = true;
    };
  }, []);

  // VAI-FR-NWOW: a drag commits once, when it ends.
  const commitSilence = () => {
    if (silence === saved) return;
    const before = saved;
    setSaved(silence);
    patchAppPreferences({ voiceVadSilenceMs: silence }).catch(() => {
      // The next commit tries again.
      setSaved(before);
      logWarn(["frontend"], "the silence timeout was not saved", {});
    });
  };

  const chooseSensitivity = (next: VoiceVadSensitivity) => {
    setSensitivity(next);
    patchAppPreferences({ voiceVadSensitivity: next }).catch(() => {
      logWarn(["frontend"], "the sensitivity was not saved", {});
    });
  };

  return (
    <div data-testid="voice-vad-settings" style={{ marginTop: 20 }}>
      <h3 className="t-h3">VAD</h3>
      {/* VAI-FR-MZZU. */}
      <div className="t-ui-sm t-muted" data-testid="voice-vad-scope" style={{ marginBottom: 10 }}>
        These settings apply to VAD only.
      </div>
      <div style={{ marginBottom: 12, maxWidth: 360 }}>
        <label className="t-label" htmlFor="voice-vad-silence">
          Silence timeout — {seconds(silence)}
        </label>
        <input
          id="voice-vad-silence"
          type="range"
          data-testid="voice-vad-silence"
          min={SILENCE_MIN_MS}
          max={SILENCE_MAX_MS}
          step={SILENCE_STEP_MS}
          value={silence}
          aria-valuetext={seconds(silence)}
          onChange={(event) => setSilence(Number(event.target.value))}
          onPointerUp={commitSilence}
          onKeyUp={commitSilence}
          onBlur={commitSilence}
          style={{ display: "block", width: "100%", marginTop: 8, accentColor: "var(--accent)" }}
        />
        <span className="t-ui-xs t-muted">
          {seconds(SILENCE_MIN_MS)} – {seconds(SILENCE_MAX_MS)}
        </span>
      </div>
      <div role="radiogroup" aria-label="Sensitivity">
        <span className="t-label" style={{ marginRight: 12 }}>
          Sensitivity
        </span>
        {LEVELS.map(({ value, name }) => (
          <label key={value} style={{ marginRight: 12 }}>
            <input
              type="radio"
              name="voice-vad-sensitivity"
              checked={sensitivity === value}
              onChange={() => chooseSensitivity(value)}
            />{" "}
            {name}
          </label>
        ))}
      </div>
    </div>
  );
}
