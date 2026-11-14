/**
 * The map session of the Map tab (`SMP-specification-map.md` SMP-FR-QNUH).
 *
 * The viewport renders only the active tab, so the Map tab unmounts on every
 * switch. What the author built up — the active lens, the loaded index, the
 * selection, the view, the focus, the toggles, and the state of the Delivery
 * lens — lives here, in a store the shell owns, and outlives every mount.
 *
 * Never written to disk. A project or worktree switch clears it (SMP-FR-MEZK),
 * and closing the tab resets only the views (SMP-FR-BXAP).
 */
import { specMapOps, type SpecMapOps } from "../../api/specMap";
import { logError, logInfo, logWarn } from "../../logging";
import type { OpenSpecFailure } from "../../types";
import type { DeliverySource } from "../../types/delivery";
import { hasNotice, toOpenSpecFailure } from "../openspec/failureNotice";
import { buildTree, refExists, type TreeIndex } from "./tree";
import { isOpenSpecIndex, type MapRef, type SpecificationIndex } from "./types";

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
  /** SMI-FR-QCUC: show the last level, whatever the depth of the node. */
  lastLevel?: boolean;
}

/** SMP-FR-TKUS: the two lenses of the Map tab. */
export type MapLens = "delivery" | "specifications";

/**
 * SMP-FR-QNUH / SMP-FR-RNLH: what the Delivery lens (`MDL-map-delivery-lens.md`)
 * keeps in the session. The lens owns both shapes. `data` holds the last
 * delivery plan answer and the selection, and stays when the tab closes.
 * `view` holds the pan and the zoom, and a tab close resets it to null
 * (SMP-FR-BXAP).
 */
export interface DeliveryLensSlot {
  data: unknown;
  view: unknown;
}

const EMPTY_DELIVERY_SLOT: DeliveryLensSlot = Object.freeze({ data: null, view: null });

/**
 * SMP-FR-KBXW: the code a load failed with. The backend rejects with an
 * `OpenSpecFailure` (OFP-FR-QLQJ); a string code, or `code: detail`, reads
 * as its code.
 */
export function loadFailureCode(error: unknown): string {
  return toOpenSpecFailure(error).code;
}

export type LoadStatus = "idle" | "loading" | "ready" | "error";

export interface SpecMapState {
  /** SMP-FR-TKUS: the active lens, or null until the default lens is known. */
  lens: MapLens | null;
  /** SMP-FR-TKUS / SDL-FR-CKAA: the last `"get delivery source"` answer, or null. */
  deliverySource: DeliverySource | null;
  /** SMP-FR-RNLH: the state of the Delivery lens. */
  delivery: DeliveryLensSlot;
  status: LoadStatus;
  /** SMP-FR-KBXW: the code of the load that failed, or null. */
  errorCode: string | null;
  /** SMP-FR-KBXW: the OpenSpec failure of that load, for its notice, or null. */
  errorFailure: OpenSpecFailure | null;
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
  /** SDL-FR-KXCY: the **unplanned** toggle. */
  unplanned: boolean;
  /**
   * MDL-FR-JIKK / SDL-FR-MCGI: a delivery item the Delivery lens selects,
   * reveals, and centres, consumed by the lens.
   */
  deliveryReveal: { itemId: string; nonce: number } | null;
  /** SDL-FR-CFHH: a task the Delivery lens adds, consumed by the lens. */
  planWork: { name: string; references: string[]; nonce: number } | null;
}

function initialState(): SpecMapState {
  return {
    lens: null,
    deliverySource: null,
    delivery: EMPTY_DELIVERY_SLOT,
    status: "idle",
    errorCode: null,
    errorFailure: null,
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
    unplanned: false,
    deliveryReveal: null,
    planWork: null,
  };
}

/** Two answers of `"get delivery source"` that resolve the same source. */
function sameSource(a: DeliverySource | null, b: DeliverySource | null): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return JSON.stringify(a) === JSON.stringify(b);
}

export class SpecMapSessionStore {
  private state: SpecMapState = initialState();
  private listeners = new Set<() => void>();
  private version = 0;
  private nonce = 0;
  /** Bumped by `clear`, so a load that outlives its session lands nowhere. */
  private generation = 0;
  private treeCache: TreeIndex | null = null;
  /** SMP-FR-TKUS: true while the default lens is asked for. */
  private decidingLens = false;
  /** MDL-FR-YTAP: a source read in flight, and whether another is due after it. */
  private resolving: Promise<void> | null = null;
  private resolveAgain = false;
  /**
   * SMP-FR-SWUL: whether the Delivery lens holds unsaved Flow changes. It has
   * listeners of its own, so the shell re-renders for this flag and not for
   * each pan or selection of the map.
   */
  private deliveryDirtyFlag = false;
  private dirtyListeners = new Set<() => void>();
  private deliverySaveHandler: (() => Promise<void>) | null = null;

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

