import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";

import { useDiscussionControl } from "../../hooks/useDiscussionControl";
import { useProjectArtifacts } from "../../hooks/useProjectArtifacts";
import {
  absolutePosition,
  addItem,
  ancestorsOf,
  CARD_H,
  CARD_W,
  childrenOf,
  connect,
  connectRefusal,
  CONTAINER_HEADER_H,
  CONTAINER_PADDING,
  findItem,
  innermostContainerAt,
  isContainerKind,
  MIN_CONTAINER_SIZE,
  moveItem,
  parentOf,
  resizeItem,
  subtreeOf,
  type FlowDocument,
  type FlowEditResult,
  type FlowItemKind,
  type FlowPosition,
  type FlowSize,
} from "../../state/flowDocument";
import { IDLE_SYNC, type FlowSessionStore } from "../../state/flowSessions";
import { primeEditGate } from "../../state/mainBranchWrite";
import type { DiscussionTarget, OpenableArtifact } from "../../types";
import { createFrames } from "./frames";
import {
  DEFAULT_VIEW,
  edgePath,
  isInteractive,
  MAGNET_RADIUS,
  MAX_ZOOM,
  MIN_ZOOM,
  type Drag,
  type Pan,
  type Resize,
  type ResizeSides,
  type View,
} from "./geometry";
import { refusalMessage, type FlowAction, type RefusalMessage } from "./refusals";
import { useDeliveryState } from "./useDeliveryState";

/** FLO-FR-NRBV: what the shell answers to a request to create a task's draft. */
export type CreatedDraftOutcome =
  | { ok: true; draft: { id: string; name: string } }
  /** `error` is null where the author cancelled the creation. */
  | { ok: false; error: string | null };

export interface FlowCanvasProps {
  /** The open Flow's artifact id (ASC-FR-13). */
  flowId?: string;
  /** FLO-FR-30: the shell-owned store the plan and its dirty state live in. */
  flows: FlowSessionStore;
  /** FLO-FR-12: open a referenced artifact in its natural surface. */
  onOpenArtifact?: (item: OpenableArtifact) => void;
  /**
   * FLO-FR-NRBV: create a draft with the given name through
   * `"create draft (name)"`. Without it, the canvas calls the operation itself.
   */
  onCreateDraft?: (name: string) => Promise<CreatedDraftOutcome>;
  /** FLO-FR-NRBV: open a draft that was just created in a New Artifact tab. */
  onOpenDraft?: (draft: { id: string; name: string }) => void;
  /** FLO-FR-QZUR: open a linked draft through `"open draft (id)"`. */
  onOpenDraftById?: (draftId: string) => void;
}

/**
 * Grow each container around `id` to hold what it holds, so that a drop or a
 * placement near a border never leaves a member outside (FLO-FR-44).
 */
function growAncestors(doc: FlowDocument, id: string): FlowDocument {
  let next = doc;
  for (const a of ancestorsOf(doc, id)) {
    const item = findItem(next, a);
    if (item?.size) next = resizeItem(next, a, item.size);
  }
  return next;
}

/** Turn a result op into one that also grows the containers around `id`. */
function andGrow(id: string, result: FlowEditResult): FlowEditResult {
  return result.ok ? { ok: true, doc: growAncestors(result.doc, id) } : result;
}

/**
 * The view state, the gestures, and the selection of the canvas (FLO-FR-21,
 * FLO-FR-22, FLO-FR-32, FLO-FR-33, FLO-FR-42, FLO-FR-44, FLO-FR-ZASL). The
 * plan rules are the document module's; this hook calls them and turns a
 * refusal into feedback.
 */
