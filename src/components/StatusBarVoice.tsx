/**
 * The voice indicator of the status bar (`STB-status-bar.md` STB-FR-OSOL,
 * STB-FR-JHZF, STB-FR-KWMV, STB-FR-PYRC, STB-FR-UXGE, STB-FR-ZBLQ).
 *
 * It reads the session state of voice mode and holds no state of its own. The
 * indicator opens the voice discussion, and a separate Stop control ends voice
 * mode (`VOM-voice-mode.md` VOM-FR-XDUY, VOM-FR-UXZT).
 */
import { Icon } from "./icons";
import {
  useVoiceState,
  voiceIndicatorState,
  type VoiceIndicatorState,
} from "../state/voiceSession";
import { stopVoiceMode } from "../voice/voiceController";

const NAMES: Record<VoiceIndicatorState, string> = {
  paused: "Paused",
  hearing: "Hearing speech",
  transcribing: "Transcribing",
  speaking: "Speaking",
  listening: "Listening",
  idle: "Idle",
};

function StateIcon({ state }: { state: VoiceIndicatorState }) {
  switch (state) {
    case "paused":
      return <Icon.Pause size={12} />;
    case "hearing":
      return <Icon.Waveform size={12} />;
    case "transcribing":
      return <Icon.TextLines size={12} />;
    case "speaking":
      return <Icon.Speaker size={12} />;
    case "listening":
      return <Icon.MicListening size={12} />;
    default:
      return <Icon.Mic size={12} />;
  }
}

export function StatusBarVoice({ onReveal }: { onReveal: () => void }) {
  const voice = useVoiceState();
  if (voice.activeKey === null) return null;
  const state = voiceIndicatorState(voice);
  const name = NAMES[state];
  const owner = voice.ownerName ?? "Discussion";
  return (
    <div className="status-bar__voice" data-testid="status-bar-voice">
      <button
        type="button"
        className="status-bar__voice-indicator"
        data-testid="status-bar-voice-indicator"
        data-state={state}
        aria-label={`Voice mode ${name} in ${owner} — open discussion`}
        title={`Voice mode ${name} in ${owner} — open discussion`}
        onClick={onReveal}
      >
        <StateIcon state={state} />
        <span className="status-bar__voice-name">{owner}</span>
        <span className="status-bar__voice-state">: {name}</span>
      </button>
      <button
        type="button"
        className="status-bar__btn"
        data-testid="status-bar-voice-stop"
        aria-label="Stop voice mode"
        title="Stop voice mode"
        onClick={stopVoiceMode}
      >
        <Icon.Stop size={12} />
      </button>
    </div>
  );
}
