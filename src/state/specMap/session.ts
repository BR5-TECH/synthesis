/**
 * The map session of the Map tab (`SMP-specification-map.md` SMP-FR-QNUH).
 *
 * The viewport renders only the active tab, so the Map tab unmounts on every
 * switch. What the author built up — the loaded index with its organization
 * edits and draft placements, the selection, the view, the focus, the toggles —
 * lives here, in a store the shell owns, and outlives every mount.
 *
 * Never written to disk. A project or worktree switch clears it (SMP-FR-MEZK),
 * and closing the tab resets only the view (SMP-FR-BXAP).
 */
import { specMapOps, type OrganizationEdit, type SpecMapOps } from "../../api/specMap";
import { logError, logInfo, logWarn } from "../../logging";
import { applyEdit, deleteOutcome } from "./organize";
import { buildTree, refExists, type TreeIndex } from "./tree";
import type { MapEdit, MapRef, SpecificationIndex } from "./types";

export interface MapView {
  level: number;
  scale: number;
  tx: number;
  ty: number;
  /** SMZ-FR-NUOB: false until the author pans or zooms. */
  touched: boolean;
}

export const INITIAL_VIEW: MapView = Object.freeze({
  level: 0,
  scale: 0.8,
  tx: 0,
  ty: 0,
  touched: false,
});

/** A request to show a node at its own level and centre it, consumed by the canvas. */
export interface ZoomRequest {
  ref: MapRef;
  nonce: number;
}

export type LoadStatus = "idle" | "loading" | "ready" | "error";

export interface SpecMapState {
  status: LoadStatus;
  index: SpecificationIndex | null;
  view: MapView;
  focusId: string | null;
  selection: MapRef | null;
  showDeps: boolean;
  gapsOnly: boolean;
  /** SMI-FR-QWOP: null until the author opens or closes it; the window width decides then. */
  inspectorOpen: boolean | null;
  /** SMI-FR-PRSL: a spec path that arrived before the index loaded. */
  pendingPath: string | null;
  zoom: ZoomRequest | null;
}

function initialState(): SpecMapState {
  return {
    status: "idle",
    index: null,
    view: INITIAL_VIEW,
    focusId: null,
    selection: null,
    // SME-FR-OKUH / SMN-FR-HLDQ: the two defaults.
    showDeps: true,
    gapsOnly: false,
    inspectorOpen: null,
    pendingPath: null,
    zoom: null,
  };
}

export class SpecMapSessionStore {
  private state: SpecMapState = initialState();
  private listeners = new Set<() => void>();
  private version = 0;
  private nonce = 0;
  /** Bumped by `clear`, so a load that outlives its session lands nowhere. */
  private generation = 0;
  private treeCache: TreeIndex | null = null;

  constructor(private readonly ops: SpecMapOps = specMapOps) {}

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  getVersion = (): number => this.version;

  snapshot = (): SpecMapState => this.state;

  /** The lookups over the current index, or null before it loads. */
  tree = (): TreeIndex | null => {
    const { index } = this.state;
    if (!index) return null;
    if (this.treeCache?.index !== index) this.treeCache = buildTree(index);
    return this.treeCache;
  };

  private set(patch: Partial<SpecMapState>): void {
    this.state = { ...this.state, ...patch };
    this.version += 1;
    for (const listener of [...this.listeners]) listener();
  }

  /** SMP-FR-PMCX: load the index once; a session that holds one loads nothing. */
  load = async (): Promise<void> => {
    if (this.state.status === "loading" || this.state.status === "ready") return;
    const generation = this.generation;
    this.set({ status: "loading" });
    try {
      const index = await this.ops.load();
      if (generation !== this.generation) return;
      this.set({ status: "ready", index });
      const tree = this.tree()!;
      logInfo(["frontend"], "specification map loaded", {
        roots: index.roots.length,
        specs: tree.specs.size,
        dependencies: index.dependencies.length,
      });
      const pending = this.state.pendingPath;
      if (pending !== null) {
        this.set({ pendingPath: null });
        this.selectByPath(pending);
      }
    } catch (e) {
      if (generation !== this.generation) return;
      // SMP-FR-ONSD: the error state carries no detail of its own; the log does.
      logError(["frontend"], "specification map could not be loaded", {
        reason: e instanceof Error ? e.message : String(e),
      });
      this.set({ status: "error" });
    }
  };

  /** SMP-FR-ONSD: the Retry button. */
  retry = (): Promise<void> => {
    if (this.state.status !== "error") return Promise.resolve();
    this.set({ status: "idle" });
    return this.load();
  };

