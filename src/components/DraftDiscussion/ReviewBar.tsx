/**
 * DCR-FR-11: the review bar above the page.
 *
 * It names the change under review and how many the proposal holds, and carries
 * the two whole-proposal actions. It holds its place while the document
 * scrolls, because the counter and the two actions are exactly what scrolling
 * to a change would otherwise take away.
 */
import type { DraftChangeProposal } from "../../types";
import { undecidedCount } from "../../types";

export function ReviewBar({
  proposal,
  position,
  busy,
  error,
  conflict,
  standing,
  canPrevious,
  canNext,
  undrawn,
  onPrevious,
  onNext,
  onAcceptAll,
  onRejectAll,
}: {
  proposal: DraftChangeProposal;
  /** The one-based place of the change under review, or 0 while none is. */
  position: number;
  busy: boolean;
  /** DCR-FR-16: the typed error of the last refused decision. */
  error: string | null;
  /**
   * DCR-FR-29: a change is never replaced under an unresolved edit.
   *
   * Present while the author has a rewrite on screen and storage holds another.
   * Neither is chosen for them: both are offered, and until one is taken that
   * change's decisions are off.
   */
  conflict: { onKeep: () => void; onTake: () => void } | null;
  /**
   * DCR-FR-27: which of the two things has happened, in words.
   *
   * The author is being asked to take an agent's rewriting of their own
   * document, so the one thing they most need to be sure of is whether the
   * draft has moved yet. Neither statement is a colour or a tone, and both are
   * announced.
   */
  standing: string;
  /**
   * DCR-FR-11: how the author moves through the proposal.
   *
   * Unavailable at the ends rather than absent, so the bar keeps its shape and
   * the count of what is left to decide does not move under the pointer.
   */
  canPrevious: boolean;
  canNext: boolean;
  /**
   * DCR-FR-31: the change under review is not drawn in the document.
   *
   * Said rather than left to be noticed. A bar that counts a change the page
   * does not show, and answers a move to it with nothing changing on screen,
   * reports a defect of its own as an ordinary state — and the author cannot
   * tell it from a control that does not work.
   */
  undrawn: boolean;
  onPrevious: () => void;
  onNext: () => void;
  onAcceptAll: () => void;
  onRejectAll: () => void;
}) {
  const total = proposal.hunkCount;
  const left = undecidedCount(proposal.counts);
  return (
    <div className="dds-review" role="region" aria-label="Proposed changes">
      {/* The bar stands at the foot of the column, so it is read upward: what
          the author is being told first, and the row they act on last, nearest
          the hand. */}
      {/* DCR-FR-27: whether the draft has moved yet — the one thing here the
          author must not have to go and check.

          On its own line rather than beside the counter, because it is a
          sentence and the counter is a number: sharing a row, the sentence is
          the half that gets ellipsised at the window sizes the shell supports,
          and a statement that does not fit does not state anything. */}
      <p
        className="dds-review__standing t-ui-xs"
        role="status"
        aria-live="polite"
        data-testid="review-standing"
      >
        {standing}
      </p>

      {/* DCR-FR-31: a change the review cannot draw. It is still counted, and
          still decided from here — which is why this states the fact instead of
          taking the actions away. */}
      {undrawn && (
        <p className="dds-review__undrawn t-ui-xs" role="status" aria-live="polite">
          This change is not shown in the document. You can still accept or
          reject it.
        </p>
      )}
      {/* DCR-FR-16: what happened, in words, and announced. The
          prompt is byte-for-byte what it was; the decision is retried without
          retyping anything. */}
      {error !== null && (
        <p className="dds-review__error t-ui-xs" role="alert">
          {error}
        </p>
      )}
      {/* DCR-FR-29: the two ways out, both offered. Neither is taken for the
          author — a surface that picked one would silently throw away either
          their own rewrite or the one that arrived. */}
      {conflict !== null && (
        <div className="dds-review__conflict" role="group" aria-label="Resolve the edit conflict">
          <button className="btn btn--sm" onClick={conflict.onKeep}>
            Keep what I wrote
          </button>
          <button className="btn btn--sm" onClick={conflict.onTake}>
            Take the newer text
          </button>
        </div>
      )}
      <div className="dds-review__row">
        <span
          className="dds-review__state t-ui-xs"
          role="status"
          data-testid="review-counter"
        >
          {position > 0
            ? `reviewing change ${position} of ${total}`
            : `${left} of ${total} ${left === 1 ? "change" : "changes"} still to decide`}
        </span>
        {/* DCR-FR-11: the two controls stand with the counter they move —
            which change of how many, and how to change it. Each names the
            accelerator of DCR-FR-30 that does the same thing, in the same
            treatment the chip gives its own keys, so the key is learned here
            and recognised there. */}
        <div className="dds-review__nav" role="group" aria-label="Move through the changes">
          <button
            className="btn btn--ghost btn--sm"
            disabled={busy || !canPrevious}
            title="Go to the previous change"
            onClick={onPrevious}
          >
            Previous <kbd className="dds-review__key">⌥↑</kbd>
          </button>
          <button
            className="btn btn--ghost btn--sm"
            disabled={busy || !canNext}
            title="Go to the next change"
            onClick={onNext}
          >
            Next <kbd className="dds-review__key">⌥↓</kbd>
          </button>
        </div>
        <div className="spacer" />
        {/* DCR-FR-CXZG: the convenience path, not the primary one. Reject all
            is one operation; Accept all walks the changes in order and stops
            at the first it cannot apply, which is why the two are not a pair of
            equal weights.

            DCR-FR-11: one group, so the row wraps between the controls and the
            two whole-proposal actions rather than between the two of them. Split
            across two lines they read in the wrong order — the primary action
            leading a row while the other keeps the trailing end of the row
            above it. */}
        <div className="dds-review__all">
          <button
            className="btn btn--sm"
            disabled={busy || left === 0}
            onClick={onRejectAll}
          >
            Reject all
          </button>
          <button
            className="btn btn--primary btn--sm"
            disabled={busy || left === 0}
            onClick={onAcceptAll}
          >
            Accept all
          </button>
        </div>
      </div>
    </div>
  );
}
