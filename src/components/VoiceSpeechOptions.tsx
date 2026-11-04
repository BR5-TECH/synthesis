/**
 * The speed and the style of a provider's speech (`VAI-voice-ai-settings.md`
 * VAI-FR-USDR, VAI-FR-EIHC).
 */
import { useRef } from "react";

import * as api from "../api";
import {
  SPEECH_SPEED_MAX,
  SPEECH_SPEED_MIN,
  SPEECH_SPEED_STEP,
  SPEECH_STYLE_LIMIT,
  type VoiceAiRecord,
  type VoiceProviderId,
} from "../types";
import { logWarn } from "../logging";
import { errorCode } from "../voice/voiceMessages";

/** VAI-FR-USDR, AGT-FR-MWOF: 50% to 200% in steps of 10%. */
export const SPEED_CHOICES: number[] = Array.from(
  { length: (SPEECH_SPEED_MAX - SPEECH_SPEED_MIN) / SPEECH_SPEED_STEP + 1 },
  (_, i) => SPEECH_SPEED_MIN + i * SPEECH_SPEED_STEP,
);

export function VoiceSpeechOptions({
  provider,
  record,
  verified,
  onError,
}: {
  provider: VoiceProviderId;
  record: VoiceAiRecord;
  verified: boolean;
  onError: (code: string) => void;
}) {
  const speed = record.speechSpeedPercent ?? 100;
  const style = record.speechStyle ?? "";
  const capabilities = record.capabilities;
  // VAI-FR-USDR: the values of the last save, so a save of one control does
  // not take the other back before the record refreshes.
  const sent = useRef<{ record: VoiceAiRecord; speed: number | null; style: string } | null>(null);
  if (sent.current?.record !== record) {
    sent.current = { record, speed: record.speechSpeedPercent ?? null, style };
  }
  const save = (speedPercent: number | null, nextStyle: string) => {
    if (sent.current) {
      sent.current.speed = speedPercent;
      sent.current.style = nextStyle;
    }
    api.setVoiceAiSpeechOptions(provider, speedPercent, nextStyle).catch((err: unknown) => {
      const code = errorCode(err);
      logWarn(["frontend"], "voice speech options were not saved", { code });
      onError(code);
    });
  };
  return (
    <>
      <div className="picker-field" style={{ marginBottom: 10 }}>
        <label className="picker-field__label" htmlFor="voice-speed">
          Speed
        </label>
        <select
          id="voice-speed"
          className="input"
          data-testid="voice-speed"
          value={speed}
          onChange={(e) => {
            const next = Number(e.target.value);
            save(next === 100 ? null : next, sent.current?.style ?? style);
          }}
        >
          {SPEED_CHOICES.map((value) => (
            <option key={value} value={value}>
              {value}%
            </option>
          ))}
        </select>
      </div>
      <div className="picker-field" style={{ marginBottom: 10 }}>
        <label className="picker-field__label" htmlFor="voice-style">
          Speaking style
        </label>
        <input
          id="voice-style"
          className="input"
          data-testid="voice-style"
          maxLength={SPEECH_STYLE_LIMIT}
          placeholder="Calm, slow, warm"
          defaultValue={style}
          key={`${provider}-${style}`}
          onBlur={(e) => {
            const next = e.target.value.trim();
            if (next === (sent.current?.style ?? style)) return;
            save(sent.current?.speed ?? record.speechSpeedPercent ?? null, next);
          }}
        />
      </div>
      {/* VAI-FR-EIHC. */}
      {verified && capabilities && capabilities.speechSpeed !== "server" && (
        <div className="t-ui-sm t-muted" data-testid="voice-speed-client">
          This server cannot change speed. Playback changes speed and pitch.
        </div>
      )}
      {verified && capabilities && !capabilities.speechInstructions && (
        <div className="t-ui-sm t-muted" data-testid="voice-style-unsupported">
          This server does not support a speaking style.
        </div>
      )}
    </>
  );
}