  /**
   * SMP-FR-TKUS: on the first render of a map session, ask for the delivery
   * source and show Delivery where it is not `none`. A lens the author chose
   * while the answer was in flight stays. A failed answer shows
   * Specifications.
   */
  decideLens = async (): Promise<void> => {
    if (this.state.lens !== null || this.decidingLens) return;
    this.decidingLens = true;
    const generation = this.generation;
    try {
      const source = await this.ops.getDeliverySource();
      if (generation !== this.generation) return;
      const lens: MapLens = source.kind === "none" ? "specifications" : "delivery";
      logInfo(["frontend"], "map tab chose its default lens", {
        sourceKind: source.kind,
        lens: this.state.lens ?? lens,
      });
      this.set({ deliverySource: source, lens: this.state.lens ?? lens });
    } catch (e) {
      if (generation !== this.generation) return;
      logWarn(["frontend"], "the delivery source of the map could not be read", {
        reason: e instanceof Error ? e.name : typeof e,
      });
      if (this.state.lens === null) this.set({ lens: "specifications" });
    } finally {
      if (generation === this.generation) this.decidingLens = false;
    }
  };

  /** SMP-FR-TKUS / SMP-FR-RNLH: switch the lens. The other lens keeps its state. */
  setLens = (lens: MapLens): void => {
    if (this.state.lens === lens) return;
    logInfo(["frontend"], "map tab switched its lens", { lens });
    this.set({ lens });
  };

  /**
   * SMP-FR-RNLH: the data of the Delivery lens, made on first use. Making it
   * is no change a reader sees, so it notifies nobody and is safe in a render.
   */
  ensureDeliveryData = <T>(is: (data: unknown) => data is T, make: () => T): T => {
    const data = this.state.delivery.data;
    if (is(data)) return data;
    const made = make();
    this.state = { ...this.state, delivery: { ...this.state.delivery, data: made } };
    return made;
  };

  /**
   * SMP-FR-RNLH: keep the view of the Delivery lens's canvas for its next
   * mount. Only that mount reads it, so a pan notifies nobody.
   */
  keepDeliveryView = (view: unknown): void => {
    this.state = { ...this.state, delivery: { ...this.state.delivery, view } };
  };

  /** SMP-FR-RNLH: replace a part of the state of the Delivery lens. */
  setDeliveryState = (patch: Partial<DeliveryLensSlot>): void => {
    this.set({ delivery: { ...this.state.delivery, ...patch } });
  };

  /** SDL-FR-CKAA: record a later `"get delivery source"` answer. The lens does not change. */
  setDeliverySource = (source: DeliverySource | null): void => {
    if (sameSource(this.state.deliverySource, source)) return;
    this.set({ deliverySource: source });
  };

  /**
   * MDL-FR-YTAP / LIB-FR-JTXR: read the delivery source again and record it.
   * A call while a read is in flight makes one more read after it, so the
   * newest answer is the one kept. A failed read keeps the last answer.
   */
  resolveDeliverySource = (): Promise<void> => {
    if (this.resolving) {
      this.resolveAgain = true;
      return this.resolving;
    }
    const generation = this.generation;
    const run = async (): Promise<void> => {
      do {
        this.resolveAgain = false;
        try {
          const source = await this.ops.getDeliverySource();
          if (generation !== this.generation) return;
          if (source && typeof source === "object" && "kind" in source) this.setDeliverySource(source);
        } catch (e) {
          if (generation !== this.generation) return;
          logWarn(["frontend"], "the delivery source could not be read again", {
            reason: e instanceof Error ? e.name : typeof e,
          });
        }
      } while (this.resolveAgain && generation === this.generation);
    };
    const pending = run().finally(() => {
      if (this.resolving === pending) this.resolving = null;
    });
    this.resolving = pending;
    return pending;
  };

  /**
   * MDL-FR-JIKK / DSH-FR-DFDW / NAW-FR-PHGO / SDL-FR-MCGI: show the Delivery
   * lens, with an item to select, reveal, and centre, or with none for null.
   */
  showDeliveryItem = (itemId: string | null): void => {
    this.set({
      lens: "delivery",
      ...(itemId !== null ? { deliveryReveal: { itemId, nonce: ++this.nonce } } : {}),
    });
  };

  consumeDeliveryReveal = (nonce: number): void => {
    if (this.state.deliveryReveal?.nonce === nonce) this.set({ deliveryReveal: null });
  };

  /** SDL-FR-CFHH: ask the Delivery lens to add a task, and show that lens. */
  requestPlanWork = (name: string, references: readonly string[]): void => {
    this.set({ lens: "delivery", planWork: { name, references: [...references], nonce: ++this.nonce } });
  };

  consumePlanWork = (nonce: number): void => {
    if (this.state.planWork?.nonce === nonce) this.set({ planWork: null });
  };

