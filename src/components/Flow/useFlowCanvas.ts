import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore } from "react";

import * as api from "../../api";
import {
  useProjectArtifacts,
  type ArtifactOption } from "../../hooks/useProjectArtifacts";
import { useDiscussionControl } from "../../hooks/useDiscussionControl";
import {
  absolutePosition,
  addLoop,
  addNode,
  ancestorsOf,
  connect,
  edgesSeveredByMove,
  elements,
  findElement,
  innermostLoopAt,
  loopHolds,
  minLoopSize,
  moveElement,
  removeLoop,
  resizeLoop,
  subtreeOf,
  LOOP_PADDING,
  MEMBER_NODE_H,
  type FlowDocument,
  type FlowLoop,
  type FlowPosition,
  type FlowSize,
} from "../../state/flowDocument";
import type { FlowSessionStore } from "../../state/flowSessions";
import type { DiscussionTarget, OpenableArtifact } from "../../types";
import {
  DEFAULT_VIEW,
  MAGNET_RADIUS,
  MAX_ZOOM,
  MIN_ZOOM,
  NODE_W,
  edgeCurve,
  isInteractive,
  type Drag,
  type Pan,
  type PendingMove,
  type Resize,
  type View } from "./geometry";
import { createFrames } from "./frames";

export interface FlowCanvasProps {
  /**
   * The open Flow's artifact id (ASC-FR-13). Absent only for a tab opened on
   * something the backend cannot address, which has no document to load.
   */
  flowId?: string;
  /** FLO-FR-30: the shell-owned store the graph and its dirty state live in. */
  flows: FlowSessionStore;
  /**
   * FLO-FR-12: open a node's referenced artifact in its natural surface. The
   * shell owns the tab strip, so it is what answers the single-tab focus-jump
   * rule of `TAB-tabs.md` TAB-FR-07.
   */
  onOpenArtifact?: (item: OpenableArtifact) => void;
}

/**
 * Everything the canvas holds and does, apart from what it draws.
 *
 * The state, the gestures, and the geometry live here so that `index.tsx` is
 * the rendering alone. Nothing about the behaviour changes: the hooks run in
 * the order they always did, in the component that always called them.
 */
