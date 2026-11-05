/**
 * The diagram viewer overlay (`EDT-editor.md` EDT-FR-KMSI – EDT-FR-EAOC).
 *
 * The frame, the geometry, and the focus trap are the ones of the diagram
 * editor overlay (`OverlayFrame`). The content is the diagram of the fence
 * alone, which the author zooms and pans. Zoom and pan are view state of this
 * opening: they change no byte of the buffer (EDT-FR-EAOC) and the next
 * opening starts fitted again (EDT-FR-JJDP).
 */
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";

import { logDebug, logWarn } from "../../logging";
import { Icon } from "../icons";
import { diagramElement } from "../markdownMermaidRender";
import { OverlayFrame } from "./OverlayFrame";
import type { DiagramEditorRequest } from "./store";
import { useDiagramRender, type RenderedDiagram } from "./useDiagramRender";
import { useFence } from "./useFence";
import {
  PAN_STEP,
  ZOOM_STEP,
  fitTransform,
  naturalSize,
  panBy,
  wheelFactor,
  wheelPan,
  zoomAt,
  type Size,
  type ViewTransform,
} from "./viewerGeometry";

/** EDT-FR-CCZU: the accessible names of the viewer and its controls. */
export const DIAGRAM_VIEWER_LABEL = "View Mermaid diagram";
export const FIT_LABEL = "Fit to window";
/** EDT-FR-VWNJ: the accessible name of the diagram area. */
export const VIEWER_DIAGRAM_LABEL = "Mermaid diagram";

const sizeOf = (el: HTMLElement | null): Size =>
  el ? { width: el.clientWidth, height: el.clientHeight } : { width: 0, height: 0 };

const sameSize = (a: Size | null, b: Size | null) =>
  a === b || (a !== null && b !== null && a.width === b.width && a.height === b.height);

/** EDT-FR-LJAP: the fit of the diagram, and a log of its result. Numbers only. */
function fitAndLog(area: Size, natural: Size, reason: string): ViewTransform {
  const t = fitTransform(area, natural);
  logDebug(["frontend"], "the diagram viewer fits the diagram", {
    reason,
    areaWidth: area.width,
    areaHeight: area.height,
    diagramWidth: natural.width,
    diagramHeight: natural.height,
    scale: t.scale,
    whole: natural.width * t.scale <= area.width && natural.height * t.scale <= area.height,
  });
  return t;
}

