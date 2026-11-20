/**
 * The folders the Project panel has loaded, one folder at a time
 * (LIB-FR-EMOO, LIB-FR-10, LIB-FR-11, LIB-FR-18).
 *
 * The panel holds the children of the root and of each folder it has opened,
 * and nothing else. Every read is one backend call for one folder, which the
 * project index answers off the main thread (ASC-FR-TPJO), so the size of the
 * project never reaches the window.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import type { TreeNode } from "../../types";

/** The root folder's key in the loaded map. */
export const ROOT = "";

export interface LazyTree {
  /** The children of the project root, or null before the first load. */
  root: TreeNode[] | null;
  /** The loaded children of each folder, the root included under `""`. */
  childrenOf: ReadonlyMap<string, TreeNode[]>;
  error: string | null;
  /** Each node the panel holds, by id. */
  nodeById: ReadonlyMap<string, TreeNode>;
  /** LIB-FR-EMOO: load each of `folders` that is not loaded or loading. */
  ensureLoaded: (folders: string[]) => void;
  /** LIB-FR-10: load again each of `folders` that the panel holds. */
  reloadFolders: (folders: string[]) => void;
  /** LIB-FR-11: build the index again and drop every loaded folder. */
  rescan: () => Promise<void>;
  /** Put a changed node in place, as an assignment returns it. */
  replaceNode: (node: TreeNode) => void;
  /**
   * LIB-FR-18: load the folders on the way to `id`. Resolves true when the
   * index holds it.
   */
  loadPathTo: (id: string) => Promise<boolean>;
}

/**
 * `prev` with the new children of `folder`. A child folder that is no longer
 * among them is dropped from the map with everything below it, so a folder
 * that goes and later comes back loads again rather than showing the
 * children it had before (LIB-FR-10).
 */
export function withFolder(
  prev: ReadonlyMap<string, TreeNode[]>,
  folder: string,
  children: TreeNode[],
): Map<string, TreeNode[]> {
  const next = new Map(prev);
  next.set(folder, children);
  const held = new Set(children.filter((c) => c.nodeKind === "folder").map((c) => c.id));
  const prefix = folder === ROOT ? "" : `${folder}/`;
  for (const key of prev.keys()) {
    if (key === folder || !key.startsWith(prefix) || key === ROOT) continue;
    const top = key.slice(prefix.length).split("/")[0];
    const child = prefix + top;
    if (!held.has(child)) next.delete(key);
  }
  return next;
}

