/**
 * The voice area of the shared composer (`VOM-voice-mode.md` VOM-FR-TISN).
 *
 * It takes the place of the text field. It shows the transcript, the record
 * control, the Stop control, and the two toggles. The process behind these
 * controls is `../../voice/voiceController.ts`.
 */
import { useEffect, useRef, useState } from "react";

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

const NOTES: Record<VoiceNote, string> = {
  no_speech: "No speech was heard, so nothing was posted.",
  no_agent: "No agent received this comment. Say an agent's name first, or tag one.",
  kept_in_body: "The transcript was kept as unsent text.",
};

function elapsed(since: number, now: number): string {
  const seconds = Math.max(0, Math.floor((now - since) / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/** VOM-FR-VOUG: what the record control shows in VAD mode. */
export type VadIndicator = "idle" | "listening" | "hearing" | "transcribing" | "paused";

export function vadIndicator(state: VoiceState): VadIndicator {
  // VOM-FR-OTRF: a reply can still stand paused while the transcript comes.
  if (state.phase === "transcribing" || state.phase === "posting") return "transcribing";
  if (state.playbackPaused) return "paused";
  if (!state.vadOpen) return "idle";
  return state.hearing ? "hearing" : "listening";
}

const VAD_NAMES: Record<VadIndicator, string> = {
  idle: "Idle",
  listening: "Listening",
  hearing: "Hearing speech",
  transcribing: "Transcribing",
  paused: "Paused",
};

const VAD_STATUS: Record<VadIndicator, string> = {
  idle: "Press to start listening",
  listening: "Listening…",
  hearing: "Hearing speech…",
  transcribing: "Transcribing…",
  paused: "Reply paused. Hearing speech…",
};

/** VOM-FR-HSKZ: the phase of the record control, in its name. */
function recordName(state: VoiceState): string {
  if (state.inputMode === "vad") {
    const name = VAD_NAMES[vadIndicator(state)];
    if (state.vadOpen) return `${name}. Close the voice session`;
    return state.phase === "idle" ? `${name}. Open a voice session` : name;
  }
  switch (state.phase) {
    case "opening":
    case "recording":
      return "Recording. Release or press to stop";
    case "transcribing":
      return "Transcribing";
    case "posting":
      return "Posting";
    default:
      return "Record";
  }
}

function useClock(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [active]);
  return now;
}

function statusOf(state: VoiceState, now: number): string {
  // VOM-FR-HUXY.
  if (state.transcript !== "") return state.transcript;
  // VOM-FR-SRGN.
  if (state.phase === "recording" && !state.live && state.recordingSince !== null) {
    return `Recording ${elapsed(state.recordingSince, now)}`;
  }
  if (state.inputMode === "vad") return VAD_STATUS[vadIndicator(state)];
  if (state.phase === "recording" || state.phase === "opening") return "Listening…";
  if (state.phase === "transcribing") return "Transcribing…";
  if (state.phase === "posting") return "Posting…";
  return "Press to talk";
}

function RecordIcon({ state }: { state: VoiceState }) {
  if (state.inputMode === "vad") {
    const indicator = vadIndicator(state);
    if (indicator === "listening") return <Icon.MicListening size={14} />;
    if (indicator === "hearing") return <Icon.Waveform size={14} />;
    if (indicator === "transcribing") return <Icon.TextLines size={14} />;
    if (indicator === "paused") return <Icon.Pause size={14} />;
  }
  return <Icon.Mic size={14} />;
}

/**
 * The record control. In VAD mode it also closes a session while the composer is
 * blocked (VOM-FR-ZUWW), so a session never stays open with no way to close it.
 */
export function VoiceRecordControl({ disabled }: { disabled: boolean }) {
  const state = useVoiceState();
  const vad = state.inputMode === "vad";
  const busy = state.phase === "transcribing" || state.phase === "posting";
  return (
    <button
      type="button"
      className="voice-area__record"
      data-voice-record="true"
      data-recording={!vad && (state.phase === "opening" || state.phase === "recording")}
      data-vad={vad ? vadIndicator(state) : undefined}
      data-testid="voice-record"
      aria-label={recordName(state)}
      title={recordName(state)}
      // Not `disabled` while busy: that would take the focus away after each
      // recording. A press in a busy phase does nothing.
      aria-disabled={busy && !state.vadOpen}
      disabled={disabled && !state.vadOpen}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture?.(event.pointerId);
        pressRecord();
      }}
      onPointerUp={releaseRecord}
      onPointerCancel={releaseRecord}
      // VOM-FR-PDZR: assistive technology can click with no pointer and no key. A
      // click from a pointer has `detail` 1, and `onPointerDown` already acted.
      onClick={(event) => {
        if (vad && event.detail === 0) pressRecord();
      }}
      onKeyDown={(event) => {
        // VOM-FR-PECJ.
        if ((event.key === " " || event.key === "Enter") && !event.repeat) {
          event.preventDefault();
          pressRecord();
        }
      }}
      onKeyUp={(event) => {
        if (event.key === " " || event.key === "Enter") {
          event.preventDefault();
          releaseRecord();
        }
      }}
    >
      <RecordIcon state={state} />
    </button>
  );
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
      onClick={stopSpeech}
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
  const vadBusy = state.inputMode === "ptt" && state.phase !== "idle";

  // The newest words are what the author checks while speaking.
  const transcriptRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const box = transcriptRef.current;
    if (box) box.scrollTop = box.scrollHeight;
  }, [status]);

  return (
    <div className="voice-area" data-testid="voice-area">
      <VoiceRecordControl disabled={disabled} />
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
