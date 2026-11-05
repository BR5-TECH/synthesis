/**
 * VOM-FR-VGLT: the session control while a question set stands. The reply
 * composer unmounts then, and the voice controls and an open microphone must stay
 * visible and usable. In VAD mode it holds the VAD indicator, the Mute toggle,
 * and the waveform (VOM-FR-GPNK).
 *
 * It lends the controller a handle, so voice mode sees a mounted surface. A
 * recording that starts now is a spoken answer (VOM-FR-PINR). One that started
 * before the set came finds a handle that cannot post (VOM-FR-ABPU). When the set
 * is answered the composer mounts, and listening goes on with no press
 * (VOM-FR-GTAQ).
 */
import { useEffect, useRef } from "react";

import { registerVoiceComposer } from "../../voice/voiceController";
import { useVoiceState } from "../../state/voiceSession";
import {
  VoiceFeedback,
  VoiceInputControls,
  VoiceStopControl,
  statusOf,
  useClock,
} from "./VoiceArea";
import type { VoiceState } from "../../state/voiceSession";
import type { VoiceOptions } from "../../types";

/** VOM-FR-VGLT: how to answer aloud, and whether the microphone is open or muted. */
function answerLine(voice: VoiceState, now: number): string {
  // VOM-FR-HUXY, VOM-FR-SRGN: a recording shows as it does in the voice area, so
  // a Push-to-Talk answer does not read as a closed microphone.
  if (voice.phase !== "idle") return statusOf(voice, now);
  if (voice.inputMode === "ptt") return "Press to answer the question above.";
  if (!voice.vadOpen) return "The microphone is closed.";
  if (voice.muted) return "The microphone is muted. Unmute to answer the question above.";
  if (voice.transcript !== "" || voice.hearing) return statusOf(voice, now);
  return "The microphone is open. Speak to answer the question above.";
}

export function VoiceSessionBar(props: {
  sessionKey: string;
  voiceOf: (agentId: string) => string | null;
  /** VOM-FR-UALP. */
  voiceOptionsOf?: (agentId: string) => VoiceOptions | null;
  /** VOM-FR-LNUH. */
  nicknames: () => string[];
}) {
  const voice = useVoiceState();
  const recording = voice.phase === "recording" && voice.recordingSince !== null;
  const now = useClock(recording);
  const voiceOfRef = useRef(props.voiceOf);
  voiceOfRef.current = props.voiceOf;
  const voiceOptionsOfRef = useRef(props.voiceOptionsOf);
  voiceOptionsOfRef.current = props.voiceOptionsOf;
  const nicknamesRef = useRef(props.nicknames);
  nicknamesRef.current = props.nicknames;
  const rootRef = useRef<HTMLDivElement>(null);

  // VOM-FR-LAQO: the record control that held focus went with the composer.
  // WebKit puts focus on the surface after a click on a button, so an ancestor
  // that holds focus counts as no focus too.
  useEffect(() => {
    const active = document.activeElement;
    if (active === document.body || active?.contains(rootRef.current)) {
      rootRef.current?.querySelector<HTMLElement>("[data-voice-focus]")?.focus();
    }
  }, []);

  useEffect(
    () =>
      registerVoiceComposer(props.sessionKey, {
        // VOM-FR-ABPU: only a recording from before the set reaches this.
        post: async () => "busy",
        posts: false,
        tagged: (transcript) => transcript,
        blocked: () => false,
        nicknames: () => nicknamesRef.current(),
        voiceOf: (agentId) => voiceOfRef.current(agentId),
        voiceOptionsOf: (agentId) => voiceOptionsOfRef.current?.(agentId) ?? null,
      }),
    [props.sessionKey],
  );

  return (
    // The class of the composer: the bar takes its place, its measure, and its
    // clearance of the action control (`DDS-draft-discussion.md` DDS-FR-QPHL).
    <div ref={rootRef} className="comment-card__reply voice-session-bar" data-testid="voice-session-bar">
      <div className="voice-session-bar__row">
        <VoiceInputControls disabled={false} />
        <div className="voice-session-bar__text">{answerLine(voice, now)}</div>
        <VoiceStopControl />
      </div>
      <VoiceFeedback />
    </div>
  );
}
