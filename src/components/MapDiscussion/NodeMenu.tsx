/**
 * The context menu of a node of the Map that holds the **Discuss** entry
 * (`MDL-map-delivery-lens.md` MDL-FR-HULA .. MDL-FR-JBEL,
 * `SMN-specification-map-nodes.md` SMN-FR-JHQR, SMN-FR-BSZV).
 *
 * One hook serves the nodes of both lenses. It gives a node the two handlers
 * that open the menu (the secondary click, and the context-menu key or
 * `Shift+F10`) and draws the menu where the lens puts it. The menu stands
 * outside the transformed canvas, so its fixed position is the window's.
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
  type ReactNode,
} from "react";

import { logDebug } from "../../logging";
import { Icon } from "../icons";

/** The two handlers a node takes to open its menu. */
export interface MenuBindings {
  onContextMenu: (e: MouseEvent<HTMLElement>) => void;
  onKeyDown: (e: KeyboardEvent<HTMLElement>) => void;
}

interface Opening<T> {
  target: T;
  x: number;
  y: number;
  /** MDL-FR-JBEL: the element that gets the focus back when the menu closes. */
  returnTo: HTMLElement;
}

interface NodeMenuProps {
  x: number;
  y: number;
  onDiscuss: () => void;
  onClose: (restoreFocus: boolean) => void;
}

/** LCM-FR-07: the entry carries a leading icon. */
export function NodeMenu({ x, y, onDiscuss, onClose }: NodeMenuProps) {
  const frame = useRef<HTMLDivElement | null>(null);
  const first = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    first.current?.focus();
    const onPress = (e: Event) => {
      const target = e.target as Node | null;
      if (target && frame.current?.contains(target)) return;
      onClose(false);
    };
    window.addEventListener("mousedown", onPress);
    return () => window.removeEventListener("mousedown", onPress);
  }, [onClose]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const entries = [...(frame.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
    const at = entries.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose(true);
    } else if (e.key === "Tab") {
      onClose(false);
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Home" || e.key === "End") {
      e.preventDefault();
      if (entries.length === 0) return;
      const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
      const next =
        e.key === "Home" ? 0 : e.key === "End" ? entries.length - 1 : (at + step + entries.length) % entries.length;
      entries[next]?.focus();
    }
  };

  return (
    <div
      ref={frame}
      className="menu map-node-menu"
      role="menu"
      aria-label="Node actions"
      style={{ position: "fixed", left: x, top: y, zIndex: 200 }}
      onKeyDown={onKeyDown}
    >
      <button ref={first} type="button" role="menuitem" className="menu-item" onClick={onDiscuss}>
        <Icon.Comment size={13} /> Discuss
      </button>
    </div>
  );
}

/**
 * The menu of a lens. `bindings(target)` gives the handlers of one node, and
 * `menu` is what the lens draws once, outside its canvas. With `blocked` true
 * (an edit is in progress on the canvas) a request opens no menu
 * (MDL-FR-JBEL).
 */
export function useDiscussMenu<T>(
  onDiscuss: (target: T) => void,
  options: { blocked?: boolean; onOpening?: () => void } = {},
): { bindings: (target: T | null) => MenuBindings | undefined; menu: ReactNode } {
  const [opening, setOpening] = useState<Opening<T> | null>(null);
  const latest = useRef({ onDiscuss, ...options });
  latest.current = { onDiscuss, ...options };

  const open = useCallback((target: T, from: HTMLElement, at?: { x: number; y: number }) => {
    if (latest.current.blocked) return;
    latest.current.onOpening?.();
    const rect = from.getBoundingClientRect();
    logDebug(["frontend"], "map node menu opened", { by: at ? "pointer" : "keyboard" });
    setOpening({ target, x: at?.x ?? rect.left, y: at?.y ?? rect.bottom, returnTo: from });
  }, []);

  const close = useCallback(
    (restoreFocus: boolean) => {
      setOpening((held) => {
        if (held && restoreFocus) held.returnTo.focus();
        return null;
      });
    },
    [],
  );

  const bindings = useCallback(
    (target: T | null): MenuBindings | undefined => {
      if (target === null) return undefined;
      return {
        onContextMenu: (e) => {
          e.preventDefault();
          e.stopPropagation();
          open(target, e.currentTarget, { x: e.clientX, y: e.clientY });
        },
        onKeyDown: (e) => {
          // A key typed in a field inside the node is the field's.
          if (e.target !== e.currentTarget) return;
          if (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) {
            e.preventDefault();
            e.stopPropagation();
            open(target, e.currentTarget);
          }
        },
      };
    },
    [open],
  );

  const menu = opening ? (
    <NodeMenu
      x={opening.x}
      y={opening.y}
      onClose={close}
      onDiscuss={() => {
        const { target } = opening;
        setOpening(null);
        latest.current.onDiscuss(target);
      }}
    />
  ) : null;

  return { bindings, menu };
}