export function useFlowCanvas({
  flowId,
  flows,
  onOpenArtifact }: FlowCanvasProps) {
  useSyncExternalStore(flows.subscribe, flows.getVersion);
  const { artifacts, referenceable, loaded: treeLoaded } = useProjectArtifacts();

  const [selectedElement, setSelectedElement] = useState<string | null>(null);
  const [selectedEdge, setSelectedEdge] = useState<string | null>(null);
  /**
   * FLO-FR-48: this canvas's own arrowhead definitions. An SVG marker is
   * reached by document-wide id, so the name has to be unique in the document
   * and not merely within this subtree. Only the active tab's surface is
   * mounted today, which would make one fixed name enough — but that is the
   * shell's invariant to keep (`Viewport`), not this canvas's to depend on, and
   * a second mounted canvas would otherwise silently draw with the first one's
   * markers. The colons React puts in a generated id are dropped: this ends up
   * inside a `url(#…)` reference, where they read as a pseudo-element.
   */
  const canvasId = useId().replace(/:/g, "");
  const arrowId = `flow-arrow-${canvasId}`;
  const arrowActiveId = `flow-arrow-active-${canvasId}`;
  /**
   * FLO-FR-48: the height each node is painting at, reported by the node itself.
   * A node absent from here has not been measured yet and falls back to the
   * nominal height, which is what the first render of a graph draws against.
   */
  const [heights, setHeights] = useState<Record<string, number>>({});
  const onMeasureNode = useCallback((id: string, height: number) => {
    // Only a changed height re-renders: a measurement that agrees with what is
    // already stored must not put the canvas into a render loop with itself.
    setHeights((prev) => (prev[id] === height ? prev : { ...prev, [id]: height }));
  }, []);
  const [view, setView] = useState<View>(DEFAULT_VIEW);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [resize, setResize] = useState<Resize | null>(null);
  const [pan, setPan] = useState<Pan | null>(null);
  /** The element an in-flight connection drag started from (FLO-FR-15). */
  const [connectingFrom, setConnectingFrom] = useState<string | null>(null);
  /** FLO-FR-32: where the in-flight connection's free end currently is. */
  const [connectPoint, setConnectPoint] = useState<FlowPosition | null>(null);
  /** FLO-FR-33: the element the in-flight connection has snapped to, if any. */
  const [magnet, setMagnet] = useState<string | null>(null);
  /** FLO-FR-16 / FLO-FR-17 / FLO-FR-40: why the last connection was refused. */
  const [refusal, setRefusal] = useState<string | null>(null);
  /** FLO-FR-07: the add affordance's menu of what can be added. */
  const [addMenuOpen, setAddMenuOpen] = useState(false);
  /** ACT-FR-12: bumped to dismiss whatever the action control had open. */
  const [dismissAction, setDismissAction] = useState(0);
  /** FLO-FR-43: a move whose cost in edges the author has not answered for yet. */
  const [pendingMove, setPendingMove] = useState<PendingMove | null>(null);
  /** FLO-FR-45: a loop removal whose scope the author has not chosen yet. */
  const [pendingLoopRemoval, setPendingLoopRemoval] = useState<string | null>(null);
  const surfaceRef = useRef<HTMLDivElement>(null);
  const addTriggerRef = useRef<HTMLButtonElement>(null);
  const addMenuRef = useRef<HTMLDivElement>(null);
  /**
   * The in-flight gestures, mirrored where the window-level release handler
   * below can both read and clear them.
   *
   * Refs rather than the state above because that handler is registered once
   * per gesture and would otherwise close over a stale snapshot — and because
   * clearing the ref first is what makes ending a gesture idempotent, so a
   * release seen by both React and the window listener commits exactly once.
   */
  const dragRef = useRef<Drag | null>(null);
  const resizeRef = useRef<Resize | null>(null);
  const panRef = useRef<Pan | null>(null);
  const connectingRef = useRef<string | null>(null);
  /** The element the release will connect to: the magnet, or an exact drop. */
  const magnetRef = useRef<string | null>(null);

  const session = flowId ? flows.get(flowId) : undefined;

  /**
   * FLO-FR-34 / ACT-FR-02: the action control is bound to the Flow file this tab
   * is open on. FLO-FR-35: neither the panel nor the composer is part of the
   * canvas — opening either lays a surface over the graph without re-laying it,
   * moves no node, changes no edge, and adds nothing to the document.
   */
  const discussionTarget = useMemo<DiscussionTarget | undefined>(
    () => (flowId ? { kind: "artifact", artifactId: flowId } : undefined),
    [flowId],
  );
  const control = useDiscussionControl(discussionTarget);
  const doc = session?.doc ?? null;

  const byId = useMemo(
    () => new Map(artifacts.map((a) => [a.id, a])),
    [artifacts],
  );

  const edit = (fn: (d: FlowDocument) => FlowDocument) => {
    if (flowId) flows.applyEdit(flowId, fn);
  };

  // A selection can outlive the thing it points at: this canvas remounts on
  // every tab switch, and an external reload (FLO-FR-31) can replace the graph
  // under it. Resolving against the live document keeps the affordances inert
  // rather than applying an edit that changes nothing — which would raise a
  // false unsaved state (FLO-FR-26) and make the next tab close write an
  // unchanged graph.
  const selected = doc && selectedElement ? findElement(doc, selectedElement) : null;

  const toGraph = (clientX: number, clientY: number) => {
    const rect = surfaceRef.current?.getBoundingClientRect();
    return {
      x: (clientX - (rect?.left ?? 0) - view.x) / view.scale,
      y: (clientY - (rect?.top ?? 0) - view.y) / view.scale };
  };

  /**
   * Where an element sits on the canvas right now, including whatever an
   * in-flight drag has moved it by. A loop's members travel with it, which is
   * why the delta is applied to the whole dragged subtree (FLO-FR-44).
   */
  const absOf = (id: string): FlowPosition => {
    if (!doc) return { x: 0, y: 0 };
    const base = absolutePosition(doc, id);
    if (drag?.subtree.has(id)) {
      return {
        x: base.x + (drag.x - drag.startAbs.x),
        y: base.y + (drag.y - drag.startAbs.y) };
    }
    return base;
  };

  /** A loop's size right now, including an in-flight resize. */
  const sizeOf = (loop: FlowLoop): FlowSize =>
    resize?.loopId === loop.id ? resize.size : loop.size;

  /**
   * FLO-FR-07 / FLO-FR-42: place a new element. It lands in the middle of what
   * the author is looking at, and joins the innermost loop whose bounds contain
   * that point — the same rule a drop follows.
   */
  const placeAt = (): { absolute: FlowPosition; parentId?: string } => {
    const rect = surfaceRef.current?.getBoundingClientRect();
    const centre = {
      x: ((rect?.width ?? 800) / 2 - view.x) / view.scale - NODE_W / 2,
      y: ((rect?.height ?? 600) / 2 - view.y) / view.scale - MEMBER_NODE_H / 2 };
    const parentId = doc
      ? innermostLoopAt(doc, {
          x: centre.x + NODE_W / 2,
          y: centre.y + MEMBER_NODE_H / 2 })
      : undefined;
    return { absolute: centre, parentId };
  };

  /** Rebase a canvas point into `parentId`'s coordinate frame. */
  const relativeTo = (
    absolute: FlowPosition,
    parentId: string | undefined,
  ): FlowPosition => {
    if (!doc || parentId === undefined) return absolute;
    const origin = absolutePosition(doc, parentId);
    return { x: absolute.x - origin.x, y: absolute.y - origin.y };
  };

  const onAddNode = () => {
    if (!doc) return;
    const { absolute, parentId } = placeAt();
    edit((d) => addNode(d, relativeTo(absolute, parentId), parentId));
  };

  const onAddLoop = () => {
    if (!doc) return;
    const { absolute, parentId } = placeAt();
    edit((d) => addLoop(d, relativeTo(absolute, parentId), parentId));
  };

  /** FLO-FR-07 / ACT-FR-12: opening the add menu dismisses the action control. */
  const openAddMenu = () => {
    setAddMenuOpen(true);
    setDismissAction((n) => n + 1);
  };

  /**
   * FLO-FR-07: close the menu and put focus back where it was opened from, so a
   * keyboard author who dismissed it is not dropped onto the document body.
   */
  const closeAddMenu = ({ restoreFocus = false } = {}) => {
    setAddMenuOpen(false);
    if (restoreFocus) addTriggerRef.current?.focus();
  };

  /**
   * FLO-FR-07: a pointer-down anywhere else **in the tab** dismisses it — not
   * only on the canvas. The canvas's own handler cannot answer for the band of
   * Flow fields above it, so the whole tab is watched while the menu is open.
   */
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

  /**
   * FLO-FR-07: the menu takes focus when it opens. Without it the trigger keeps
   * focus, Escape never reaches the menu, and a keyboard author cannot reach the
   * entries at all.
   */
  useEffect(() => {
    if (!addMenuOpen) return;
    addMenuRef.current?.querySelector("button")?.focus();
  }, [addMenuOpen]);

  // FLO-FR-22: Fit is a view operation. It frames the saved layout and changes
  // nothing about the document, so it never marks the Flow unsaved.
  const onFit = () => {
    const all = doc ? elements(doc) : [];
    if (all.length === 0) {
      setView(DEFAULT_VIEW);
      return;
    }
    let minX = Infinity;
    let minY = Infinity;
    let maxX = -Infinity;
    let maxY = -Infinity;
    for (const e of all) {
      const at = absOf(e.id);
      const w = e.kind === "loop" ? e.size.width : NODE_W;
      const h = e.kind === "loop" ? e.size.height : MEMBER_NODE_H;
      minX = Math.min(minX, at.x);
      minY = Math.min(minY, at.y);
      maxX = Math.max(maxX, at.x + w);
      maxY = Math.max(maxY, at.y + h);
    }
    const width = maxX - minX;
    const height = maxY - minY;
    const rect = surfaceRef.current?.getBoundingClientRect();
    // A zero-sized rect is what jsdom and a not-yet-laid-out canvas report;
    // falling back to the content's own extent keeps the scale at 1 there
    // rather than collapsing it to the floor.
    const availW = rect?.width || width;
    const availH = rect?.height || height;
    const scale = Math.min(
      MAX_ZOOM,
      Math.max(MIN_ZOOM, Math.min(availW / width, availH / height) * 0.9),
    );
    setView({ x: -minX * scale + 20, y: -minY * scale + 20, scale });
  };

  const zoomBy = (delta: number) =>
    setView((v) => ({
      ...v,
      scale: Math.min(
        MAX_ZOOM,
        Math.max(MIN_ZOOM, Math.round((v.scale + delta) * 100) / 100),
      ) }));

  /**
   * Selection follows whatever the pointer went down on, decided from the event
   * target rather than by each element stopping propagation: the fields inside
   * an element have to keep their own pointer behaviour, and an element that
   * swallowed the event to select itself would take that away.
   */
  const onSurfacePointerDown = (e: React.PointerEvent) => {
    const el = e.target as HTMLElement | null;
    // FLO-FR-07 / ACT-FR-12: a pointer-down anywhere else in the tab dismisses
    // the add menu.
    if (!el?.closest?.(".flow-addmenu")) setAddMenuOpen(false);
    const elementEl = el?.closest?.("[data-element-id]") as HTMLElement | null;
    const edgeEl = el?.closest?.("[data-edge-id]") as HTMLElement | null;
    if (elementEl?.dataset.elementId) {
      setSelectedElement(elementEl.dataset.elementId);
      setSelectedEdge(null);
    } else if (edgeEl?.dataset.edgeId) {
      setSelectedEdge(edgeEl.dataset.edgeId);
      setSelectedElement(null);
    } else if (isInteractive(el) || el?.closest?.(".flow-overlay")) {
      // An on-canvas control (FLO-FR-07, FLO-FR-22), or the chrome around one —
      // the zoom readout between the buttons is part of the control cluster and
      // not the canvas behind it. It neither pans nor clears the selection the
      // user is working with.
      return;
    } else {
      setSelectedElement(null);
      setSelectedEdge(null);
      // FLO-FR-22: a press on empty canvas pans the view. View state only — no
      // position in the document moves, so panning never marks the Flow unsaved.
      const started: Pan = {
        startX: e.clientX,
        startY: e.clientY,
        originX: view.x,
        originY: view.y };
      panRef.current = started;
      setPan(started);
    }
  };

  const onElementDragStart = (id: string, e: React.PointerEvent) => {
    if (!doc) return;
    // Pointer capture keeps the move and release targeted at this element even
    // while the cursor is outside the canvas — without it a drag that ends over
    // the toolbar above never sees its `pointerup`, and the element goes on
    // following the cursor until some later, unrelated click drops it somewhere
    // the author never chose.
    e.currentTarget.setPointerCapture?.(e.pointerId);
    const at = absolutePosition(doc, id);
    const p = toGraph(e.clientX, e.clientY);
    const started: Drag = {
      elementId: id,
      subtree: subtreeOf(doc, id),
      offsetX: p.x - at.x,
      offsetY: p.y - at.y,
      startAbs: at,
      x: at.x,
      y: at.y,
      moved: false };
    dragRef.current = started;
    setDrag(started);
  };

  /**
   * FLO-FR-44: the smallest this loop may be made, in graph units, measured from
   * what it is actually holding.
   *
   * `minLoopSize` is the document's floor and knows only a node's nominal
   * extent, but a node's height grows with its rendered prompt (FLO-FR-13). A
   * clamp against the nominal height would let a resize leave a tall member
   * hanging outside its container, so the rendered rectangles are measured and
   * the larger of the two floors wins.
   */
  const measuredMinSize = (loopId: string): FlowSize => {
    const floor = doc ? minLoopSize(doc, loopId) : { width: 0, height: 0 };
    const box = surfaceRef.current?.querySelector(
      `[data-element-id="${CSS.escape(loopId)}"]`,
    );
    if (!doc || !box) return floor;
    const origin = box.getBoundingClientRect();
    let width = floor.width;
    let height = floor.height;
    for (const member of elements(doc)) {
      if (member.parentId !== loopId) continue;
      const el = surfaceRef.current?.querySelector(
        `[data-element-id="${CSS.escape(member.id)}"]`,
      );
      if (!el) continue;
      const rect = el.getBoundingClientRect();
      // Viewport pixels back into graph units: the viewport layer is scaled, so
      // a measured extent is `scale` times the number the document carries.
      width = Math.max(
        width,
        (rect.right - origin.left) / view.scale + LOOP_PADDING,
      );
      height = Math.max(
        height,
        (rect.bottom - origin.top) / view.scale + LOOP_PADDING,
      );
    }
    return { width, height };
  };

  const onLoopResizeStart = (loop: FlowLoop, e: React.PointerEvent) => {
    e.currentTarget.setPointerCapture?.(e.pointerId);
    const started: Resize = {
      loopId: loop.id,
      startX: e.clientX,
      startY: e.clientY,
      from: { ...loop.size },
      min: measuredMinSize(loop.id),
      size: { ...loop.size } };
    resizeRef.current = started;
    setResize(started);
  };

  const onSurfacePointerMove = (e: React.PointerEvent) => {
    // A gesture whose release was never seen leaves no state behind to act on —
    // but if one ever did, this is what keeps it from tracking a pointer with no
    // button held down.
    if (e.buttons === 0) {
      if (
        dragRef.current ||
        panRef.current ||
        connectingRef.current ||
        resizeRef.current
      ) {
        // The release happened somewhere nothing could see it — the pointer left
        // the window. That is an abandon, not a drop: completing the connection
        // the magnet last held would mint an edge the user never released to
        // create, and a Flow has no undo (FLO-FR-32).
        endGesture({ abandon: true });
      }
      return;
    }
    if (panRef.current) {
      const p = panRef.current;
      setView((v) => ({
        ...v,
        x: p.originX + (e.clientX - p.startX),
        y: p.originY + (e.clientY - p.startY) }));
      return;
    }
    if (resizeRef.current) {
      const r = resizeRef.current;
      // FLO-FR-44: clamped as it is dragged, not only when it lands — a
      // container that shrinks past its members mid-gesture shows them spilling
      // out of it, which is the very thing the rule exists to prevent.
      const next: Resize = {
        ...r,
        size: {
          width: Math.max(
            r.min.width,
            r.from.width + (e.clientX - r.startX) / view.scale,
          ),
          height: Math.max(
            r.min.height,
            r.from.height + (e.clientY - r.startY) / view.scale,
          ) } };
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
    const p = toGraph(e.clientX, e.clientY);
    const x = p.x - current.offsetX;
    const y = p.y - current.offsetY;
    const next = {
      ...current,
      x,
      y,
      moved: current.moved || x !== current.x || y !== current.y };
    dragRef.current = next;
    setDrag(next);
  };

  /**
   * FLO-FR-33: the element an in-flight connection should snap to, or null.
   *
   * The nearest element whose anchor is within the snap radius of the pointer,
   * and which the connection could actually land on: the source itself, one
   * already connected in this direction, and one in a different container are
   * skipped, because snapping to any of them would replace a connection the
   * author can still complete with one that can only be refused.
   */
  const magnetFor = (point: FlowPosition, from: string): string | null => {
    if (!doc) return null;
    let best: string | null = null;
    let bestDistance = MAGNET_RADIUS;
    for (const element of elements(doc)) {
      if (element.id === from) continue;
      // Distance first: this runs on every pointer move, and the rule check
      // below walks the graph. On a Flow of several hundred elements only the
      // handful actually in reach reach it (FLO NFR).
      const centre = anchorCentre(element.id);
      if (!centre) continue;
      const distance = Math.hypot(centre.x - point.x, centre.y - point.y);
      if (distance > bestDistance) continue;
      if (!connect(doc, from, element.id).ok) continue;
      best = element.id;
      bestDistance = distance;
    }
    return best;
  };

  /** FLO-FR-32 / FLO-FR-33: follow the connection's free end and its snap. */
  const trackConnection = (clientX: number, clientY: number) => {
    const from = connectingRef.current;
    if (!from) return;
    const point = toGraph(clientX, clientY);
    const snapped = magnetFor(point, from);
    magnetRef.current = snapped;
    setConnectPoint(point);
    setMagnet(snapped);
  };

  /**
   * End whatever gesture is in flight. FLO-FR-21: a move lands as one edit on
   * release, and a drag that never moved the element writes nothing — so
   * clicking an element to select it stays clean.
   *
   * Idempotent by clearing the refs before acting on them, because a release is
   * seen twice: once by React on the way up, and once by the window-level
   * listener that exists for the releases React never sees.
   */
  const endGesture = ({ abandon = false } = {}) => {
    const moved = dragRef.current;
    const resized = resizeRef.current;
    // A connection released over empty canvas with nothing in reach — or
    // outside the window — is abandoned rather than refused: the user let go of
    // it, they did not ask for something the rules forbid. Clearing the refs
    // here is also what stops a forgotten connection from attaching itself to
    // the next element clicked.
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
    if (resized) {
      edit((d) => resizeLoop(d, resized.loopId, resized.size));
    }
    if (moved?.moved) commitMove(moved);
    if (!abandon && from && target) completeConnection(from, target);
  };

  /**
   * FLO-FR-42 / FLO-FR-43: land a drag.
   *
   * Which container the element joins is decided by where it was dropped — the
   * innermost loop whose bounds contain it, excluding the element and its own
   * contents, since a loop is never placed inside itself (FLO-FR-41). A move
   * that leaves a container severs the edges joining the element to what it
   * leaves behind, so the author is asked before it lands; one that costs no
   * edge moves with no question asked.
   */
  const commitMove = (moved: Drag) => {
    if (!doc) return;
    const absolute = { x: moved.x, y: moved.y };
    const width = doc.loops.find((l) => l.id === moved.elementId)?.size.width ?? NODE_W;
    const height =
      doc.loops.find((l) => l.id === moved.elementId)?.size.height ?? MEMBER_NODE_H;
    const centre = { x: absolute.x + width / 2, y: absolute.y + height / 2 };
    const newParentId = innermostLoopAt(doc, centre, moved.subtree);
    const severed = edgesSeveredByMove(doc, moved.elementId, newParentId);
    if (severed.length > 0) {
      setPendingMove({
        elementId: moved.elementId,
        name: findElement(doc, moved.elementId)?.name ?? moved.elementId,
        absolute,
        newParentId,
        severed });
      return;
    }
    edit((d) => moveElement(d, moved.elementId, absolute, newParentId));
  };

  /**
   * A release the canvas never sees still ends the gesture, and a move it never
   * sees still carries the connection. Pointer capture covers an element drag,
   * but a connection drag cannot use it — the drop target is read from what the
   * release landed on — so without this a connection dragged off the canvas
   * would freeze its preview and stay armed indefinitely.
   */
  useEffect(() => {
    if (!drag && !pan && !connectingFrom && !resize) return;
    const onMove = (e: PointerEvent) => {
      if (!connectingRef.current) return;
      // The release happened where nothing could see it. Re-arming the magnet
      // from a pointer with no button held would leave the connection live
      // across the whole window, and the user's next unrelated click anywhere
      // would complete it.
      if (e.buttons === 0) {
        endGesture({ abandon: true });
        return;
      }
      trackConnection(e.clientX, e.clientY);
    };
    const onRelease = () => endGesture();
    // A cancel is not a release — the OS or the browser took the pointer away,
    // and the user never let go over anything. Completing the connection the
    // magnet was holding would mint an edge nobody asked for (FLO-FR-32).
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
   * FLO-FR-15 – FLO-FR-18 / FLO-FR-40: complete a connection from `from` onto
   * `targetId`.
   *
   * The rules live in `connect`; every refusal is answered here, client-side and
   * immediately, so a rejected self-connection, duplicate, or cross-container
   * connection costs no backend round-trip (FLO NFR). A connection that closes a
   * cycle is not a refusal — it is how a loop or a retry is expressed
   * (FLO-FR-18).
   */
  const completeConnection = (from: string, targetId: string) => {
    if (!doc) return;
    const result = connect(doc, from, targetId);
    if (result.ok) {
      setRefusal(null);
      edit((d) => {
        const again = connect(d, from, targetId);
        return again.ok ? again.doc : d;
      });
      return;
    }
    setRefusal(
      result.refused === "self"
        ? "An element cannot be connected to itself."
        : result.refused === "duplicate"
          ? "Those elements are already connected in that direction."
          : result.refused === "cross-container"
            ? "Only elements in the same loop can be connected. Connect the loop itself instead."
            : "That element is no longer on the canvas.",
    );
  };

  /**
   * FLO-FR-12: the click-through. `"open artifact by id"` resolves the stable
   * key the single-tab rule compares (TAB-FR-04) and confirms the artifact is
   * still there; the shell then opens it in its natural surface — an Editor
   * tab, or a Flow tab for a nested Flow (LIB-FR-03).
   *
   * The routing type comes from the resolved reference rather than from the
   * `kind` hint, because the hint distinguishes only Flow from Markdown from
   * plain text while the tab's chip wants the artifact's actual type.
   */
  const onOpenReferenced = (artifact: ArtifactOption) => {
    void api
      .openArtifactById(artifact.id)
      .then((opened) =>
        onOpenArtifact?.({
          id: opened.key,
          name: artifact.displayName,
          artifactType:
            opened.kind === "flow" ? "flow" : artifact.artifactType }),
      )
      .catch(() => {
        // An artifact that has gone since the tree was read opens nothing,
        // which is exactly what an unresolved reference does.
      });
  };

  /**
   * FLO-FR-45: removing a loop that holds nothing takes it outright; one that
   * holds anything asks which the author means.
   */
  const onRemoveLoop = (loopId: string) => {
    if (!doc) return;
    if (loopHolds(doc, loopId)) {
      setPendingLoopRemoval(loopId);
      return;
    }
    edit((d) => removeLoop(d, loopId, "cascade"));
    setSelectedElement(null);
  };

  /**
   * The border geometry an edge is drawn against: see `frames.ts`. Built here,
   * on every render, from the live document and the heights the nodes report.
   */
  const { anchorCentre, anchorPair, anchorToward, edgeAnchors } = createFrames({
    doc,
    heights,
    absOf,
    sizeOf });

  /**
   * FLO-FR-25: the name is required and missing. Only ever a marking — an
   * unnamed Flow still saves, so this can never cost the author their work.
   */
  const nameMissing = !!doc && doc.name.trim() === "";

  /**
   * FLO-FR-32: the provisional edge for the connection in flight, or null when
   * none is. It ends at the snapped element's anchor when there is one and at
   * the pointer otherwise, so "release here and this is the edge you get" is
   * literally what the canvas is drawing.
   */
  const connectionPreview = (() => {
    if (!connectingFrom || !connectPoint || !doc) return null;
    if (!anchorCentre(connectingFrom)) return null;
    // Snapped, both ends are elements and land exactly where the finished edge
    // would; loose, only the source has a border to stop at and the other end
    // is the pointer itself.
    const snapped = magnet ? anchorPair(connectingFrom, magnet) : null;
    const loose = snapped ? null : anchorToward(connectingFrom, connectPoint);
    const start = snapped ? { x: snapped.ax, y: snapped.ay } : loose!;
    const end = snapped ? { x: snapped.bx, y: snapped.by } : connectPoint;
    return {
      snapped: !!snapped,
      bx: end.x,
      by: end.y,
      // The pointer is no element and has no border to be square to, so the
      // loose end simply follows the drag.
      d: edgeCurve(
        start.x,
        start.y,
        end.x,
        end.y,
        snapped ? snapped.faceA : loose!.face,
        snapped ? snapped.faceB : null,
      ) };
  })();

  // Loops render behind their members and outermost-first, so a nested loop sits
  // above the one containing it and every node sits above every loop.
  const orderedLoops = doc
    ? [...doc.loops].sort(
        (a, b) => ancestorsOf(doc, a.id).length - ancestorsOf(doc, b.id).length,
      )
    : [];

  const counts = doc
    ? [
        `${doc.nodes.length} node${doc.nodes.length === 1 ? "" : "s"}`,
        ...(doc.loops.length > 0
          ? [`${doc.loops.length} loop${doc.loops.length === 1 ? "" : "s"}`]
          : []),
        `${doc.edges.length} edge${doc.edges.length === 1 ? "" : "s"}`,
      ].join(" · ")
    : "";

  return {
    referenceable,
    treeLoaded,
    byId,
    session,
    doc,
    control,
    selected,
    selectedElement,
    setSelectedElement,
    selectedEdge,
    setSelectedEdge,
    arrowId,
    arrowActiveId,
    onMeasureNode,
    view,
    magnet,
    refusal,
    addMenuOpen,
    setAddMenuOpen,
    dismissAction,
    pendingMove,
    setPendingMove,
    pendingLoopRemoval,
    setPendingLoopRemoval,
    surfaceRef,
    addTriggerRef,
    addMenuRef,
    connectingRef,
    magnetRef,
    edit,
    absOf,
    sizeOf,
    onAddNode,
    onAddLoop,
    openAddMenu,
    closeAddMenu,
    onFit,
    zoomBy,
    onSurfacePointerDown,
    onSurfacePointerMove,
    onElementDragStart,
    onLoopResizeStart,
    endGesture,
    onOpenReferenced,
    onRemoveLoop,
    edgeAnchors,
    nameMissing,
    connectionPreview,
    orderedLoops,
    counts,
    beginConnection };

  /** FLO-FR-32: arm a connection drag from `id`. */
  function beginConnection(id: string, e: React.PointerEvent) {
    setRefusal(null);
    connectingRef.current = id;
    magnetRef.current = null;
    setConnectingFrom(id);
    setMagnet(null);
    // The preview exists from the first frame (FLO-FR-32), but nothing is
    // snapped yet: a press with no drag must not complete a connection to an
    // element merely sitting in reach.
    setConnectPoint(toGraph(e.clientX, e.clientY));
  }
}
