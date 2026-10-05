// The PDF viewer tab (`specifications/ui/PDV-pdf-viewer.md`): how it reads and
// opens a PDF, the states it shows, how it follows the collection, and what it
// logs. The backend is mocked at `invoke` and PDF.js at the module seam of
// `pdfDocument.ts`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetViewStateForTest } from "../../state/documentViewState";
import {
  emitDocuments,
  entry,
  ID,
  invokeMock,
  listenerCount,
  logErrorMock,
  logInfoMock,
  logWarnMock,
  resetFixtures,
} from "../DocumentViewer/viewerFixtures";
import { PdfViewer } from ".";
import {
  page,
  pdfPayload,
  pdfReadCalls,
  pdfReadRejects,
  pdfReadReturns,
  pdfjsFake,
  releaseRenders,
  resetPdfFixtures,
  state,
} from "./pdfFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../DocumentViewer/viewerFixtures")).invokeMock,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../DocumentViewer/viewerFixtures")).listenMock,
}));
vi.mock("../../logging", async () => (await import("../DocumentViewer/viewerFixtures")).loggingMock);
vi.mock("./pdfDocument", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./pdfDocument")>()),
  loadPdfjs: async () => (await import("./pdfFixtures")).pdfjsFake,
}));

beforeEach(() => {
  resetFixtures();
  resetPdfFixtures();
  resetViewStateForTest();
  // Not `mockClear`: a test can end before its open calls `getDocument`, and
  // `mockClear` keeps the `mockImplementationOnce` that the test did not use.
  // The next test then gets a document that never opens.
  pdfjsFake.getDocument.mockReset();
  state.pages = [page("Alpha page"), page("Beta page"), page("Gamma page")];
});
afterEach(cleanup);

const viewer = () => <PdfViewer documentId={ID} />;

async function opened() {
  render(viewer());
  await screen.findByRole("region", { name: "PDF page" });
  await waitFor(() => expect(screen.queryByText("Loading page…")).toBeNull());
}

describe("PDV-FR-YLWC: reading and opening", () => {
  it("PDV-FR-YLWC: shows Loading PDF… while the bytes are read", async () => {
    invokeMock.mockImplementation(() => new Promise(() => {}));
    render(viewer());
    expect(await screen.findByText("Loading PDF…")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "PDF page" })).toBeNull();
  });

  it("PDV-FR-YLWC: shows Loading PDF… while PDF.js opens the bytes", async () => {
    pdfReadReturns(pdfPayload());
    let release: () => void = () => {};
    pdfjsFake.getDocument.mockImplementationOnce(() => ({
      destroy: async () => {},
      promise: new Promise((resolve) => {
        release = () => resolve({ numPages: 1, getPage: async () => ({}) });
      }),
    }));
    render(viewer());
    // Loading PDF… also shows while the bytes are read, so wait until PDF.js
    // opens them.
    await waitFor(() => expect(pdfjsFake.getDocument).toHaveBeenCalledTimes(1));
    expect(screen.getByText("Loading PDF…")).toBeInTheDocument();
    release();
  });

  it("PDV-FR-YLWC, PDV-FR-KJWR: a PDF that PDF.js cannot open shows This PDF could not be displayed. and logs without detail", async () => {
    pdfReadReturns(pdfPayload());
    state.openError = new Error("bad xref in /secret/guide.pdf");
    render(viewer());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "This PDF could not be displayed.",
    );
    expect(screen.queryByRole("toolbar")).toBeNull();
    expect(logErrorMock).toHaveBeenCalledTimes(1);
    expect(JSON.stringify(logErrorMock.mock.calls)).not.toContain("/secret");
    expect(JSON.stringify(logErrorMock.mock.calls)).not.toContain("guide.pdf");
  });

  it("PDV-FR-YLWC: a password-protected PDF shows the password text and never asks for a password", async () => {
    pdfReadReturns(pdfPayload());
    state.openError = { name: "PasswordException", message: "No password given" };
    const { container } = render(viewer());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "This PDF is protected by a password and cannot be displayed.",
    );
    expect(container.querySelector("input[type='password']")).toBeNull();
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(logWarnMock).toHaveBeenCalledTimes(1);
  });

  it("PDV-FR-YLWC, PDV-FR-KJWR: a read that fails for another reason is a failed display and an ERROR record", async () => {
    pdfReadRejects("io_error: /secret/guide.pdf");
    render(viewer());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "This PDF could not be displayed.",
    );
    expect(logErrorMock).toHaveBeenCalledWith(["frontend"], expect.any(String), {
      reason: "read",
    });
    expect(JSON.stringify(logErrorMock.mock.calls)).not.toContain("/secret");
  });
});

