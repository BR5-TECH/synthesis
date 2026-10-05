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
import { useLayoutEffect, useRef, useState } from "react";

import type { DecoratedHunk } from "./hunkDecorations";
import { HUNK_ATTR } from "./hunkDecorations";

/** The chip's own height, so a deletion's chip sits inside the space it reserves. */
const CHIP_H = 34;

export function HunkChip({
  hunk,
  hostRef,
  busy,
  onAccept,
  onReject,
  onDiscuss,
}: {
  hunk: DecoratedHunk;
  /** The element the chip is positioned within — the document's scroller. */
  hostRef: React.RefObject<HTMLElement | null>;
  busy: boolean;
  onAccept: () => void;
  onReject: () => void;
  onDiscuss: () => void;
}) {
  const [box, setBox] = useState<{ top: number; left: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  // Measured from the decorated element rather than from ProseMirror positions:
  // what the chip must sit over is the padding the decoration reserves, which is
  // a property of the rendered box and not of the document.
  useLayoutEffect(() => {
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
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(place);
    observer.observe(host);
    return () => observer.disconnect();
  }, [hunk.id, hunk.kind, hunk.state, hostRef]);

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
        Accept <kbd className="dds-chip__key">⏎</kbd>
      </button>
      <button
        className="btn btn--sm"
        disabled={busy}
        title="Reject this change"
        onClick={onReject}
      >
        Reject <kbd className="dds-chip__key">⌫</kbd>
      </button>
      <button
        className="btn btn--ghost btn--sm"
        disabled={busy}
        title="Reply to this change in the discussion"
        onClick={onDiscuss}
      >
        Discuss →
      </button>
      {hunk.lost && (
        <span className="dds-chip__lost t-ui-xs" role="status">
          no longer in the prompt
        </span>
      )}
    </div>
  );
}