export function useFlowCanvas({ flowId, flows }: FlowCanvasProps) {
  useSyncExternalStore(flows.subscribe, flows.getVersion);
  const project = useProjectArtifacts();
  // EDT-FR-UPNU: the guard of the file's branch is read when the Flow opens.
  useEffect(() => primeEditGate(), [flowId]);

  const session = flowId ? flows.get(flowId) : undefined;
  const doc = session?.doc ?? null;
  const sync = flowId ? flows.syncState(flowId) : IDLE_SYNC;
  /** FLO-FR-KHBY: while a sync or a resolve runs, no edit affordance acts. */
  const inert = sync.running !== null;
  const delivery = useDeliveryState(doc);

  /** FLO-FR-34 / FLO-FR-36: the action control is bound to the Flow file. */
  const discussionTarget = useMemo<DiscussionTarget | undefined>(
    () => (flowId ? { kind: "artifact", artifactId: flowId } : undefined),
    [flowId],
  );
  const control = useDiscussionControl(discussionTarget);

  const canvasId = useId().replace(/:/g, "");
  const [selection, setSelection] = useState<string[]>([]);
  const [selectedEdge, setSelectedEdge] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [heights, setHeights] = useState<Record<string, number>>({});
  const [view, setView] = useState<View>(DEFAULT_VIEW);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [resize, setResize] = useState<Resize | null>(null);
  const [pan, setPan] = useState<Pan | null>(null);
  const [connectingFrom, setConnectingFrom] = useState<string | null>(null);
  const [connectPoint, setConnectPoint] = useState<FlowPosition | null>(null);
  const [magnet, setMagnet] = useState<string | null>(null);
  const [refusal, setRefusal] = useState<RefusalMessage | null>(null);
  const [addMenuOpen, setAddMenuOpen] = useState(false);
  const [dismissAction, setDismissAction] = useState(0);
  const [editingName, setEditingName] = useState<string | null>(null);

  const surfaceRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<Drag | null>(null);
  const resizeRef = useRef<Resize | null>(null);
  const panRef = useRef<Pan | null>(null);
  const connectingRef = useRef<string | null>(null);
  const magnetRef = useRef<string | null>(null);

  const onMeasureCard = useCallback((id: string, height: number) => {
    setHeights((prev) => (prev[id] === height ? prev : { ...prev, [id]: height }));
  }, []);

  /** FLO-FR-26: every change to the plan goes through the store. */
  const edit = useCallback(
    (fn: (d: FlowDocument) => FlowDocument) => {
      if (flowId) flows.applyEdit(flowId, fn);
    },
    [flowId, flows],
  );

  /**
   * Apply an edit that can be refused. The rule runs on the live plan first,
   * so a refusal gives its feedback and changes nothing (FLO-FR-46).
   */
  const tryEdit = (
    op: (d: FlowDocument) => FlowEditResult,
    action: FlowAction,
    kind?: FlowItemKind,
  ): boolean => {
    if (!doc || inert) return false;
    const first = op(doc);
    if (!first.ok) {
      setRefusal(refusalMessage(doc, action, first.refusal, kind));
      return false;
    }
    setRefusal(null);
    edit((d) => {
      const again = op(d);
      return again.ok ? again.doc : d;
    });
    return true;
  };

  // A selection can outlive what it names: a reload can replace the plan.
  const liveSelection = doc ? selection.filter((id) => findItem(doc, id)) : [];
  const selectedId = liveSelection.length === 1 ? liveSelection[0] : null;
  const selected = doc && selectedId ? (findItem(doc, selectedId) ?? null) : null;
  const liveEdge =
    doc && selectedEdge && doc.edges.some((e) => e.id === selectedEdge) ? selectedEdge : null;

  const toCanvas = (clientX: number, clientY: number): FlowPosition => {
    const rect = surfaceRef.current?.getBoundingClientRect();
    return {
      x: (clientX - (rect?.left ?? 0) - view.x) / view.scale,
      y: (clientY - (rect?.top ?? 0) - view.y) / view.scale,
    };
  };

  /** Where an item sits now, with an in-flight drag or resize applied. */
  const absOf = (id: string): FlowPosition => {
    if (!doc) return { x: 0, y: 0 };
    if (resize?.itemId === id) return { x: resize.rect.x, y: resize.rect.y };
    const base = absolutePosition(doc, id);
    if (drag?.subtree.has(id)) {
      return { x: base.x + (drag.x - drag.startAbs.x), y: base.y + (drag.y - drag.startAbs.y) };
    }
    return base;
  };

  const sizeOf = (id: string): FlowSize | undefined => {
    if (resize?.itemId === id) return { width: resize.rect.width, height: resize.rect.height };
    return doc ? findItem(doc, id)?.size : undefined;
  };

  const frames = createFrames({ doc, heights, collapsed, absOf, sizeOf });

  /**
   * FLO-FR-42: the container a drop at `point` joins: the innermost container
   * whose bounds contain it. A collapsed container takes a drop only on what it
   * shows, its header.
   */
  const dropTargetAt = (point: FlowPosition, exclude: ReadonlySet<string>) => {
    if (!doc) return undefined;
    const skip = new Set(exclude);
    for (const item of doc.items) if (frames.hiddenBy(item.id)) skip.add(item.id);
    let id = innermostContainerAt(doc, point, skip);
    while (id && collapsed.has(id)) {
      const r = frames.rectOf(id);
      if (r && point.y <= r.y + r.height) break;
      id = parentOf(doc, id);
    }
    return id;
  };

  /** Keep a placed item clear of its container's header and border. */
  const clampInto = (parentId: string | undefined, at: FlowPosition): FlowPosition => {
    if (!doc || parentId === undefined) return at;
    const origin = absolutePosition(doc, parentId);
    return {
      x: Math.max(at.x, origin.x + CONTAINER_PADDING),
      y: Math.max(at.y, origin.y + CONTAINER_HEADER_H),
    };
  };

  /**
   * FLO-FR-07 / FLO-FR-42 / FLO-FR-OAOZ: add an item of `kind` in the middle
   * of the view. It joins the container it lands in, on the terms of a drop.
   */
  const addKind = (kind: FlowItemKind) => {
    if (!doc || inert) return;
    const rect = surfaceRef.current?.getBoundingClientRect();
    const centre = {
      x: ((rect?.width || 800) / 2 - view.x) / view.scale,
      y: ((rect?.height || 600) / 2 - view.y) / view.scale,
    };
    const extent = isContainerKind(kind)
      ? { width: 0, height: 0 }
      : { width: CARD_W, height: CARD_H };
    const parentId = dropTargetAt(centre, new Set());
    const at = clampInto(parentId, {
      x: Math.round(centre.x - extent.width / 2),
      y: Math.round(centre.y - extent.height / 2),
    });
    let added: string | null = null;
    tryEdit(
      (d) => {
        const r = addItem(d, kind, at, parentId);
        if (r.ok) added = r.itemId;
        return r.ok ? andGrow(r.itemId, r) : r;
      },
      "add",
      kind,
    );
    if (added) {
      setSelection([added]);
      setSelectedEdge(null);
    }
  };

  const openAddMenu = () => {
    setAddMenuOpen(true);
    setDismissAction((n) => n + 1);
  };

  // FLO-FR-07: a press anywhere else in the tab closes the add menu.
  useEffect(() => {
    if (!addMenuOpen) return;
    const onDown = (e: PointerEvent) => {
      const el = e.target as HTMLElement | null;
      if (el?.closest?.(".flow-addmenu")) return;
      setAddMenuOpen(false);
    };
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, [addMenuOpen]);

  /** FLO-FR-22: Fit frames the plan. A view change, never an edit. */
  const onFit = () => {
    if (!doc || doc.items.length === 0) {
      setView(DEFAULT_VIEW);
      return;
    }
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const item of doc.items) {
      if (item.parentId !== undefined) continue;
      const r = frames.rectOf(item.id);
      if (!r) continue;
      minX = Math.min(minX, r.x);
      minY = Math.min(minY, r.y);
      maxX = Math.max(maxX, r.x + r.width);
      maxY = Math.max(maxY, r.y + r.height);
    }
    const width = Math.max(maxX - minX, 1);
    const height = Math.max(maxY - minY, 1);
    const rect = surfaceRef.current?.getBoundingClientRect();
    const availW = rect?.width || width;
    const availH = rect?.height || height;
    const scale = Math.min(
      MAX_ZOOM,
      Math.max(MIN_ZOOM, Math.min(availW / width, availH / height) * 0.9),
    );
    setView({ x: -minX * scale + 24, y: -minY * scale + 24, scale });
  };

  const zoomBy = (delta: number) =>
    setView((v) => ({
      ...v,
      scale: Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.round((v.scale + delta) * 100) / 100)),
    }));

  /** FSR-FR-PGBX: show an item, its containers expanded, in the middle of the view. */
  const revealItem = (id: string) => {
    if (!doc || !findItem(doc, id)) return;
    const chain = ancestorsOf(doc, id);
    setCollapsed((prev) => {
      if (!chain.some((a) => prev.has(a))) return prev;
      const next = new Set(prev);
      chain.forEach((a) => next.delete(a));
      return next;
    });
    setSelection([id]);
    setSelectedEdge(null);
    const at = absolutePosition(doc, id);
    const item = findItem(doc, id)!;
    const size = item.size ?? { width: CARD_W, height: heights[id] || CARD_H };
    const rect = surfaceRef.current?.getBoundingClientRect();
    const w = rect?.width || 800;
    const h = rect?.height || 600;
    setView((v) => ({
      ...v,
      x: w / 2 - (at.x + Math.min(size.width, w / v.scale) / 2) * v.scale,
      y: h / 2 - (at.y + Math.min(size.height, h / v.scale) / 2) * v.scale,
    }));
  };

  /** FLO-FR-ZASL: collapse or expand a container. View state, never an edit. */
  const toggleCollapsed = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const chrome =
    ".flow-panel, .flow-edge-panel, .flow-review, .flow-overlay, .flow-refusal, .flow-dialog, .flow-addmenu, .flow-actionhost, [data-flow-chrome]";

  /**
   * Selection follows what the pointer went down on. A press on empty canvas,
   * or on the body of a container, clears it and pans the view (FLO-FR-22).
   */
  const onSurfacePointerDown = (e: React.PointerEvent) => {
    const el = e.target as HTMLElement | null;
    if (!el?.closest?.(".flow-addmenu")) setAddMenuOpen(false);
    if (el?.closest?.(chrome)) return;
    // FLO-FR-32: a press on a port draws a connection and selects nothing.
    if (el?.closest?.(".flow-port")) return;
    const itemEl = el?.closest?.("[data-item-id]") as HTMLElement | null;
    const edgeEl = el?.closest?.("[data-edge-id]") as HTMLElement | null;
    if (itemEl?.dataset.itemId) {
      const id = itemEl.dataset.itemId;
      setSelection([id]);
      setSelectedEdge(null);
      if (editingName !== id) setEditingName(null);
      return;
    }
    if (edgeEl?.dataset.edgeId) {
      setSelectedEdge(edgeEl.dataset.edgeId);
      setSelection([]);
      setEditingName(null);
      return;
    }
    if (isInteractive(el)) return;
    setSelection([]);
    setSelectedEdge(null);
    setEditingName(null);
    setRefusal(null);
    const started: Pan = { startX: e.clientX, startY: e.clientY, originX: view.x, originY: view.y };
    panRef.current = started;
    setPan(started);
  };

  /** FLO-FR-21: start a drag of an item and everything it holds. */
  const onItemDragStart = (id: string, e: React.PointerEvent) => {
    if (!doc || inert) return;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    const at = absolutePosition(doc, id);
    const p = toCanvas(e.clientX, e.clientY);
    const started: Drag = {
      itemId: id,
      subtree: subtreeOf(doc, id),
      offsetX: p.x - at.x,
      offsetY: p.y - at.y,
      startAbs: at,
      x: at.x,
      y: at.y,
      pointer: p,
      moved: false,
    };
    dragRef.current = started;
    setDrag(started);
  };

  /**
   * FLO-FR-44: start a resize from the given sides. A side cannot pass the
   * members, so a resize never puts a member outside the container.
   */
  const onResizeStart = (id: string, sides: ResizeSides, e: React.PointerEvent) => {
    const item = doc ? findItem(doc, id) : undefined;
    if (!doc || !item?.size || inert) return;
    e.stopPropagation();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    const at = absolutePosition(doc, id);
    const members = childrenOf(doc, id);
    let left = Infinity;
    let top = Infinity;
    let right = -Infinity;
    let bottom = -Infinity;
    for (const m of members) {
      const ext = m.size ?? { width: CARD_W, height: heights[m.id] || CARD_H };
      left = Math.min(left, m.position.x);
      top = Math.min(top, m.position.y);
      right = Math.max(right, m.position.x + ext.width);
      bottom = Math.max(bottom, m.position.y + ext.height);
    }
    const any = members.length > 0;
    const from = { x: at.x, y: at.y, width: item.size.width, height: item.size.height };
    const started: Resize = {
      itemId: id,
      sides,
      startX: e.clientX,
      startY: e.clientY,
      from,
      maxLeft: any ? at.x + left - CONTAINER_PADDING : Infinity,
      maxTop: any ? at.y + top - CONTAINER_HEADER_H : Infinity,
      minRight: any ? at.x + right + CONTAINER_PADDING : -Infinity,
      minBottom: any ? at.y + bottom + CONTAINER_PADDING : -Infinity,
      minWidth: MIN_CONTAINER_SIZE.width,
      minHeight: MIN_CONTAINER_SIZE.height,
      rect: from,
    };
    resizeRef.current = started;
    setResize(started);
  };

  /** FLO-FR-44: the rectangle a resize gives for a pointer at (clientX, clientY). */
  const resizedRect = (r: Resize, clientX: number, clientY: number) => {
    const dx = (clientX - r.startX) / view.scale;
    const dy = (clientY - r.startY) / view.scale;
    let left = r.from.x;
    let top = r.from.y;
    let right = r.from.x + r.from.width;
    let bottom = r.from.y + r.from.height;
    if (r.sides.left) left = Math.min(r.from.x + dx, r.maxLeft, right - r.minWidth);
    if (r.sides.right) right = Math.max(right + dx, r.minRight, left + r.minWidth);
    if (r.sides.top) top = Math.min(r.from.y + dy, r.maxTop, bottom - r.minHeight);
    if (r.sides.bottom) bottom = Math.max(bottom + dy, r.minBottom, top + r.minHeight);
    return {
      x: Math.round(left),
      y: Math.round(top),
      width: Math.round(right - left),
      height: Math.round(bottom - top),
    };
  };

  /**
   * FLO-FR-33: the item a connection in flight snaps to: the nearest shown
   * item within reach that the rules would accept, the deepest one when the
   * pointer is inside several.
   */
  const magnetFor = (point: FlowPosition, from: string): string | null => {
    if (!doc) return null;
    const near: Array<{ id: string; distance: number; depth: number }> = [];
    for (const item of doc.items) {
      if (item.id === from || frames.hiddenBy(item.id)) continue;
      const distance = frames.distanceTo(item.id, point);
      if (distance > MAGNET_RADIUS) continue;
      near.push({ id: item.id, distance, depth: ancestorsOf(doc, item.id).length });
    }
    near.sort((a, b) => a.distance - b.distance || b.depth - a.depth);
    for (const n of near) if (!connectRefusal(doc, from, n.id)) return n.id;
    return null;
  };

  const trackConnection = (clientX: number, clientY: number) => {
    const from = connectingRef.current;
    if (!from) return;
    const point = toCanvas(clientX, clientY);
    const snapped = magnetFor(point, from);
    magnetRef.current = snapped;
    setConnectPoint(point);
    setMagnet(snapped);
  };

  /** FLO-FR-32: arm a connection from `id`. */
  const beginConnection = (id: string, e: React.PointerEvent) => {
    if (inert) return;
    setRefusal(null);
    connectingRef.current = id;
    magnetRef.current = null;
    setConnectingFrom(id);
    setMagnet(null);
    setConnectPoint(toCanvas(e.clientX, e.clientY));
  };

  /** A release on an item lands an in-flight connection on it (FLO-FR-33). */
  const onItemPointerUp = (id: string) => {
    if (connectingRef.current) magnetRef.current = id;
  };

  const onSurfacePointerMove = (e: React.PointerEvent) => {
    if (e.buttons === 0) {
      if (dragRef.current || panRef.current || connectingRef.current || resizeRef.current) {
        endGesture({ abandon: true });
      }
      return;
    }
    if (panRef.current) {
      const p = panRef.current;
      setView((v) => ({
        ...v,
        x: p.originX + (e.clientX - p.startX),
        y: p.originY + (e.clientY - p.startY),
      }));
      return;
    }
    if (resizeRef.current) {
      const next = { ...resizeRef.current, rect: resizedRect(resizeRef.current, e.clientX, e.clientY) };
      resizeRef.current = next;
      setResize(next);
      return;
    }
    if (connectingRef.current) {
      trackConnection(e.clientX, e.clientY);
      return;
    }
    const current = dragRef.current;
    if (!current) return;
    const p = toCanvas(e.clientX, e.clientY);
    const x = Math.round(p.x - current.offsetX);
    const y = Math.round(p.y - current.offsetY);
    const next: Drag = {
      ...current,
      x,
      y,
      pointer: p,
      moved: current.moved || x !== current.x || y !== current.y,
    };
    dragRef.current = next;
    setDrag(next);
  };

  /**
   * FLO-FR-42 / FLO-FR-OAOZ / FLO-FR-GZCH: land a drag. The pointer decides the
   * container the item joins. A refused drop leaves the item where it was and
   * says why.
   */
  const commitMove = (moved: Drag) => {
    if (!doc) return;
    const item = findItem(doc, moved.itemId);
    if (!item) return;
    const target = dropTargetAt(moved.pointer, moved.subtree);
    const at = clampInto(target, { x: moved.x, y: moved.y });
    tryEdit(
      (d) => andGrow(moved.itemId, moveItem(d, moved.itemId, at, target)),
      "move",
      item.kind,
    );
  };

  /** FLO-FR-44: land a resize, the members staying where they are. */
  const commitResize = (r: Resize) => {
    const { itemId, rect } = r;
    tryEdit((d) => {
      const item = findItem(d, itemId);
      if (!item) return { ok: true, doc: d };
      let next = d;
      const at = absolutePosition(d, itemId);
      if (rect.x !== at.x || rect.y !== at.y) {
        const members = childrenOf(d, itemId).map((m) => ({
          id: m.id,
          at: absolutePosition(d, m.id),
        }));
        const moved = moveItem(next, itemId, { x: rect.x, y: rect.y }, item.parentId);
        if (!moved.ok) return moved;
        next = moved.doc;
        for (const m of members) {
          const back = moveItem(next, m.id, m.at, itemId);
          if (!back.ok) return back;
          next = back.doc;
        }
      }
      next = resizeItem(next, itemId, { width: rect.width, height: rect.height });
      return { ok: true, doc: growAncestors(next, itemId) };
    }, "move");
  };

  /** FLO-FR-16 / FLO-FR-17 / FLO-FR-ZEPI / FLO-FR-GZCH / FLO-FR-CEVI. */
  const completeConnection = (from: string, to: string) => {
    if (!tryEdit((d) => connect(d, from, to), "connect")) {
      setRefusal((r) => (r && from !== to ? { ...r, attempt: { from, to } } : r));
    }
  };

  /**
   * End the gesture in flight. Idempotent, because a release is seen by React
   * and by the window listener both.
   */
  const endGesture = ({ abandon = false } = {}) => {
    const moved = dragRef.current;
    const resized = resizeRef.current;
    const from = connectingRef.current;
    const target = magnetRef.current;
    dragRef.current = null;
    resizeRef.current = null;
    panRef.current = null;
    connectingRef.current = null;
    magnetRef.current = null;
    setDrag(null);
    setResize(null);
    setPan(null);
    setConnectingFrom(null);
    setConnectPoint(null);
    setMagnet(null);
    if (resized && !abandon) commitResize(resized);
    if (moved?.moved && !abandon) commitMove(moved);
    if (!abandon && from && target) completeConnection(from, target);
  };

  // A release or a move the canvas never sees still ends or carries the
  // gesture (FLO-FR-33: a release the canvas never sees abandons the drag).
  useEffect(() => {
    if (!drag && !pan && !connectingFrom && !resize) return;
    const onMove = (e: PointerEvent) => {
      if (!connectingRef.current) return;
      if (e.buttons === 0) {
        endGesture({ abandon: true });
        return;
      }
      trackConnection(e.clientX, e.clientY);
    };
    const onRelease = () => endGesture();
    const onCancel = () => endGesture({ abandon: true });
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onRelease);
    window.addEventListener("pointercancel", onCancel);
    return () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onRelease);
      window.removeEventListener("pointercancel", onCancel);
    };
  });

  /**
   * FLO-FR-32: the provisional edge of a connection in flight: dashed, with
   * the arrowhead at its free end, which ends on the snapped item when there
   * is one.
   */
  const connectionPreview = (() => {
    if (!connectingFrom || !connectPoint || !doc) return null;
    const snapped = magnet ? frames.route(connectingFrom, magnet) : null;
    if (snapped) {
      return {
        snapped: true,
        d: edgePath(snapped.ax, snapped.ay, snapped.bx, snapped.by, snapped.faceA, snapped.faceB),
      };
    }
    const start = frames.anchorToward(connectingFrom, connectPoint);
    return {
      snapped: false,
      d: edgePath(start.x, start.y, connectPoint.x, connectPoint.y, start.face, null),
    };
  })();

  return {
    project,
    session,
    doc,
    sync,
    inert,
    delivery,
    control,
    canvasId,
    selection: liveSelection,
    setSelection,
    selected,
    selectedEdge: liveEdge,
    setSelectedEdge,
    collapsed,
    toggleCollapsed,
    heights,
    onMeasureCard,
    view,
    zoomBy,
    onFit,
    revealItem,
    frames,
    absOf,
    sizeOf,
    drag,
    resize,
    magnet,
    refusal,
    setRefusal,
    addMenuOpen,
    setAddMenuOpen,
    openAddMenu,
    dismissAction,
    addKind,
    editingName,
    setEditingName,
    surfaceRef,
    edit,
    tryEdit,
    onSurfacePointerDown,
    onSurfacePointerMove,
    onItemDragStart,
    onResizeStart,
    beginConnection,
    onItemPointerUp,
    endGesture,
    connectionPreview,
  };
}

export type FlowCanvasState = ReturnType<typeof useFlowCanvas>;
