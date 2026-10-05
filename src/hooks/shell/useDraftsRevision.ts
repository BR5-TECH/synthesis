/**
 * DRP-FR-05 / DRS-FR-22: the counter every surface that lists drafts reloads
 * on, and the backend channel that bumps it.
 */
import { useCallback, useEffect, useState } from "react";
import { onDraftsChanged } from "../../events";

export function useDraftsRevision(): {
  draftsRevision: number;
  bumpDrafts: () => void;
} {
  /**
   * DRP-FR-05: bumped whenever the draft set moves — a draft created, renamed,
   * graduated, or restatused from a New Artifact tab. The Drafts panel reloads
   * on it, so the list reflects a change made in the viewport without the author
   * refreshing anything.
   *
   * Bumped from two places: directly by the surfaces of this window, and by the
   * backend's `"drafts changed"` event below (DRS-FR-22). The direct bump is not
   * redundant — it lands in the same commit as the action that caused it, where
   * the event arrives a tick later, and it is what keeps the panel correct if
   * the event is ever dropped.
   */
  const [draftsRevision, setDraftsRevision] = useState(0);
  const bumpDrafts = useCallback(() => setDraftsRevision((n) => n + 1), []);

  // DRP-FR-05 / DRS-FR-22: a draft changed anywhere — including in another
  // window, or by a write this session did not make.
  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    void onDraftsChanged(bumpDrafts).then((fn) => {
      if (cancelled) fn();
      else stop = fn;
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [bumpDrafts]);

  return { draftsRevision, bumpDrafts };
}