  /**
   * Apply one edit to the session, then hand it to its stub operation
   * (SMO-FR-CZLA, SMD-FR-OYLC). Returns false when the tree rules refuse it.
   */
  dispatch = (edit: MapEdit): boolean => {
    const { index } = this.state;
    const tree = this.tree();
    if (!index || !tree) return false;
    const siblingId =
      edit.kind === "delete" ? deleteOutcome(tree, edit.nodeId).siblingId : null;
    const next = applyEdit(index, edit);
    if (next === index) {
      logWarn(["frontend"], "specification map edit refused", { kind: edit.kind });
      return false;
    }
    let selection = this.state.selection;
    let zoom = this.state.zoom;
    let focusId = this.state.focusId;
    if (edit.kind === "create") {
      // SMO-FR-NXDL: the new node is selected and shown at its own level.
      selection = { kind: "index", id: edit.node.id };
      zoom = { ref: selection, nonce: ++this.nonce };
    }
    if (edit.kind === "delete" && siblingId) {
      // SMO-FR-OBRF: the selection moves to the sibling that took the children.
      selection = { kind: "index", id: siblingId };
    }
    const nextTree = buildTree(next);
    // SMI-FR-WDAB: a subject that left the tree leaves no selection behind.
    if (selection && !refExists(nextTree, selection)) selection = null;
    if (focusId && !nextTree.nodes.has(focusId)) focusId = null;
    this.set({ index: next, selection, zoom, focusId });
    this.persist(edit);
    return true;
  };

  private persist(edit: MapEdit): void {
    const report = (e: unknown) =>
      logWarn(["frontend"], "specification map edit could not be saved", {
        kind: edit.kind,
        reason: e instanceof Error ? e.message : String(e),
      });
    if (edit.kind === "attachDraft") {
      void this.ops
        .attachDraft({ draftId: edit.draft.draftId, nodeId: edit.nodeId })
        .catch(report);
      return;
    }
    if (edit.kind === "move" && edit.ref.kind === "planned") {
      // SMD-FR-QPAM: a planned chip's move is a new placement.
      void this.ops
        .attachDraft({ draftId: edit.ref.draftId, nodeId: edit.parentId! })
        .catch(report);
      return;
    }
    const organization = toOrganizationEdit(edit);
    if (organization) void this.ops.saveOrganization(organization).catch(report);
  }

  select = (ref: MapRef | null): void => {
    this.set({ selection: ref });
  };

  /**
   * SMI-FR-PRSL: select the spec node a project path names, show it at the last
   * level, and centre it. A path no spec node holds changes nothing; a path
   * that arrives before the index loads applies once it loads.
   */
  selectByPath = (path: string): void => {
    const tree = this.tree();
    if (!tree) {
      this.set({ pendingPath: path });
      return;
    }
    const code = tree.specByPath.get(path);
    if (!code) return;
    const ref: MapRef = { kind: "spec", code };
    this.set({ selection: ref, zoom: { ref, nonce: ++this.nonce } });
  };

  /** SMI-FR-RPCO: show a node at the level it renders at and centre it. */
  zoomTo = (ref: MapRef): void => {
    this.set({ zoom: { ref, nonce: ++this.nonce } });
  };

  consumeZoom = (nonce: number): void => {
    if (this.state.zoom?.nonce === nonce) this.set({ zoom: null });
  };

  setView = (view: MapView): void => {
    this.set({ view });
  };

  /** SMZ-FR-FOXU / SMZ-FR-LWEQ: focus a subtree, or end the focus with null. */
  setFocus = (focusId: string | null): void => {
    this.set({ focusId, view: INITIAL_VIEW });
  };

  toggleDeps = (): void => {
    this.set({ showDeps: !this.state.showDeps });
  };

  toggleGaps = (): void => {
    this.set({ gapsOnly: !this.state.gapsOnly });
  };

  setInspectorOpen = (open: boolean): void => {
    this.set({ inspectorOpen: open });
  };

  /** SMP-FR-BXAP: closing the tab resets the view and the focus, and nothing else. */
  resetView = (): void => {
    this.set({ view: INITIAL_VIEW, focusId: null, zoom: null });
  };

  /** SMP-FR-MEZK: a project or worktree switch discards the whole session. */
  clear = (): void => {
    this.generation += 1;
    this.treeCache = null;
    this.state = initialState();
    this.version += 1;
    for (const listener of [...this.listeners]) listener();
  };
}

/** The wire shape of an edit, for the edits `"save specification map organization"` takes. */
export function toOrganizationEdit(edit: MapEdit): OrganizationEdit | null {
  switch (edit.kind) {
    case "create":
      return { kind: "create", parentId: edit.parentId, node: edit.node };
    case "edit":
      return { kind: "edit", nodeId: edit.nodeId, label: edit.label, summary: edit.summary };
    case "delete":
      return { kind: "delete", nodeId: edit.nodeId };
    case "move":
      if (edit.ref.kind === "planned") return null;
      return {
        kind: "move",
        nodeId: edit.ref.kind === "spec" ? edit.ref.code : edit.ref.id,
        parentId: edit.parentId,
        index: edit.index,
      };
    default:
      return null;
  }
}