export function DiagramViewerOverlay({ request }: { request: DiagramEditorRequest }) {
  const areaRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const shownSource = useRef<string | null>(null);
  const { source, finish } = useFence(request, () => shownSource.current);
  shownSource.current = source;
  const { empty, settled, lastRendered } = useDiagramRender(source);

  // EDT-FR-JRDD: a source that stops being a valid diagram closes the viewer.
  // A render in progress is not an error: the viewer keeps what it shows.
  const invalid = empty || settled?.state === "error";
  useEffect(() => {
    if (!invalid) return;
    logDebug(["frontend"], "the diagram viewer has no valid diagram", { empty });
    finish("not-a-diagram");
  }, [invalid, empty, finish]);

  // EDT-FR-LVFQ: the diagram that shows. While a changed source renders, the
  // last diagram of this opening stays.
  const shown: RenderedDiagram | null =
    settled?.state === "rendered" ? settled : invalid ? null : lastRendered;

  const [area, setArea] = useState<Size>({ width: 0, height: 0 });
  const [natural, setNatural] = useState<Size | null>(null);
  const [transform, setTransform] = useState<ViewTransform>({ scale: 1, x: 0, y: 0 });
  /** EDT-FR-JJDP: the diagram stays fitted until the author zooms or pans. */
  const [fitted, setFitted] = useState(true);

  // EDT-FR-VWNJ: the drawing of the fence, with its natural size, so the
  // transform alone sets the size on screen.
  useLayoutEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const node = shown ? diagramElement(shown.svg, shown.id) : null;
    let size: Size | null = null;
    if (node) {
      size = naturalSize(node);
      if (size) {
        node.setAttribute("width", String(size.width));
        node.setAttribute("height", String(size.height));
      }
      (node as SVGElement).style.removeProperty("max-width");
      if (!size) logWarn(["frontend"], "the diagram viewer cannot find the size of the diagram");
    }
    stage.replaceChildren(...(node ? [node] : []));
    setNatural((last) => (sameSize(last, size) ? last : size));
  }, [shown]);

  // EDT-FR-JJDP: the size of the diagram area, again at each change.
  useLayoutEffect(() => {
    const el = areaRef.current;
    if (!el) return;
    const measure = () =>
      setArea((last) => {
        const next = sizeOf(el);
        return sameSize(last, next) ? last : next;
      });
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // EDT-FR-LJAP, EDT-FR-JJDP: while fitted, a new size or a new render fits
  // the diagram again.
  // A new drawing is logged once; a change of size is not, so that a resize
  // by drag does not fill the log.
  const loggedFit = useRef<Size | null>(null);
  useLayoutEffect(() => {
    if (!fitted || !natural) return;
    if (loggedFit.current !== natural && area.width > 0 && area.height > 0) {
      loggedFit.current = natural;
      setTransform(fitAndLog(area, natural, "render"));
      return;
    }
    setTransform(fitTransform(area, natural));
  }, [fitted, area, natural, shown]);

  // EDT-FR-ELUJ: focus moves into the diagram area.
  useEffect(() => {
    areaRef.current?.focus();
  }, []);

  /** EDT-FR-LJAP: the Fit control and the key `0`. */
  const fit = useCallback(() => {
    setFitted(true);
    if (natural) setTransform(fitAndLog(sizeOf(areaRef.current), natural, "fit control"));
  }, [natural]);

  /** EDT-FR-MZIF, EDT-FR-PDBC: a zoom or a pan ends the fit. */
  const change = useCallback((next: (t: ViewTransform) => ViewTransform) => {
    setFitted(false);
    setTransform(next);
  }, []);

  // EDT-FR-MZIF, EDT-FR-PDBC: a pinch, or the wheel with Ctrl or Cmd held,
  // zooms about the pointer. The wheel alone pans. A native listener, because
  // the listener that React adds for the wheel cannot stop the page scroll.
  useEffect(() => {
    const el = areaRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      if (e.ctrlKey || e.metaKey) {
        // An event with no vertical delta does not zoom, so it keeps the fit.
        if (e.deltaY === 0) return;
        const box = el.getBoundingClientRect();
        const factor = wheelFactor(e.deltaY, e.deltaMode, e.ctrlKey);
        change((t) => zoomAt(t, factor, e.clientX - box.left, e.clientY - box.top));
        return;
      }
      const { dx, dy } = wheelPan(e.deltaX, e.deltaY, e.deltaMode, e.shiftKey);
      if (dx !== 0 || dy !== 0) change((t) => panBy(t, dx, dy));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [change]);

  /** EDT-FR-MZIF, EDT-FR-PDBC, EDT-FR-LJAP: the keys of the viewer. */
  const onKeyDown = (e: ReactKeyboardEvent<HTMLElement>) => {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const centre = sizeOf(areaRef.current);
    const zoom = (factor: number) =>
      change((t) => zoomAt(t, factor, centre.width / 2, centre.height / 2));
    switch (e.key) {
      case "+":
      case "=":
        zoom(ZOOM_STEP);
        break;
      case "-":
      case "_":
        zoom(1 / ZOOM_STEP);
        break;
      case "0":
        fit();
        break;
      // The diagram moves in the direction of the key.
      case "ArrowLeft":
        change((t) => panBy(t, -PAN_STEP, 0));
        break;
      case "ArrowRight":
        change((t) => panBy(t, PAN_STEP, 0));
        break;
      case "ArrowUp":
        change((t) => panBy(t, 0, -PAN_STEP));
        break;
      case "ArrowDown":
        change((t) => panBy(t, 0, PAN_STEP));
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  /** EDT-FR-PDBC: a drag with the primary button pans the diagram. */
  const [dragging, setDragging] = useState(false);
  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const target = e.currentTarget;
    target.focus();
    target.setPointerCapture?.(e.pointerId);
    let lastX = e.clientX;
    let lastY = e.clientY;
    setDragging(true);
    const onMove = (ev: PointerEvent) => {
      const dx = ev.clientX - lastX;
      const dy = ev.clientY - lastY;
      lastX = ev.clientX;
      lastY = ev.clientY;
      if (dx !== 0 || dy !== 0) change((t) => panBy(t, dx, dy));
    };
    const onUp = () => {
      setDragging(false);
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
  };

  return (
    <OverlayFrame
      label={DIAGRAM_VIEWER_LABEL}
      className="diagram-viewer"
      scrimTestId="diagram-viewer-scrim"
      home={areaRef}
      onClose={finish}
      onKeyDown={onKeyDown}
      controls={
        <>
          <span className="diagram-viewer__zoom" data-testid="diagram-viewer-zoom">
            {Math.round(transform.scale * 100)}%
          </span>
          <button
            type="button"
            className="btn btn--ghost btn--sm diagram-viewer__fit"
            aria-label={FIT_LABEL}
            title={FIT_LABEL}
            onClick={fit}
          >
            <Icon.Fit size={12} />
            <span>Fit</span>
          </button>
        </>
      }
    >
      <div
        ref={areaRef}
        className={`diagram-viewer__area doc${dragging ? " diagram-viewer__area--dragging" : ""}`}
        role="img"
        aria-label={VIEWER_DIAGRAM_LABEL}
        tabIndex={0}
        data-fitted={fitted ? "true" : undefined}
        onPointerDown={onPointerDown}
        // EDT-FR-JBZU: a link in a diagram does nothing here either.
        onClick={(event) => event.preventDefault()}
      >
        <div
          ref={stageRef}
          className="mermaid-diagram diagram-viewer__stage"
          data-testid="diagram-viewer-stage"
          style={{
            transform: `translate(${transform.x}px, ${transform.y}px) scale(${transform.scale})`,
          }}
        />
      </div>
    </OverlayFrame>
  );
}
