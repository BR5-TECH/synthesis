import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../api";
import { onProjectTreeChanged } from "../../events";
import {
  markRevealConsumed,
  revealAlreadyConsumed,
} from "../../state/panelReveal";
import {
  matchesLens,
  matchesText,
  presentTypes,
  typeChip,
  typeLensPositions,
} from "../../artifactTypes";
import type { LibraryPanel } from "../../hooks/useLibraryPanelState";
import {
  FILTERED_BODY_CLASS,
  PanelFilteredState,
} from "../PanelEmptyState";
import { Icon } from "../icons";
import { SelectorRow } from "../SelectorRow";
import { TreeRow } from "./TreeRow";
import {
  ContextMenu,
  RecursiveDeleteConfirm,
  type ContextMenuState,
} from "./menu";
import type {
  ArtifactType,
  AssignScope,
  OpenableArtifact,
  PanelRevealRequest,
  TreeNode,
} from "../../types";

export { pressLandsOutside, rejectRename, type RenameRejection } from "./menu";

interface LibraryProps {
  /**
   * The expand/filter state and its persistence (LIB-FR-14 … LIB-FR-17), owned
   * by `VPanel` rather than by this component: switching the vertical panel to
   * another surface unmounts the Library, and the state has to outlive that
   * (LIB-FR-06).
   */
  panel: LibraryPanel;
  onOpenArtifact: (node: OpenableArtifact) => void;
  // LCM-FR-05: switch the Notes vertical-panel surface to an artifact's scope
  // without opening it in a tab. A navigation affordance only.
  onShowNotes: (artifact: OpenableArtifact) => void;
  // LCM-FR-10: open the New File modal with the right-clicked folder as its
  // starting location.
  onNewFile?: (folder: TreeNode) => void;
  // LCM-FR-08: open the New Artifact modal scoped to the right-clicked folder.
  onNewArtifact: (folder: TreeNode) => void;
  // LCM-FR-09: open the New Folder modal with the right-clicked folder as its
  // starting parent.
  onNewFolder?: (folder: TreeNode) => void;
  /**
   * NFW-FR-04 / NFI-FR-04: hand each freshly-loaded tree up to the shell. This
   * panel is the only surface that scans, so the folder set the New Folder and New
   * File windows offer comes from here rather than from a scan of their own.
   */
  onTreeLoaded?: (tree: TreeNode) => void;
  /**
   * LIB-FR-18: a pending reveal-and-select, or null.
   *
   * Carries a nonce rather than being a bare id, because two consecutive
   * requests can name the same node — an Editor tab activated, a Dashboard
   * activated, that same tab activated again (SNV-FR-64) — and a consumer keyed
   * on the id alone would treat the second as nothing to do. Reached from a
   * creation (NAW-FR-10 / NFI-FR-12 / NFW-FR-11), a Notes group header
   * (NTS-FR-11), and the active tab being followed (SNV-FR-66).
   */
  reveal?: PanelRevealRequest | null;
  /** LIB-FR-VMAQ: open the Map tab, or jump focus to it. */
  onViewSpecMap?: () => void;
}

