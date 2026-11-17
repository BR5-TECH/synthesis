/**
 * DCR-FR-12: the action chip on the change under review.
 *
 * A React overlay positioned over the space the change reserves for it, rather
 * than a decoration widget: these are three real controls that must be
 * focusable, disableable, and reachable in the tab order, and a widget's DOM is
 * created and destroyed by ProseMirror underneath React.
 *
 * It never covers a line of the document. Every change draws a block of its
 * own that holds the space open — the head of the proposed text, and a block
 * below the struck passage of a deletion — and this is placed in that space.
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { Icon } from "../icons";
import { DIAGRAM_LAYOUT_EVENT } from "../markdownMermaid";
import type { DecoratedHunk } from "./hunkDecorations";
import { HUNK_ATTR } from "./hunkDecorations";

/** DCR-FR-HBTM: the accessible name and the tooltip of Compare. */
export const COMPARE_LABEL = "Compare diagrams";

/** The chip's own height, so a deletion's chip sits inside the space it reserves. */
const CHIP_H = 34;

export function HunkChip({
  hunk,
  hostRef,
  busy,
  onAccept,
  onReject,
  onDiscuss,
  onCompare,
}: {
  hunk: DecoratedHunk;
  /** The element the chip is positioned within — the document's scroller. */
  hostRef: React.RefObject<HTMLElement | null>;
  busy: boolean;
  onAccept: () => void;
  onReject: () => void;
  onDiscuss: () => void;
  /**
   * DCR-FR-HBTM: opens the comparison pop-up of a diagram hunk. Given the
   * Compare button, which takes the focus back when the pop-up closes.
   */
  onCompare?: (opener: HTMLElement) => void;
}) {
  const [box, setBox] = useState<{ top: number; left: number } | null>(null);
  // DCR-FR-12: a rejected change can still be taken or talked about while its
  // proposal is open, so its chip holds Accept and Discuss and says why there
  // is no Reject.
  const rejected = hunk.state === "rejected";
  const ref = useRef<HTMLDivElement>(null);
  const compareRef = useRef<HTMLButtonElement>(null);

  // DCR-FR-UKMP: where the pop-up closes on a hunk that is decided, the chip
  // goes with the hunk, and the focus that the Compare button holds goes to the
  // document column. A layout cleanup runs while the button is still in place.
  useLayoutEffect(
    () => () => {
      const host = hostRef.current;
      if (!host || !compareRef.current || document.activeElement !== compareRef.current) return;
      if (!host.hasAttribute("tabindex")) host.setAttribute("tabindex", "-1");
      host.focus({ preventScroll: true });
    },
    [hostRef],
  );

  // Measured from the decorated element rather than from ProseMirror positions:
  // what the chip must sit over is the padding the decoration reserves, which is
  // a property of the rendered box and not of the document.
  //
  // A passive effect, not a layout effect: when the chip and its host mount in
  // one commit, a child's layout effect runs before the parent's ref is set, so
  // the host is not there yet and the chip never shows. A passive effect runs
  // after every ref of the commit is set.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const place = () => {
      const marked = host.querySelectorAll<HTMLElement>(
        `[${HUNK_ATTR}="${CSS.escape(hunk.id)}"]`,
      );
      if (marked.length === 0) {
        setBox(null);
        return;
      }
      /**
       * DCR-FR-12: the chip covers no line of the document, so it is placed
       * over the space the change reserves for it — which is always a block the
       * change draws of its own, never the document's text.
       *
       * A replacement and an insertion reserve it at the head of the proposed
       * text. A deletion proposes none, so it draws a block below the struck
       * passage for exactly this. Placed against the struck text instead, the
       * chip would sit over the words the author is deciding about.
       */
      const own = [...marked].find(
        (el) =>
          el.classList.contains("hunk--add") ||
          el.classList.contains("hunk--del-foot"),
      );
      const target = own ?? marked[0];
      const a = target.getBoundingClientRect();
      const b = host.getBoundingClientRect();
      const top =
        own === undefined
          ? // Nothing of the change's own is drawn, which leaves the struck
            // text. Its foot is the one edge no line of it is on.
            a.bottom - b.top + host.scrollTop - CHIP_H
          : a.top - b.top + host.scrollTop;
      setBox({ top, left: a.left - b.left });
    };
    place();
    // DCR-FR-XNPE: a diagram that settles changes the height of the blocks.
    host.addEventListener(DIAGRAM_LAYOUT_EVENT, place);
    if (typeof ResizeObserver === "undefined") {
      return () => host.removeEventListener(DIAGRAM_LAYOUT_EVENT, place);
    }
    const observer = new ResizeObserver(place);
    observer.observe(host);
    return () => {
      observer.disconnect();
      host.removeEventListener(DIAGRAM_LAYOUT_EVENT, place);
    };
  }, [hunk.id, hunk.kind, hunk.state, hostRef]);

  // The chip stays inside its host: in a narrow pane, a chip placed at the
  // change's left edge would put its last control past the right edge.
  useEffect(() => {
    const host = hostRef.current;
    const chip = ref.current;
    if (box === null || !host || !chip) return;
    const max = Math.max(0, host.clientWidth - chip.offsetWidth);
    if (box.left > max) setBox({ top: box.top, left: max });
  }, [box, hostRef]);

  if (box === null) return null;

  return (
    <div
      ref={ref}
      className="dds-chip"
      style={{ top: box.top, left: box.left }}
      role="group"
      aria-label={`Actions for change ${hunk.position} of ${hunk.total}`}
      data-testid="hunk-chip"
    >
      <span className="dds-chip__at t-ui-xs">
        {hunk.position} / {hunk.total}
      </span>
      {rejected && (
        <span className="dds-chip__state t-ui-xs" data-testid="hunk-chip-rejected">
          Rejected
        </span>
      )}
      {/* DCR-FR-31: a change whose text the prompt no longer holds has nowhere
          to go, so Accept states that reason where the action would have been
          and Reject stays live — rejecting is the only act that clears it. */}
      <button
        className="btn btn--primary btn--sm"
        disabled={busy || hunk.lost}
        title={
          hunk.lost
            ? "The text this change alters is no longer in the prompt"
            : "Accept this change"
        }
        onClick={onAccept}
      >
        Accept
      </button>
      {!rejected && (
        <button
          className="btn btn--sm"
          disabled={busy}
          title="Reject this change"
          onClick={onReject}
        >
          Reject
        </button>
      )}
      {/* DCR-FR-MTFD: a discussion of its own about this one change. */}
      <button
        className="btn btn--ghost btn--sm"
        disabled={busy}
        title="Discuss this change in a discussion of its own"
        onClick={onDiscuss}
      >
        Discuss →
      </button>
      {/* DCR-FR-HBTM: a diagram hunk is compared side by side. Compare decides
          nothing, so it stays on while a decision is in flight. */}
      {hunk.diagram && onCompare && (
        <button
          ref={compareRef}
          type="button"
          className="btn btn--ghost btn--sm"
          aria-label={COMPARE_LABEL}
          title={COMPARE_LABEL}
          onClick={(event) => onCompare(event.currentTarget)}
        >
          <Icon.SideBySide size={12} />
          <span>Compare</span>
        </button>
      )}
      {hunk.lost && (
        <span className="dds-chip__lost t-ui-xs" role="status">
          no longer in the prompt
        </span>
      )}
    </div>
  );
}
