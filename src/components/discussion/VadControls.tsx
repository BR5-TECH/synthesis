/**
 * The voice controls of VAD mode (`VOM-voice-mode.md` VOM-FR-TISN, VOM-FR-VOUG,
 * VOM-FR-QKTE, VOM-FR-CUCN, VOM-FR-WFHN).
 *
 * A VAD session opens by itself, so VAD mode has no record control. In its place
 * stand the VAD indicator, which is not a button, the Mute toggle, the Open
 * again control while no session is open, and the waveform.
 */
import { useEffect, useRef, useState } from "react";

import { useVoiceState, type VoiceState } from "../../state/voiceSession";
import { getMicLevel, subscribeMicLevel } from "../../state/voiceLevel";
import { openVadSession, setVadMuted } from "../../voice/voiceController";
import { Icon } from "../icons";

/** VOM-FR-VOUG: what the VAD indicator shows. */
export type VadIndicator = "idle" | "listening" | "hearing" | "transcribing" | "paused";

/** VOM-FR-GBCN: a chunk or an utterance transcribes, while the microphone can stay open. */
export function transcribing(state: VoiceState): boolean {
  return state.transcribing > 0 || state.phase === "transcribing" || state.phase === "posting";
}

export function vadIndicator(state: VoiceState): VadIndicator {
  // VOM-FR-VOUG: the precedence of VOM-FR-XDUY, with no Speaking state.
  if (state.playbackPaused) return "paused";
  if (state.hearing || state.phase === "recording") return "hearing";
  if (transcribing(state)) return "transcribing";
  return state.vadOpen ? "listening" : "idle";
}

const VAD_NAMES: Record<VadIndicator, string> = {
  idle: "Idle",
  listening: "Listening",
  hearing: "Hearing speech",
  transcribing: "Transcribing",
  paused: "Paused",
};

/** VOM-FR-VOUG, VOM-FR-HSKZ: the state, in the accessible name. */
export function vadIndicatorName(state: VoiceState): string {
  const name = `Voice session: ${VAD_NAMES[vadIndicator(state)]}`;
  return state.vadOpen && state.muted ? `${name}, muted` : name;
}

function IndicatorIcon({ indicator }: { indicator: VadIndicator }) {
  if (indicator === "listening") return <Icon.MicListening size={14} />;
  if (indicator === "hearing") return <Icon.Waveform size={14} />;
  if (indicator === "transcribing") return <Icon.TextLines size={14} />;
  if (indicator === "paused") return <Icon.Pause size={14} />;
  return <Icon.Mic size={14} />;
}

/** VOM-FR-VOUG: the state by icon and colour, and in the accessible name. Not a button. */
export function VadIndicatorView() {
  const state = useVoiceState();
  const indicator = vadIndicator(state);
  const name = vadIndicatorName(state);
  return (
    <span
      className="voice-area__record voice-area__vad-indicator"
      role="img"
      aria-label={name}
      title={name}
      data-vad={indicator}
      data-testid="voice-vad-indicator"
    >
      <IndicatorIcon indicator={indicator} />
    </span>
  );
}

/**
 * VOM-FR-QKTE, VOM-FR-MUZB, VOM-FR-RBWX: Mute keeps the session open. It is a
 * native button, so click, Space, and Enter toggle it. With no session it takes
 * focus but does nothing (VOM-FR-LAQO).
 */
export function VadMuteToggle() {
  const state = useVoiceState();
  const open = state.vadOpen;
  const muted = open && state.muted;
  const name = muted ? "Unmute microphone" : "Mute microphone";
  return (
    <button
      type="button"
      className="voice-area__mute"
      data-voice-focus="true"
      data-testid="voice-mute"
      aria-pressed={muted}
      aria-label={name}
      title={open ? name : "No voice session is open."}
      aria-disabled={!open}
      onClick={() => {
        if (open) setVadMuted(!muted);
      }}
    >
      {muted ? <Icon.MicOff size={14} /> : <Icon.Mic size={14} />}
    </button>
  );
}

