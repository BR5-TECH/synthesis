/**
 * The Drafts panel's status position, filter text and expanded folder set, and
 * their persistence (DRP-FR-07, DRP-FR-14 / PSS-FR-20).
 *
 * This lives in a hook rather than inside `DraftsPanel` for the reason
 * `useLibraryPanelState` does: `VPanel` swaps `<DraftsPanel>` out for another
 * vertical-panel surface, which unmounts it outright, and DRP-FR-14 requires all
 * three to survive exactly that. The caller is `VPanel`, whose lifetime is one
 * project + one content root — so a pending write survives a surface switch, and
 * a worktree switch destroys it with the rest of the subtree rather than
 * flushing it, which is what keeps the outgoing worktree's expanded folders out
 * of the incoming worktree's store (PSS-FR-16).
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import type { DraftsStatusFilter } from "../types";

/**
 * How long a burst of panel-state changes is coalesced before one write
 * (DRP-FR-14): a run of keystrokes in the text filter, or several folders
 * expanded in succession, produce a single save rather than one per change.
 */
export const PERSIST_DEBOUNCE_MS = 300;

/** DRP-FR-07: active is the default — a retired draft is not what the author
 *  came to the panel for, and the archived ones are one position away. */
const DEFAULT_FILTER: DraftsStatusFilter = "active";

/** What `DraftsPanel` renders from and mutates. */
export interface DraftsPanel {
  filter: DraftsStatusFilter;
  setFilter: (filter: DraftsStatusFilter) => void;
  text: string;
  setText: (text: string) => void;
  /**
   * DRP-FR-14: expansion is recorded, not collapse — a folder renders expanded
   * iff its drafts-root-relative path is in this set, so an unknown folder,
   * a newly created one included, is collapsed.
   */
  expanded: Set<string>;
  toggleExpanded: (path: string) => void;
  /** DRP-FR-23 / DRP-FR-26: reveal a folder the author is about to work in. */
  expandAll: (paths: string[]) => void;
  /**
   * DRP-FR-24 / DRP-FR-30: carry the expansion of a folder — and of everything
   * beneath it — from the path it sat at to the one it now sits at.
   *
   * The set is keyed by path because a drafts folder has no id: its identity IS
   * where it sits (DRS-FR-29). That is the right key for persistence, and it
   * means a rename or a move silently collapses the folder and its whole
   * expanded subtree unless the keys are rewritten with it.
   */
  reparentExpanded: (from: string, to: string) => void;
}

/**
 * A stable identity for the persisted triple, so a value that merely arrived
 * from the backend — or one changed and changed straight back — is never written
 * again. Paths are compared sorted, so the same set has one signature however it
 * was arrived at.
 */
function signature(
  filter: DraftsStatusFilter,
  text: string,
  paths: string[],
): string {
  return JSON.stringify([filter, text, paths]);
}

function isFilter(value: unknown): value is DraftsStatusFilter {
  return value === "active" || value === "archived" || value === "all";
}

