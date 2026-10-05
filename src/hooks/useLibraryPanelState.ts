/**
 * The Library panel's expand/filter state and its persistence (LIB-FR-14 …
 * LIB-FR-17).
 *
 * This lives in a hook rather than inside `Library` because of the component's
 * lifetime. `VPanel` swaps `<Library>` out for `<Changes>` / `<Notes>` when the
 * user picks another vertical-panel surface, which unmounts the Library
 * outright; holding the state there would lose both the in-memory set (which
 * LIB-FR-06 requires to survive a collapse/hide cycle) and any write still
 * inside the debounce window.
 *
 * The caller is `VPanel`, whose lifetime is exactly one project + one content
 * root: it stays mounted across surface switches, and `App` remounts the whole
 * shell subtree — keyed on the project path and the content-root epoch — when
 * the active worktree changes. That is the boundary this hook needs on both
 * sides. A pending write survives a surface switch, and a worktree switch
 * destroys it with the rest of the subtree rather than flushing it, which is
 * what keeps the outgoing worktree's expanded paths out of the incoming
 * worktree's store (PSS-FR-16).
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import {
  DEFAULT_TYPE_LENS,
  filterToLens,
  lensToFilter,
  type TypeLens,
} from "../artifactTypes";

/**
 * How long a burst of panel-state changes is coalesced before one write
 * (LIB-FR-17): a run of keystrokes in the text filter, or several folders
 * expanded in succession, produce a single save rather than one per change.
 */
export const PERSIST_DEBOUNCE_MS = 300;

/** What `Library` renders from and mutates. */
export interface LibraryPanel {
  /**
   * LIB-FR-15: expansion is recorded, not collapse — a folder renders expanded
   * iff its id (its project-relative path, ASC-FR-13) is in this set, so an
   * unknown folder, including a newly-created one, is collapsed.
   */
  expanded: Set<string>;
  toggleExpanded: (id: string) => void;
  /** NAW-FR-10: expand every ancestor of a revealed node. */
  expandAll: (ids: string[]) => void;
  typeFilter: TypeLens;
  setTypeFilter: (lens: TypeLens) => void;
  text: string;
  setText: (text: string) => void;
}

/**
 * A stable identity for the persisted triple, so a value that merely arrived
 * from the backend — or one that was changed and changed straight back — is
 * never written again. Paths are compared sorted, so the same set of folders
 * has one signature however it was arrived at.
 */
function signature(paths: string[], lens: TypeLens, text: string): string {
  return JSON.stringify([paths, lens, text]);
}

