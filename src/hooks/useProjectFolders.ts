/**
 * The project's folder set, shared by the shell with whoever needs to offer a
 * folder as a choice — the Parent Folder and Location selects of the New
 * Folder, New File, and New Artifact windows (NFW-FR-04, NFI-FR-04, NTA-FR-04).
 *
 * The list is read from the project index with one `"search project paths"`
 * call (ASC-FR-AVVY), which walks no folder of the disk. The hook watches
 * `"project tree changed"` and marks itself stale, and a consumer about to
 * open calls `ensureFresh()`, which reads only in that case. A list that is
 * not stale opens the window with no call at all.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import { onProjectTreeChanged } from "../events";

/** One selectable folder: its project-relative path, and how it reads. */
export interface FolderOption {
  path: string;
  label: string;
}

export interface ProjectFolders {
  folders: FolderOption[];
  /** Read the folder list iff it may have gone stale. Called before opening. */
  ensureFresh: () => void;
}

/**
 * NFW-FR-04 / NFI-FR-04 / NTA-FR-04: every folder of the project, for the
 * Location and Parent Folder selects, read from `"search project paths"` with
 * `kind = folder` and no limit (ASC-FR-AVVY).
 *
 * `contentRootKey` identifies the project + active worktree the list describes.
 *
 * This hook is mounted in `App`, deliberately OUTSIDE the shell subtree that a
 * project or worktree switch remounts — the New Folder window it feeds is mounted
 * out there too. So it has to be told when the content root changed, or a switch
 * would leave the previous project's folders on offer, marked fresh.
 */
export function useProjectFolders(contentRootKey: string): ProjectFolders {
  const [folders, setFolders] = useState<FolderOption[]>([]);
  // Nothing has been read yet, so the first consumer has to fetch.
  const stale = useRef(true);
  /**
   * Bumped whenever the list is invalidated — by a tree change, or by the content
   * root changing under us. A read captures it at dispatch and may only clear the
   * stale flag if it has not moved since: otherwise a read that started BEFORE an
   * invalidation and resolved after it would mark a pre-change list fresh, and the
   * next open would trust an incomplete set of parents.
   */
  const generation = useRef(0);
  /** A read already in flight, so two opens in quick succession issue one. */
  const reading = useRef(false);
  /** The content root the list currently describes, readable when a read lands. */
  const activeRoot = useRef(contentRootKey);

  const invalidate = useCallback(() => {
    generation.current += 1;
    stale.current = true;
  }, []);

  // A project or worktree switch: the paths this list holds name folders in the
  // outgoing content root, so they are dropped rather than left on offer.
  useEffect(() => {
    activeRoot.current = contentRootKey;
    invalidate();
    setFolders([]);
    // A read still in flight against the OUTGOING root must not block one for
    // the incoming root — its own result is discarded when it lands.
    reading.current = false;
  }, [contentRootKey, invalidate]);

  // ASC-FR-10: the debounced structural-change channel. Only the flag is set —
  // no read — so a change while nothing needs the list costs nothing.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onProjectTreeChanged(invalidate).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [invalidate]);

  const ensureFresh = useCallback(() => {
    if (!stale.current || reading.current) return;
    const dispatchedGeneration = generation.current;
    const dispatchedRoot = activeRoot.current;
    reading.current = true;
    void api
      .searchProjectPaths("", "folder")
      .then(({ paths }) => {
        // A read that outlived its content root describes a DIFFERENT
        // project's folders, so it is discarded outright.
        if (activeRoot.current !== dispatchedRoot) return;
        setFolders(paths.map((path) => ({ path, label: path })));
        // Invalidated mid-flight by a tree change, same root: shown, but the
        // list stays stale so the next open reads again.
        if (generation.current === dispatchedGeneration) stale.current = false;
      })
      // A list that cannot be read degrades to whatever was last read (at worst
      // just the project root); creating a folder still works, and the next
      // open retries because the stale flag was never cleared.
      .catch(() => {})
      .finally(() => {
        reading.current = false;
      });
  }, []);

  return { folders, ensureFresh };
}