describe("PDV-FR-GPLH: the bytes", () => {
  it("PDV-FR-GPLH: reads with the document id only, and gives PDF.js the decoded bytes and no URL or path", async () => {
    pdfReadReturns(pdfPayload("p1", "%PDF-1.7 some bytes"));
    await opened();
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("read_document_pdf", { id: ID });
    expect(pdfjsFake.getDocument).toHaveBeenCalledTimes(1);
    const args = state.getDocumentArgs[0];
    expect(args.data).toBeInstanceOf(Uint8Array);
    expect(new TextDecoder().decode(args.data as Uint8Array)).toBe(
      "%PDF-1.7 some bytes",
    );
    expect(Object.keys(args)).not.toContain("url");
    expect(Object.keys(args)).not.toContain("path");
    expect(JSON.stringify(Object.values(args).filter((v) => typeof v === "string"))).not.toMatch(
      /https?:|file:|\//,
    );
  });
});

describe("PDV-FR-VKRL: one page at a time", () => {
  it("PDV-FR-VKRL: renders only the displayed page to a canvas with a text layer above it", async () => {
    pdfReadReturns(pdfPayload());
    await opened();
    expect(state.renders.map((r) => r.page)).toEqual([1]);
    const region = screen.getByRole("region", { name: "PDF page" });
    const canvas = region.querySelector("canvas");
    const layer = region.querySelector(".pdf-text-layer");
    expect(canvas).not.toBeNull();
    expect(layer).not.toBeNull();
    expect(layer!.textContent).toBe("Alpha page");
    // The text layer follows the canvas in the sheet, so it sits above it.
    expect(canvas!.nextElementSibling).toBe(layer);
  });

  it("PDV-FR-VKRL: shows Loading page… in place of the page while it renders", async () => {
    pdfReadReturns(pdfPayload());
    state.holdRenders = true;
    render(viewer());
    expect(await screen.findByText("Loading page…")).toBeInTheDocument();
    const sheet = document.querySelector(".pdf-page-sheet");
    expect(sheet).toHaveAttribute("data-state", "loading");
    // The page shows Loading page… before it starts the render. Release only
    // a render that is held, or the render that starts later stays held.
    await waitFor(() => expect(state.renders).toHaveLength(1));
    expect(screen.getByText("Loading page…")).toBeInTheDocument();
    expect(sheet).toHaveAttribute("data-state", "loading");
    releaseRenders();
    await waitFor(() => expect(screen.queryByText("Loading page…")).toBeNull());
    expect(sheet).toHaveAttribute("data-state", "ready");
  });

  it("PDV-FR-VKRL, PDV-FR-KJWR: a page that fails to render says so and logs an ERROR without detail", async () => {
    pdfReadReturns(pdfPayload());
    render(viewer());
    await screen.findByRole("region", { name: "PDF page" });
    await waitFor(() => expect(screen.queryByText("Loading page…")).toBeNull());
    cleanup();
    resetFixtures();
    pdfReadReturns(pdfPayload("p9"));
    pdfjsFake.getDocument.mockImplementationOnce(() => ({
      destroy: async () => {},
      promise: Promise.resolve({
        numPages: 1,
        getPage: async () => {
          throw new Error("render failure in /secret/guide.pdf");
        },
      }),
    }));
    render(viewer());
    expect(await screen.findByText("This PDF could not be displayed.")).toBeInTheDocument();
    expect(logErrorMock).toHaveBeenCalledWith(["frontend"], expect.any(String), {
      reason: "render",
    });
    expect(JSON.stringify(logErrorMock.mock.calls)).not.toContain("/secret");
  });

  it("PDV-FR-VKRL: a PDF of several hundred pages opens without rendering them all", async () => {
    pdfReadReturns(pdfPayload());
    state.pages = Array.from({ length: 500 }, (_, i) => page(`Page ${i + 1}`));
    await opened();
    expect(screen.getByText("of 500")).toBeInTheDocument();
    expect(state.renders).toHaveLength(1);
    expect(state.textContentReads).toEqual([1]);
  });
});

