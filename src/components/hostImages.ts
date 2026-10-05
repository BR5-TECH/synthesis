/**
 * EDT-FR-85 / EDT-FR-86: the two halves of image support the **host** of an
 * editing surface owns, and the surface's side of each.
 *
 * An image's destination is resolved by the tab that hosts the surface, not by
 * the surface itself: an Editor tab on a project file resolves nothing, and a
 * New Artifact tab resolves against the draft (per `NAW-new-artifact.md`
 * NAW-FR-52). Image data pasted or dropped is offered to the host first: an
 * Editor tab takes none, so pasting a picture into a project file is unchanged,
 * and a New Artifact tab takes it (NAW-FR-50).
 *
 * Both are **presentation and offer** and neither touches the document's own
 * bytes. The resolution is a node decoration, so the Markdown behind a drawn
 * image and behind a placeholder alike is preserved exactly and the author can
 * repair a broken destination on either surface (EDT-FR-85). The pending state
 * of an insertion is a widget decoration at the position the reference will
 * take, so it occupies the cursor position without occupying a character
 * (NAW-FR-51).
 */
import { Extension } from "@tiptap/react";
import type { Node as PMNode } from "@tiptap/pm/model";
import { NodeSelection, Plugin, PluginKey, type EditorState, type Transaction } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";

/** Image data a paste or a drop carried, ready for the host to take. */
export interface PastedImage {
  /** The IANA media type the clipboard or the drop declared. */
  mediaType: string;
  /** The name the image arrived under, `null` where it arrived with none. */
  filename: string | null;
  /** The bytes, base64-encoded, which is how they cross to the backend. */
  data: string;
}

/**
 * What a host lends an editing surface. A surface with no host resolves no
 * destination and takes no picture, which is the Editor tab's own behaviour
 * (EDT-FR-86).
 */
export interface EditorImageHost {
  /**
   * EDT-FR-85: content to draw for one Markdown destination, or `null` where
   * the host does not resolve it.
   *
   * Called when the image is actually shown rather than when the document
   * loads, so a prompt carrying many pictures costs nothing for the ones the
   * author has not scrolled to.
   */
  resolve: (destination: string) => Promise<string | null>;
  /**
   * EDT-FR-86: the Markdown to insert for this image, or `null` where the host
   * does not take it.
   *
   * The host decides what is written; the surface applies it as one ordinary
   * edit at the cursor.
   */
  accept?: (image: PastedImage) => Promise<string | null>;
}

/** How one image destination currently stands (EDT-FR-85). */
export type ImageResolution =
  | { state: "pending" }
  | { state: "resolved"; src: string }
  | { state: "unresolved" };

export interface HostImagePayload {
  /**
   * EDT-FR-86: whether this surface has a host at all.
   *
   * A surface with none resolves no destination, so it decorates none: an
   * Editor tab on a project file draws the `src` the Markdown carries, exactly
   * as it does today. Distinct from "a host that has not answered yet", which
   * is what the pending placeholder is for.
   */
  hosted: boolean;
  /** By Markdown destination, exactly as the document carries it. */
  resolutions: Record<string, ImageResolution>;
  /** Where an insertion is waiting on its store, `null` when none is. */
  inserting: number | null;
  /** What the pending state reads, so the wording lives with its host. */
  insertingLabel: string;
}

export const hostImageKey = new PluginKey<HostImagePayload>("synthesisHostImages");

export const IMAGE_PENDING_CLASS = "editor__image--pending";
export const IMAGE_UNRESOLVED_CLASS = "editor__image--unresolved";
export const INSERTING_CLASS = "editor__image-inserting";
/**
 * The class the label beside a placeholder carries.
 *
 * A **sibling element** rather than generated content on the image itself:
 * `img` is a replaced element, and browsers do not paint `::after` on one — so
 * a stylesheet that draws the destination that way draws nothing at all, and
 * the placeholder is an empty frame, which is the one thing EDT-FR-85 says it
 * must never be.
 */
export const IMAGE_LABEL_CLASS = "editor__image-label";
/**
 * The Markdown destination, carried on the rendered element.
 *
 * The `src` attribute is what a resolution **overrides** — with the drawn bytes
 * or with a transparent stand-in — so it is exactly the wrong thing to read a
 * destination back from. This carries the author's own text unchanged, which is
 * what the host is asked about and what a reader is told is broken.
 */
export const IMAGE_DESTINATION_ATTR = "data-destination";

