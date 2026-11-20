import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../api";
import { onProjectTreeChanged } from "../../events";
import {
  markRevealConsumed,
  revealAlreadyConsumed,
} from "../../state/panelReveal";
import { presentTypes, typeChip, typeLensPositions } from "../../artifactTypes";
import { logWarn } from "../../logging";
import type { LibraryPanel } from "../../hooks/useLibraryPanelState";
import {
  FILTERED_BODY_CLASS,
  PanelFilteredState,
} from "../PanelEmptyState";
import { Icon } from "../icons";
import { SelectorRow } from "../SelectorRow";
import { LoadingRow, TreeRow, TruncatedRow } from "./TreeRow";
import { ancestorsOf, filtersToRelax, foldersToLoad, lensOf, visibleRows } from "./treeModel";
import { useLazyTree } from "./useLazyTree";
import { useTreeFilter } from "./useTreeFilter";
import { VirtualRows } from "./VirtualRows";
import { noteOwnerDeleted } from "../../voice/voiceController";
import { runMainBranchWrite, WRITE_CANCELLED } from "../../state/mainBranchWrite";
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
  reveal,
  onViewSpecMap,
}: LibraryProps) {
  const { expanded, toggleExpanded, expandAll, typeFilter, text } = panel;
  // LIB-FR-EMOO: the folders this panel has loaded, one at a time.
  const tree = useLazyTree();
  const [opError, setError] = useState<string | null>(null);
  const error = opError ?? tree.error;
  // LIB-FR-PPAT: with text in the filter, the backend answers it.
  const lens = lensOf(typeFilter);
  const filter = useTreeFilter(text, lens);
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

  /**
   * LIB-FR-19: the types the project holds, from `"list project artifacts"`.
   * Read on mount and on every tree change, so a type gains its button the
   * moment the project's first file of that type appears.
   */
  const [projectTypes, setProjectTypes] = useState<ArtifactType[]>([]);
  const loadProjectTypes = useCallback(() => {
    api
      .listProjectArtifacts()
      .then((nodes) => setProjectTypes(nodes.flatMap((n) => (n.artifactType ? [n.artifactType] : []))))
      .catch((e: unknown) =>
        logWarn(["frontend"], "project panel: artifact list failed", { reason: String(e) }),
      );
  }, []);
  useEffect(() => {
    loadProjectTypes();
  }, [loadProjectTypes]);

  // LIB-FR-10: on the debounced `"project tree changed"`, load again each
  // loaded folder whose answer changed, run the filter again, and read the
  // types again. Expand/collapse and selection are preserved because neither
  // is derived from the loaded folders (LIB-FR-06).
  const reloadFolders = tree.reloadFolders;
  const refreshFilter = filter.refresh;
  const loadedRef = useRef(tree.childrenOf);
  loadedRef.current = tree.childrenOf;
  useEffect(() => {
    // Capture the unlisten fn synchronously via a cancel flag so an
    // unmount/remount (React Strict Mode double-invoke, HMR) cannot leave two
    // subscribers registered and double-reload.
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onProjectTreeChanged((payload) => {
      reloadFolders(payload.changedFolders ?? [...loadedRef.current.keys()]);
      refreshFilter();
      loadProjectTypes();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [reloadFolders, refreshFilter, loadProjectTypes]);

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
  // LIB-FR-18: the folders on the way to a revealed node are loaded through
  // `"list tree path"` once per request, so a node in a folder this panel has
  // not opened can be revealed. `pathAsked` holds the nonce asked for, and
  // `pathMissing` the nonce the index answered "not held" for.
  const pathAsked = useRef<number | null>(null);
  const [pathMissing, setPathMissing] = useState<number | null>(null);
  const loadPathTo = tree.loadPathTo;
  useEffect(() => {
    if (!revealId || revealNonce === undefined) return;
    if (revealAlreadyConsumed(revealNonce)) return;
    if (tree.nodeById.has(revealId) || pathAsked.current === revealNonce) return;
    pathAsked.current = revealNonce;
    loadPathTo(revealId)
      .then((found) => {
        if (!found) setPathMissing(revealNonce);
      })
      .catch(() => setPathMissing(revealNonce));
  }, [revealId, revealNonce, tree.nodeById, loadPathTo]);

  useEffect(() => {
    if (!revealId || revealNonce === undefined) return;
    if (revealAlreadyConsumed(revealNonce)) return;
    const optimistic = reveal?.optimistic ?? false;
    const node = tree.nodeById.get(revealId) ?? null;

    if (!node && !optimistic) {
      // A tree that has not answered yet says nothing about whether the node
      // exists, so a follow waits for the path read rather than giving up.
      if (pathMissing !== revealNonce) return;
      // SNV-FR-67: the index does not hold it. Spend the request without
      // touching the selection, the expansion, or the filters — and without
      // `expandAll`, which would mark the panel state touched and *persist*
      // an expansion of ancestors that were never revealed.
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
    expandAll(ancestorsOf(revealId));
    // `expandAll` and `relaxFiltersFor` close over the live filters and are
    // re-created each render; depending on them would re-run the reveal on every
    // render, so this keys on the nonce and the nodes it needs to resolve against.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revealNonce, revealId, tree.nodeById, pathMissing, reveal?.optimistic]);

  // Stable identities, so a memoised row does no work when the panel renders
  // for a reason that is not its own (LIB-FR-BXMZ). The latest callbacks are
  // read through a ref.
  const latest = useRef({ toggleExpanded, onOpenArtifact });
  latest.current = { toggleExpanded, onOpenArtifact };
  const toggleFolder = useCallback((node: TreeNode) => {
    setSelectedId(node.id);
    latest.current.toggleExpanded(node.id);
  }, []);

  const openFile = useCallback((node: TreeNode) => {
    setSelectedId(node.id);
    latest.current.onOpenArtifact({
      id: node.id,
      name: node.name,
      artifactType: node.artifactType,
      chip: node.artifactType ? typeChip(node.artifactType) : undefined,
    });
  }, []);

  const openContextMenu = useCallback((e: React.MouseEvent, node: TreeNode) => {
    setCtx({ x: e.clientX, y: e.clientY, node });
  }, []);

  // LIB-FR-11: manual rescan affordance. The expanded folders load again as
  // their rows render (LIB-FR-EMOO).
  const rescan = async () => {
    setError(null);
    await tree.rescan();
    filter.refresh();
    loadProjectTypes();
  };

  // LCM-FR-04: assign / clear type. Each returns the node, which re-tags at
  // once; the folders whose answer changed reload on `"project tree changed"`.
  const assignType = async (node: TreeNode, type: ArtifactType) => {
    setCtx(null);
    const scope: AssignScope = node.nodeKind === "folder" ? "folder" : "file";
    try {
      const next = await api.assignArtifactType(node.path, type, scope);
      tree.replaceNode(next);
    } catch (e) {
      setError(String(e));
    }
  };

  const clearType = async (node: TreeNode) => {
    setCtx(null);
    try {
      const next = await api.clearArtifactType(node.path);
      tree.replaceNode(next);
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
      // MBW-FR-DBGA: a write to a main branch asks for the confirmation first.
      const done = await runMainBranchWrite("delete_artifact", () =>
        api.deletePath(node.path, false),
      );
      if (done === WRITE_CANCELLED) return;
      setError(null);
      // CVP-FR-51, VOM-FR-LSBQ: voice mode ends with the file that owns it.
      noteOwnerDeleted({ kind: "artifact", artifactId: node.path });
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
      const done = await runMainBranchWrite("delete_artifact", () =>
        api.deletePath(node.path, true),
      );
      if (done === WRITE_CANCELLED) return;
      setError(null);
      // LCM-FR-11, VOM-FR-LSBQ: and with a folder that holds that file.
      noteOwnerDeleted({ kind: "artifact", artifactId: node.path });
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
      const done = await runMainBranchWrite("paste_artifact", () =>
        api.copyPathIntoFolder(clipboard, folder.path),
      );
      if (done === WRITE_CANCELLED) return;
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
      const done = await runMainBranchWrite("rename_item", () =>
        api.renamePath(node.path, newName),
      );
      if (done === WRITE_CANCELLED) return;
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
   * LIB-FR-18: a reveal always ends with the node visible, so relax whichever
   * local filter is hiding it — and no more than that, leaving the panel's
   * filtering untouched when the active filters already admit the node
   * (`filtersToRelax`). Relaxed filters persist like any other filter change
   * (LIB-FR-17).
   */
  const relaxFiltersFor = (node: TreeNode) => {
    const relax = filtersToRelax(node, typeFilter, text, filter.result);
    if (relax.lens) panel.setTypeFilter("files");
    if (relax.text) panel.setText("");
  };

  // LIB-FR-PPAT: with text in the filter and no answer yet, the tree shows a
  // loading row, not the unfiltered tree, so it never flashes rows the filter
  // is about to hide.
  const filterPending = text.trim() !== "" && filter.result === null;

  // LIB-FR-BXMZ: the rows, flat, in tree order. Only those in view render.
  const rows = useMemo(
    () =>
      filterPending
        ? []
        : visibleRows({
            root: tree.root,
            childrenOf: tree.childrenOf,
            expanded,
            typeFilter,
            filtered: filter.result,
          }),
    [filterPending, tree.root, tree.childrenOf, expanded, typeFilter, filter.result],
  );

  // LIB-FR-EMOO: an expanded folder on a row loads its children.
  const ensureLoaded = tree.ensureLoaded;
  useEffect(() => {
    if (filter.result) return;
    const missing = foldersToLoad(rows);
    if (missing.length > 0) ensureLoaded(missing);
  }, [rows, filter.result, ensureLoaded]);

  const scrollRef = useRef<HTMLDivElement>(null);
  const revealIndex = useMemo(
    () =>
      revealId ? rows.findIndex((r) => r.kind === "node" && r.node.id === revealId) : -1,
    [rows, revealId],
  );
  const scrollTo = useMemo(
    () => (revealNonce === undefined ? null : { index: revealIndex, nonce: revealNonce }),
    [revealIndex, revealNonce],
  );
  const nodeRows = rows.filter((r) => r.kind === "node").length;
  const treeReady = tree.root !== null && (text.trim() === "" || filter.result !== null);

  /**
   * LIB-FR-19: the lens offers the types the project has rather than the ten
   * it could have. A folder's own folder-scope assignment counts alongside a
   * file's resolved type (ASC-FR-18).
   */
  const lensPositions = useMemo(
    () => typeLensPositions(presentTypes(projectTypes), typeFilter),
    [projectTypes, typeFilter],
  );

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
          !error && treeReady && nodeRows === 0
            ? FILTERED_BODY_CLASS
            : "vpanel__body"
        }
        ref={scrollRef}
      >
        {error && (
          <div style={{ padding: "8px 12px", fontSize: "var(--fs-ui-sm)", color: "var(--fg-3)" }}>
            {error}
          </div>
        )}
        {!error && treeReady && nodeRows === 0 && (
          <PanelFilteredState>{emptyMessage}</PanelFilteredState>
        )}
        {!error && (tree.root === null || filterPending) && <LoadingRow depth={0} />}
        <VirtualRows
          rows={rows}
          scrollRef={scrollRef}
          scrollTo={scrollTo}
          renderRow={(row) => {
            if (row.kind === "loading") return <LoadingRow key={row.key} depth={row.depth} />;
            if (row.kind === "truncated") return <TruncatedRow key="truncated" />;
            const { node, depth, open } = row;
            return (
              <TreeRow
                key={node.id}
                node={node}
                depth={depth}
                open={open}
                selected={selectedId === node.id}
                revealed={revealId === node.id}
                onToggle={toggleFolder}
                onOpen={openFile}
                onContextMenu={openContextMenu}
              />
            );
          }}
        />
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
