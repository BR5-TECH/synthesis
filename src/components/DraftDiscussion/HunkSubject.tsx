/**
 * DDS-FR-WKTQ / DDS-FR-PBXN: the **subject strip** of a hunk discussion — the
 * change the conversation is about, above the conversation.
 *
 * It stands outside the message scroller, so the change stays in view however
 * far the author reads back. Without it only the opening comment names the
 * change, and that comment scrolls away.
 *
 * The state comes from `DCP-draft-change-proposals.md` by way of the store and
 * the text from the proposal's own hunk reading, never from the attachment,
 * which carries the identity alone (CMT-FR-67).
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { Icon } from "../icons";
import { hunkStateWord } from "../ProposalHunkControl";
import { logWarn } from "../../logging";
import { hunkPosition, hunkStateOf } from "../../state/draftProposals";
import { useProposalHunks } from "../../state/proposalHunks";
import type { ProposalHunkReference } from "../../types";
import {
  hunkSourceOf,
  isShownInDocument,
  showHunkInDocument,
  useHunkProposal,
} from "./hunkDiscussion";

/** DDS-FR-PBXN: the texts the strip cuts to a few lines. */
const CUT_TEXTS = ".dds-subject__body, .dds-subject__note";

/**
 * DDS-FR-PBXN: whether the strip cuts any of its texts.
 *
 * Measured rather than counted, because where a text is cut depends on the
 * width of the column: a count offers **Show more** for a text that shows whole
 * in a wide column, and none for a text a narrow one cuts.
 */
function cutsText(strip: HTMLElement): boolean {
  return [...strip.querySelectorAll<HTMLElement>(CUT_TEXTS)].some(
    (el) => el.scrollHeight > el.clientHeight + 1,
  );
}

export function HunkSubject({
  reference,
  place = null,
}: {
  reference: ProposalHunkReference;
  /**
   * DDS-FR-RHKW: the place the composer already names, said until the
   * proposal's own reading answers, so the strip and the composer agree.
   */
  place?: { position: number; total: number } | null;
}) {
  const { draftId, artifactId, proposalId, hunkId } = reference;
  // DDS-FR-EHDK: a change of a draft proposal or of a prompt or DOC proposal.
  const known = useHunkProposal(reference);
  const proposal = known.kind === "known" ? known.proposal : null;
  const reading = useProposalHunks(proposalId, hunkSourceOf(reference));
  const hunk =
    reading?.status === "read" ? reading.hunks.hunks.find((h) => h.id === hunkId) : undefined;
  const [expanded, setExpanded] = useState(false);
  const [cut, setCut] = useState(false);
  const stripRef = useRef<HTMLElement>(null);

  // A different change is a different subject, which starts cut again.
  useEffect(() => setExpanded(false), [proposalId, hunkId]);

  // A reading that answered without this change names a change the proposal no
  // longer holds. The strip still names it; the reason is worth a record.
  const missing = reading?.status === "read" && hunk === undefined;
  useEffect(() => {
    if (missing) {
      logWarn(["frontend"], "a discussed change is not in its proposal's reading", {
        draftId,
        artifactId,
        proposalId,
        hunkId,
      });
    }
  }, [missing, draftId, artifactId, proposalId, hunkId]);

  const state = proposal ? hunkStateOf(proposal, hunkId) : undefined;
  const position = proposal ? hunkPosition(proposal, hunkId) : (place?.position ?? 0);
  const total = proposal ? proposal.hunkCount : (place?.total ?? 0);
  const name = position > 0 ? `Change ${position} of ${total}` : "Change";
  const word =
    state !== undefined
      ? hunkStateWord(state)
      : known.kind === "gone"
        ? "no longer available"
        : null;
  const reachable = isShownInDocument(proposal, state);
  // DDS-FR-PBXN: measured again when the texts change and when the column is
  // resized. A strip shown whole measures nothing: its Show less stays.
  useLayoutEffect(() => {
    const strip = stripRef.current;
    if (!strip || expanded) return;
    const measure = () => setCut(cutsText(strip));
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(strip);
    return () => observer.disconnect();
  }, [expanded, hunk?.before, hunk?.after, hunk?.note]);
  const long = expanded || cut;

  return (
    <section
      ref={stripRef}
      className="dds-subject"
      aria-label="The change this discussion is about"
      data-testid="discussion-subject"
      data-expanded={expanded}
    >
      <div className="dds-subject__head t-ui-xs">
        <Icon.Diff size={11} />
        <span className="dds-subject__name">{name}</span>
        <span className="dds-subject__path" title={reference.path}>
          {reference.path}
        </span>
        {/* DDS-FR-YSNB: the state as one lowercase word beside its dot. */}
        {word !== null && (
          <span
            className="dds-subject__state"
            data-state={state ?? "missing"}
            data-testid="discussion-subject-state"
          >
            <span className="dds-subject__dot" aria-hidden="true" />
            {word}
          </span>
        )}
      </div>
      {hunk?.note && (
        <p className="dds-subject__note t-ui-xs" data-testid="discussion-subject-note">
          {hunk.note}
        </p>
      )}
      {/* DDS-FR-PBXN: the text the change removes and adds, in the tones of
          DCR-FR-07. The signs say which is which without the colour, and the
          hidden words say it to assistive technology. */}
      {hunk?.before && (
        <p className="dds-subject__text dds-subject__text--del" data-testid="discussion-subject-before">
          <span className="dds-subject__sign" aria-hidden="true">−</span>
          <span className="sr-only">Removes: </span>
          <span className="dds-subject__body">{hunk.before}</span>
        </p>
      )}
      {hunk?.after && (
        <p className="dds-subject__text dds-subject__text--add" data-testid="discussion-subject-after">
          <span className="dds-subject__sign" aria-hidden="true">+</span>
          <span className="sr-only">Adds: </span>
          <span className="dds-subject__body">{hunk.after}</span>
        </p>
      )}
      {reading?.status === "failed" && (
        <p className="dds-subject__note t-ui-xs" role="status">
          The text of this change could not be read.
        </p>
      )}
      {(long || reachable) && (
        <div className="dds-subject__actions">
          {long && (
            <button
              type="button"
              className="dds-subject__action t-ui-xs"
              aria-expanded={expanded}
              onClick={() => setExpanded((open) => !open)}
            >
              {expanded ? "Show less" : "Show more"}
            </button>
          )}
          {reachable && (
            <button
              type="button"
              className="dds-subject__action dds-subject__action--go t-ui-xs"
              onClick={() => showHunkInDocument(reference)}
            >
              Show in document
            </button>
          )}
        </div>
      )}
    </section>
  );
}
