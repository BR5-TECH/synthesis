/**
 * Asking the host what each of the document's image destinations resolves to
 * (`../../../specifications/ui/EDT-editor.md` EDT-FR-85).
 *
 * Driven off the document's own version so a destination the author repairs on
 * either surface is resolved without the tab reloading, and off the mode so
 * entering the rich surface resolves what the raw one never drew. A destination
 * already answered is not asked about again, and one is asked about only when
 * it comes into view — so a prompt carrying fifty pictures costs the surface
 * the pictures the author is actually looking at.
 */
import { useEffect } from "react";
import type { Editor as TiptapEditor } from "@tiptap/react";

import {
  imageDestinations,
  IMAGE_DESTINATION_ATTR,
  type EditorImageHost,
  type ImageResolution,
} from "../hostImages";

export function useHostImageResolution({
  editor,
  mode,
  docVersion,
  hostRef,
  resolutionsRef,
  onResolved,
}: {
  editor: TiptapEditor | null;
  mode: string;
  /** Stands in for the mutable document, which is not a React value. */
  docVersion: number;
  /** Read through a ref, because the effect is not re-run when the host moves. */
  hostRef: React.MutableRefObject<EditorImageHost | undefined>;
  /** What has already been answered, so nothing is asked about twice. */
  resolutionsRef: React.MutableRefObject<Record<string, ImageResolution>>;
  onResolved: (
    next: (held: Record<string, ImageResolution>) => Record<string, ImageResolution>,
  ) => void;
}): void {
  useEffect(() => {
    const host = hostRef.current;
    if (!editor || mode !== "wysiwyg" || !host) return;
    const destinations = imageDestinations(editor.state.doc);
    const unknown = destinations.filter(
      (d) => !(d in resolutionsRef.current),
    );
    if (unknown.length === 0) return;
    let live = true;

    const ask = (destination: string) => {
      if (!live || destination in resolutionsRef.current) return;
      onResolved((current) =>
        destination in current
          ? current
          : { ...current, [destination]: { state: "pending" } },
      );
      void host
        .resolve(destination)
        .then((src) => {
          if (!live) return;
          onResolved((current) => ({
            ...current,
            [destination]: src
              ? { state: "resolved", src }
              : { state: "unresolved" },
          }));
        })
        .catch(() => {
          if (!live) return;
          onResolved((current) => ({
            ...current,
            [destination]: { state: "unresolved" },
          }));
        });
    };

    // NAW-FR-52: an image is fetched **when it is actually shown** rather than
    // when the prompt loads, so a prompt carrying many pictures costs nothing
    // for the ones the author has not scrolled to. Each image's own element is
    // watched, and the ask happens the first time one comes into view.
    if (typeof IntersectionObserver === "undefined") {
      // No observer to watch with — a test environment, or a browser without
      // one. Asking now is the honest fallback: the alternative is a document
      // whose pictures never load at all.
      unknown.forEach(ask);
      return () => {
        live = false;
      };
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          const destination = (entry.target as HTMLElement).getAttribute(
            IMAGE_DESTINATION_ATTR,
          );
          if (destination) ask(destination);
          observer.unobserve(entry.target);
        }
      },
      // A margin, so an image just below the fold is ready by the time the
      // author reaches it rather than blank for a frame after they do.
      { root: null, rootMargin: "200px" },
    );
    // Deferred by a task, because this effect runs before the surface has
    // rendered the nodes the transaction produced — observing now would observe
    // an empty document and no image would ever be asked for.
    const attach = () => {
      if (!live) return;
      for (const element of editor.view.dom.querySelectorAll("img")) {
        // The author's own destination, not the `src` a resolution may already
        // have overridden.
        const destination =
          element.getAttribute(IMAGE_DESTINATION_ATTR) ?? element.getAttribute("src");
        if (destination && unknown.includes(destination)) {
          observer.observe(element);
        }
      }
    };
    const scheduled = setTimeout(attach, 0);
    return () => {
      live = false;
      clearTimeout(scheduled);
      observer.disconnect();
    };
  }, [editor, mode, docVersion]);
}
