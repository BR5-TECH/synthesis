/**
 * The voice area of the shared composer (`VOM-voice-mode.md` VOM-FR-TISN).
 *
 * It takes the place of the text field. It shows the transcript, the record
 * control, the Stop control, and the two toggles. The process behind these
 * controls is `../../voice/voiceController.ts`.
 */
import { useEffect, useRef, useState } from "react";

import { useVoiceState, type VoiceNote, type VoiceState } from "../../state/voiceSession";
import { voiceMessage } from "../../voice/voiceMessages";
import {
  pressRecord,
  releaseRecord,
  retryRecording,
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

/** VOM-FR-HSKZ: the phase of the record control, in its name. */
function recordName(state: VoiceState): string {
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
  if (state.phase === "recording" || state.phase === "opening") return "Listening…";
  if (state.phase === "transcribing") return "Transcribing…";
  if (state.phase === "posting") return "Posting…";
  return "Press to talk";
}

export function VoiceArea({ disabled }: { disabled: boolean }) {
  const state = useVoiceState();
  const recording = state.phase === "recording";
  const now = useClock(recording && !state.live);
  const busy = state.phase === "transcribing" || state.phase === "posting";
  const status = statusOf(state, now);

  // The newest words are what the author checks while speaking.
  const transcriptRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const box = transcriptRef.current;
    if (box) box.scrollTop = box.scrollHeight;
  }, [status]);

  return (
    <div className="voice-area" data-testid="voice-area">
      <button
        type="button"
        className="voice-area__record"
        data-voice-record="true"
        data-recording={state.phase === "opening" || recording}
        data-testid="voice-record"
        aria-label={recordName(state)}
        title={recordName(state)}
        // Not `disabled` while busy: that would take the focus away after each
        // recording. A press in a busy phase does nothing.
        aria-disabled={busy}
        disabled={disabled}
        onPointerDown={(event) => {
          event.currentTarget.setPointerCapture?.(event.pointerId);
          pressRecord();
        }}
        onPointerUp={releaseRecord}
        onPointerCancel={releaseRecord}
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
        <Icon.Mic size={14} />
      </button>
      <div ref={transcriptRef} className="voice-area__transcript" data-testid="voice-transcript">
        {status}
      </div>
      {state.playing && (
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
      )}
      <div className="voice-area__toggles">
        <div className="voice-toggle" role="group" aria-label="Input mode">
          <button
            type="button"
            className="voice-toggle__option"
            aria-pressed="true"
            aria-label="Push-to-Talk"
            title="Push-to-Talk"
          >
            PTT
          </button>
          <button
            type="button"
            className="voice-toggle__option"
            aria-pressed="false"
            disabled
            title="Voice activity detection comes later."
            aria-description="Voice activity detection comes later."
          >
            VAD
          </button>
        </div>
        <div className="voice-toggle" role="group" aria-label="Response mode">
          <button
            type="button"
            className="voice-toggle__option"
            aria-pressed={state.responseMode === "text"}
            onClick={() => setResponseMode("text")}
          >
            Text
          </button>
          <button
            type="button"
            className="voice-toggle__option"
            aria-pressed={state.responseMode === "voice"}
            disabled={!state.providerSpeaks}
            title={
              state.providerSpeaks ? "Speak replies" : "The active provider has no speech route."
            }
            aria-description={
              state.providerSpeaks ? undefined : "The active provider has no speech route."
            }
            onClick={() => setResponseMode("voice")}
          >
            Voice
          </button>
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
