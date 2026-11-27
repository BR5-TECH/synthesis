/**
 * SNV-FR-WCQH: a secondary click opens only a context menu of the application.
 *
 * Outside text, the web content's own menu holds only "Reload", which is a
 * browser command. This guard stops that menu where no application menu took
 * the click. It stands aside in two cases, where the menu is the system text
 * menu (Cut, Copy, Paste, spelling, Look Up):
 *
 * - the target is in an editable field: a text input or a textarea (read-only
 *   included, so its text can be copied), or an editable rich-text region. A
 *   terminal is such a region: its host is `contenteditable`.
 * - the target is in the current text selection, and that selection is not
 *   empty (SNV-FR-TLXS).
 */
import { inEditableRegion, isTextControl, targetElement } from "./editableTarget";
import { logDebug } from "./logging";

/** Whether the native text menu may open on this element. */
function inEditableField(element: Element): boolean {
  if (inEditableRegion(element)) return true;
  return isTextControl(element) && !element.disabled;
}

/** The caret position under the pointer, where the engine can give one. */
function caretAt(doc: Document, x: number, y: number): { node: Node; offset: number } | null {
  if (typeof doc.caretPositionFromPoint === "function") {
    const position = doc.caretPositionFromPoint(x, y);
    return position ? { node: position.offsetNode, offset: position.offset } : null;
  }
  // WebKit before Safari 18.4 has only the older call.
  const legacy = (doc as Document & { caretRangeFromPoint?: (x: number, y: number) => Range | null })
    .caretRangeFromPoint;
  if (typeof legacy === "function") {
    const range = legacy.call(doc, x, y);
    return range ? { node: range.startContainer, offset: range.startOffset } : null;
  }
  return null;
}

/**
 * Whether the click is in the current text selection. The target must touch
 * the selection; where the engine can give the caret under the pointer, that
 * caret must also be in a selected range, so a click on a large container that
 * holds the selection somewhere else does not count.
 */
function inSelection(event: MouseEvent, element: Element): boolean {
  const doc = element.ownerDocument;
  const selection = doc.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return false;
  const ranges: Range[] = [];
  for (let i = 0; i < selection.rangeCount; i++) {
    const range = selection.getRangeAt(i);
    if (!range.collapsed && range.intersectsNode(element)) ranges.push(range);
  }
  if (ranges.length === 0) return false;
  const caret = caretAt(doc, event.clientX, event.clientY);
  if (!caret) return true;
  for (const range of ranges) {
    try {
      if (range.isPointInRange(caret.node, caret.offset)) return true;
    } catch {
      // A caret in another document or a doctype node is not in this range.
    }
  }
  return false;
}

/**
 * Installs the guard on `target` and returns the function that removes it.
 *
 * The guard listens in the bubble phase, so every application menu below has
 * seen the event first, and `defaultPrevented` tells that one of them took it.
 */
export function installContextMenuGuard(target: Window = window): () => void {
  const onContextMenu = (event: Event) => {
    const e = event as MouseEvent;
    if (e.defaultPrevented) return;
    const element = targetElement(e.target);
    if (element && (inEditableField(element) || inSelection(e, element))) return;
    e.preventDefault();
    logDebug(["frontend"], "native context menu suppressed", {
      tag: element?.tagName.toLowerCase() ?? "none",
    });
  };
  target.addEventListener("contextmenu", onContextMenu);
  return () => target.removeEventListener("contextmenu", onContextMenu);
}
