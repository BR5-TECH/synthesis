/**
 * The frame that the diagram editor overlay (EDT-FR-SJGJ) and the diagram
 * viewer overlay (EDT-FR-KMSI) share: the scrim, the modal dialog, its title
 * bar, its geometry, and its focus trap. Only the content differs.
 */
import {
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";

import { Icon } from "../icons";
import { isForwardedKey } from "./fence";
import {
  EDGES,
  clampRect,
  moveRect,
  openRect,
  resizeRect,
  type Edge,
  type Rect,
} from "./geometry";

/** The parts of an overlay that take focus with Tab (EDT-FR-HGAZ, EDT-FR-ELUJ). */
export const FOCUSABLE =
  "textarea, button:not([disabled]), [role='separator'][tabindex], [tabindex='0']";

const windowBounds = () => ({ width: window.innerWidth, height: window.innerHeight });

export interface OverlayFrameProps {
  /** The accessible name of the dialog, and its title. */
  label: string;
  /** The class of the dialog, after `modal`. */
  className: string;
  /** The test id of the scrim. */
  scrimTestId: string;
  /** Where focus goes when a key reaches the window outside the overlay. */
  home: RefObject<HTMLElement | null>;
  /**
   * The parts of the overlay where a press keeps its default, so the author
   * can select text there. Elsewhere a press on a part that takes no focus
   * keeps the focus where it is.
   */
  selectable?: string;
  /** Controls of the title bar that come before the Close control. */
  controls?: ReactNode;
  /** The Close control or Escape closes the overlay (EDT-FR-SRXG, EDT-FR-ELUJ). */
  onClose: (reason: "close" | "escape") => void;
  /** A key pressed inside the overlay, before the window could see it. */
  onKeyDown?: (e: ReactKeyboardEvent<HTMLElement>) => void;
  children: ReactNode;
}

export function OverlayFrame({
  label,
  className,
  scrimTestId,
  home,
  selectable,
  controls,
  onClose,
  onKeyDown,
  children,
}: OverlayFrameProps) {
  const [rect, setRect] = useState<Rect>(() => openRect(windowBounds()));
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

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
        home.current?.focus();
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopImmediatePropagation();
        closeRef.current("escape");
        return;
      }
      if (e.key !== "Tab") return;
      if (!dialog) return;
      const items = Array.from(dialog.querySelectorAll<HTMLElement>(FOCUSABLE));
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      // The dialog itself holds the focus after a press on a part that takes
      // no focus.
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
  }, [home]);

  // EDT-FR-NKGX: the overlay stays inside the window when the window shrinks.
  useEffect(() => {
    const onResize = () => setRect((r) => clampRect(r, windowBounds()));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

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

  return (
    <div
      className="scrim diagram-editor-scrim"
      data-testid={scrimTestId}
      // EDT-FR-SJGJ: a click on the scrim does nothing, and focus stays inside.
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) e.preventDefault();
      }}
    >
      <div
        ref={dialogRef}
        className={`modal diagram-overlay ${className}`}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        // EDT-FR-HGAZ: a press on a part that takes no focus puts the focus on
        // the dialog, so the focus stays inside the overlay.
        tabIndex={-1}
        style={{
          left: `${rect.left}px`,
          top: `${rect.top}px`,
          width: `${rect.width}px`,
          height: `${rect.height}px`,
        }}
        // EDT-FR-SJGJ: the window takes no keyboard interaction outside the
        // overlay, so a key pressed here reaches no shortcut of the window.
        onKeyDown={(e) => {
          onKeyDown?.(e);
          e.stopPropagation();
        }}
        onMouseDown={(e) => {
          const target = e.target as Element;
          if (selectable && target.closest(selectable)) return;
          if (!target.closest(FOCUSABLE)) e.preventDefault();
        }}
      >
        <div
          className={`modal__head diagram-overlay__head ${className}__head`}
          onPointerDown={onTitlePointerDown}
        >
          <div className="modal__title">{label}</div>
          <div className="diagram-overlay__controls">
            {controls}
            <button
              type="button"
              className="btn btn--ghost btn--icon btn--sm"
              aria-label="Close"
              title="Close"
              onClick={() => onClose("close")}
            >
              <Icon.X size={12} />
            </button>
          </div>
        </div>
        {children}
        {EDGES.map((edge) => (
          <div
            key={edge}
            className={`diagram-overlay__edge diagram-overlay__edge--${edge}`}
            data-edge={edge}
            aria-hidden="true"
            onPointerDown={onEdgePointerDown(edge)}
          />
        ))}
      </div>
    </div>
  );
}