/**
 * A transparent one-pixel GIF.
 *
 * Stands in for the bytes of an image that is pending or unresolved, so the
 * element the placeholder is rendered on never attempts a request of its own and
 * never shows a browser's own broken-image glyph. What the reader sees is the
 * label beside it (EDT-FR-85), which CSS draws from the class; what assistive
 * technology reads is the accessible name below.
 */
const BLANK =
  "data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7";

const EMPTY: HostImagePayload = {
  hosted: false,
  resolutions: {},
  inserting: null,
  insertingLabel: "Inserting image…",
};

/**
 * EDT-FR-85: the accessible name an unresolved destination renders with —
 * stating that the image could not be loaded and naming the destination.
 *
 * Exported because the tab announces the same fact through its own
 * accessibility feedback (NAW-FR-54) and the two must say the same thing.
 */
export function unresolvedImageLabel(destination: string, alt: string): string {
  const named = alt.trim() ? `${alt.trim()}: ` : "";
  return `${named}Image could not be loaded — ${destination}`;
}

/**
 * NAW-FR-54: select the image the caret is beside, so the removal action is
 * reachable **without a pointer**.
 *
 * ProseMirror walks the caret straight across an inline leaf without ever
 * selecting it, so arrowing over a picture never raises the action — the one
 * route to it would be a mouse, which NAW-FR-54 forbids. This turns the natural
 * gesture into what it looks like it should do: extending the selection onto an
 * image selects that image.
 *
 * `direction` is `1` for a step forward and `-1` for a step back. Returns the
 * transaction to dispatch, or `null` where the caret is not beside an image and
 * the key belongs to the surface's ordinary handling.
 */
export function selectAdjacentImage(
  state: EditorState,
  direction: 1 | -1,
): Transaction | null {
  const { selection } = state;
  if (!selection.empty) return null;
  const at = direction === 1 ? selection.from : selection.from - 1;
  if (at < 0 || at >= state.doc.content.size) return null;
  const node = state.doc.nodeAt(at);
  if (!node || node.type.name !== "image") return null;
  return state.tr.setSelection(NodeSelection.create(state.doc, at));
}

/**
 * NAW-FR-53: the image the surface has selected, or `null` where the selection
 * is not exactly one image.
 *
 * Exactly one image node and nothing else, so the removal action removes **that
 * Markdown reference and nothing else**: a selection spanning a picture and the
 * sentence around it is not an image the author has selected, and offering the
 * action over it would delete prose they never meant to lose.
 */
export function selectedImageAt(
  doc: PMNode,
  from: number,
  to: number,
): { from: number; to: number; src: string } | null {
  if (from < 0 || from >= doc.content.size) return null;
  const node = doc.nodeAt(from);
  if (!node || node.type.name !== "image") return null;
  if (to !== from + node.nodeSize) return null;
  return {
    from,
    to,
    src: typeof node.attrs.src === "string" ? node.attrs.src : "",
  };
}

/** The destination of every image node in the document, in document order. */
export function imageDestinations(doc: PMNode): string[] {
  const seen: string[] = [];
  doc.descendants((node) => {
    if (node.type.name !== "image") return true;
    const src = typeof node.attrs.src === "string" ? node.attrs.src : "";
    if (src && !seen.includes(src)) seen.push(src);
    return true;
  });
  return seen;
}

/**
 * The label that stands beside a placeholder, as its own element.
 *
 * `aria-hidden`, because the image itself already carries the same words as its
 * accessible name — this is what a **sighted** reader sees, and announcing it
 * twice would be worse than not drawing it.
 */
function labelWidget(text: string): () => HTMLElement {
  return () => {
    const label = document.createElement("span");
    label.className = IMAGE_LABEL_CLASS;
    label.setAttribute("aria-hidden", "true");
    label.textContent = text;
    return label;
  };
}

