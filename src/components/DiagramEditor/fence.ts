/**
 * EDT-FR-EBEZ, EDT-FR-HBEB, EDT-FR-SHYX: the fence that the diagram editor
 * overlay edits, read from and written to the buffer of its surface.
 *
 * The overlay keeps no copy of the source. It reads the fence from the
 * document of the surface, writes each edit as an ordinary transaction of that
 * surface, and sends undo and redo to the surface, which serves them from the
 * one history of the artifact.
 */
import type { Node as PMNode } from "@tiptap/pm/model";
import type { EditorView } from "@tiptap/pm/view";

import { isDiagramFence } from "../markdownMermaid";

/** Where the fence is: its start, and its place among the diagram fences. */
export interface FencePlace {
  pos: number;
  /** The index of the fence among the diagram fences of the document. */
  ordinal: number;
  /** How many diagram fences the document holds. */
  count: number;
}

/** The start position of each diagram fence of `doc`, in document order. */
export function fenceStarts(doc: PMNode): number[] {
  const starts: number[] = [];
  doc.descendants((node, pos) => {
    if (isDiagramFence(node)) {
      starts.push(pos);
      return false;
    }
    return !node.isTextblock;
  });
  return starts;
}

/** The place of the diagram fence that starts at `pos`, or null. */
export function placeAt(doc: PMNode, pos: number): FencePlace | null {
  const starts = fenceStarts(doc);
  const ordinal = starts.indexOf(pos);
  if (ordinal < 0) return null;
  return { pos, ordinal, count: starts.length };
}

/**
 * EDT-FR-SHYX: the place of the fence after the document changed, or null when
 * the change removed it.
 *
 * An edit in the overlay keeps the fence where it starts. An undo, a reload,
 * or a reset replaces the whole document, so a position says nothing after
 * one. The fence is then the one at the same place among the diagram fences,
 * provided the document holds as many as before. When the number changed, the
 * fence is the one fence that holds the source the overlay shows. When no
 * fence or more than one holds it, the fence cannot be told apart from the
 * others, and the overlay closes.
 */
export function relocate(doc: PMNode, last: FencePlace, source: string): FencePlace | null {
  const starts = fenceStarts(doc);
  if (starts.length === last.count) {
    const pos = starts[last.ordinal];
    return pos === undefined ? null : { pos, ordinal: last.ordinal, count: starts.length };
  }
  // Another fence came or went. The fence is still there when exactly one
  // fence holds the source that the overlay shows.
  const same = starts.filter((pos) => sourceAt(doc, pos) === source);
  if (same.length !== 1) return null;
  return { pos: same[0], ordinal: starts.indexOf(same[0]), count: starts.length };
}

/** The source of the fence that starts at `pos`. */
export function sourceAt(doc: PMNode, pos: number): string {
  return doc.nodeAt(pos)?.textContent ?? "";
}

/**
 * EDT-FR-EBEZ: write `next` as the source of the fence at `pos`, as one
 * ordinary edit of the surface.
 *
 * Only the part that differs is replaced, so the selection and the comment
 * anchors in the rest of the fence stay where they are.
 */
export function writeSource(view: EditorView, pos: number, next: string): boolean {
  const node = view.state.doc.nodeAt(pos);
  if (!node || !isDiagramFence(node)) return false;
  const current = node.textContent;
  if (current === next) return true;
  let head = 0;
  const shorter = Math.min(current.length, next.length);
  while (head < shorter && current[head] === next[head]) head++;
  let tail = 0;
  while (
    tail < shorter - head &&
    current[current.length - 1 - tail] === next[next.length - 1 - tail]
  ) {
    tail++;
  }
  const start = pos + 1;
  const from = start + head;
  const to = start + current.length - tail;
  const text = next.slice(head, next.length - tail);
  view.dispatch(view.state.tr.insertText(text, from, to));
  return true;
}

/**
 * EDT-FR-HBEB: send an undo or a redo to the surface.
 *
 * Each surface that can hold the fence claims undo and redo on an element
 * that holds its editing surface, in the capture phase (EDT-FR-22). The key
 * press that the author made in the overlay is sent again from the editing
 * surface, so it reaches that claim and the one history of the artifact.
 */
/** The key presses that the overlay sends to a surface (EDT-FR-HBEB). */
const forwarded = new WeakSet<Event>();

/** Whether `event` is a key press that the overlay sent to its surface. */
export function isForwardedKey(event: Event): boolean {
  return forwarded.has(event);
}

export function sendHistoryKey(
  view: EditorView,
  direction: "undo" | "redo",
  keys: { metaKey: boolean; ctrlKey: boolean },
): void {
  const modifier = keys.metaKey || keys.ctrlKey ? keys : { metaKey: false, ctrlKey: true };
  const event = new KeyboardEvent("keydown", {
    key: "z",
    code: "KeyZ",
    shiftKey: direction === "redo",
    metaKey: modifier.metaKey,
    ctrlKey: modifier.ctrlKey,
    bubbles: true,
    cancelable: true,
  });
  forwarded.add(event);
  view.dom.dispatchEvent(event);
}

/**
 * Whether the surface is out of sight: an element that holds it is hidden.
 * The rich surface of an Editor tab is hidden while the tab shows its raw text.
 */
export function surfaceHidden(view: EditorView): boolean {
  let el: HTMLElement | null = view.dom;
  while (el) {
    if (el.hidden || el.style.display === "none") return true;
    el = el.parentElement;
  }
  return !view.dom.isConnected;
}