describe("PDV-FR-KJWR: logging", () => {
  it("PDV-FR-KJWR: an INFO record carries the page count and the byte length and nothing else", async () => {
    pdfReadReturns(pdfPayload("p1", "%PDF-1.7 twelve+"));
    await opened();
    expect(logInfoMock).toHaveBeenCalledTimes(1);
    const [domains, , fields] = logInfoMock.mock.calls[0];
    expect(domains).toEqual(["frontend"]);
    expect(fields).toEqual({ pages: 3, bytes: "%PDF-1.7 twelve+".length });
    const everything = JSON.stringify([
      ...logInfoMock.mock.calls,
      ...logWarnMock.mock.calls,
      ...logErrorMock.mock.calls,
    ]);
    expect(everything).not.toContain("guide.pdf");
    expect(everything).not.toContain("Alpha");
  });

  it("PDV-FR-KJWR: the search text and the page text never reach a log record", async () => {
    pdfReadReturns(pdfPayload());
    const user = userEvent.setup();
    await opened();
    await user.type(screen.getByRole("searchbox", { name: "Search text" }), "alpha");
    await screen.findByText("1 of 1");
    const everything = JSON.stringify([
      ...logInfoMock.mock.calls,
      ...logWarnMock.mock.calls,
      ...logErrorMock.mock.calls,
    ]);
    expect(everything.toLowerCase()).not.toContain("alpha");
  });
});

describe("PDV-FR-SLWS: unavailable", () => {
  it.each(["unavailable", "unknown_document"])(
    "PDV-FR-SLWS, PDV-FR-KJWR: a read that fails as %s shows the unavailable state and logs a warning",
    async (code) => {
      pdfReadRejects(code);
      render(viewer());
      expect(await screen.findByText("This document is unavailable.")).toBeInTheDocument();
      expect(screen.getByText(/may have been moved, deleted/)).toBeInTheDocument();
      expect(screen.queryByRole("toolbar")).toBeNull();
      expect(logWarnMock).toHaveBeenCalledWith(["frontend"], expect.any(String), {
        reason: "unavailable",
      });
    },
  );

  it("PDV-FR-SLWS: an entry that turns unavailable replaces the content and shows no stale page", async () => {
    pdfReadReturns(pdfPayload());
    await opened();
    await emitDocuments([entry({ format: "pdf", status: "unavailable", revision: undefined })]);
    expect(await screen.findByText("This document is unavailable.")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "PDF page" })).toBeNull();
    expect(document.querySelector("canvas")).toBeNull();
  });

  it("PDV-FR-SLWS: a document that is no longer in the collection shows the unavailable state", async () => {
    pdfReadReturns(pdfPayload());
    await opened();
    await emitDocuments([]);
    expect(await screen.findByText("This document is unavailable.")).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "PDF page" })).toBeNull();
  });
});

describe("PDV-FR-YOQS: following the collection", () => {
  it("PDV-FR-YOQS: a changed revision reads the new bytes, keeps the page, the zoom, and the search, and searches again", async () => {
    pdfReadReturns(pdfPayload("p1"));
    const user = userEvent.setup();
    await opened();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    await user.type(screen.getByRole("searchbox", { name: "Search text" }), "page");
    await screen.findByText("2 of 3");
    expect(pdfReadCalls()).toHaveLength(1);

    state.pages = [page("New page one"), page("New page two page"), page("x"), page("y")];
    pdfReadReturns(pdfPayload("p2", "%PDF-new"));
    await emitDocuments([entry({ format: "pdf", revision: "p2" })]);

    await waitFor(() => expect(pdfReadCalls()).toHaveLength(2));
    await waitFor(() => expect(screen.getByText("of 4")).toBeInTheDocument());
    expect(screen.getByRole("textbox", { name: "Page number" })).toHaveValue("2");
    expect(screen.getByText("125%")).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "Search text" })).toHaveValue("page");
    // The search ran again against the new content: three matches, and the
    // current one is the first at or after page 2.
    expect(await screen.findByText("2 of 3")).toBeInTheDocument();
    await waitFor(() =>
      expect(document.querySelector(".pdf-text-layer")!.textContent).toBe("New page two page"),
    );
  });

  it("PDV-FR-YOQS: an unchanged revision reads nothing again", async () => {
    pdfReadReturns(pdfPayload("p1"));
    await opened();
    await emitDocuments([entry({ format: "pdf", revision: "p1" })]);
    expect(pdfReadCalls()).toHaveLength(1);
  });

  it("PDV-FR-YOQS: the page is clamped to the new page count", async () => {
    pdfReadReturns(pdfPayload("p1"));
    const user = userEvent.setup();
    await opened();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.click(screen.getByRole("button", { name: "Next page" }));
    expect(screen.getByRole("textbox", { name: "Page number" })).toHaveValue("3");
    state.pages = [page("only one")];
    pdfReadReturns(pdfPayload("p2"));
    await emitDocuments([entry({ format: "pdf", revision: "p2" })]);
    await waitFor(() => expect(screen.getByText("of 1")).toBeInTheDocument());
    expect(screen.getByRole("textbox", { name: "Page number" })).toHaveValue("1");
  });

  it("PDV-FR-YOQS: an unavailable viewer that sees its document come back with a revision shows it again", async () => {
    pdfReadReturns(pdfPayload("p1"));
    await opened();
    await emitDocuments([entry({ format: "pdf", status: "unavailable", revision: undefined })]);
    await screen.findByText("This document is unavailable.");
    pdfReadReturns(pdfPayload("p2"));
    await emitDocuments([entry({ format: "pdf", revision: "p2" })]);
    expect(await screen.findByRole("region", { name: "PDF page" })).toBeInTheDocument();
    expect(screen.queryByText("This document is unavailable.")).toBeNull();
  });
});

