import { useCallback, useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";

import { useDiscussionControl } from "../../hooks/useDiscussionControl";
import { useProjectArtifacts } from "../../hooks/useProjectArtifacts";
import {
  absolutePosition,
  addItem,
  boardOrders,
  resolveBoardLayout,
  ancestorsOf,
  CARD_H,
  CARD_W,
  connect,
  connectRefusal,
  CONTAINER_HEADER_H,
  docOrUnchanged,
  CONTAINER_PADDING,
  findItem,
  innermostContainerAt,
  isColumnMilestone,
  isContainerKind,
  moveMilestone,
  parentOf,
  placeItem,
  reparentItem,
  resizeContainer,
  setCollapsed,
  subtreeOf,
  type FlowDocument,
  type FlowEditResult,
  type FlowItem,
  type FlowItemKind,
  type FlowLayout,
  type FlowPosition,
  type FlowSize,
  type StoredLayout,
} from "../../state/flowDocument";
import { isEditHeld, primeEditGate, waitEditGate } from "../../state/mainBranchWrite";
import type { DiscussionTarget, ItemDeliveryState, OpenableArtifact } from "../../types";
import type { FlowBinding } from "./binding";
import { boardTargetAt, pointOutside } from "./boardDrop";
import { commitBoardDrop } from "./boardCommit";
import { columnDropIndex, columnsOf } from "./columns";
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
import { useFlowLayout, type ContainerMeasure } from "./useFlowLayout";
import { useFlowSource } from "./useFlowSource";
import { useBoardDrag } from "./useBoardDrag";
import { resizedRect, resizeFrom } from "./resize";

/** DBR-FR-MYVI: how far a press moves, in screen pixels, before a drag of the board starts. */
export const BOARD_DRAG_THRESHOLD = 4;

/** The layout before the stored layout is read: nothing is drawn yet. */
const EMPTY_PLACEMENT: FlowLayout = {};

/** FLO-FR-NRBV: what the shell answers to a request to create a task's draft. */
export type CreatedDraftOutcome =
  | { ok: true; draft: { id: string; name: string } }
  /** `error` is null where the author cancelled the creation. */
  | { ok: false; error: string | null };

export interface FlowCanvasProps {
  /** The plan the canvas shows, and where its edits go (`./binding.ts`). */
  binding: FlowBinding;
  /**
   * MDL-FR-WMGU: the host keeps every edit affordance inert, for example while
   * a publish runs.
   */
  inert?: boolean;
  /** MDL-FR-KKYH / MDL-FR-JRGK: what the host adds at the trailing end of the band. */
  bandExtras?: ReactNode;
  /**
   * MDL-FR-LSKL / MDL-FR-IEWL / MDL-FR-MWMO: the host's notice lines, below
   * the band and above the canvas.
   */
  belowBand?: ReactNode;
  /**
   * SDL-FR-KNOP / MDL-FR-QEXS: rows the host adds at the end of the selected
   * item's panel.
   */
  panelExtras?: (item: FlowItem) => ReactNode;
  /** MDL-FR-PUKL: entries the host adds to the add menu; `close` closes it. */
  addMenuExtras?: (close: () => void) => ReactNode;
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
  /**
   * SMP-FR-RNLH / SMP-FR-QNUH: the view and the selection the canvas starts
   * from. Without it, the canvas starts from the default view (FLO-FR-22).
   */
  initialView?: FlowViewState;
  /** SMP-FR-RNLH: each change of the view or of the selection. */
  onViewChange?: (state: FlowViewState) => void;
  /** MDL-FR-GDUP: each new answer of the delivery state, or null before one. */
  onDeliveryStates?: (states: ReadonlyMap<string, ItemDeliveryState> | null) => void;
  /**
   * MDL-FR-HULA / MDL-FR-QELI: the **Discuss** entry of the context menu of an
   * item or a milestone. Without it, an item has no context menu.
   */
  onDiscuss?: (item: FlowItem) => void;
  /** DBR-FR-ERHW: lay the plan out as the board of the Delivery lens. */
  board?: boolean;
}

/** SMP-FR-RNLH: the view state of a canvas that a host keeps for it. */
export interface FlowViewState {
  view: View;
  selection: string[];
}

/**
 * The view state, the gestures, and the selection of the canvas (FLO-FR-21,
 * FLO-FR-22, FLO-FR-32, FLO-FR-33, FLO-FR-42, FLO-FR-44, FLO-FR-ZASL). The
 * plan rules are the document module's; this hook calls them and turns a
 * refusal into feedback.
 */
export function useFlowCanvas({
  binding,
  inert: hostInert = false,
  initialView,
  onViewChange,
  board = false,
}: FlowCanvasProps) {
  const source = useFlowSource(binding);
  const { flowId, session, doc, delivery } = source;
  const project = useProjectArtifacts();
  // EDT-FR-UPNU: the guard of the file's branch is read when the Flow opens.
  // A plan binding writes no file.
  useEffect(() => {
    if (source.mode === "file") primeEditGate();
  }, [flowId, source.mode]);

  /** MDL-FR-WMGU: while the host keeps the canvas inert, no edit affordance acts. */
  const inert = hostInert;
  const [heights, setHeights] = useState<Record<string, number>>({});
  const [containerSizes, setContainerSizes] = useState<Record<string, ContainerMeasure>>({});
  const measuredSizes = useMemo(
    () => ({ cards: heights, containers: containerSizes }),
    [heights, containerSizes],
  );
  const {
    layout: resolved,
    collapsed,
    commit: commitLayout,
    reset: resetLayout,
    stored,
    commitStored,
    boardNone,
  } = useFlowLayout(doc, source.scope, source.layouts, measuredSizes, board);
  /** DBR-FR-ERHW: the board shows once the stored layout is read. */
  const boardMode = board && doc !== null && stored !== null;
  const storedRef = useRef<StoredLayout | null>(stored);
  storedRef.current = stored;
  const layout: FlowLayout = resolved ?? EMPTY_PLACEMENT;
  // The newest layout and its commit, for an answer that lands after a render.
  const layoutRef = useRef<FlowLayout | null>(resolved);
  layoutRef.current = resolved;
  const commitRef = useRef(commitLayout);
  commitRef.current = commitLayout;

  /**
   * FLO-FR-34 / FLO-FR-36: the action control is bound to the Flow file. A
   * plan binding carries no action control (MDL-FR-NTVL).
   */
  const discussionTarget = useMemo<DiscussionTarget | undefined>(
    () => (flowId ? { kind: "artifact", artifactId: flowId } : undefined),
    [flowId],
  );
  const control = useDiscussionControl(discussionTarget);

  const canvasId = useId().replace(/:/g, "");
  const [selection, setSelection] = useState<string[]>(() => initialView?.selection ?? []);
  const [selectedEdge, setSelectedEdge] = useState<string | null>(null);
  const [view, setView] = useState<View>(() => initialView?.view ?? DEFAULT_VIEW);
  // SMP-FR-RNLH: the host keeps the view state of a canvas that unmounts.
  const viewChange = useRef(onViewChange);
  viewChange.current = onViewChange;
  useEffect(() => {
    viewChange.current?.({ view, selection });
  }, [view, selection]);
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
  /**
   * FLO-FR-09 / FLO-FR-21: the card a press went down on. It captures the
   * pointer only once the drag moves: a capture at the press retargets the
   * double-click to the card, so the name would never see it.
   */
  const dragCapture = useRef<{ el: Element; pointerId: number } | null>(null);
  const resizeRef = useRef<Resize | null>(null);
  const panRef = useRef<Pan | null>(null);
  const connectingRef = useRef<string | null>(null);
  const magnetRef = useRef<string | null>(null);

  const onMeasureCard = useCallback((id: string, height: number) => {
    setHeights((prev) => (prev[id] === height ? prev : { ...prev, [id]: height }));
  }, []);

  /** MDL-FR-ECPZ: a container reports the room its header and its content need. */
  const onMeasureContainer = useCallback((id: string, next: ContainerMeasure) => {
    setContainerSizes((prev) => {
      const was = prev[id];
      return was && was.headWidth === next.headWidth && was.memberTop === next.memberTop
        ? prev
        : { ...prev, [id]: next };
    });
  }, []);

  /**
   * FLO-FR-26: every change to the plan goes through the binding. Returns true
   * when the edit changed the plan.
   */
  const edit = source.edit;

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

  /**
   * DBR-FR-QCAJ / DBR-FR-CZEU: while an item of the board is dragged, the
   * board shows without it, and with the placeholder where it would land. A
   * milestone column moves as a column (FLO-FR-OGPF).
   */
  const draggedItem = doc && drag ? findItem(doc, drag.itemId) : undefined;
  const lifting =
    boardMode && drag?.moved && draggedItem && !isColumnMilestone(draggedItem) ? drag : null;
  const preview =
    lifting && doc && stored
      ? resolveBoardLayout(doc, stored, {
          lifted: lifting.subtree,
          gap:
            lifting.drop && !lifting.drop.collapsed
              ? {
                  holder: lifting.drop.holder,
                  index: lifting.drop.index,
                  height: layout[lifting.itemId]?.size?.height ?? CARD_H,
                }
              : null,
        })
      : null;
  const shown: FlowLayout = preview?.layout ?? layout;

  /** Where an item sits now, with an in-flight drag or resize applied. */
  const absOf = (id: string): FlowPosition => {
    if (!doc) return { x: 0, y: 0 };
    if (resize?.itemId === id) return { x: resize.rect.x, y: resize.rect.y };
    if (drag?.subtree.has(id) && (!boardMode || drag.moved)) {
      const base = absolutePosition(doc, layout, id);
      return { x: base.x + (drag.x - drag.startAbs.x), y: base.y + (drag.y - drag.startAbs.y) };
    }
    return absolutePosition(doc, shown, id);
  };

  const sizeOf = (id: string): FlowSize | undefined => {
    if (resize?.itemId === id) return { width: resize.rect.width, height: resize.rect.height };
    if (lifting?.subtree.has(id)) return layout[id]?.size;
    return shown[id]?.size;
  };

  const frames = createFrames({ doc, heights, collapsed, absOf, sizeOf });
  /** FLO-FR-SQUS / FLO-FR-IAOQ: the milestone columns and the No milestone column. */
  const columnsBase = doc ? columnsOf(doc, layout, frames.rectOf) : { order: [], arrows: [], none: null };
  // FLO-FR-IAOQ / DBR-FR-ZGNB: on the board, the No milestone column is a column of the board.
  const columns = boardMode ? { ...columnsBase, none: preview?.none ?? boardNone } : columnsBase;


  /**
   * FLO-FR-42: the container a drop at `point` joins: the innermost container
   * whose bounds contain it. A collapsed container takes a drop only on what it
   * shows, its header.
   */
  const dropTargetAt = (point: FlowPosition, exclude: ReadonlySet<string>) => {
    if (!doc) return undefined;
    const skip = new Set(exclude);
    for (const item of doc.items) if (frames.hiddenBy(item.id)) skip.add(item.id);
    let id = innermostContainerAt(doc, layout, point, skip);
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
    const origin = absolutePosition(doc, layout, parentId);
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
    const first = addItem(doc, kind, parentId);
    if (!tryEdit((d) => addItem(d, kind, parentId), "add", kind) || !first.ok) return;
    // FLO-FR-21: the new item sits where the author sees it, in the layout.
    commitLayout(placeItem(first.doc, layout, first.itemId, at));
    setSelection([first.itemId]);
    setSelectedEdge(null);
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

  /**
   * FLO-FR-MGJH: zoom by `factor` about a point of the screen, which stays in
   * place, within the range of the zoom controls.
   */
  const zoomAt = useCallback((factor: number, clientX: number, clientY: number) => {
    const rect = surfaceRef.current?.getBoundingClientRect();
    const px = clientX - (rect?.left ?? 0);
    const py = clientY - (rect?.top ?? 0);
    setView((v) => {
      const scale = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.scale * factor));
      if (scale === v.scale) return v;
      const ratio = scale / v.scale;
      return { x: px - (px - v.x) * ratio, y: py - (py - v.y) * ratio, scale };
    });
  }, []);

  /** FLO-FR-MFPM: move the view by a scroll distance, in pixels. */
  const panBy = useCallback((dx: number, dy: number) => {
    if (dx === 0 && dy === 0) return;
    setView((v) => ({ ...v, x: v.x + dx, y: v.y + dy }));
  }, []);

  /**
   * MDL-FR-JIKK / SDL-FR-MCGI: select an item, expand each collapsed container
   * that holds it, and centre it. Returns false when the plan has no such item.
   */
  const revealItem = (id: string): boolean => {
    if (!doc || !resolved || !findItem(doc, id)) return false;
    const chain = ancestorsOf(doc, id);
    let expanded = layout;
    for (const a of chain) expanded = setCollapsed(doc, expanded, a, false);
    commitLayout(expanded);
    setSelection([id]);
    setSelectedEdge(null);
    centreOn(id, view);
    return true;
  };

  /**
   * MDL-FR-JIKK: the item a reveal centred, where it was, and the view that
   * centred it. The layout can still move the item after the reveal, for
   * example when the headers of a GitHub plan are measured and the columns
   * grow (FLO-FR-SQUS). Until the author moves the view, the canvas centres
   * the item again where it lands.
   */
  const centred = useRef<{ id: string; x: number; y: number; view: View } | null>(null);

  const centreOn = (id: string, from: View) => {
    if (!doc) return;
    const at = absolutePosition(doc, layout, id);
    const size = layout[id]?.size ?? { width: CARD_W, height: heights[id] || CARD_H };
    const rect = surfaceRef.current?.getBoundingClientRect();
    const w = rect?.width || 800;
    const h = rect?.height || 600;
    const next: View = {
      ...from,
      x: w / 2 - (at.x + Math.min(size.width, w / from.scale) / 2) * from.scale,
      y: h / 2 - (at.y + Math.min(size.height, h / from.scale) / 2) * from.scale,
    };
    centred.current = { id, x: at.x, y: at.y, view: next };
    setView(next);
  };

  useEffect(() => {
    const kept = centred.current;
    if (!kept || !doc) return;
    if (view !== kept.view || !findItem(doc, kept.id)) {
      centred.current = null;
      return;
    }
    const at = absolutePosition(doc, layout, kept.id);
    if (at.x !== kept.x || at.y !== kept.y) centreOn(kept.id, view);
  });

  /**
   * FLO-FR-ZASL / FLO-FR-TDHR: collapse or expand a container. The layout
   * keeps it; it is never an edit.
   */
  const toggleCollapsed = (id: string) => {
    if (!doc || !resolved) return;
    commitLayout(setCollapsed(doc, layout, id, !collapsed.has(id)));
  };

  const chrome =
    ".flow-panel, .flow-edge-panel, .flow-overlay, .flow-refusal, .flow-dialog, .flow-addmenu, .flow-actionhost, [data-flow-chrome]";

  /**
   * Selection follows what the pointer went down on. A press on empty canvas,
   * or on the body of a container, clears it and pans the view (FLO-FR-22).
   */
  const onSurfacePointerDown = (e: React.PointerEvent) => {
    // MDL-FR-JBEL: a secondary press opens a menu and changes no selection.
    if (e.button === 2) return;
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
    if (!doc || inert || e.button === 2) return;
    dragCapture.current = { el: e.currentTarget, pointerId: e.pointerId };
    const at = absolutePosition(doc, layout, id);
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
      startClient: { x: e.clientX, y: e.clientY },
      client: { x: e.clientX, y: e.clientY },
    };
    dragRef.current = started;
    setDrag(started);
  };

  /**
   * FLO-FR-44: start a resize from the given sides. A side cannot pass the
   * members, so a resize never puts a member outside the container.
   */
  const onResizeStart = (id: string, sides: ResizeSides, e: React.PointerEvent) => {
    if (!doc || inert) return;
    const started = resizeFrom(doc, layout, heights, id, sides, e.clientX, e.clientY);
    if (!started) return;
    e.stopPropagation();
    e.currentTarget.setPointerCapture?.(e.pointerId);
    resizeRef.current = started;
    setResize(started);
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
      const next = { ...resizeRef.current, rect: resizedRect(resizeRef.current, e.clientX, e.clientY, view.scale) };
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
    // DBR-FR-MYVI: on the board, a press that moves less than the threshold is a click.
    const start = current.startClient;
    const moved = boardMode
      ? current.moved ||
        (start !== undefined &&
          Math.hypot(e.clientX - start.x, e.clientY - start.y) > BOARD_DRAG_THRESHOLD)
      : current.moved || x !== current.x || y !== current.y;
    const next: Drag = retarget({
      ...current,
      x,
      y,
      pointer: p,
      moved,
      client: { x: e.clientX, y: e.clientY },
    });
    const capture = dragCapture.current;
    if (next.moved && capture) {
      dragCapture.current = null;
      try {
        capture.el.setPointerCapture?.(capture.pointerId);
      } catch {
        // The pointer is gone already; the window listener ends the drag.
      }
    }
    dragRef.current = next;
    setDrag(next);
  };

  /** DBR-FR-CZEU / DBR-FR-EEKD: the slot of the board under the pointer of `next`. */
  const retarget = (next: Drag): Drag => {
    if (!boardMode || !doc || !stored || !next.moved) return next;
    const item = findItem(doc, next.itemId);
    if (!item || isColumnMilestone(item)) return next;
    // DBR-FR-BBDZ: a pointer outside the canvas has no target, so a release there puts the item back.
    if (pointOutside(surfaceRef.current?.getBoundingClientRect(), next.client)) return { ...next, drop: null, hint: null };
    const target = boardTargetAt({
      doc,
      item,
      lifted: next.subtree,
      point: next.pointer,
      orders: boardOrders(doc, stored),
      collapsed,
      none: columns.none,
      rectOf: frames.rectOf,
      hidden: (id) => frames.hiddenBy(id) !== undefined,
    });
    return { ...next, drop: target.drop, hint: target.hint };
  };

  /**
   * FLO-FR-21 / FLO-FR-42 / FLO-FR-OAOZ / FLO-FR-GZCH / FLO-FR-TDHR: land a
   * drag. The pointer decides the container the item joins. A drop into
   * another container is an edit; a refused drop leaves the item where it was
   * and says why. The new place is the layout's, stored once.
   */
  const commitMove = (moved: Drag) => {
    if (!doc || inert) return;
    const item = findItem(doc, moved.itemId);
    if (!item) return;
    if (isColumnMilestone(item)) {
      // FLO-FR-OGPF: a milestone column has no stored position. Its drop
      // changes the milestone order, which is an edit (FLO-FR-26).
      const index = columnDropIndex(columns, frames.rectOf, item.id, moved.pointer.x);
      const reorder = () => edit((d) => moveMilestone(d, item.id, index));
      // EDT-FR-UPNU: the gate can hold the change while the branch is read or
      // while the author confirms a write to a main branch. The new order goes
      // in once the gate lets it, and not at all after a cancel.
      if (!reorder() && isEditHeld()) {
        void waitEditGate().then((confirmed) => {
          if (confirmed) reorder();
        });
      }
      return;
    }
    if (boardMode) {
      if (!stored || !moved.drop) return;
      commitBoardDrop({
        doc,
        stored,
        item,
        drop: moved.drop,
        mode: source.mode === "file" ? "file" : "plan",
        edit,
        editAndWait: source.editAndWait,
        commitStored,
        latestStored: () => storedRef.current,
        refuse: (r) => setRefusal(r ? refusalMessage(doc, "move", r, item.kind) : null),
      });
      return;
    }
    const target = dropTargetAt(moved.pointer, moved.subtree);
    const at = clampInto(target, { x: moved.x, y: moved.y });
    if (target === item.parentId) {
      commitLayout(placeItem(doc, layout, moved.itemId, at));
      return;
    }
    const first = reparentItem(doc, moved.itemId, target);
    if (!first.ok) {
      setRefusal(refusalMessage(doc, "move", first.refusal, item.kind));
      return;
    }
    setRefusal(null);
    const before = layout[moved.itemId];
    const answer = source.editAndWait((d) => docOrUnchanged(d, reparentItem(d, moved.itemId, target)));
    commitLayout(placeItem(first.doc, layout, moved.itemId, at));
    // MDL-FR-PUCM: a refused move restores the plan of the last answer, so the
    // item goes back to its old place in its old container.
    void answer?.then((a) => {
      if (a.ok || !before) return;
      const latest = layoutRef.current;
      if (latest) commitRef.current({ ...latest, [moved.itemId]: before });
    });
  };

  /** FLO-FR-44 / FLO-FR-TDHR: land a resize, the members staying where they are. */
  const commitResize = (r: Resize) => {
    if (!doc || inert) return;
    commitLayout(resizeContainer(doc, layout, r.itemId, r.rect));
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
    dragCapture.current = null;
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

  // DBR-FR-BBDZ / DBR-FR-IQOM: Escape puts the item back; an edge pans the view.
  useBoardDrag({
    active: lifting !== null,
    extent: () => {
      let width = columns.none ? columns.none.x + columns.none.width : 0;
      let height = columns.none?.height ?? 0;
      for (const i of doc?.items ?? []) {
        const r = i.parentId === undefined ? frames.rectOf(i.id) : null;
        if (r) {
          width = Math.max(width, r.x + r.width);
          height = Math.max(height, r.y + r.height);
        }
      }
      return { width, height };
    },
    dragRef,
    surfaceRef,
    view,
    panBy,
    onPanned: (next) => {
      const again = retarget(next);
      dragRef.current = again;
      setDrag(again);
    },
    onCancel: () => endGesture({ abandon: true }),
  });

  return {
    binding,
    source,
    project,
    session,
    doc,
    layout,
    layoutReady: resolved !== null,
    resetLayout,
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
    onMeasureContainer,
    columns,
    view,
    zoomBy,
    zoomAt,
    panBy,
    onFit,
    revealItem,
    commitLayout,
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
    board: boardMode,
    /** DBR-FR-CZEU: the placeholder of the drag, in canvas units, or null. */
    boardGap: preview?.gapRect ?? null,
    /** DBR-FR-CZEU: the holder of the target stack (null is No milestone), or undefined. */
    dropHolder: lifting?.drop ? lifting.drop.holder : undefined,
    /** DBR-FR-EEKD: the hint of a stack under the pointer that cannot hold the dragged kind. */
    dragHint: lifting?.hint ?? null,
    /** DBR-FR-EEKD: no stack under the pointer takes the drop. */
    dragRefused: lifting !== null && !lifting.drop,
    lifted: lifting?.subtree ?? null,
  };
}

export type FlowCanvasState = ReturnType<typeof useFlowCanvas>;
