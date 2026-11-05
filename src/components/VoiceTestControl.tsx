/**
 * The **Hold to test** control of a Voice AI settings tab
 * (`VAI-voice-ai-settings.md` VAI-FR-HTVQ, VAI-FR-ZCMB, VAI-FR-NWRL,
 * VAI-FR-YBQE).
 */
import { useCallback, useEffect, useRef, useState } from "react";

import type { VoiceAiRecord, VoiceProviderId } from "../types";
import { voiceMessage } from "../voice/voiceMessages";
import {
  IDLE_TEST,
  createVoiceTest,
  type VoiceTest,
  type VoiceTestState,
} from "../voice/voiceTest";
import { Icon } from "./icons";

function statusOf(state: VoiceTestState): string | null {
  switch (state.phase) {
    case "opening":
    case "listening":
      return "Listening…";
    case "transcribing":
      return "Transcribing…";
    case "speaking":
      return "Speaking…";
    case "idle":
      return state.nothingHeard ? "Nothing was heard. Hold the control while you speak." : null;
  }
}

export function VoiceTestControl({
  provider,
  record,
  enabled,
}: {
  provider: VoiceProviderId;
  record: VoiceAiRecord;
  /** VAI-FR-HTVQ: the saved record is verified, and the tab holds no edit. */
  enabled: boolean;
}) {
  const [state, setState] = useState<VoiceTestState>(IDLE_TEST);
  // The saved voice of the tab, as it stands at the time of the speech.
  const recordRef = useRef(record);
  recordRef.current = record;
  const test = useRef<VoiceTest | null>(null);
  if (test.current === null) {
    test.current = createVoiceTest(
      provider,
      () => ({
        name: recordRef.current.defaultVoice || null,
        options: {
          speedPercent: recordRef.current.speechSpeedPercent ?? null,
          style: recordRef.current.speechStyle ?? "",
        },
      }),
      setState,
    );
  }

  const holding = useRef(false);
  const press = useCallback(() => {
    if (holding.current) return;
    holding.current = true;
    test.current?.press();
  }, []);
  // VAI-FR-NWRL: one release for one press, from whichever route comes first.
  const release = useCallback(() => {
    if (!holding.current) return;
    holding.current = false;
    test.current?.release();
  }, []);

  // VAI-FR-NWRL: a release outside the control ends the hold, and the unmount
  // of the control (a move to another tab, the close of the window) cancels
  // the test and closes the microphone.
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
      holding.current = false;
      test.current?.cancel();
    };
  }, [release]);
  // VAI-FR-NWRL: a disabled control gets no release event.
  useEffect(() => {
    if (!enabled) {
      holding.current = false;
      test.current?.cancel();
      setState(IDLE_TEST);
    }
  }, [enabled]);

  const active = state.phase === "opening" || state.phase === "listening";
  const status = statusOf(state);
  return (
    <div
      className="picker-field"
      // A note above the control can end with no margin of its own.
      style={{ marginTop: 10, marginBottom: 10 }}
      data-testid="voice-test"
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
        <button
          type="button"
          className="btn btn--sm"
          data-testid="voice-test-hold"
          data-recording={active}
          aria-pressed={active}
          disabled={!enabled}
          onPointerDown={(event) => {
            event.currentTarget.setPointerCapture?.(event.pointerId);
            press();
          }}
          onPointerUp={release}
          onPointerCancel={release}
          onLostPointerCapture={release}
          onKeyDown={(event) => {
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
          <Icon.Mic size={12} /> Hold to test
        </button>
        <span className="t-ui-sm t-muted">
          {enabled
            ? "Hold, speak, and release. You hear what you said in the voice of this tab."
            : "Verify this provider first."}
        </span>
      </div>
      {(state.transcript || status || state.error) && (
        <div
          className="t-ui-sm"
          role="status"
          data-testid="voice-test-status"
          style={{ marginTop: 6 }}
        >
          {state.transcript ? <q data-testid="voice-test-transcript">{state.transcript}</q> : null}
          {state.transcript && status ? " " : null}
          {status && <span className="t-muted">{status}</span>}
          {state.error && (
            <span className="picker-error" data-testid="voice-test-error">
              ✗ {voiceMessage(state.error)}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
