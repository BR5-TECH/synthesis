/**
 * The voice of an agent in the persona editor (`AGT-agents.md` AGT-FR-DIVM,
 * AGT-FR-MWOF): the voice, the speed, and the speaking style.
 */
import { SPEECH_STYLE_LIMIT } from "../types";
import { SPEED_CHOICES } from "./VoiceSpeechOptions";

export interface AgentVoiceValue {
  voice: string;
  /** Null for the provider's speed. */
  speedPercent: number | null;
  style: string;
}

export function AgentVoiceFields({
  value,
  voices,
  ready,
  onChange,
}: {
  value: AgentVoiceValue;
  /** The provider's voices, or null for free text (AGT-FR-DIVM). */
  voices: string[] | null | undefined;
  ready: boolean;
  onChange: (next: AgentVoiceValue) => void;
}) {
  const { voice } = value;
  return (
    <>
      {/* AGT-FR-DIVM. */}
      <label style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        <span className="t-ui-sm">Voice</span>
        {voices ? (
          <select
            className="input"
            // The sticky footer covers the end of the card, so a focused control
            // scrolls clear of it.
            style={{ scrollMarginBottom: 72 }}
            data-testid="agent-voice"
            disabled={!ready}
            value={voice}
            onChange={(e) => onChange({ ...value, voice: e.target.value })}
          >
            <option value="">Provider default</option>
            {voice !== "" && !voices.includes(voice) && <option value={voice}>{voice}</option>}
            {voices.map((name) => (
              <option key={name} value={name}>
                {name}
              </option>
            ))}
          </select>
        ) : (
          <input
            className="input"
            style={{ scrollMarginBottom: 72 }}
            data-testid="agent-voice"
            disabled={!ready}
            placeholder="Provider default"
            value={voice}
            onChange={(e) => onChange({ ...value, voice: e.target.value })}
          />
        )}
        <span className="t-ui-sm t-muted">Optional.</span>
      </label>

      {/* AGT-FR-MWOF. */}
      <div style={{ display: "flex", gap: 8 }}>
        <label style={{ display: "flex", flexDirection: "column", gap: 4, flex: "0 0 auto" }}>
          <span className="t-ui-sm">Voice speed</span>
          <select
            className="input"
            style={{ scrollMarginBottom: 72 }}
            data-testid="agent-voice-speed"
            disabled={!ready}
            value={value.speedPercent ?? ""}
            onChange={(e) =>
              onChange({
                ...value,
                speedPercent: e.target.value === "" ? null : Number(e.target.value),
              })
            }
          >
            <option value="">Provider default</option>
            {SPEED_CHOICES.map((choice) => (
              <option key={choice} value={choice}>
                {choice}%
              </option>
            ))}
          </select>
        </label>
        <label style={{ display: "flex", flexDirection: "column", gap: 4, flex: "1 1 auto" }}>
          <span className="t-ui-sm">Speaking style</span>
          <input
            className="input"
            style={{ scrollMarginBottom: 72 }}
            data-testid="agent-voice-style"
            disabled={!ready}
            maxLength={SPEECH_STYLE_LIMIT}
            placeholder="Provider default"
            value={value.style}
            onChange={(e) => onChange({ ...value, style: e.target.value })}
          />
        </label>
      </div>
    </>
  );
}
