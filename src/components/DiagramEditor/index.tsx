/**
 * The diagram editor overlay (`EDT-editor.md` EDT-FR-SJGJ – EDT-FR-WPGE).
 *
 * A modal drawn inside the window that owns the surface, over a scrim. The
 * left pane edits the source of one diagram fence in the buffer of the
 * artifact; the right pane shows the live diagram of that source. Each window
 * mounts one host, and the host mounts the overlay only while it is open, so
 * each opening starts fresh and nothing stays after it closes (EDT-FR-SRXG).
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";

import { logDebug } from "../../logging";
import { Icon } from "../icons";
import { DiagramPreview } from "./DiagramPreview";
import {
  isForwardedKey,
  placeAt,
  relocate,
  sendHistoryKey,
  sourceAt,
  surfaceHidden,
  writeSource,
  type FencePlace,
} from "./fence";
import {
  EDGES,
  SPLIT_STEP,
  clampRect,
  clampSplit,
  moveRect,
  openRect,
  resizeRect,
  OPEN_SPLIT,
  MAX_SPLIT,
  MIN_SPLIT,
  type Edge,
  type Rect,
} from "./geometry";
import {
  closeDiagramEditor,
  diagramEditorRequest,
  returnFocusToFence,
  subscribeDiagramEditor,
  watchDiagramSurface,
  type CloseReason,
  type DiagramEditorRequest,
} from "./store";

/** EDT-FR-HVGG: the accessible names of the overlay and its parts. */
export const DIAGRAM_EDITOR_LABEL = "Edit Mermaid diagram";
export const SOURCE_LABEL = "Diagram source";
export const DIVIDER_LABEL = "Resize the panes";

const FOCUSABLE = "textarea, button:not([disabled]), [role='separator'][tabindex]";

/** Each window mounts one host: the main window and the Settings window. */
export function DiagramEditorHost() {
  const request = useSyncExternalStore(subscribeDiagramEditor, diagramEditorRequest);
  if (!request) return null;
  return <DiagramEditorOverlay key={request.id} request={request} />;
}

const windowBounds = () => ({ width: window.innerWidth, height: window.innerHeight });

