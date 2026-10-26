/**
 * The diagram editor overlay (`EDT-editor.md` EDT-FR-SJGJ – EDT-FR-WPGE) and
 * the host of the diagram viewer overlay (EDT-FR-KMSI – EDT-FR-EAOC).
 *
 * A modal drawn inside the window that owns the surface, over a scrim. The
 * left pane edits the source of one diagram fence in the buffer of the
 * artifact; the right pane shows the live diagram of that source; the hints
 * area under them shows the syntax of the chart type. Each window mounts one
 * host, and the host mounts an overlay only while it is open, so each opening
 * starts fresh and nothing stays after it closes (EDT-FR-SRXG).
 */
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";

import { DiagramPreview } from "./DiagramPreview";
import { DiagramViewerOverlay } from "./DiagramViewer";
import { sendHistoryKey, writeSource } from "./fence";
import {
  SPLIT_STEP,
  clampSplit,
  dragSplit,
  splitLabel,
  OPEN_SPLIT,
  MAX_SPLIT,
  MIN_SPLIT,
} from "./geometry";
import { HintsArea } from "./HintsArea";
import { indentSource, outdentSource } from "./indent";
import { OverlayFrame, TAB_INPUT_ATTR } from "./OverlayFrame";
import { diagramEditorRequest, subscribeDiagramEditor, type DiagramEditorRequest } from "./store";
import { useFence } from "./useFence";

/** EDT-FR-HVGG: the accessible names of the overlay and its parts. */
export const DIAGRAM_EDITOR_LABEL = "Edit Mermaid diagram";
export const SOURCE_LABEL = "Diagram source";
export const DIVIDER_LABEL = "Resize the panes";

/** Each window mounts one host: the main window and the Settings window. */
export function DiagramEditorHost() {
  const request = useSyncExternalStore(subscribeDiagramEditor, diagramEditorRequest);
  if (!request) return null;
  if (request.kind === "view") return <DiagramViewerOverlay key={request.id} request={request} />;
  return <DiagramEditorOverlay key={request.id} request={request} />;
}

function DiagramEditorOverlay({ request }: { request: DiagramEditorRequest }) {
  const { view } = request;
  const sourceRef = useRef<HTMLTextAreaElement>(null);
  const { place, source, setSource, finish } = useFence(
    request,
    () => sourceRef.current?.value ?? null,
  );
  const [split, setSplit] = useState(OPEN_SPLIT);
  /** EDT-FR-SPAG: where on the divider the tooltip shows, while a drag lasts. */
  const [dragTipTop, setDragTipTop] = useState<number | null>(null);
  const panesRef = useRef<HTMLDivElement>(null);
  /** EDT-FR-UYBZ: the selection that an indent puts back after the render. */
  const pendingSelection = useRef<{ start: number; end: number } | null>(null);

  useLayoutEffect(() => {
    const el = sourceRef.current;
    const pending = pendingSelection.current;
    if (!el || !pending) return;
    pendingSelection.current = null;
    el.setSelectionRange(pending.start, pending.end);
  }, [source]);

  // EDT-FR-HGAZ: focus moves into the source pane, with the caret in it.
  useEffect(() => {
    const el = sourceRef.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(0, 0);
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
    const at = place.current;
    if (!at) return;
    setSource(next);
    if (!writeSource(view, at.pos, next)) finish("write-failed");
  };

  /**
   * EDT-FR-UYBZ: Tab indents and Shift+Tab outdents the source. Ctrl+Tab is
   * left to the overlay, which moves focus with it (EDT-FR-CFCQ).
   */
  const onSourceTab = (e: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    e.preventDefault();
    if (!place.current) return;
    const el = e.currentTarget;
    const edit = (e.shiftKey ? outdentSource : indentSource)(
      el.value,
      el.selectionStart,
      el.selectionEnd,
    );
    if (edit.value === el.value) return;
    pendingSelection.current = { start: edit.start, end: edit.end };
    onSourceChange(edit.value);
  };

  /** EDT-FR-HBEB: the undo and redo keys of the surface. */
  const onSourceKeyDown = (e: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    // A key that ends an IME composition is the composition's, not an indent.
    // WebKit gives it the key code 229 and no `isComposing`.
    const composing = e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229;
    if (e.key === "Tab" && !e.ctrlKey && !e.metaKey && !e.altKey && !composing) {
      onSourceTab(e);
      return;
    }
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

  /** EDT-FR-ULPU: the divider dragged by the pointer. */
  const onDividerPointerDown = (e: ReactPointerEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    const panes = panesRef.current;
    if (!panes) return;
    e.preventDefault();
    const target = e.currentTarget;
    target.setPointerCapture?.(e.pointerId);
    const place = (ev: { clientX: number; clientY: number }) => {
      const box = panes.getBoundingClientRect();
      if (box.width <= 0) return;
      // EDT-FR-UPYC: the middle holds the divider while the pointer is near it.
      setSplit(dragSplit((ev.clientX - box.left) / box.width));
      // EDT-FR-SPAG: the tooltip follows the pointer along the divider.
      setDragTipTop(Math.min(Math.max(ev.clientY - box.top, 0), box.height));
    };
    place(e);
    const onMove = (ev: PointerEvent) => place(ev);
    const onUp = () => {
      setDragTipTop(null);
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
    <OverlayFrame
      label={DIAGRAM_EDITOR_LABEL}
      className="diagram-editor"
      scrimTestId="diagram-editor-scrim"
      home={sourceRef}
      // EDT-FR-HGAZ: a press in the preview pane or in the hints area keeps
      // its default, so the author can select and copy the text there.
      selectable=".diagram-editor__preview, .diagram-hints"
      onClose={finish}
    >
      {/* EDT-FR-NKGX: the hints area takes the bottom 20% of the whole overlay,
          and the title bar and the panes the rest. */}
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
          {...{ [TAB_INPUT_ATTR]: "" }}
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
          aria-valuetext={splitLabel(split)}
          data-dragging={dragTipTop !== null}
          tabIndex={0}
          onPointerDown={onDividerPointerDown}
          onKeyDown={onDividerKeyDown}
        >
          {dragTipTop !== null && (
            // EDT-FR-SPAG: the split while the drag lasts. The separator gives
            // the same text to assistive technology through its value.
            <span
              className="diagram-editor__split-tip"
              data-testid="diagram-split-tip"
              aria-hidden="true"
              style={{ top: dragTipTop }}
            >
              {splitLabel(split)}
            </span>
          )}
        </div>
        <DiagramPreview source={source} />
      </div>
      <HintsArea source={source} />
    </OverlayFrame>
  );
}