export function useDraftsPanelState(): DraftsPanel {
  const [filter, setFilter] = useState<DraftsStatusFilter>(DEFAULT_FILTER);
  const [text, setText] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  // Nothing is written back until the persisted state has landed, or the
  // initial defaults would overwrite what was stored.
  const [restored, setRestored] = useState(false);

  const persisted = useRef<string | null>(null);
  // Dispatch ordinal of the most recent write, so an older write settling out
  // of order cannot claim `persisted`.
  const writeSeq = useRef(0);

  /**
   * Which fields the author has already changed. Both filter controls are
   * interactive from the first paint, so a keystroke can land before the restore
   * resolves; applying the stored value over it would silently revert what they
   * just did.
   */
  const touched = useRef({ filter: false, text: false, expanded: false });

  /**
   * The current values, mirrored synchronously so the restore below can read
   * what the author has already done without waiting for a render to commit.
   */
  const current = useRef({
    filter: DEFAULT_FILTER as DraftsStatusFilter,
    text: "",
    expanded: new Set<string>(),
  });

  // DRP-FR-14: restore for this worktree. Runs once per project + content root,
  // so a worktree switch restores the incoming worktree's own record and nothing
  // is carried across.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      let state: Awaited<ReturnType<typeof api.loadDraftsPanelState>> | null =
        null;
      try {
        state = await api.loadDraftsPanelState();
      } catch {
        // A state that cannot be read leaves the panel on its defaults rather
        // than blocking it; nothing here is worth an error banner
        // (DRP non-functional requirements).
      }
      if (cancelled) return;

      const storedFilter = isFilter(state?.statusFilter)
        ? state.statusFilter
        : DEFAULT_FILTER;
      const storedText = state?.textFilter ?? "";
      const storedPaths = state?.expandedFolders ?? [];

      if (!touched.current.filter) {
        current.current.filter = storedFilter;
        setFilter(storedFilter);
      }
      if (!touched.current.text) {
        current.current.text = storedText;
        setText(storedText);
      }
      // Expansion merges rather than being overridden: everything starts
      // collapsed, so the only thing the author can do before the restore lands
      // is expand something, and a union keeps both their click and the tree
      // they were expecting back.
      current.current.expanded = touched.current.expanded
        ? new Set([...current.current.expanded, ...storedPaths])
        : new Set(storedPaths);
      setExpanded(current.current.expanded);

      persisted.current = signature(
        current.current.filter,
        current.current.text,
        [...current.current.expanded].sort(),
      );
      setRestored(true);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // DRP-FR-14: persist after any change, coalescing rapid ones into a single
  // write. The record is written whole, so persisting one value carries the
  // other two through unchanged.
  useEffect(() => {
    if (!restored) return;
    const paths = [...expanded].sort();
    const next = signature(filter, text, paths);
    if (persisted.current === next) return;
    const timer = setTimeout(() => {
      // Two writes can be in flight at once. Whichever reaches disk last is what
      // the store holds, so only the newest dispatch may record what is
      // persisted; an older one settling later must not claim the guard.
      const ticket = ++writeSeq.current;
      void api
        .saveDraftsPanelState({
          statusFilter: filter,
          textFilter: text,
          expandedFolders: paths,
        })
        // Only a write that actually landed counts as persisted, so a failure is
        // retried by the next change rather than assumed durable.
        .then(() => {
          if (ticket === writeSeq.current) persisted.current = next;
        })
        .catch(() => {});
    }, PERSIST_DEBOUNCE_MS);
    // Cancelled rather than flushed, for the reason the Library's is: by the
    // time this hook unmounts the backend already resolves project-local storage
    // against the *new* worktree (PSS-FR-16), so a flush would write the
    // outgoing worktree's expanded folders into the incoming one's store.
    return () => clearTimeout(timer);
  }, [filter, text, expanded, restored]);

  // Every setter marks its field touched, updates the synchronous mirror, and
  // then sets state — in that order, so a restore resolving between any two of
  // those steps still sees a field the author has changed.
  return {
    filter,
    setFilter: (value) => {
      touched.current.filter = true;
      current.current.filter = value;
      setFilter(value);
    },
    text,
    setText: (value) => {
      touched.current.text = true;
      current.current.text = value;
      setText(value);
    },
    expanded,
    toggleExpanded: (path) => {
      touched.current.expanded = true;
      const next = new Set(current.current.expanded);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      current.current.expanded = next;
      setExpanded(next);
    },
    expandAll: (paths) => {
      const wanted = paths.filter((p) => p !== "");
      if (wanted.length === 0) return;
      if (wanted.every((p) => current.current.expanded.has(p))) return;
      touched.current.expanded = true;
      const next = new Set(current.current.expanded);
      for (const path of wanted) next.add(path);
      current.current.expanded = next;
      setExpanded(next);
    },
    reparentExpanded: (from, to) => {
      if (from === "" || from === to) return;
      const moved = [...current.current.expanded].filter(
        (p) => p === from || p.startsWith(`${from}/`),
      );
      if (moved.length === 0) return;
      touched.current.expanded = true;
      const next = new Set(current.current.expanded);
      for (const path of moved) {
        next.delete(path);
        next.add(`${to}${path.slice(from.length)}`);
      }
      current.current.expanded = next;
      setExpanded(next);
    },
  };
}
