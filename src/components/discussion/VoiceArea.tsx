/**
 * The voice area of the shared composer (`VOM-voice-mode.md` VOM-FR-TISN).
 *
 * It takes the place of the text field. It shows the transcript, the record
 * control in Push-to-Talk or the controls of VAD mode (`./VadControls.tsx`), the
 * Stop control, and the two toggles. The process behind these controls is
 * `../../voice/voiceController.ts`.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import {
  useVoiceState,
  type VoiceInputMode,
  type VoiceNote,
  type VoiceResponseMode,
  type VoiceState,
} from "../../state/voiceSession";
import { voiceMessage } from "../../voice/voiceMessages";
import {
  pressRecord,
  releaseRecord,
  retryRecording,
  setInputMode,
  setResponseMode,
  stopSpeech,
} from "../../voice/voiceController";
import { Icon } from "../icons";
import { VadControls, transcribing, vadIndicator, type VadIndicator } from "./VadControls";

export { transcribing, vadIndicator } from "./VadControls";

const NOTES: Record<VoiceNote, string> = {
  no_speech: "No speech was heard, so nothing was posted.",
  no_agent: "No agent received this comment. Say an agent's name first, or tag one.",
  kept_in_body: "The transcript was kept as unsent text.",
  translating: "Translating…",
  not_translated: "Not translated. The original text was posted.",
};

function elapsed(since: number, now: number): string {
  const seconds = Math.max(0, Math.floor((now - since) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

const VAD_STATUS: Record<VadIndicator, string> = {
  idle: "The voice session is not open.",
  listening: "Listening…",
  hearing: "Hearing speech…",
  transcribing: "Transcribing…",
  paused: "Reply paused. Hearing speech…",
};

/** VOM-FR-HSKZ: the phase of the record control, in its name. */
function recordName(state: VoiceState): string {
  // VOM-FR-QTAQ: Recording, then Transcribing, then Idle.
  if (state.phase === "opening" || state.phase === "recording") {
    return "Recording. Release or press to stop";
  }
  return transcribing(state) ? "Transcribing. Press to record again" : "Record";
}

export function useClock(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active]);
  return now;
}

export function statusOf(state: VoiceState, now: number): string {
  // VOM-FR-HUXY.
  if (state.transcript !== "") return state.transcript;
  // VOM-FR-SRGN.
  if (state.phase === "recording" && !state.live && state.recordingSince !== null) {
    return `Recording ${elapsed(state.recordingSince, now)}`;
  }
  if (state.inputMode === "vad") {
    if (state.phase === "opening") return "Opening the voice session…";
    const indicator = vadIndicator(state);
    // VOM-FR-MUZB: muted shows by text too, and not by colour alone.
    if (state.vadOpen && state.muted && indicator === "listening") {
      return "Muted. Unmute to speak.";
    }
    return VAD_STATUS[indicator];
  }
  if (state.phase === "recording" || state.phase === "opening") return "Listening…";
  if (transcribing(state)) return "Transcribing…";
  return "Press to talk";
}

/** The record control of Push-to-Talk. VAD mode has none (VOM-FR-PDZR). */
export function VoiceRecordControl({ disabled }: { disabled: boolean }) {
  const state = useVoiceState();
  // VOM-FR-QTAQ: a new press can start a recording while Transcribing shows.
  const busy = state.phase === "opening";
  const holding = useRef(false);
  const press = useCallback(() => {
    holding.current = true;
    pressRecord();
  }, []);
  // VOM-FR-FZJY: one release for one press, from whichever route comes first.
  const release = useCallback(() => {
    if (!holding.current) return;
    holding.current = false;
    releaseRecord();
  }, []);
  // VOM-FR-FZJY: a release outside the control, and the unmount of the
  // control, end the hold.
  useEffect(() => {
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === " " || event.key === "Enter") release();
    };
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", release);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", release);
      window.removeEventListener("keyup", onKeyUp);
      release();
    };
  }, [release]);
  // VOM-FR-FZJY: a disabled control gets no release event.
  useEffect(() => {
    if (disabled) release();
  }, [disabled, release]);
  return (
    <button
      type="button"
      className="voice-area__record"
      data-voice-focus="true"
      data-recording={state.phase === "opening" || state.phase === "recording"}
      data-testid="voice-record"
      aria-label={recordName(state)}
      title={recordName(state)}
      // Not `disabled` while busy: that would take the focus away after each
      // recording. A press in a busy phase does nothing.
      aria-disabled={busy}
      disabled={disabled}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture?.(event.pointerId);
        press();
      }}
      onPointerUp={release}
      onPointerCancel={release}
      onLostPointerCapture={release}
      onKeyDown={(event) => {
        // VOM-FR-PECJ.
        if ((event.key === " " || event.key === "Enter") && !event.repeat) {
          event.preventDefault();
          press();
        }
      }}
      onKeyUp={(event) => {
        if (event.key === " " || event.key === "Enter") {
          event.preventDefault();
          release();
        }
      }}
    >
      <Icon.Mic size={14} />
    </button>
  );
}

