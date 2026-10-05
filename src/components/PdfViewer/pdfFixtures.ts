// A fake of PDF.js for the PDF viewer tests. jsdom has no canvas and no worker,
// so the tests mock the library at the module seam of `pdfDocument.ts` and use
// this object in its place. The fake keeps the parts of the real API that the
// viewer calls: `getDocument`, the document, its pages, the render task, and the
// text layer that puts the page text into the DOM.
import { vi } from "vitest";
import type { DocumentPdf } from "../../types";
import { ID, invokeMock } from "../DocumentViewer/viewerFixtures";

export interface FakeTextItem {
  str: string;
  hasEOL?: boolean;
}

/** The text items of each page of a fake PDF. */
export type FakePages = FakeTextItem[][];

export interface RenderCall {
  page: number;
  scale: number;
}

export const state = {
  pages: [] as FakePages,
  /** An error that `getDocument` rejects with, or null. */
  openError: null as unknown,
  /** When set, a page render stays pending until `releaseRenders` runs. */
  holdRenders: false,
  renders: [] as RenderCall[],
  renderCancels: 0,
  textLayerCancels: 0,
  textContentReads: [] as number[],
  documentsOpened: 0,
  documentsDestroyed: 0,
  getDocumentArgs: [] as Record<string, unknown>[],
};

let pendingRenders: (() => void)[] = [];

export function resetPdfFixtures(): void {
  state.pages = [];
  state.openError = null;
  state.holdRenders = false;
  state.renders = [];
  state.renderCancels = 0;
  state.textLayerCancels = 0;
  state.textContentReads = [];
  state.documentsOpened = 0;
  state.documentsDestroyed = 0;
  state.getDocumentArgs = [];
  pendingRenders = [];
}

export function releaseRenders(): void {
  const waiting = pendingRenders;
  pendingRenders = [];
  for (const release of waiting) release();
}

function fakePageProxy(pages: FakePages, number: number) {
  return {
    getViewport: ({ scale }: { scale: number }) => ({
      width: 612 * scale,
      height: 792 * scale,
      scale,
    }),
    render: ({ viewport }: { viewport: { scale: number } }) => {
      state.renders.push({ page: number, scale: viewport.scale });
      let cancelled = false;
      const promise = new Promise<void>((resolve) => {
        if (state.holdRenders) pendingRenders.push(resolve);
        else resolve();
      });
      return {
        promise,
        cancel: () => {
          cancelled = true;
          state.renderCancels += 1;
          return cancelled;
        },
      };
    },
    getTextContent: async () => {
      state.textContentReads.push(number);
      return { items: pages[number - 1] ?? [] };
    },
  };
}

function fakeDocument(pages: FakePages) {
  return {
    numPages: pages.length,
    getPage: async (number: number) => fakePageProxy(pages, number),
  };
}

class FakeTextLayer {
  textDivs: HTMLElement[] = [];
  constructor(
    private options: {
      textContentSource: { items: { str?: string }[] };
      container: HTMLElement;
    },
  ) {}
  render(): Promise<void> {
    for (const item of this.options.textContentSource.items) {
      if (typeof item.str !== "string") continue;
      const span = document.createElement("span");
      span.textContent = item.str;
      this.options.container.appendChild(span);
      this.textDivs.push(span);
    }
    return Promise.resolve();
  }
  cancel(): void {
    state.textLayerCancels += 1;
  }
}

/** What `getDocument` returns: a loading task. A test may return its own. */
export interface FakeTask {
  destroy: () => Promise<void>;
  promise: Promise<unknown>;
}

export const pdfjsFake = {
  GlobalWorkerOptions: { workerSrc: "" },
  TextLayer: FakeTextLayer,
  getDocument: vi.fn((params: Record<string, unknown>): FakeTask => {
    state.getDocumentArgs.push(params);
    const pages = state.pages;
    const task: FakeTask = {
      destroy: vi.fn(async () => {
        state.documentsDestroyed += 1;
      }),
      promise: (async () => {
        if (state.openError !== null) throw state.openError;
        state.documentsOpened += 1;
        return fakeDocument(pages);
      })(),
    };
    // A rejected promise nobody awaits yet is not an unhandled error.
    task.promise.catch(() => {});
    return task;
  }),
};

/** A page of plain items, one per string. */
export function page(...strings: string[]): FakeTextItem[] {
  return strings.map((str) => ({ str }));
}

export const PDF_ID = ID;

export function pdfPayload(revision = "p1", bytes = "%PDF-1.7 fake"): DocumentPdf {
  return {
    id: ID,
    name: "guide.pdf",
    revision,
    bytes_base64: btoa(bytes),
  };
}

/** Make `read_document_pdf` answer with the payload. */
export function pdfReadReturns(value: DocumentPdf): void {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "read_document_pdf") return Promise.resolve(value);
    throw new Error(`unexpected command ${cmd}`);
  });
}

/** Make `read_document_pdf` refuse with the given error. */
export function pdfReadRejects(error: unknown): void {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "read_document_pdf") return Promise.reject(error);
    throw new Error(`unexpected command ${cmd}`);
  });
}

export const pdfReadCalls = () =>
  invokeMock.mock.calls.filter((call) => call[0] === "read_document_pdf");
