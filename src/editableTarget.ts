/**
 * The checks that tell whether an event target is in a field that holds text
 * the user can edit. The drop guard (SNV-FR-UKER) and the context menu guard
 * (SNV-FR-WCQH) share them.
 */

/** Input types that hold text. */
export const TEXT_INPUT_TYPES = ["", "text", "search", "url", "email", "tel", "password"];

/** The element of an event target: a text node gives its parent element. */
export function targetElement(target: EventTarget | null): Element | null {
  if (target instanceof Element) return target;
  if (target instanceof Node) return target.parentElement;
  return null;
}

/**
 * Whether the element is in an editable rich-text region. The nearest
 * `contenteditable` decides, so an island marked `false` inside an editable
 * region is not editable.
 */
export function inEditableRegion(element: Element): boolean {
  const host = element.closest("[contenteditable]");
  return !!host && host.getAttribute("contenteditable")?.toLowerCase() !== "false";
}

/** Whether the element is a textarea or an input of a text type. */
export function isTextControl(
  element: Element,
): element is HTMLTextAreaElement | HTMLInputElement {
  if (element instanceof HTMLTextAreaElement) return true;
  return (
    element instanceof HTMLInputElement &&
    TEXT_INPUT_TYPES.includes(element.getAttribute("type")?.toLowerCase() ?? "")
  );
}
