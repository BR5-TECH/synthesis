/**
 * The fragment discussions a host lends the Editor
 * (`../../../specifications/ui/DDS-draft-discussion.md` DDS-FR-QMBC).
 *
 * An embedded Editor that has no comment rail of its own — the New Artifact tab,
 * whose discussions render in its own column — still marks the passages its
 * fragment discussions are about, offers Comment on a selection, and reports a
 * click on a mark. This hook turns the host's list into the shape the anchor
 * layer already places, so a mark is placed, highlighted, and scrolled to by the
 * same code that serves a rail.
 */
import { useEffect, useMemo } from "react";
import type { RefObject } from "react";

import type { EditorFragmentHost } from "./props";

/** The thread shape `useCommentAnchors` places. */
interface HostThread {
  thread: { id: string; resolved: boolean };
  anchor: { start: number; end: number; quote: string };
}

export function useFragmentHost(
  host: EditorFragmentHost | undefined,
  setFocusedThread: (id: string | null) => void,
  rootRef: RefObject<HTMLElement | null>,
): { threads: HostThread[]; active: boolean } {
  const fragments = host?.fragments;
  const threads = useMemo<HostThread[]>(
    () =>
      (fragments ?? []).map((f) => ({
        thread: { id: f.id, resolved: f.resolved },
        anchor: { start: f.start, end: f.end, quote: f.quote },
      })),
    [fragments],
  );

  // DDS-FR-QMBC: focusing a discussion in the column focuses its mark, which is
  // what scrolls the passage into view.
  const focusedId = host?.focusedId ?? null;
  useEffect(() => {
    if (host) setFocusedThread(focusedId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusedId, host !== undefined]);

  // DDS-FR-QMBC: activating the quote in the column scrolls its passage into
  // view, again each time it is asked for.
  const reveal = host?.reveal ?? null;
  useEffect(() => {
    if (!reveal) return;
    const mark = rootRef.current?.querySelector<HTMLElement>(
      `[data-comment-thread="${CSS.escape(reveal.id)}"]`,
    );
    mark?.scrollIntoView?.({ block: "center" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reveal?.n]);

  return { threads, active: host !== undefined };
}