export function Library({
  panel,
  onOpenArtifact,
  onShowNotes,
  onNewFile,
  onNewArtifact,
  onNewFolder,
  onTreeLoaded,
  reveal,
  onViewSpecMap,
}: LibraryProps) {
  const { expanded, toggleExpanded, expandAll, typeFilter, text } = panel;
  const [tree, setTree] = useState<TreeNode | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Selection is session-local and deliberately not part of the persisted panel
  // state (LIB-FR-14), so it starts empty on every open of a project.
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [ctx, setCtx] = useState<ContextMenuState | null>(null);
  // The panel's session-local copy clipboard (LCM-FR-03): at most one copied
  // node path, set by **Copy** and consumed by folder **Paste**. Copy-only — a
  // Paste does not clear it, so the same source can be pasted into several
  // folders.
  const [clipboard, setClipboard] = useState<string | null>(null);
  // LCM-FR-11: the folder whose recursive-delete confirmation is open, or null.
  // Set only when the backend reported the folder non-empty, so the window
  // appears for exactly the case that cannot be undone.
  const [confirmRecursive, setConfirmRecursive] = useState<TreeNode | null>(null);

  // `onTreeLoaded` is read through a ref so `loadTree` keeps a stable identity:
  // it is the dependency of the mount effect and of the event subscription, and a
  // new identity on each render would re-subscribe the watcher listener every
  // time the shell re-rendered.
  const publishTree = useRef(onTreeLoaded);
  publishTree.current = onTreeLoaded;

  const loadTree = useCallback(async () => {
    try {
      const next = await api.loadProjectTree();
      setTree(next);
      publishTree.current?.(next);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  // Initial load. The panel-state restore is issued independently, when `VPanel`
  // mounts — not chained behind this — so the first painted tree is already in
  // its restored shape rather than flashing the default and then re-expanding
  // (LIB non-functional requirement).
  useEffect(() => {
    void loadTree();
  }, [loadTree]);

  // LIB-FR-10: reload on the debounced backend `"project tree changed"` event.
  // Expand/collapse and selection are preserved because neither is derived from
  // the tree payload — selection is local state and the expanded set lives in
  // the panel state above this component (LIB-FR-06).
  useEffect(() => {
    // Capture the unlisten fn synchronously via a cancel flag so an
    // unmount/remount (React Strict Mode double-invoke, HMR) cannot leave two
    // subscribers registered and double-reload.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onProjectTreeChanged(() => {
      void loadTree();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [loadTree]);

  const revealId = reveal?.id ?? null;
  const revealNonce = reveal?.nonce;

  /**
   * LIB-FR-18: expand every ancestor of the revealed node, make it the
   * selection, and relax whichever local filter would hide it.
   *
   * **Whether the node has to exist first depends on who asked.** A creation
   * reveal is optimistic — the file was written a moment ago and arrives with
   * the next watcher-driven reload — so it selects immediately and the row
   * highlights when it renders. A **tab-follow** is not: SNV-FR-67 requires a
   * target the tree does not hold to change nothing at all, and selecting it
   * anyway would clear whatever the author had selected and, worse, `expandAll`
   * would mark the panel state touched and *persist* an expansion of ancestors
   * that were never revealed — re-opening a folder the author had deliberately
   * collapsed, and writing that to disk.
   *
   * Keyed on the NONCE, and on the shared consumed-mark rather than a local ref:
   * `reveal` is a standing prop, and `VPanel` unmounts this component on every
   * surface switch, so a component-local guard would let a finished reveal
   * re-apply the moment the author came back to this panel (SNV-FR-68).
   */
  useEffect(() => {
    if (!revealId || revealNonce === undefined) return;
    if (revealAlreadyConsumed(revealNonce)) return;
    const optimistic = reveal?.optimistic ?? false;

    // A tree that has not loaded yet says nothing about whether the node exists,
    // so a follow waits for it rather than giving up. Consuming here instead
    // would kill every reveal issued before the first tree lands — which is most
    // of them, the panel having only just mounted.
    if (!optimistic && !tree) return;

    const node = tree ? findNode(tree, revealId) : null;
    if (!node && !optimistic) {
      // SNV-FR-67: the tree is loaded and does not hold it. Spend the request
      // without touching the selection, the expansion, or the filters — and
      // without `expandAll`, which would mark the panel state touched and
      // *persist* an expansion of ancestors that were never revealed.
      markRevealConsumed(revealNonce);
      return;
    }
    // An optimistic request whose node has not arrived stays unconsumed, so the
    // reload carrying it still relaxes the filters that would hide it.
    if (node) {
      markRevealConsumed(revealNonce);
      relaxFiltersFor(node);
    }

    setSelectedId(revealId);
    const segs = revealId.split("/");
    const ancestors: string[] = [];
    for (let i = 1; i < segs.length; i++) {
      ancestors.push(segs.slice(0, i).join("/"));
    }
    expandAll(ancestors);
    // `expandAll` and `relaxFiltersFor` close over the live filters and are
    // re-created each render; depending on them would re-run the reveal on every
    // render, so this keys on the nonce and the tree it needs to resolve against.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revealNonce, revealId, tree, reveal?.optimistic]);

  const toggleFolder = (node: TreeNode) => {
    setSelectedId(node.id);
    toggleExpanded(node.id);
  };

  const openFile = (node: TreeNode) => {
    setSelectedId(node.id);
    onOpenArtifact({
      id: node.id,
      name: node.name,
      artifactType: node.artifactType,
      chip: node.artifactType ? typeChip(node.artifactType) : undefined,
    });
  };

  // LIB-FR-11: manual rescan affordance.
  const rescan = async () => {
    try {
      const next = await api.rescanProjectTree();
      setTree(next);
      // Published like any other load: a rescan is exactly the case where the
      // folder set may have changed under a watcher that missed it (NFW-FR-04).
      publishTree.current?.(next);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  // LCM-FR-04: assign / clear type. Both return the freshly-scanned tree so the
  // affected tags update without a separate manual reload.
  const assignType = async (node: TreeNode, type: ArtifactType) => {
    setCtx(null);
    const scope: AssignScope = node.nodeKind === "folder" ? "folder" : "file";
    try {
      const next = await api.assignArtifactType(node.path, type, scope);
      setTree(next);
      publishTree.current?.(next);
    } catch (e) {
      setError(String(e));
    }
  };

  const clearType = async (node: TreeNode) => {
    setCtx(null);
    try {
      const next = await api.clearArtifactType(node.path);
      setTree(next);
      publishTree.current?.(next);
    } catch (e) {
      setError(String(e));
    }
  };

  /**
   * LCM-FR-02 / LCM-FR-11: delete a file or folder, two-phase.
   *
   * The first call always goes out with `recursive = false`. A file, and a
   * folder that holds nothing, is removed by it and nothing further happens. A
   * folder holding anything comes back as the typed `ERR_NOT_EMPTY` having
   * removed nothing, and that error — not any inspection of the rendered tree —
   * is what raises the recursive-delete confirmation.
   *
   * Asking the backend rather than counting `node.children` is the whole point:
   * the tree deliberately hides gitignored and excluded content (ASC-FR-09), so
   * a folder can render as empty while holding a large subtree. Deciding from
   * the tree would skip the prompt in exactly the case the prompt exists for.
   *
   * The tree reflects a completed removal via the watcher-driven
   * `"project tree changed"` reload (LIB-FR-10).
   */
  const deleteNode = async (node: TreeNode) => {
    setCtx(null);
    try {
      await api.deletePath(node.path, false);
      setError(null);
    } catch (e) {
      if (String(e).includes(api.ERR_NOT_EMPTY)) {
        setConfirmRecursive(node);
        return;
      }
      setError(String(e));
    }
  };

  // LCM-FR-11: the user consented to taking the subtree, so the same operation
  // is re-invoked with `recursive` set. Dismissing instead invokes nothing
  // further and leaves the folder and its contents exactly as they were.
  const confirmRecursiveDelete = async (node: TreeNode) => {
    setConfirmRecursive(null);
    try {
      await api.deletePath(node.path, true);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  // LCM-FR-02: Copy places the node's path on the in-panel clipboard; it
  // performs no filesystem mutation on its own.
  const copyNode = (node: TreeNode) => {
    setCtx(null);
    setClipboard(node.path);
  };

  // LCM-FR-03: Paste duplicates the clipboard source into the right-clicked
  // folder. A backend collision error is surfaced inline and leaves the tree
  // unchanged.
  const pasteInto = async (folder: TreeNode) => {
    setCtx(null);
    if (clipboard == null) return;
    try {
      await api.copyPathIntoFolder(clipboard, folder.path);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  // LCM-FR-02: Rename the node to a new basename (validated client-side in the
  // menu before this runs). The reload is watcher-driven (LIB-FR-10).
  const renameNode = async (node: TreeNode, newName: string) => {
    setCtx(null);
    try {
      await api.renamePath(node.path, newName);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  // LCM-FR-05: jump to an artifact's Notes scope without opening a tab.
  const showNotes = (node: TreeNode) => {
    setCtx(null);
    onShowNotes({
      id: node.id,
      name: node.name,
      artifactType: node.artifactType,
      chip: node.artifactType ? typeChip(node.artifactType) : undefined,
    });
  };

  /**
   * Whether a node is itself eligible under the active artifact-type and text
   * filters, AND-combined (LIB-FR-05). The type lens has three regimes: "files"
   * imposes no type constraint; "artifacts" requires any resolved type; otherwise
   * the type must match exactly.
   *
   * A folder answers this on its own type — the folder-scope assignment it
   * carries (ASC-FR-18), absent unless the user gave it one — which is what makes
   * a *typed* folder visible under **All artifacts** and under its own type lens
   * from the moment it exists, with nothing inside it yet (LIB-FR-09).
   */
  const nodeMatches = (node: TreeNode): boolean =>
    matchesLens(typeFilter, node.artifactType) && matchesText(text, node.name);

  /**
   * LIB-FR-09: a folder is shown when its subtree holds at least one visible file
   * OR the folder is itself eligible. The first half is why the default **All
   * artifacts** lens hides folders whose subtree holds no supported artifacts; the
   * second is why an empty folder is surfaced rather than swallowed — one carrying
   * an artifact type under **All artifacts** and its own type lens, and one
   * carrying none under **All files**, which constrains nothing.
   */
  const subtreeMatches = (node: TreeNode): boolean => {
    if (node.nodeKind === "file") return nodeMatches(node);
    return nodeMatches(node) || (node.children ?? []).some(subtreeMatches);
  };

  const visibleChildren = (node: TreeNode): TreeNode[] =>
    (node.children ?? []).filter(subtreeMatches);

  /** Locate a node by id (its project-relative path, ASC-FR-13). */
  const findNode = (node: TreeNode, id: string): TreeNode | null => {
    if (node.id === id) return node;
    for (const child of node.children ?? []) {
      const hit = findNode(child, id);
      if (hit) return hit;
    }
    return null;
  };

  /**
   * `subtreeMatches` with one of the two filters neutralised, which is what
   * LIB-FR-18 needs in order to touch only the filters that would actually hide a
   * revealed node.
   *
   * `admittedByText` is exactly "would this node be visible with the lens on **All
   * files**" — because that lens constrains nothing, leaving the text filter as
   * the only test — and `admittedByLens` is "would it be visible with the text
   * filter cleared". So each of these answers whether relaxing the *other* filter
   * on its own is enough.
   */
  const admittedByLens = (node: TreeNode): boolean =>
    matchesLens(typeFilter, node.artifactType) ||
    (node.nodeKind === "folder" && (node.children ?? []).some(admittedByLens));

  const admittedByText = (node: TreeNode): boolean =>
    matchesText(text, node.name) ||
    (node.nodeKind === "folder" && (node.children ?? []).some(admittedByText));

  /**
   * LIB-FR-18: a reveal always ends with the node visible, so relax whichever
   * local filter is hiding it — and no more than that, leaving the panel's
   * filtering untouched when the active lens already admits the node. Relaxed
   * filters persist like any other filter change (LIB-FR-17), which falls out of
   * going through the panel's own setters.
   */
  const relaxFiltersFor = (node: TreeNode) => {
    if (subtreeMatches(node)) return;
    // Dropping the lens is preferred where it suffices: **All files** is the
    // panel's documented escape hatch (LIB-FR-09), while the text filter is
    // something the user typed and would have to type again.
    if (admittedByText(node)) panel.setTypeFilter("files");
    else if (admittedByLens(node)) panel.setText("");
    else {
      // Each filter hides the node on its own, so neither relaxation alone
      // reveals it.
      panel.setTypeFilter("files");
      panel.setText("");
    }
  };

  const renderNodes = (nodes: TreeNode[], depth: number): React.ReactNode[] =>
    nodes.flatMap((node) => {
      const isFolder = node.nodeKind === "folder";
      // LIB-FR-15: expanded iff recorded as such — an unknown folder, including
      // a newly-created one, renders collapsed.
      const open = isFolder && expanded.has(node.id);
      const rows: React.ReactNode[] = [
        <TreeRow
          key={node.id}
          node={node}
          depth={depth}
          open={open}
          selected={selectedId === node.id}
          revealed={revealId === node.id}
          onToggle={toggleFolder}
          onOpen={openFile}
          onContextMenu={(e, n) => setCtx({ x: e.clientX, y: e.clientY, node: n })}
        />,
      ];
      if (isFolder && open) {
        rows.push(...renderNodes(visibleChildren(node), depth + 1));
      }
      return rows;
    });

  const topLevel = tree ? visibleChildren(tree) : [];

  /**
   * LIB-FR-19: the lens offers the types the project has rather than the eight
   * it could have. Recomputed from every tree load (LIB-FR-10), so a type gains
   * its button the moment the project's first file of that type appears and
   * loses it once the author has moved off it.
   *
   * A folder's own folder-scope assignment counts alongside a file's resolved
   * type (ASC-FR-18) — a folder typed `scenario` with nothing in it yet is
   * exactly the case where the lens has somewhere to go.
   */
  const lensPositions = useMemo(() => {
    const types: (ArtifactType | undefined)[] = [];
    const walk = (node: TreeNode) => {
      types.push(node.artifactType);
      for (const child of node.children ?? []) walk(child);
    };
    if (tree) walk(tree);
    return typeLensPositions(presentTypes(types), typeFilter);
  }, [tree, typeFilter]);

  // The empty-state copy reflects the active lens so "All files" / a specific
  // type don't misreport an empty result as "No artifacts found."
  const emptyMessage =
    typeFilter === "files"
      ? "No files found."
      : typeFilter === "artifacts"
        ? "No artifacts found."
        : `No ${typeFilter} artifacts found.`;

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div className="panel-header">
        <span className="panel-header__title">Project</span>
        <div className="panel-header__actions">
          <button
            className="btn btn--ghost btn--icon btn--sm"
            title="Rescan project tree"
            aria-label="Rescan project tree"
            onClick={() => void rescan()}
          >
            <Icon.History size={12} />
          </button>
        </div>
      </div>
      {/* SNV-FR-58: the text filter first, directly beneath the panel header,
          and the lens beneath it and directly above the tree it narrows — the
          order every vertical panel uses. */}
      <div className="panel-controls">
        <div className="search-input" style={{ height: 24 }}>
          <Icon.Search size={12} />
          <input
            placeholder="Filter tree…"
            aria-label="Filter tree"
            value={text}
            onChange={(e) => panel.setText(e.target.value)}
          />
        </div>
        <SelectorRow
          label="Filter by type"
          positions={lensPositions}
          value={typeFilter}
          onChange={panel.setTypeFilter}
        />
      </div>
      <div
        className={
          // SNV-FR-61: with the tree narrowed to nothing, the message sits
          // where the tree would have been rather than in its top-left corner.
          // This panel makes no distinction between a narrowed tree and an
          // empty project — `emptyMessage` reflects the lens either way — so
          // the one treatment covers both.
          !error && tree && topLevel.length === 0
            ? FILTERED_BODY_CLASS
            : "vpanel__body"
        }
      >
        {error && (
          <div style={{ padding: "8px 12px", fontSize: "var(--fs-ui-sm)", color: "var(--fg-3)" }}>
            {error}
          </div>
        )}
        {!error && tree && topLevel.length === 0 && (
          <PanelFilteredState>{emptyMessage}</PanelFilteredState>
        )}
        {renderNodes(topLevel, 0)}
      </div>
      {/* LIB-FR-VMAQ: pinned below the tree, outside the scroll region, so it
          stays in view while the tree scrolls. */}
      {onViewSpecMap && (
        <div className="library__footer">
          <button type="button" className="btn btn--sm" onClick={onViewSpecMap}>
            <Icon.Layers size={12} /> View map
          </button>
        </div>
      )}
      {ctx && (
        <ContextMenu
          // Key on the node so right-clicking a different node remounts the menu
          // and resets its internal mode/submenu/draft state, rather than
          // retargeting an in-progress confirm/rename at the previous node.
          key={ctx.node.id}
          {...ctx}
          clipboard={clipboard}
          onClose={() => setCtx(null)}
          onNewFile={() => {
            const node = ctx.node;
            setCtx(null);
            // LCM-FR-10: hand the folder off as the modal's *starting* location —
            // a starting point rather than a commitment (NFI-FR-07).
            onNewFile?.(node);
          }}
          onNewArtifact={() => {
            const node = ctx.node;
            setCtx(null);
            // LCM-FR-08: hand the folder off to the modal as the fixed location.
            onNewArtifact(node);
          }}
          onNewFolder={() => {
            const node = ctx.node;
            setCtx(null);
            // LCM-FR-09: hand the folder off as the modal's *starting* parent —
            // a starting point rather than a commitment (NFW-FR-07).
            onNewFolder?.(node);
          }}
          onAssign={(type) => void assignType(ctx.node, type)}
          onClear={() => void clearType(ctx.node)}
          onDelete={() => void deleteNode(ctx.node)}
          onCopy={() => copyNode(ctx.node)}
          onPaste={() => void pasteInto(ctx.node)}
          onRename={(newName) => void renameNode(ctx.node, newName)}
          onNotes={() => showNotes(ctx.node)}
        />
      )}
      {/* LCM-FR-11: raised only once the backend has reported the folder
          non-empty, so it never stands between the user and an ordinary
          delete. */}
      {confirmRecursive && (
        <RecursiveDeleteConfirm
          node={confirmRecursive}
          onCancel={() => setConfirmRecursive(null)}
          onConfirm={() => void confirmRecursiveDelete(confirmRecursive)}
        />
      )}
    </div>
  );
}