function buildDecorations(doc: PMNode, payload: HostImagePayload): DecorationSet {
  const decos: Decoration[] = [];
  doc.descendants((node, pos) => {
    if (node.type.name !== "image") return true;
    if (!payload.hosted) return true;
    const src = typeof node.attrs.src === "string" ? node.attrs.src : "";
    const alt = typeof node.attrs.alt === "string" ? node.attrs.alt : "";
    const resolution = payload.resolutions[src];
    const after = pos + node.nodeSize;
    if (!resolution || resolution.state === "pending") {
      decos.push(
        Decoration.node(pos, after, {
          class: IMAGE_PENDING_CLASS,
          src: BLANK,
          [IMAGE_DESTINATION_ATTR]: src,
          "aria-busy": "true",
          "aria-label": `Loading image — ${src}`,
        }),
        // `side: 1` keeps the label after the image rather than before it when
        // a decoration and the caret share the position.
        Decoration.widget(after, labelWidget("Loading image…"), { side: 1 }),
      );
      return true;
    }
    if (resolution.state === "resolved") {
      // The document's own `src` is the Markdown destination and stays exactly
      // as the author wrote it; what is drawn is an override on the rendered
      // element alone (EDT-FR-85).
      decos.push(
        Decoration.node(pos, after, {
          src: resolution.src,
          [IMAGE_DESTINATION_ATTR]: src,
        }),
      );
      return true;
    }
    decos.push(
      Decoration.node(pos, after, {
        class: IMAGE_UNRESOLVED_CLASS,
        src: BLANK,
        [IMAGE_DESTINATION_ATTR]: src,
        role: "img",
        "aria-label": unresolvedImageLabel(src, alt),
        "data-unresolved": src,
      }),
      // EDT-FR-85: the destination drawn beside the frame, so a sighted reader
      // can see **which** reference is broken and repair it — never an empty
      // frame and never nothing at all.
      Decoration.widget(after, labelWidget(unresolvedImageLabel(src, alt)), {
        side: 1,
      }),
    );
    return true;
  });
  if (payload.inserting !== null && payload.inserting <= doc.content.size) {
    decos.push(
      Decoration.widget(payload.inserting, () => {
        const mark = document.createElement("span");
        mark.className = INSERTING_CLASS;
        mark.setAttribute("role", "status");
        mark.textContent = payload.insertingLabel;
        return mark;
      }),
    );
  }
  return DecorationSet.create(doc, decos);
}

/**
 * EDT-FR-85 / NAW-FR-51: the resolution of every image destination the document
 * carries, and the pending state of an insertion waiting on its store.
 *
 * Presentation only. It adds no node and changes no byte, so it occupies no
 * position in the artifact's undo history and resolution never blocks editing:
 * typing, selection, and the write schedule all proceed while an image is
 * pending or unresolved.
 */
export const HostImages = Extension.create({
  name: "hostImages",

  addProseMirrorPlugins() {
    return [
      new Plugin<HostImagePayload>({
        key: hostImageKey,
        state: {
          init: () => EMPTY,
          apply(tr, value) {
            const pushed = tr.getMeta(hostImageKey) as
              | HostImagePayload
              | undefined;
            if (pushed) return pushed;
            if (!tr.docChanged) return value;
            // The pending marker follows the text it was placed in, so typing
            // elsewhere in the prompt does not leave it stranded.
            return {
              ...value,
              inserting:
                value.inserting === null
                  ? null
                  : tr.mapping.map(value.inserting),
            };
          },
        },
        props: {
          decorations(state) {
            const payload = hostImageKey.getState(state) ?? EMPTY;
            return buildDecorations(state.doc, payload);
          },
        },
      }),
    ];
  },
});

/** Whether a file the clipboard or a drop carried is an image at all. */
function isImage(file: { type?: string }): boolean {
  return (file.type ?? "").toLowerCase().startsWith("image/");
}

/** The base64 of a `Blob`, without the `data:` prefix a data URL carries. */
async function encode(file: Blob): Promise<string> {
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  let binary = "";
  // Chunked, because spreading a multi-megabyte array into `String.fromCharCode`
  // overflows the argument list on every engine.
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    binary += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(binary);
}

/**
 * EDT-FR-86: the image a paste or a drop carried, ready to offer to the host —
 * or `null` where it carried none, in which case the paste falls back to what it
 * does today.
 */
export async function imageFromTransfer(
  transfer: DataTransfer | null,
): Promise<PastedImage | null> {
  if (!transfer) return null;
  const files = Array.from(transfer.files ?? []);
  const file = files.find(isImage);
  if (file) {
    return {
      mediaType: file.type,
      filename: file.name || null,
      data: await encode(file),
    };
  }
  // A screenshot pasted from the system clipboard arrives as an item rather than
  // as a file on some platforms, and carries no filename at all.
  const items = Array.from(transfer.items ?? []);
  for (const item of items) {
    if (item.kind !== "file" || !isImage(item)) continue;
    const blob = item.getAsFile();
    if (!blob) continue;
    return {
      mediaType: blob.type || item.type,
      filename: blob.name || null,
      data: await encode(blob),
    };
  }
  return null;
}

/** Whether a transfer carries a text flavour, which is what a fallback pastes. */
export function transferHasText(transfer: DataTransfer | null): boolean {
  if (!transfer) return false;
  return Array.from(transfer.types ?? []).some(
    (type) => type === "text/plain" || type === "text/html",
  );
}
