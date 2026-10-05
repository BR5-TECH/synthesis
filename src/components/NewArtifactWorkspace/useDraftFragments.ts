/**
 * The fragment discussions of a draft's prompt, as the tab wires them
 * (`../../../specifications/ui/DDS-draft-discussion.md` DDS-FR-QMBC,
 * `NAW-new-artifact.md` NAW-FR-14).
 *
 * Three things meet here. The Editor marks the passages and offers Comment on a
 * selection. The discussion column renders every discussion, the fragment ones
 * included, and opens a new one from its opening composer. And the draft's
 * focus registry moves the author between the two: activating a mark focuses
 * the discussion in the column, and activating the quote in the column scrolls
 * the passage into view.
 *
 * The discussion is never rendered here. This hook holds only the state that
 * is about the tab: the passage being commented on and which passage is
 * focused.
 */
import { useCallback, useMemo, useState } from "react";

import type { EditorFragmentHost } from "../Editor/props";
import type { OwnerAvailability } from "../discussion";
import { findNearest } from "../../state/commentAnchors";
import { focusDiscussion, requestPendingFocus } from "../../state/discussionFocus";
import { setDraftDiscussionHidden } from "../../state/draftDiscussion";
import { logDebug } from "../../logging";
import {
  discussionFragment,
  type Discussion,
  type FragmentTarget,
} from "../../types";

export interface DraftFragments {
  /** Lent to the Editor. Absent where the prompt takes no new comment. */
  host: EditorFragmentHost;
  /** The passage the opening composer is about to carry. */
  openingFragment: FragmentTarget | null;
  cancelOpening: () => void;
  /** Called once the opening composer posted. */
  opened: (discussion: Discussion) => void;
  availabilityOf: (discussion: Discussion) => OwnerAvailability;
  focusFragment: (fragment: FragmentTarget, discussion: Discussion) => void;
}

export function useDraftFragments(input: {
  draftId: string;
  /** The draft-relative path of the prompt on screen. */
  path: string | null;
  /** The prompt's Markdown source, which a fragment is a range in. */
  source: string;
  discussions: readonly Discussion[];
  selectedThreadId: string | null;
  setSelectedThreadId: (id: string | null) => void;
  /** Raised so the column's caret follows a new opening composer. */
  onOpeningStarted: () => void;
}): DraftFragments {
  const { draftId, path, source, discussions } = input;
  const [opening, setOpening] = useState<FragmentTarget | null>(null);
  const [reveal, setReveal] = useState<{ id: string; n: number } | null>(null);

  /**
   * CMT-FR-18: a fragment is placed by searching for its stored quote nearest
   * to its stored offsets. A quote found nowhere leaves the discussion kept and
   * shown as orphaned, never dropped.
   */
  const placed = useMemo(() => {
    const out = new Map<string, { start: number; end: number }>();
    for (const d of discussions) {
      const f = discussionFragment(d);
      if (f === null || f.path !== path) continue;
      const at = findNearest(source, f.quote, f.start);
      if (at !== -1) out.set(d.id, { start: at, end: at + f.quote.length });
    }
    return out;
  }, [discussions, path, source]);

  const fragments = useMemo(
    () =>
      discussions.flatMap((d) => {
        const range = placed.get(d.id);
        const f = discussionFragment(d);
        return range && f
          ? [{ id: d.id, ...range, quote: f.quote, resolved: d.resolved }]
          : [];
      }),
    [discussions, placed],
  );

  const selected = discussions.find((d) => d.id === input.selectedThreadId);
  const focusedId =
    selected && discussionFragment(selected) !== null ? selected.id : null;

  const showColumn = useCallback(
    () => setDraftDiscussionHidden(draftId, false),
    [draftId],
  );

  const comment = useCallback(
    (selection: { start: number; end: number; quote: string }) => {
      if (path === null) return;
      logDebug(["frontend"], "a comment on a draft prompt passage was started", {
        draftId,
      });
      setOpening({
        owner: { kind: "draft", draftId },
        path,
        start: selection.start,
        end: selection.end,
        quote: selection.quote,
      });
      showColumn();
      input.onOpeningStarted();
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [draftId, path, showColumn, input.onOpeningStarted],
  );

  const activate = useCallback(
    (discussionId: string) => {
      input.setSelectedThreadId(discussionId);
      showColumn();
      // CVP-FR-06: focus the surface that shows the discussion, or hold the
      // request for the surface that is about to.
      if (focusDiscussion(discussionId, "discussion") === "closed") {
        requestPendingFocus(discussionId, "discussion");
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [showColumn, input.setSelectedThreadId],
  );

  const host = useMemo<EditorFragmentHost>(
    () => ({
      fragments,
      focusedId,
      reveal,
      onComment: comment,
      onActivate: activate,
    }),
    [fragments, focusedId, reveal, comment, activate],
  );

  const availabilityOf = useCallback(
    (d: Discussion): OwnerAvailability => {
      const f = discussionFragment(d);
      if (f === null) return "available";
      // The prompt the fragment was written against is gone.
      if (f.path !== path) return "unavailable";
      return placed.has(d.id) ? "available" : "orphaned";
    },
    [path, placed],
  );

  const focusFragment = useCallback((_f: FragmentTarget, d: Discussion) => {
    setReveal((prev) => ({ id: d.id, n: (prev?.n ?? 0) + 1 }));
  }, []);

  return {
    host,
    openingFragment: opening,
    cancelOpening: () => setOpening(null),
    opened: (d) => {
      setOpening(null);
      input.setSelectedThreadId(d.id);
    },
    availabilityOf,
    focusFragment,
  };
}
