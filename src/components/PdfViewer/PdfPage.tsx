/**
 * The displayed page of the PDF viewer (PDV-FR-VKRL, PDV-FR-BEQL, PDV-FR-BPXG,
 * PDV-FR-KDVB, PDV-FR-INXM).
 *
 * The page is a canvas with a selectable text layer above it. Only this page
 * renders. A change of page or zoom, a new document, and an unmount each cancel
 * the render in progress. While the page renders, the viewer shows
 * "Loading page…" in place of the page.
 */
import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";
import { logError } from "../../logging";
import { loadPdfjs, type PdfDocument } from "./pdfDocument";
import { applyHighlights, type PageMatch } from "./pdfHighlights";
import type { PageEntry, PageTextStore } from "./pdfSearch";

export type PageKey = "next" | "previous" | "first" | "last";

interface TextLayerLike {
  textDivs: HTMLElement[];
  render(): Promise<unknown>;
  cancel(): void;
}

interface RenderTaskLike {
  promise: Promise<unknown>;
  cancel(): void;
}

interface Layout {
  entry: PageEntry;
  textDivs: HTMLElement[];
}

const KEYS: Record<string, PageKey> = {
  PageDown: "next",
  PageUp: "previous",
  Home: "first",
  End: "last",
};

export interface PdfPageProps {
  doc: PdfDocument;
  store: PageTextStore;
  /** 1-based number of the displayed page. */
  page: number;
  scale: number;
  /** The matches on this page. The list changes only when a match changes. */
  matches: PageMatch[];
  /** Changes when the current match must be scrolled into view. */
  reveal: number;
  onKey: (key: PageKey) => void;
}

export function PdfPage({
  doc,
  store,
  page,
  scale,
  matches,
  reveal,
  onKey,
}: PdfPageProps) {
  const regionRef = useRef<HTMLDivElement>(null);
  const sheetRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const layerRef = useRef<HTMLDivElement>(null);
  const shownPage = useRef<number | null>(null);
  const revealed = useRef(0);
  const [status, setStatus] = useState<"loading" | "ready" | "error">(
    "loading",
  );
  const [layout, setLayout] = useState<Layout | null>(null);

  useEffect(() => {
    const region = regionRef.current;
    const sheet = sheetRef.current;
    const canvas = canvasRef.current;
    const layer = layerRef.current;
    if (!region || !sheet || !canvas || !layer) return;
    let cancelled = false;
    let renderTask: RenderTaskLike | null = null;
    let textLayer: TextLayerLike | null = null;

    setStatus("loading");
    setLayout(null);
    layer.replaceChildren();
    if (shownPage.current !== page) {
      shownPage.current = page;
      region.scrollTop = 0;
      region.scrollLeft = 0;
    }

    void (async () => {
      try {
        const [pdfjs, proxy, entry] = await Promise.all([
          loadPdfjs(),
          doc.getPage(page),
          store.entry(page),
        ]);
        if (cancelled) return;
        const viewport = proxy.getViewport({ scale });
        const ratio = window.devicePixelRatio || 1;
        const width = viewport.width;
        const height = viewport.height;
        sheet.style.width = `${width}px`;
        sheet.style.height = `${height}px`;
        canvas.width = Math.max(1, Math.floor(width * ratio));
        canvas.height = Math.max(1, Math.floor(height * ratio));
        canvas.style.width = `${width}px`;
        canvas.style.height = `${height}px`;
        renderTask = proxy.render({
          canvas,
          viewport,
          transform: ratio === 1 ? undefined : [ratio, 0, 0, ratio, 0, 0],
        }) as unknown as RenderTaskLike;
        const created = new pdfjs.TextLayer({
          textContentSource: entry.content as never,
          container: layer,
          viewport,
        }) as unknown as TextLayerLike;
        textLayer = created;
        // The CSS round() function sets the size in PDF.js. An older web view
        // may not know it, so the size is set in pixels as well.
        layer.style.width = `${width}px`;
        layer.style.height = `${height}px`;
        await Promise.all([renderTask.promise, created.render()]);
        if (cancelled) return;
        setLayout({ entry, textDivs: created.textDivs });
        setStatus("ready");
      } catch {
        if (cancelled) return;
        logError(["frontend"], "pdf page render failed", { reason: "render" });
        setStatus("error");
      }
    })();

    return () => {
      cancelled = true;
      try {
        renderTask?.cancel();
        textLayer?.cancel();
      } catch {
        // A task that is finished has nothing to cancel.
      }
    };
  }, [doc, store, page, scale]);

  useEffect(() => {
    const layer = layerRef.current;
    if (!layer || layout === null) return;
    const mark = applyHighlights(
      layer,
      layout.textDivs,
      layout.entry.text,
      matches,
    );
    if (mark !== null && revealed.current !== reveal) {
      revealed.current = reveal;
      mark.scrollIntoView?.({ block: "center", inline: "nearest" });
    }
  }, [layout, matches, reveal]);

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (event.target !== event.currentTarget) return;
    const key = KEYS[event.key];
    if (key === undefined) return;
    event.preventDefault();
    onKey(key);
  };

  const sheetStyle = { "--scale-factor": scale } as CSSProperties;

  return (
    <div
      ref={regionRef}
      className="pdf-page-region"
      role="region"
      aria-label="PDF page"
      aria-busy={status === "loading"}
      tabIndex={0}
      onKeyDown={onKeyDown}
    >
      {status === "loading" && (
        <p className="pdf-page-message">Loading page…</p>
      )}
      {status === "error" && (
        <p className="pdf-page-message">This PDF could not be displayed.</p>
      )}
      <div
        ref={sheetRef}
        className="pdf-page-sheet"
        data-state={status}
        style={sheetStyle}
      >
        <canvas ref={canvasRef} className="pdf-page-canvas" />
        <div ref={layerRef} className="pdf-text-layer" />
      </div>
    </div>
  );
}
