/**
 * The live bubble of voice mode (`CVP-conversation-presentation.md`
 * CVP-FR-TGPV, CVP-FR-GHWN; `VOM-voice-mode.md` VOM-FR-NCHC, VOM-FR-MBHB).
 *
 * One local bubble at the end of the log of the voice discussion. It joins the
 * chunk transcripts of each chunked utterance that has not settled, and the
 * live text of the open chunk after them. It is not a comment and is not saved.
 * It is not an `aria-live` region (VOM-FR-HSKZ).
 */
import { partShows, useLiveBubble, type LivePart } from "../../state/liveBubble";
import type { AgentTurn } from "../../types";
import { useParticipantLabel } from "../../state/projectIdentity";
import { voiceMessage } from "../../voice/voiceMessages";

/** AGC-FR-ANBX: a turn that a speech registered has no trigger comment yet. */
function speechRegistered(turn: AgentTurn): boolean {
  return Boolean(turn.speechId) && !turn.triggerCommentId;
}

/** CVP-FR-TGPV: only a turn that the speech registered shows below the bubble. */
export function splitAroundBubble(
  turns: readonly AgentTurn[],
  bubbleShown: boolean,
): { before: AgentTurn[]; after: AgentTurn[] } {
  if (!bubbleShown) return { before: [...turns], after: [] };
  return {
    before: turns.filter((t) => !speechRegistered(t)),
    after: turns.filter(speechRegistered),
  };
}

const CHUNK_LOST = "One part was not heard.";

function PartText({ part }: { part: LivePart }) {
  return (
    <>
      {part.chunks.map((chunk) =>
        chunk.failed ? (
          <span
            key={chunk.index}
            className="live-bubble__error"
            data-testid="live-bubble-error"
            title={voiceMessage(chunk.failed)}
          >
            {CHUNK_LOST}{" "}
          </span>
        ) : chunk.text ? (
          <span key={chunk.index}>{chunk.text} </span>
        ) : null,
      )}
      {part.liveText !== "" && <span className="live-bubble__live">{part.liveText}</span>}
    </>
  );
}

/** The bubble of a discussion, or nothing while no chunk shows. */
export function useLiveBubbleNode(discussionId: string): React.ReactNode {
  const parts = useLiveBubble(discussionId).filter(partShows);
  const participantLabel = useParticipantLabel();
  if (parts.length === 0) return null;
  return (
    <div className="comment live-bubble" data-own="true" data-testid="live-bubble">
      <div className="comment__head">
        <span className="comment__author">{participantLabel({ kind: "human", login: "" })}</span>
        {/* CVP-FR-GHWN: a marker in place of the timestamp, by text. */}
        <span className="comment__time live-bubble__marker" data-testid="live-bubble-marker">
          Speaking…
        </span>
      </div>
      <div className="comment__body">
        {parts.map((part) => (
          <p key={part.captureId} className="live-bubble__part">
            <PartText part={part} />
          </p>
        ))}
      </div>
    </div>
  );
}
