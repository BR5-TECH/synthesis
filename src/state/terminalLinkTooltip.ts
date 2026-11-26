/**
 * The tooltip of a terminal link (`../../specifications/ui/TRM-terminals.md`
 * TRM-FR-KIRO, TRM-FR-DCBR).
 *
 * The library gives a link a `hover` call and no position, so the tooltip
 * follows the last pointer position over the terminal. The tooltip is a child
 * of `document.body` with a fixed position: the terminal host is the editable
 * element of the library, and a child of it could receive the input. It takes
 * no focus and no pointer events, so it is not a floating overlay.
 */
import { isMacPlatform } from "../platform";

/** The space between the pointer and the tooltip, and between the tooltip and the window edge. */
const POINTER_GAP_PX = 14;
const EDGE_GAP_PX = 4;

export interface LinkTooltip {
  /** TRM-FR-KIRO: shows the tooltip of `url` at the pointer. */
  show(url: string): void;
  /** TRM-FR-DCBR: hides the tooltip. It is safe to call when no tooltip shows. */
  hide(): void;
  /** Hides the tooltip and removes it and its listeners. */
  dispose(): void;
}

/** TRM-FR-KIRO: the hint of the click that opens a link. */
export function linkHint(mac: boolean = isMacPlatform()): string {
  return mac ? "⌘-click to open" : "Ctrl+click to open";
}

/** TRM-FR-KIRO, TRM-FR-DCBR: makes the link tooltip of the terminal in `host`. */
export function newLinkTooltip(host: HTMLElement, mac: boolean = isMacPlatform()): LinkTooltip {
  let element: HTMLDivElement | null = null;
  let pointer: { x: number; y: number } | null = null;
  let shown = false;
  let disposed = false;
  /** A link that showed before the first pointer position; it shows at the next move. */
  let pending: string | null = null;

  function make(): HTMLDivElement {
    const tip = document.createElement("div");
    tip.className = "terminal-link-tooltip";
    tip.setAttribute("role", "tooltip");
    const address = document.createElement("div");
    address.className = "terminal-link-tooltip__address";
    const hint = document.createElement("div");
    hint.className = "terminal-link-tooltip__hint";
    hint.textContent = linkHint(mac);
    tip.append(address, hint);
    return tip;
  }

  /** Places the tooltip under and after the pointer, inside the window. */
  function place() {
    if (!element || !pointer) return;
    const width = element.offsetWidth;
    const height = element.offsetHeight;
    const maxX = window.innerWidth - width - EDGE_GAP_PX;
    let x = Math.min(pointer.x + POINTER_GAP_PX, maxX);
    let y = pointer.y + POINTER_GAP_PX;
    if (y + height + EDGE_GAP_PX > window.innerHeight) y = pointer.y - POINTER_GAP_PX - height;
    x = Math.max(EDGE_GAP_PX, x);
    y = Math.max(EDGE_GAP_PX, y);
    element.style.left = `${x}px`;
    element.style.top = `${y}px`;
  }

  // The capture phase gets the pointer position before the library's own
  // listener on the host looks for a link at it.
  const onMove = (event: MouseEvent) => {
    pointer = { x: event.clientX, y: event.clientY };
    if (pending !== null) show(pending);
    else if (shown) place();
  };
  const onLeave = () => hide();
  host.addEventListener("mousemove", onMove, { capture: true });
  host.addEventListener("mouseleave", onLeave);

  function hide() {
    shown = false;
    pending = null;
    element?.remove();
  }

  function show(url: string) {
    if (disposed || !host.isConnected) return;
    // The tooltip shows at the pointer, so it waits for a pointer position.
    if (!pointer) {
      pending = url;
      return;
    }
    pending = null;
    element ??= make();
    const address = element.firstElementChild as HTMLElement;
    address.textContent = url;
    if (!element.isConnected) document.body.appendChild(element);
    shown = true;
    place();
  }

  return {
    show,
    hide,
    dispose() {
      disposed = true;
      hide();
      element = null;
      host.removeEventListener("mousemove", onMove, { capture: true });
      host.removeEventListener("mouseleave", onLeave);
    },
  };
}
