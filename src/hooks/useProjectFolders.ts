/**
 * The project's folder set, shared by the shell with whoever needs to offer a
 * folder as a choice — today the New Folder window's **Parent Folder** select
 * (NFW-FR-04).
 *
 * NFW's non-functional requirement is that opening that window issues no backend
 * call and costs no filesystem walk: the list comes from the tree the Library
 * already holds. The Library is the one surface that scans, so it *publishes* its
 * tree here on every load (`publishTree`) and this hook keeps the flattened
 * folder list. Because the Library is unmounted whenever the vertical panel shows
 * another surface, a tree change arriving in that window has nobody to reload —
 * so the hook also watches `"project tree changed"` and marks itself stale, and a
 * consumer about to open calls `ensureFresh()`, which scans only in that case.
 *
 * The result: the common path (Library mounted, list already published) opens the
 * window with no call at all, and the list is still complete when it is not.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import { onProjectTreeChanged } from "../events";
import type { TreeNode } from "../types";

/** One selectable folder: its project-relative path, and how it reads. */
export interface FolderOption {
  path: string;
  label: string;
}

/**
 * Flatten a scanned tree into every folder under the root, depth-first in tree
 * order. NFW-FR-04 wants *every* folder that exists — including the ones the
 * Library's active lens hides — so this filters on nothing but node kind, and a
 * folder is offered whether or not it holds anything.
 */
export function flattenFolders(root: TreeNode): FolderOption[] {
  const out: FolderOption[] = [];
  const walk = (node: TreeNode) => {
    for (const c of node.children ?? []) {
      if (c.nodeKind === "folder") {
        out.push({ path: c.path, label: c.path });
        walk(c);
      }
    }
  };
  walk(root);
  return out;
}

/** Every node path in a scanned tree — files and folders alike. */
export function flattenPaths(root: TreeNode): Set<string> {
  const out = new Set<string>();
  const walk = (node: TreeNode) => {
    for (const c of node.children ?? []) {
      out.add(c.id);
      if (c.nodeKind === "folder") walk(c);
    }
  };
  walk(root);
  return out;
}

export interface ProjectFolders {
  folders: FolderOption[];
  /**
   * SNV-FR-67: whether the published tree holds this node — a file or a folder,
   * by the stable path-derived key the tree is built on (ASC-FR-13).
   *
   * `null` means "nothing published yet", which is deliberately NOT the same as
   * `false`: a caller deciding whether to navigate must refuse only on positive
   * evidence of absence, or the first tab activated after a mount — before the
   * Library has ever rendered — would silently do nothing at all.
   *
   * Answered from the tree the Library already publishes, so it costs no read.
   * The classified-artifact list is not usable for this: it holds only files the
   * scan gave a type, and a plain text file is exactly as revealable as a Spec.
   */
  hasPath: (path: string) => boolean | null;
  /** The Library hands its freshly-loaded tree up; this is the no-scan path. */
  publishTree: (tree: TreeNode) => void;
  /** Scan iff the published list may have gone stale. Called before opening. */
  ensureFresh: () => void;
}

/**
 * `contentRootKey` identifies the project + active worktree the list describes.
 *
 * This hook is mounted in `App`, deliberately OUTSIDE the shell subtree that a
 * project or worktree switch remounts — the New Folder window it feeds is mounted
 * out there too. So it has to be told when the content root changed, or a switch
 * would leave the previous project's folders on offer, marked fresh, until the
 * remounted Library got around to publishing.
 */
export function useProjectFolders(contentRootKey: string): ProjectFolders {
  const [folders, setFolders] = useState<FolderOption[]>([]);
  // Nothing has been published yet, so the first consumer has to fetch.
  const stale = useRef(true);
  /**
   * Bumped whenever the list is invalidated — by a tree change, or by the content
   * root changing under us. A scan captures it at dispatch and may only clear the
   * stale flag if it has not moved since: otherwise a scan that started BEFORE an
   * invalidation and resolved after it would mark a pre-change list fresh, and the
   * next open would trust an incomplete set of parents.
   */
  const generation = useRef(0);
  /** A scan already in flight, so two opens in quick succession issue one walk. */
  const scanning = useRef(false);
  /** The content root the list currently describes, readable when a scan lands. */
  const activeRoot = useRef(contentRootKey);

  /**
   * Every path the last published tree held. A ref rather than state: it is read
   * at the moment a navigation is decided, never rendered, so making it state
   * would re-render every consumer of this hook on each tree load for nothing.
   */
  const paths = useRef<Set<string> | null>(null);

  const publishTree = useCallback((tree: TreeNode) => {
    stale.current = false;
    paths.current = flattenPaths(tree);
    setFolders(flattenFolders(tree));
  }, []);

  const hasPath = useCallback(
    (path: string) => (paths.current ? paths.current.has(path) : null),
    [],
  );

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
    // The paths name nodes in the outgoing content root. Dropped to `null`
    // rather than to an empty set: "unknown" must not read as "absent", or every
    // follow would be refused until the incoming Library republished.
    paths.current = null;
    // A scan still in flight against the OUTGOING root must not block one for the
    // incoming root — its own result is discarded when it lands (see below), so
    // leaving this set would make the next open no-op and the window would come up
    // with no parents at all.
    scanning.current = false;
  }, [contentRootKey, invalidate]);

  // ASC-FR-10: the debounced structural-change channel. Only the flag is set —
  // no scan — so a change while nothing needs the list costs nothing, and the
  // Library's own reload (which republishes) is not doubled.
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
    if (!stale.current || scanning.current) return;
    const dispatchedGeneration = generation.current;
    const dispatchedRoot = activeRoot.current;
    scanning.current = true;
    void api
      .loadProjectTree()
      .then((tree) => {
        // A scan that outlived its content root describes a DIFFERENT project's
        // folders. Discarded outright: offering the previous project's paths as
        // parents in the new one would be worse than offering none, and the switch
        // already emptied the list and left it stale, so the next open reads the
        // incoming root.
        if (activeRoot.current !== dispatchedRoot) return;
        if (generation.current === dispatchedGeneration) {
          publishTree(tree);
        } else {
          // Invalidated mid-flight by a tree change, same root. The tree is real
          // and names this project's folders, just possibly one change behind — so
          // it is shown, while the list stays stale so the next open scans again.
          setFolders(flattenFolders(tree));
        }
      })
      // A list that cannot be read degrades to whatever was last published (at
      // worst just the project root); creating a folder still works, and the next
      // open retries because the stale flag was never cleared.
      .catch(() => {})
      .finally(() => {
        scanning.current = false;
      });
  }, [publishTree]);

  return { folders, hasPath, publishTree, ensureFresh };
}