describe("PDV-FR-INXM: read-only and release", () => {
  it("PDV-FR-INXM: offers no way to change, save, print, or export the PDF", async () => {
    pdfReadReturns(pdfPayload());
    const { container } = render(viewer());
    await screen.findByRole("region", { name: "PDF page" });
    for (const name of [/save/i, /print/i, /export/i, /download/i, /annotat/i]) {
      expect(screen.queryByRole("button", { name })).toBeNull();
    }
    expect(container.querySelector("[contenteditable], [role='textbox'][contenteditable]")).toBeNull();
    expect(invokeMock.mock.calls.every((c) => c[0] === "read_document_pdf")).toBe(true);
  });

  it("PDV-FR-INXM: closing the tab cancels the render in progress and releases the PDF.js document", async () => {
    pdfReadReturns(pdfPayload());
    state.holdRenders = true;
    const view = render(viewer());
    await screen.findByText("Loading page…");
    // The page shows Loading page… before it starts the render.
    await waitFor(() => expect(state.renders).toHaveLength(1));
    view.unmount();
    expect(state.renderCancels).toBe(1);
    expect(state.textLayerCancels).toBe(1);
    await waitFor(() => expect(state.documentsDestroyed).toBeGreaterThanOrEqual(1));
    releaseRenders();
  });

  it("PDV-FR-INXM: replacing the content releases the old PDF.js document", async () => {
    pdfReadReturns(pdfPayload("p1"));
    await opened();
    expect(state.documentsDestroyed).toBe(0);
    pdfReadReturns(pdfPayload("p2"));
    await emitDocuments([entry({ format: "pdf", revision: "p2" })]);
    await waitFor(() => expect(state.documentsOpened).toBe(2));
    await waitFor(() => expect(state.documentsDestroyed).toBe(1));
  });

  it("PDV-FR-INXM: changing the page cancels the render of the old page", async () => {
    pdfReadReturns(pdfPayload());
    const user = userEvent.setup();
    await opened();
    const before = state.renderCancels;
    await user.click(screen.getByRole("button", { name: "Next page" }));
    expect(state.renderCancels).toBeGreaterThan(before);
  });

  it("PDV-FR-GPLH: the viewer follows the event and stops following when it closes", async () => {
    pdfReadReturns(pdfPayload());
    const view = render(viewer());
    await screen.findByRole("region", { name: "PDF page" });
    expect(listenerCount()).toBe(1);
    view.unmount();
    await waitFor(() => expect(listenerCount()).toBe(0));
  });
});

describe("PDV-FR-DXUX: the state belongs to the tab", () => {
  it("PDV-FR-DXUX: the page, the zoom, and the search survive an unmount of the same tab and are written nowhere", async () => {
    pdfReadReturns(pdfPayload());
    const user = userEvent.setup();
    const first = render(viewer());
    await screen.findByRole("region", { name: "PDF page" });
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    await user.type(screen.getByRole("searchbox", { name: "Search text" }), "beta");
    first.unmount();
    render(viewer());
    await screen.findByRole("region", { name: "PDF page" });
    expect(screen.getByRole("textbox", { name: "Page number" })).toHaveValue("2");
    expect(screen.getByText("125%")).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "Search text" })).toHaveValue("beta");
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
  });

  it("PDV-FR-DXUX: a new tab for the same document starts on the first page at 100 percent with no search", async () => {
    pdfReadReturns(pdfPayload());
    const user = userEvent.setup();
    const first = render(viewer());
    await screen.findByRole("region", { name: "PDF page" });
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.click(screen.getByRole("button", { name: "Zoom in" }));
    await user.type(screen.getByRole("searchbox", { name: "Search text" }), "beta");
    first.unmount();
    // The shell forgets the state when the tab leaves the strip.
    resetViewStateForTest();
    render(viewer());
    await screen.findByRole("region", { name: "PDF page" });
    expect(screen.getByRole("textbox", { name: "Page number" })).toHaveValue("1");
    expect(screen.getByText("100%")).toBeInTheDocument();
    expect(screen.getByRole("searchbox", { name: "Search text" })).toHaveValue("");
  });
});