  /** SDL-FR-VUQH: turning **unplanned** on turns **gaps only** off. */
  toggleUnplanned = (): void => {
    const on = !this.state.unplanned;
    this.set({ unplanned: on, ...(on ? { gapsOnly: false } : {}) });
  };

  subscribeDeliveryDirty = (listener: () => void): (() => void) => {
    this.dirtyListeners.add(listener);
    return () => {
      this.dirtyListeners.delete(listener);
    };
  };

  /** SMP-FR-SWUL: whether the Map tab shows its dirty indicator. */
  deliveryDirty = (): boolean => this.deliveryDirtyFlag;

  /** SMP-FR-SWUL: the Delivery lens sets this while it holds unsaved Flow changes. */
  setDeliveryDirty = (dirty: boolean): void => {
    if (this.deliveryDirtyFlag === dirty) return;
    this.deliveryDirtyFlag = dirty;
    for (const listener of [...this.dirtyListeners]) listener();
  };

  /**
   * SMP-FR-SWUL: the Delivery lens registers the write of its unsaved Flow
   * changes (FLO-FR-27), and null when it holds none. File → Save calls
   * `saveDelivery` while the Map tab is the active tab.
   */
  setDeliverySave = (handler: (() => Promise<void>) | null): void => {
    this.deliverySaveHandler = handler;
  };

  /** SMP-FR-SWUL: write the unsaved changes of the Delivery lens, if any. */
  saveDelivery = async (): Promise<void> => {
    if (!this.deliveryDirtyFlag || !this.deliverySaveHandler) return;
    await this.deliverySaveHandler();
  };

  /**
   * SMP-FR-PMCX: load the index once; a session that holds one loads nothing.
   * SMP-FR-RNLH / SMP-FR-ONSD: a failed load stays failed, through a lens
   * switch or a remount, until Retry (`retry`).
   */
  load = async (): Promise<void> => {
    if (this.state.status !== "idle") return;
    const generation = this.generation;
    this.set({ status: "loading" });
    try {
      const index = await this.ops.load();
      if (generation !== this.generation) return;
      this.set({ status: "ready", errorCode: null, errorFailure: null, index });
      const tree = this.tree()!;
      // SMI-FR-WDAB: a selection the new index holds no node of clears.
      const selected = this.state.selection;
      if (selected && !refExists(tree, selected)) this.set({ selection: null });
      logInfo(["frontend"], "specification map loaded", {
        source: index.source ?? "demo",
        roots: index.roots.length,
        specs: tree.specs.size,
        changes: tree.changes.size,
        dependencies: index.dependencies.length,
      });
      const pending = this.state.pendingPath;
      if (pending !== null) {
        this.set({ pendingPath: null });
        this.selectByPath(pending);
      }
    } catch (e) {
      if (generation !== this.generation) return;
      // SMP-FR-ONSD / SMP-FR-KBXW: the error state names a code it routes on,
      // and the log holds the code alone.
      const failure = toOpenSpecFailure(e);
      const code = failure.code;
      logError(["frontend"], "specification map could not be loaded", {
        code,
        cause: failure.cause ?? "",
      });
      this.set({
        status: "error",
        errorCode: code,
        errorFailure: hasNotice(failure) ? failure : null,
      });
    }
  };

  /** SMP-FR-ONSD: the Retry button. */
  retry = (): Promise<void> => {
    if (this.state.status !== "error") return Promise.resolve();
    this.set({ status: "idle" });
    return this.load();
  };

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
    if (isOpenSpecIndex(tree.index)) {
      // SMI-FR-QCUC: every requirement of a capability has its spec file as
      // its path, so the file names the capability node.
      const holder = tree.holderByPath.get(path);
      if (!holder) return;
      const ref: MapRef = { kind: "index", id: holder };
      this.set({ selection: ref, zoom: { ref, nonce: ++this.nonce, lastLevel: true } });
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

  /** SDL-FR-VUQH: turning **gaps only** on turns **unplanned** off. */
  toggleGaps = (): void => {
    const on = !this.state.gapsOnly;
    this.set({ gapsOnly: on, ...(on ? { unplanned: false } : {}) });
  };

  setInspectorOpen = (open: boolean): void => {
    this.set({ inspectorOpen: open });
  };

  /**
   * SMP-FR-BXAP: closing the tab resets the view of each lens and the focus,
   * and nothing else.
   */
  resetView = (): void => {
    this.set({
      view: INITIAL_VIEW,
      focusId: null,
      zoom: null,
      delivery: { ...this.state.delivery, view: null },
    });
  };

  /** SMP-FR-MEZK: a project or worktree switch discards the whole session. */
  clear = (): void => {
    this.generation += 1;
    this.treeCache = null;
    this.decidingLens = false;
    this.resolving = null;
    this.resolveAgain = false;
    this.deliverySaveHandler = null;
    this.state = initialState();
    this.version += 1;
    for (const listener of [...this.listeners]) listener();
    this.setDeliveryDirty(false);
  };
}