export function useLibraryPanelState(): LibraryPanel {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [typeFilter, setTypeFilter] = useState<TypeLens>(DEFAULT_TYPE_LENS);
  const [text, setText] = useState("");
  // Nothing is written back until the persisted state has landed, or the
  // initial defaults would overwrite what was stored.
  const [restored, setRestored] = useState(false);

  // The panel state last written, so a value that merely arrived from the
  // backend is never written straight back.
  const persisted = useRef<string | null>(null);
  // Dispatch ordinal of the most recent write, so an older write settling out
  // of order cannot claim `persisted` (see the persist effect).
  const writeSeq = useRef(0);

  /**
   * Which fields the user has already changed. The filter input and the type
   * select are interactive from the first paint, so a keystroke can land before
   * the restore resolves; applying the stored value over it would silently
   * revert what the user just did. A touched field keeps the user's value and
   * the restore fills in only the rest.
   */
  const touched = useRef({ expanded: false, lens: false, text: false });

  /**
   * The current values, mirrored synchronously so the restore below can read
   * what the user has already done without waiting for a render to commit. The
   * setters write here first and then to state; nothing else may write these.
   */
  const current = useRef({
    expanded: new Set<string>(),
    lens: DEFAULT_TYPE_LENS as TypeLens,
    text: "",
  });

  // LIB-FR-14: restore the expanded set and the two local filters for this
  // worktree. Runs once per project + content root, so a worktree switch
  // restores the incoming worktree's own record and nothing is carried across
  // (LIB-FR-13).
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      let state: Awaited<ReturnType<typeof api.loadLibraryPanelState>> | null =
        null;
      try {
        state = await api.loadLibraryPanelState();
      } catch {
        // A state that cannot be read leaves the panel on its defaults rather
        // than blocking it; nothing here is worth an error banner.
      }
      if (cancelled) return;

      const paths = state?.expandedPaths ?? [];
      const lens = filterToLens(state?.artifactTypeFilter);
      const restoredText = state?.textFilter ?? "";

      // A field the user already touched keeps their value; an untouched one
      // takes the stored value. The result is what is actually in effect, so
      // the guard below is seeded with the merged triple rather than with
      // either half alone — and the first real change persists the merge.
      //
      // Expansion merges rather than being overridden: everything starts
      // collapsed, so the only thing a user can do before the restore lands is
      // expand something, and a union keeps both their click and the layout
      // they were expecting back. Replacing either way would throw one of them
      // away for no reason.
      if (touched.current.expanded) {
        current.current.expanded = new Set([
          ...current.current.expanded,
          ...paths,
        ]);
      } else {
        current.current.expanded = new Set(paths);
      }
      setExpanded(current.current.expanded);
      if (!touched.current.lens) {
        current.current.lens = lens;
        setTypeFilter(lens);
      }
      if (!touched.current.text) {
        current.current.text = restoredText;
        setText(restoredText);
      }

      persisted.current = signature(
        [...current.current.expanded].sort(),
        current.current.lens,
        current.current.text,
      );
      setRestored(true);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // LIB-FR-17: persist after any change to the expanded set, the lens, or the
  // text filter, coalescing rapid changes into a single write. The record is
  // written whole, so persisting one value carries the other two through
  // unchanged.
  useEffect(() => {
    if (!restored) return;
    // Sorted so the persisted order is stable across sessions and the guard
    // below is insensitive to the order folders happened to be expanded in.
    const paths = [...expanded].sort();
    const next = signature(paths, typeFilter, text);
    if (persisted.current === next) return;
    const timer = setTimeout(() => {
      // Two writes can be in flight at once — a slow one, then a newer one
      // dispatched before it settled. Whichever reaches disk last is what the
      // store holds, so only the newest dispatch may record what is persisted;
      // an older one settling later must not claim the guard, or a change back
      // to its value would be suppressed and never written.
      const ticket = ++writeSeq.current;
      void api
        .saveLibraryPanelState({
          expandedPaths: paths,
          artifactTypeFilter: lensToFilter(typeFilter),
          textFilter: text,
        })
        // Only a write that actually landed counts as persisted, so a failure
        // is retried by the next change rather than being assumed durable
        // (LIB non-functional requirement).
        .then(() => {
          if (ticket === writeSeq.current) persisted.current = next;
        })
        .catch(() => {});
    }, PERSIST_DEBOUNCE_MS);
    // Cancelling rather than flushing is deliberate. This hook is unmounted
    // only by a project or worktree switch, and by then the backend already
    // resolves project-local storage against the *new* worktree (PSS-FR-16), so
    // a flush would write the outgoing worktree's expanded paths into the
    // incoming one's store. A surface switch does not reach here at all — the
    // hook outlives it — so the case this drops is at most a few hundred
    // milliseconds of toggling immediately before changing worktree.
    return () => clearTimeout(timer);
  }, [expanded, typeFilter, text, restored]);

  // Every setter marks its field touched, updates the synchronous mirror, and
  // then sets state — in that order, so a restore resolving between any two of
  // those steps still sees a field the user has changed and the value they
  // changed it to.
  return {
    expanded,
    toggleExpanded: (id) => {
      touched.current.expanded = true;
      const next = new Set(current.current.expanded);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      current.current.expanded = next;
      setExpanded(next);
    },
    expandAll: (ids) => {
      if (ids.length === 0) return;
      if (ids.every((id) => current.current.expanded.has(id))) return;
      touched.current.expanded = true;
      const next = new Set(current.current.expanded);
      for (const id of ids) next.add(id);
      current.current.expanded = next;
      setExpanded(next);
    },
    typeFilter,
    setTypeFilter: (lens) => {
      touched.current.lens = true;
      current.current.lens = lens;
      setTypeFilter(lens);
    },
    text,
    setText: (value) => {
      touched.current.text = true;
      current.current.text = value;
      setText(value);
    },
  };
}
