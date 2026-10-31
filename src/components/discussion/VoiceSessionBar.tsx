/**
 * VOM-FR-VGLT: the session control while a question set stands. The reply
 * composer unmounts then, and an open microphone must stay visible and closable.
 *
 * It lends the controller a standby handle, so the session sees a mounted surface
 * and a blocked gate. When the set is answered the composer mounts, and listening
 * goes on with no press (VOM-FR-GTAQ).
 */
import { useEffect, useRef } from "react";

import { registerVoiceComposer } from "../../voice/voiceController";
import { VoiceFeedback, VoiceRecordControl, VoiceStopControl } from "./VoiceArea";

export function VoiceSessionBar(props: {
  sessionKey: string;
  voiceOf: (agentId: string) => string | null;
}) {
  const voiceOfRef = useRef(props.voiceOf);
  voiceOfRef.current = props.voiceOf;
  const rootRef = useRef<HTMLDivElement>(null);

  // VOM-FR-LAQO: the record control that held focus went with the composer.
  useEffect(() => {
    if (document.activeElement === document.body) {
      rootRef.current?.querySelector<HTMLElement>("[data-voice-record]")?.focus();
    }
  }, []);

  useEffect(
    () =>
      registerVoiceComposer(props.sessionKey, {
        // An utterance that ends now joins the session body (VOM-FR-ABPU).
        post: async () => "busy",
        tagged: (transcript) => transcript,
        blocked: () => true,
        // The gate starts no utterance, so nothing reads hints until the
        // composer is back with the roster.
        nicknames: () => [],
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
          The microphone is open. Speech is not recorded until you answer the questions.
        </div>
        <VoiceStopControl />
      </div>
      <VoiceFeedback />
    </div>
  );
}