function DiagramEditorOverlay({ request }: { request: DiagramEditorRequest }) {
  const { view } = request;
  const placeRef = useRef<FencePlace | null>(null);
  const placed = useRef(false);
  if (!placed.current) {
    placed.current = true;
    placeRef.current = placeAt(view.state.doc, request.fence);
  }
  const [source, setSource] = useState(() => sourceAt(view.state.doc, request.fence));
  const [rect, setRect] = useState<Rect>(() => openRect(windowBounds()));
  const [split, setSplit] = useState(OPEN_SPLIT);
  const dialogRef = useRef<HTMLDivElement>(null);
  const sourceRef = useRef<HTMLTextAreaElement>(null);
  const panesRef = useRef<HTMLDivElement>(null);

  /**
   * Close from inside the overlay, and put focus where the requirement says:
   * the Edit button of the fence (EDT-FR-HGAZ), or the surface when the fence
   * is gone (EDT-FR-SHYX). An inert surface keeps focus where its own modal
   * puts it (EDT-FR-WPGE).
   */
  const finish = useCallback(
    (reason: CloseReason) => {
      const place = placeRef.current;
      if (reason === "close" || reason === "escape") {
        const dom = place ? view.nodeDOM(place.pos) : null;
        const button =
          dom instanceof HTMLElement ? dom.querySelector<HTMLElement>(".mermaid-edit") : null;
        (button ?? request.opener)?.focus();
        // The fence draws its button again when its render settles, so the
        // new button takes the focus too (EDT-FR-HGAZ).
        if (place) returnFocusToFence(view, place.pos);
      } else if ((reason === "fence-removed" || reason === "write-failed") && !view.isDestroyed) {
        view.focus();
      }
      closeDiagramEditor(reason);
    },
    [view, request.opener],
  );

  // EDT-FR-SHYX, EDT-FR-WPGE: follow the fence through each new state of the
  // surface. A fence that is gone, or a surface that takes no input, closes
  // the overlay.
  useEffect(() => {
    if (!placeRef.current) {
      logDebug(["frontend"], "the diagram editor found no fence to edit");
      finish("fence-removed");
      return;
    }
    const follow = (change: "update" | "destroy") => {
      if (change === "destroy") {
        closeDiagramEditor("surface-ended");
        return;
      }
      if (!view.editable) {
        finish("surface-inert");
        return;
      }
      const last = placeRef.current;
      const next = last ? relocate(view.state.doc, last, sourceRef.current?.value ?? "") : null;
      if (!next) {
        finish("fence-removed");
        return;
      }
      placeRef.current = next;
      setSource(sourceAt(view.state.doc, next.pos));
    };
    return watchDiagramSurface(view, follow);
  }, [view, finish]);

  // The rich surface of an Editor tab is hidden while the tab shows its raw
  // text, and an undo can switch the tab there. A fence the author cannot see
  // is a fence the overlay no longer edits.
  useEffect(() => {
    if (typeof MutationObserver === "undefined") return;
    const observer = new MutationObserver(() => {
      if (surfaceHidden(view)) finish("surface-hidden");
    });
    observer.observe(document.body, {
      subtree: true,
      attributes: true,
      attributeFilter: ["style", "hidden"],
    });
    return () => observer.disconnect();
  }, [view, finish]);

  // EDT-FR-HGAZ: focus moves into the source pane, with the caret in it.
  useEffect(() => {
    const el = sourceRef.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(0, 0);
  }, []);

  // EDT-FR-SRXG: Escape closes the overlay. EDT-FR-HGAZ: Tab and Shift+Tab stay
  // inside it. Captured ahead of the surfaces beneath, which must not react to
  // a key that the overlay answers.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const dialog = dialogRef.current;
      // EDT-FR-SJGJ: a key that reaches the window while focus is outside the
      // overlay goes to no surface and no shortcut. Focus goes back inside.
      if (
        dialog &&
        !isForwardedKey(e) &&
        !(e.target instanceof Node && dialog.contains(e.target))
      ) {
        e.preventDefault();
        e.stopImmediatePropagation();
        sourceRef.current?.focus();
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        finish("escape");
        return;
      }
      if (e.key !== "Tab") return;
      if (!dialog) return;
      const items = Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      // The dialog itself holds the focus after a press in the preview pane.
      if (!(active instanceof Node) || !dialog.contains(active) || active === dialog) {
        e.preventDefault();
        (e.shiftKey ? last : first).focus();
      } else if (e.shiftKey && active === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [finish]);

  // EDT-FR-NKGX: the overlay stays inside the window when the window shrinks.
  useEffect(() => {
    const onResize = () => setRect((r) => clampRect(r, windowBounds()));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  // EDT-FR-HBEB: undo and redo from the native Edit menu reach the textarea as
  // a `beforeinput`. They go to the one history of the artifact, never to the
  // own history of the textarea.
  useEffect(() => {
    const el = sourceRef.current;
    if (!el) return;
    const onBeforeInput = (e: Event) => {
      const type = (e as InputEvent).inputType;
      if (type !== "historyUndo" && type !== "historyRedo") return;
      e.preventDefault();
      e.stopPropagation();
      sendHistoryKey(view, type === "historyRedo" ? "redo" : "undo", {
        metaKey: false,
        ctrlKey: true,
      });
    };
    el.addEventListener("beforeinput", onBeforeInput, true);
    return () => el.removeEventListener("beforeinput", onBeforeInput, true);
  }, [view]);

  /** EDT-FR-EBEZ: an edit in the source pane is an edit of the buffer. */
  const onSourceChange = (next: string) => {
    const place = placeRef.current;
    if (!place) return;
    setSource(next);
    if (!writeSource(view, place.pos, next)) finish("write-failed");
  };

  /** EDT-FR-HBEB: the undo and redo keys of the surface. */
  const onSourceKeyDown = (e: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if (!e.metaKey && !e.ctrlKey) return;
    const key = e.key.toLowerCase();
    const isRedo = key === "y" || (key === "z" && e.shiftKey);
    const isUndo = key === "z" && !e.shiftKey;
    if (!isUndo && !isRedo) return;
    e.preventDefault();
    const caret = e.currentTarget.selectionStart;
    sendHistoryKey(view, isRedo ? "redo" : "undo", e);
    // The restored source replaces the text; the caret stays near where it was.
    const el = e.currentTarget;
    requestAnimationFrame(() => {
      if (!el.isConnected) return;
      const at = Math.min(caret, el.value.length);
      el.setSelectionRange(at, at);
    });
  };

  /** EDT-FR-NKGX: a pointer drag that moves or resizes the overlay. */
  const drag = (
    e: ReactPointerEvent<HTMLElement>,
    apply: (start: Rect, dx: number, dy: number) => Rect,
  ) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const target = e.currentTarget;
    const startX = e.clientX;
    const startY = e.clientY;
    const start = rect;
    target.setPointerCapture?.(e.pointerId);
    const onMove = (ev: PointerEvent) =>
      setRect(apply(start, ev.clientX - startX, ev.clientY - startY));
    const onUp = () => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
  };

  const onTitlePointerDown = (e: ReactPointerEvent<HTMLElement>) => {
    if ((e.target as Element).closest("button")) return;
    drag(e, (start, dx, dy) => moveRect(start, dx, dy, windowBounds()));
  };

  const onEdgePointerDown = (edge: Edge) => (e: ReactPointerEvent<HTMLElement>) =>
    drag(e, (start, dx, dy) => resizeRect(start, edge, dx, dy, windowBounds()));

  /** EDT-FR-ULPU: the divider dragged by the pointer. */
  const onDividerPointerDown = (e: ReactPointerEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    const panes = panesRef.current;
    if (!panes) return;
    e.preventDefault();
    const target = e.currentTarget;
    target.setPointerCapture?.(e.pointerId);
    const onMove = (ev: PointerEvent) => {
      const box = panes.getBoundingClientRect();
      if (box.width <= 0) return;
      setSplit(clampSplit((ev.clientX - box.left) / box.width));
    };
    const onUp = () => {
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
  };

  /** EDT-FR-HVGG: the arrow keys move the divider. */
  const onDividerKeyDown = (e: ReactKeyboardEvent<HTMLElement>) => {
    const step =
      e.key === "ArrowLeft" ? -SPLIT_STEP : e.key === "ArrowRight" ? SPLIT_STEP : 0;
    if (e.key === "Home") setSplit(MIN_SPLIT);
    else if (e.key === "End") setSplit(MAX_SPLIT);
    else if (step !== 0) setSplit((s) => clampSplit(s + step));
    else return;
    e.preventDefault();
  };

  return (
    <div
      className="scrim diagram-editor-scrim"
      data-testid="diagram-editor-scrim"
      // EDT-FR-SJGJ: a click on the scrim does nothing, and focus stays inside.
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) e.preventDefault();
      }}
    >
      <div
        ref={dialogRef}
        className="modal diagram-editor"
        role="dialog"
        aria-modal="true"
        aria-label={DIAGRAM_EDITOR_LABEL}
        // EDT-FR-HGAZ: a press in the preview pane puts the focus on the
        // dialog, so the focus stays inside the overlay.
        tabIndex={-1}
        style={{
          left: `${rect.left}px`,
          top: `${rect.top}px`,
          width: `${rect.width}px`,
          height: `${rect.height}px`,
        }}
        // EDT-FR-SJGJ: the window takes no keyboard interaction outside the
        // overlay, so a key pressed here reaches no shortcut of the window.
        onKeyDown={(e) => e.stopPropagation()}
        // EDT-FR-HGAZ: a press on a part that takes no focus keeps the focus
        // where it is, inside the overlay. A press in the preview pane keeps
        // its default, so the author can select and copy the error text.
        onMouseDown={(e) => {
          const target = e.target as Element;
          if (target.closest(".diagram-editor__preview")) return;
          if (!target.closest(FOCUSABLE)) e.preventDefault();
        }}
      >
        <div className="modal__head diagram-editor__head" onPointerDown={onTitlePointerDown}>
          <div className="modal__title">{DIAGRAM_EDITOR_LABEL}</div>
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Close"
            title="Close"
            onClick={() => finish("close")}
          >
            <Icon.X size={12} />
          </button>
        </div>
        <div
          ref={panesRef}
          className="diagram-editor__panes"
          style={{ gridTemplateColumns: `${split}fr auto ${1 - split}fr` }}
        >
          <textarea
            ref={sourceRef}
            className="diagram-editor__source"
            aria-label={SOURCE_LABEL}
            spellCheck={false}
            autoCapitalize="off"
            autoCorrect="off"
            value={source}
            onChange={(e) => onSourceChange(e.target.value)}
            onKeyDown={onSourceKeyDown}
          />
          <div
            className="diagram-editor__divider"
            role="separator"
            aria-orientation="vertical"
            aria-label={DIVIDER_LABEL}
            aria-valuemin={Math.round(MIN_SPLIT * 100)}
            aria-valuemax={Math.round(MAX_SPLIT * 100)}
            aria-valuenow={Math.round(split * 100)}
            tabIndex={0}
            onPointerDown={onDividerPointerDown}
            onKeyDown={onDividerKeyDown}
          />
          <DiagramPreview source={source} />
        </div>
        {EDGES.map((edge) => (
          <div
            key={edge}
            className={`diagram-editor__edge diagram-editor__edge--${edge}`}
            data-edge={edge}
            aria-hidden="true"
            onPointerDown={onEdgePointerDown(edge)}
          />
        ))}
      </div>
    </div>
  );
}