export function useLazyTree(): LazyTree {
  const [childrenOf, setChildrenOf] = useState<Map<string, TreeNode[]>>(() => new Map());
  const [error, setError] = useState<string | null>(null);
  // The latest request in flight, by folder. A ref: it decides whether to send
  // a request and which answer to keep, and is never rendered. Only the answer
  // to the latest request of a folder is kept, so a read that describes the
  // tree from before a burst never lands over one that describes it after.
  const pending = useRef(new Map<string, number>());
  const counter = useRef(0);
  // The token of the read whose answer each folder shows now. A path read
  // (LIB-FR-18) skips a folder that a newer read already answered.
  const shown = useRef(new Map<string, number>());
  // Bumped by a rescan, so an answer to a request made before it is dropped.
  const generation = useRef(0);
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const load = useCallback((folder: string, force = false) => {
    if (!force && pending.current.has(folder)) return;
    const token = ++counter.current;
    pending.current.set(folder, token);
    const current = () => mounted.current && pending.current.get(folder) === token;
    api
      .listTreeChildren(folder)
      .then((children) => {
        if (!current()) return;
        if (!Array.isArray(children)) {
          // An answer that is not a list says nothing about the folder, so
          // the folder stays as it was rather than break the rows.
          logWarn(["frontend"], "project panel: folder read gave no list", { folder });
          return;
        }
        shown.current.set(folder, token);
        setChildrenOf((prev) => withFolder(prev, folder, children));
        setError(null);
      })
      .catch((e: unknown) => {
        if (!current()) return;
        const message = String(e);
        if (message.includes(api.ERR_PATH_NOT_IN_TREE)) {
          // The folder went away between the event and the read. Its parent's
          // reload drops it from the rows, so this is not an error.
          logDebug(["frontend"], "project panel: folder no longer in the tree", {
            folder,
          });
          setChildrenOf((prev) => {
            if (!prev.has(folder)) return prev;
            const next = new Map(prev);
            next.delete(folder);
            return next;
          });
          return;
        }
        logWarn(["frontend"], "project panel: folder read failed", { folder, reason: message });
        if (folder === ROOT) setError(message);
      })
      .finally(() => {
        if (pending.current.get(folder) === token) pending.current.delete(folder);
      });
  }, []);

  // The root loads on mount.
  useEffect(() => {
    load(ROOT);
  }, [load]);

  const childrenRef = useRef(childrenOf);
  childrenRef.current = childrenOf;

  const ensureLoaded = useCallback(
    (folders: string[]) => {
      for (const folder of folders) {
        if (!childrenRef.current.has(folder)) load(folder);
      }
    },
    [load],
  );

  const reloadFolders = useCallback(
    (folders: string[]) => {
      for (const folder of folders) {
        // A reload is never lost behind a read already in flight, which can
        // describe the tree from before the burst.
        if (childrenRef.current.has(folder)) load(folder, true);
      }
    },
    [load],
  );

  const rescan = useCallback(async () => {
    generation.current += 1;
    pending.current.clear();
    shown.current.clear();
    try {
      const root = await api.rescanProjectTree();
      if (!mounted.current) return;
      if (!Array.isArray(root)) {
        logWarn(["frontend"], "project panel: rescan gave no list");
        return;
      }
      setChildrenOf(new Map([[ROOT, root]]));
      setError(null);
    } catch (e) {
      if (!mounted.current) return;
      setError(String(e));
    }
  }, []);

  const replaceNode = useCallback((node: TreeNode) => {
    setChildrenOf((prev) => {
      const at = node.id.lastIndexOf("/");
      const parent = at < 0 ? ROOT : node.id.slice(0, at);
      const siblings = prev.get(parent);
      if (!siblings) return prev;
      const next = new Map(prev);
      next.set(
        parent,
        siblings.map((s) => (s.id === node.id ? { ...s, ...node, children: undefined } : s)),
      );
      return next;
    });
  }, []);

  const loadPathTo = useCallback(async (id: string) => {
    const asked = generation.current;
    const token = ++counter.current;
    const folders = await api.listTreePath(id);
    if (!mounted.current || asked !== generation.current) return false;
    if (folders !== null && !Array.isArray(folders)) {
      logWarn(["frontend"], "project panel: path read gave no list", { id });
      return false;
    }
    if (!folders) return false;
    // A folder read sent after this path read, answered or still in flight,
    // describes the tree from later, so it wins over this answer.
    const fresh = folders.filter(
      (entry) =>
        (pending.current.get(entry.folder) ?? 0) < token &&
        (shown.current.get(entry.folder) ?? 0) < token,
    );
    for (const entry of fresh) shown.current.set(entry.folder, token);
    setChildrenOf((prev) => {
      const next = new Map(prev);
      for (const entry of fresh) next.set(entry.folder, entry.children);
      return next;
    });
    return true;
  }, []);

  const nodeById = useMemo(() => {
    const map = new Map<string, TreeNode>();
    for (const list of childrenOf.values()) for (const node of list) map.set(node.id, node);
    return map;
  }, [childrenOf]);

  return {
    root: childrenOf.get(ROOT) ?? null,
    childrenOf,
    error,
    nodeById,
    ensureLoaded,
    reloadFolders,
    rescan,
    replaceNode,
    loadPathTo,
  };
}