/** VOM-FR-TISN, VOM-FR-VGLT: the record control, or the controls of VAD mode. */
export function VoiceInputControls({ disabled }: { disabled: boolean }) {
  const state = useVoiceState();
  return state.inputMode === "vad" ? <VadControls /> : <VoiceRecordControl disabled={disabled} />;
}

/** VOM-FR-QDWW. */
export function VoiceStopControl() {
  const state = useVoiceState();
  if (!state.playing) return null;
  return (
    <button
      type="button"
      className="btn btn--ghost btn--icon-sm comment-composer__control"
      aria-label="Stop speech"
      title="Stop speech"
      data-testid="voice-stop"
      onClick={(event) => {
        // VOM-FR-LAQO: this control goes with the speech, so focus does not fall
        // to the document.
        const record = event.currentTarget.parentElement?.querySelector<HTMLElement>(
          "[data-voice-focus]",
        );
        stopSpeech();
        record?.focus();
      }}
    >
      <Icon.Stop size={13} />
    </button>
  );
}

function ToggleOption(props: {
  name: string;
  pressed: boolean;
  children: React.ReactNode;
  reason?: string;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      className="voice-toggle__option"
      aria-pressed={props.pressed}
      aria-label={props.name}
      title={props.reason ?? props.name}
      aria-description={props.reason}
      disabled={props.reason !== undefined}
      onClick={props.onSelect}
    >
      {props.children}
    </button>
  );
}

const INPUT_MODES: { mode: VoiceInputMode; name: string; icon: React.ReactNode }[] = [
  { mode: "ptt", name: "Push-to-Talk", icon: <Icon.KeyPress size={13} /> },
  { mode: "vad", name: "VAD", icon: <Icon.Waveform size={13} /> },
];

const RESPONSE_MODES: { mode: VoiceResponseMode; name: string; icon: React.ReactNode }[] = [
  { mode: "text", name: "Text", icon: <Icon.TextLines size={13} /> },
  { mode: "voice", name: "Voice", icon: <Icon.Speaker size={13} /> },
];

const VAD_BUSY = "VAD is not available during a recording.";
const NO_SPEECH_ROUTE = "The active provider has no speech route.";
const VAD_SOUND = "Other applications can sound lower during a voice session.";

/** VOM-FR-RTSU: OS voice processing runs for as long as the microphone is open. */
function isMacPlatform(): boolean {
  return typeof navigator !== "undefined" && /mac/i.test(navigator.platform);
}

export function VoiceArea({ disabled }: { disabled: boolean }) {
  const state = useVoiceState();
  const recording = state.phase === "recording";
  const now = useClock(recording && !state.live);
  const status = statusOf(state, now);
  const vadBusy = state.inputMode === "ptt" && (state.phase !== "idle" || transcribing(state));

  // The newest words are what the author checks while speaking.
  const transcriptRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const box = transcriptRef.current;
    if (box) box.scrollTop = box.scrollHeight;
  }, [status]);

  return (
    <div className="voice-area" data-testid="voice-area">
      <VoiceInputControls disabled={disabled} />
      <div ref={transcriptRef} className="voice-area__transcript" data-testid="voice-transcript">
        {status}
      </div>
      <VoiceStopControl />
      <div className="voice-area__toggles">
        {state.inputMode === "vad" && isMacPlatform() && (
          <span
            className="voice-area__info"
            role="img"
            tabIndex={0}
            aria-label={VAD_SOUND}
            title={VAD_SOUND}
            data-testid="voice-vad-hint"
          >
            <Icon.Info size={13} />
          </span>
        )}
        <div className="voice-toggle" role="group" aria-label="Input mode">
          {INPUT_MODES.map(({ mode, name, icon }) => (
            <ToggleOption
              key={mode}
              name={name}
              pressed={state.inputMode === mode}
              reason={mode === "vad" && vadBusy ? VAD_BUSY : undefined}
              onSelect={() => setInputMode(mode)}
            >
              {icon}
            </ToggleOption>
          ))}
        </div>
        <div className="voice-toggle" role="group" aria-label="Response mode">
          {RESPONSE_MODES.map(({ mode, name, icon }) => (
            <ToggleOption
              key={mode}
              name={name}
              pressed={state.responseMode === mode}
              reason={mode === "voice" && !state.providerSpeaks ? NO_SPEECH_ROUTE : undefined}
              onSelect={() => setResponseMode(mode)}
            >
              {icon}
            </ToggleOption>
          ))}
        </div>
      </div>
    </div>
  );
}

/** VOM-FR-ZFSA, VOM-FR-WICV, VOM-FR-RENV: below the field. */
export function VoiceFeedback() {
  const state = useVoiceState();
  if (state.error) {
    return (
      <div className="comment-card__error" role="alert" data-testid="voice-error">
        {voiceMessage(state.error)}
        {state.canRetry && (
          <button
            type="button"
            className="btn btn--ghost btn--sm voice-feedback__retry"
            data-testid="voice-retry"
            onClick={() => void retryRecording()}
          >
            Retry
          </button>
        )}
      </div>
    );
  }
  if (state.note) {
    return (
      <div className="voice-feedback__note" role="status" data-testid="voice-note">
        {NOTES[state.note]}
      </div>
    );
  }
  return null;
}
