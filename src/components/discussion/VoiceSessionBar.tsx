/**
 * VOM-FR-VGLT: the session control while a question set stands. The reply
 * composer unmounts then, and the record control and an open microphone must stay
 * visible and usable.
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
  VoiceRecordControl,
  VoiceStopControl,
  statusOf,
  useClock,
} from "./VoiceArea";

export function VoiceSessionBar(props: {
  sessionKey: string;
  voiceOf: (agentId: string) => string | null;
  /** VOM-FR-LNUH. */
  nicknames: () => string[];
}) {
  const voice = useVoiceState();
  const recording = voice.phase === "recording" && voice.recordingSince !== null;
  const now = useClock(recording);
  const voiceOfRef = useRef(props.voiceOf);
  voiceOfRef.current = props.voiceOf;
  const nicknamesRef = useRef(props.nicknames);
  nicknamesRef.current = props.nicknames;
  const rootRef = useRef<HTMLDivElement>(null);

  // VOM-FR-LAQO: the record control that held focus went with the composer.
  // WebKit puts focus on the surface after a click on a button, so an ancestor
  // that holds focus counts as no focus too.
  useEffect(() => {
    const active = document.activeElement;
    if (active === document.body || active?.contains(rootRef.current)) {
      rootRef.current?.querySelector<HTMLElement>("[data-voice-record]")?.focus();
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
      }),
    [props.sessionKey],
  );

  return (
    // The class of the composer: the bar takes its place, its measure, and its
    // clearance of the action control (`DDS-draft-discussion.md` DDS-FR-QPHL).
    <div ref={rootRef} className="comment-card__reply voice-session-bar" data-testid="voice-session-bar">
      <div className="voice-session-bar__row">
        <VoiceRecordControl disabled={false} />
        <div className="voice-session-bar__text">
          {/* VOM-FR-HUXY, VOM-FR-SRGN: a recording shows as it does in the voice
              area, so a Push-to-Talk answer does not read as a closed microphone. */}
          {voice.phase !== "idle"
            ? statusOf(voice, now)
            : voice.vadOpen
              ? "The microphone is open. Speak to answer the question above."
              : "Press to answer the question above."}
        </div>
        <VoiceStopControl />
      </div>
      <VoiceFeedback />
    </div>
  );
}