/** VOM-FR-CUCN: no session is open, and the author can try one time more. */
export function VadReopenControl() {
  const state = useVoiceState();
  if (state.vadOpen || state.phase !== "idle" || state.activeKey === null) return null;
  return (
    <button
      type="button"
      className="btn btn--ghost btn--sm voice-area__reopen"
      data-testid="voice-vad-reopen"
      onClick={() => openVadSession("again")}
    >
      Open again
    </button>
  );
}

/** VOM-FR-VQIU: the fewest bars. */
export const MIN_BARS = 16;
/** VOM-FR-VQIU: the width of one bar and of the gap after it, in px. */
const BAR_PITCH = 5;
/** VOM-FR-PJOC: the time between two updates, in ms. */
const MOTION_MS = 50;
const REDUCED_MS = 500;

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function flatBars(count: number): number[] {
  return Array<number>(count).fill(0);
}

/** VOM-FR-VQIU: as many bars as the width holds, and never less than the minimum. */
export function barsFor(width: number): number {
  return Math.max(MIN_BARS, Math.floor((width + 2) / BAR_PITCH));
}

/** VOM-FR-VQIU: the bar count of a waveform that fills its width. */
function useBarCount(fill: boolean): [React.RefObject<HTMLSpanElement | null>, number] {
  const ref = useRef<HTMLSpanElement | null>(null);
  const [count, setCount] = useState(MIN_BARS);
  useEffect(() => {
    const el = ref.current;
    if (!fill || !el || typeof ResizeObserver === "undefined") return;
    const measure = () => setCount(barsFor(el.clientWidth));
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [fill]);
  return [ref, count];
}

/**
 * VOM-FR-WFHN, VOM-FR-ZDLY, VOM-FR-PJOC: the live level of the microphone. It is
 * decorative. It updates on the audio that comes and has no timer of its own.
 * With `fill`, it takes the free width of its row (VOM-FR-VQIU).
 */
export function VoiceWaveform({ fill = false }: { fill?: boolean }) {
  const state = useVoiceState();
  const flat = !state.vadOpen || state.muted;
  const [ref, count] = useBarCount(fill);
  const [bars, setBars] = useState<readonly number[]>(() => flatBars(MIN_BARS));
  useEffect(() => {
    if (flat) {
      setBars(flatBars(count));
      return;
    }
    const step = prefersReducedMotion() ? REDUCED_MS : MOTION_MS;
    let last = 0;
    let history = flatBars(count);
    const take = (level: number) => {
      const now = Date.now();
      if (now - last < step) return;
      last = now;
      history = [...history.slice(1), level];
      setBars(history);
    };
    take(getMicLevel());
    return subscribeMicLevel(take);
  }, [flat, count]);
  return (
    <span
      ref={ref}
      className={fill ? "voice-waveform voice-waveform--fill" : "voice-waveform"}
      aria-hidden="true"
      data-flat={flat}
      data-reduced-motion={prefersReducedMotion()}
      data-testid="voice-waveform"
    >
      {bars.map((level, index) => (
        <span
          // The bars are positions, not items, so the index is their identity.
          key={index}
          className="voice-waveform__bar"
          style={{ transform: `scaleY(${Math.max(0.12, level)})` }}
        />
      ))}
    </span>
  );
}

/**
 * VOM-FR-TISN, VOM-FR-VGLT: the controls of VAD mode, in this order. The voice
 * area puts the waveform after its status, where it fills the free width
 * (VOM-FR-WFHN), so it asks for the controls with no waveform.
 */
export function VadControls({ waveform = true }: { waveform?: boolean }) {
  return (
    <>
      <VadIndicatorView />
      <VadMuteToggle />
      <VadReopenControl />
      {waveform && <VoiceWaveform />}
    </>
  );
}
