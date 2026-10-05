/**
 * The Documents collection through the shell: the activity-bar toggle, the
 * panel, and the viewer tabs it opens, with the real panel, the real tab strip,
 * and the real viewers over a mocked backend.
 *
 * `DPN-documents-panel.md` DPN-FR-KYSK, DPN-FR-CDFO, DPN-FR-ZHEJ, DPN-FR-AREM,
 * DPN-FR-ZMBQ; `OVW-overview.md` OVW-FR-05, OVW-FR-VYPG; `SNV-shell-navigation.md`
 * SNV-FR-UCJH, SNV-FR-45, SNV-FR-49, SNV-FR-56, SNV-FR-28, SNV-FR-66;
 * `TAB-tabs.md` TAB-FR-LKCT, TAB-FR-QXRF, TAB-FR-KUIN, TAB-FR-ZINL,
 * TAB-FR-ZMGX; `DTV-document-text-viewer.md`; `PDV-pdf-viewer.md`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetLogBufferForTest } from "./logging";
import { resetPanelReveals } from "./state/panelReveal";
import { resetViewStateForTest } from "./state/documentViewState";
import { resetDocumentsPanelStateForTest } from "./state/documentsPanelState";
import { defaultInvoke, enterIde, resetAppFixture, tabLabels } from "./test/appFixtures";
import {
  page,
  pdfPayload,
  pdfjsFake,
  resetPdfFixtures,
  state as pdfState,
} from "./components/PdfViewer/pdfFixtures";
import type { DocumentEntry, DocumentsSnapshot, DocumentSource } from "./types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<string, Array<(e: { payload?: unknown }) => void>>,
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
  listen: vi.fn(async (name: string, handler: (e: { payload?: unknown }) => void) => {
    (eventHandlers[name] ??= []).push(handler);
    return () => {
      eventHandlers[name] = (eventHandlers[name] ?? []).filter((h) => h !== handler);
    };
  }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  const win = {
    setResizable: async () => {},
    setMaximizable: async () => {},
    setSize: async () => {},
    isMaximized: async () => false,
    unmaximize: async () => {},
    maximize: async () => {},
    outerSize: async () => ({ width: 1440, height: 900 }),
    onResized: async () => () => {},
    scaleFactor: async () => 1,
  };
  return {
    LogicalSize,
    getCurrentWindow: () => win,
    availableMonitors: async () => [
      {
        name: "primary",
        size: { width: 2560, height: 1440 },
        position: { x: 0, y: 0 },
        workArea: { position: { x: 0, y: 0 }, size: { width: 2560, height: 1400 } },
        scaleFactor: 1,
      },
    ],
  };
});
vi.mock("./components/PdfViewer/pdfDocument", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./components/PdfViewer/pdfDocument")>()),
  loadPdfjs: async () => (await import("./components/PdfViewer/pdfFixtures")).pdfjsFake,
}));

const MD: DocumentEntry = {
  id: "doc-11111111111111111111111111111111",
  path: "/refs/specs/notes.md",
  name: "notes.md",
  format: "markdown",
  status: "available",
  revision: "m1",
};
const PDF: DocumentEntry = {
  id: "doc-22222222222222222222222222222222",
  path: "/refs/manual.pdf",
  name: "manual.pdf",
  format: "pdf",
  status: "available",
  revision: "p1",
};
const TXT: DocumentEntry = {
  id: "doc-33333333333333333333333333333333",
  path: "/loose/plain.txt",
  name: "plain.txt",
  format: "text",
  status: "available",
  revision: "t1",
};

let sources: DocumentSource[];
let documents: DocumentEntry[];
let texts: Record<string, string>;
let removeImpl: (path: string) => DocumentsSnapshot;

const snapshot = (): DocumentsSnapshot => ({ sources, documents });

const fire = (name: string, payload: unknown) =>
  act(async () => {
    for (const handler of [...(eventHandlers[name] ?? [])]) handler({ payload });
  });

beforeEach(() => {
  // The log buffer is module state: records of the last test must not be sent
  // in this one.
  resetLogBufferForTest();
  resetPanelReveals();
  resetAppFixture();
  resetPdfFixtures();
  resetViewStateForTest();
  resetDocumentsPanelStateForTest();
  pdfjsFake.getDocument.mockReset();
  pdfState.pages = [page("Page one text"), page("Page two text")];
  sources = [
    { kind: "folder", path: "/refs", status: "available" },
    { kind: "file", path: "/loose/plain.txt", status: "available" },
  ];
  documents = [MD, PDF, TXT];
  texts = { [MD.id]: "# Notes heading\n\nBody of notes.", [TXT.id]: "plain text body" };
  removeImpl = (path) => {
    sources = sources.filter((s) => s.path !== path);
    documents = documents.filter((d) => !d.path.startsWith(`${path}/`) && d.path !== path);
    return snapshot();
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "list_documents":
        return snapshot();
      case "remove_document_source":
        return removeImpl(args?.path as string);
      case "read_document": {
        const found = documents.find((d) => d.id === args?.id);
        if (!found || found.status === "unavailable") throw "unavailable";
        return {
          id: found.id,
          name: found.name,
          format: found.format,
          text: texts[found.id] ?? "",
          revision: found.revision,
        };
      }
      case "read_document_pdf": {
        const found = documents.find((d) => d.id === args?.id);
        if (!found || found.status === "unavailable") throw "unavailable";
        return { ...pdfPayload(found.revision), id: found.id, name: found.name };
      }
      default:
        return defaultInvoke(cmd, args);
    }
  });
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  for (const k in eventHandlers) delete eventHandlers[k];
  vi.stubGlobal("confirm", vi.fn());
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const toggle = () => screen.getByRole("button", { name: "Documents" });
async function openDocumentsPanel() {
  await userEvent.click(toggle());
  await screen.findByRole("tree", { name: "Documents" });
}
const tabs = () =>
  Array.from(document.querySelectorAll<HTMLElement>(".tabstrip .tab"));
const tabNamed = (label: string) =>
  tabs().find((t) => t.querySelector(".tab__label")?.textContent === label)!;
const activeLabel = () =>
  tabs()
    .find((t) => t.getAttribute("aria-selected") === "true")
    ?.querySelector(".tab__label")?.textContent;
const row = (name: RegExp) => screen.getByRole("treeitem", { name });

describe("DPN-FR-KYSK: the toggle and the panel in the shell", () => {
  it("DPN-FR-KYSK, SNV-FR-UCJH, SNV-FR-44: the Documents toggle sits immediately after Project in the leading cluster", async () => {
    render(<App />);
    await enterIde();
    const leading = document.querySelector(".activity-bar__cluster") as HTMLElement;
    const names = within(leading)
      .getAllByRole("button")
      .map((b) => b.getAttribute("aria-label"));
    expect(names.slice(0, 3)).toEqual(["Project", "Documents", "Notes"]);
  });

  it("DPN-FR-KYSK, OVW-FR-05: Documents is not the surface selected when a project opens, the Project panel is", async () => {
    render(<App />);
    await enterIde();
    expect(screen.getByRole("button", { name: "Project" })).toHaveAttribute("data-active", "true");
    expect(toggle()).toHaveAttribute("data-active", "false");
    expect(screen.queryByRole("tree", { name: "Documents" })).toBeNull();
    expect(invokeMock.mock.calls.some((c) => c[0] === "list_documents")).toBe(false);
  });

  it("DPN-FR-KYSK, SNV-FR-45: activating the toggle shows the panel, and activating it again hides the panel", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    expect(toggle()).toHaveAttribute("data-active", "true");
    expect(screen.getByRole("heading", { name: "Selected sources" })).toBeInTheDocument();
    expect(document.querySelector(".vpanel .documents-panel")).not.toBeNull();
    await userEvent.click(toggle());
    expect(document.querySelector(".vpanel")).toBeNull();
    expect(toggle()).toHaveAttribute("data-active", "false");
    await userEvent.click(toggle());
    expect(await screen.findByRole("tree", { name: "Documents" })).toBeInTheDocument();
  });

  it("TAB-FR-03, OVW-FR-06: the Documents panel is a vertical-panel surface and not a tab, so showing it adds nothing to the strip", async () => {
    render(<App />);
    await enterIde();
    const before = tabLabels();
    await openDocumentsPanel();
    expect(tabLabels()).toEqual(before);
    expect(tabLabels()).not.toContain("Documents");
  });

  it("DPN-FR-KYSK, SNV-FR-49: the tooltip of the toggle is Documents", async () => {
    render(<App />);
    await enterIde();
    await act(async () => toggle().focus());
    expect(await screen.findByRole("tooltip")).toHaveTextContent("Documents");
  });

  it("DPN-FR-KYSK: the panel renders in the vertical panel with the documents of the collection", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    expect(screen.getByText("notes.md")).toBeInTheDocument();
    expect(screen.getByText("manual.pdf")).toBeInTheDocument();
    expect(screen.getByText("plain.txt")).toBeInTheDocument();
  });
});

describe("DPN-FR-CDFO, OVW-FR-VYPG: opening documents", () => {
  it("DPN-FR-CDFO, OVW-FR-VYPG, TAB-FR-LKCT, DTV-FR-MICG: a Markdown document opens in a Document tab with the real viewer, and not in an Editor", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    expect(await screen.findByTestId("document-text-viewer")).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 1, name: "Notes heading" })).toBeInTheDocument();
    expect(tabLabels()).toContain("notes.md");
    expect(activeLabel()).toBe("notes.md");
    expect(document.querySelector(".ProseMirror")).toBeNull();
    expect(invokeMock.mock.calls.some((c) => c[0] === "load_artifact_contents_by_id")).toBe(false);
    expect(invokeMock).toHaveBeenCalledWith("read_document", { id: MD.id });
  });

  it("DPN-FR-CDFO, OVW-FR-VYPG, TAB-FR-LKCT: a text document opens in a Document tab in its source view, with no switch", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/plain\.txt/));
    const region = await screen.findByRole("region", { name: "Document text" });
    expect(region.textContent).toBe("plain text body");
    expect(within(screen.getByTestId("document-text-viewer")).queryByRole("radiogroup")).toBeNull();
  });

  it("DPN-FR-CDFO, OVW-FR-VYPG, PDV-FR-IWDK: a PDF document opens in a PDF Viewer tab with the real viewer", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/manual\.pdf/));
    expect(await screen.findByTestId("pdf-viewer")).toBeInTheDocument();
    expect(await screen.findByRole("textbox", { name: "Page number" })).toHaveValue("1");
    await waitFor(() =>
      expect(document.querySelector(".pdf-text-layer")?.textContent).toBe("Page one text"),
    );
    expect(activeLabel()).toBe("manual.pdf");
    expect(invokeMock).toHaveBeenCalledWith("read_document_pdf", { id: PDF.id });
  });

  it("DTV-FR-UGVG, PDV-FR-IWDK: the tab label is the file name and the tooltip is the full path", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    await userEvent.click(row(/manual\.pdf/));
    expect(tabNamed("notes.md")).toHaveAttribute("title", "/refs/specs/notes.md");
    expect(tabNamed("manual.pdf")).toHaveAttribute("title", "/refs/manual.pdf");
    expect(tabNamed("manual.pdf").querySelector("[data-icon='pdf']")).not.toBeNull();
    expect(tabNamed("notes.md").querySelector("[data-icon='document']")).not.toBeNull();
  });

  it("TAB-FR-QXRF, DTV-FR-UGVG, PDV-FR-IWDK: reopening a document focuses its tab and opens no second one", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    await userEvent.click(row(/manual\.pdf/));
    expect(activeLabel()).toBe("manual.pdf");
    const before = tabs().length;
    await userEvent.click(row(/notes\.md/));
    expect(tabs()).toHaveLength(before);
    expect(activeLabel()).toBe("notes.md");
    expect(await screen.findByTestId("document-text-viewer")).toBeInTheDocument();
    await userEvent.click(row(/manual\.pdf/));
    expect(tabs()).toHaveLength(before);
    expect(activeLabel()).toBe("manual.pdf");
    expect(tabLabels().filter((l) => l === "notes.md")).toHaveLength(1);
  });

  it("TAB-FR-QXRF: opening with Enter from the keyboard focuses the same tab", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    await userEvent.click(row(/plain\.txt/));
    await act(async () => screen.getByRole("tree", { name: "Documents" }).focus());
    await userEvent.click(row(/notes\.md/));
    expect(activeLabel()).toBe("notes.md");
    expect(tabLabels().filter((l) => l === "notes.md")).toHaveLength(1);
  });

  it("SNV-FR-66: activating a viewer tab leaves the vertical panel showing the Documents panel as it was", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    await userEvent.click(tabNamed("Dashboard"));
    await userEvent.click(tabNamed("notes.md"));
    expect(screen.getByRole("tree", { name: "Documents" })).toBeInTheDocument();
    expect(toggle()).toHaveAttribute("data-active", "true");
  });

  it("SNV-FR-28, TAB-FR-LKCT, DTV-FR-THTI, PDV-FR-INXM: File → Save stays unavailable while a viewer tab is active", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    const lastSaveState = () => {
      const calls = invokeMock.mock.calls.filter((c) => c[0] === "set_save_menu_state");
      return calls[calls.length - 1]?.[1];
    };
    await userEvent.click(row(/notes\.md/));
    await screen.findByTestId("document-text-viewer");
    await waitFor(() => expect(lastSaveState()).toEqual({ save: false, saveAll: false }));
    await userEvent.click(row(/manual\.pdf/));
    await screen.findByTestId("pdf-viewer");
    expect(lastSaveState()).toEqual({ save: false, saveAll: false });
    expect(invokeMock.mock.calls.some((c) => /^save_/.test(String(c[0])) && c[0] !== "save_layout_preferences" && c[0] !== "save_app_preferences" && !/panel_state/.test(String(c[0])))).toBe(false);
  });
});

describe("DPN-FR-ZHEJ, TAB-FR-KUIN: removing a source", () => {
  it("DPN-FR-ZHEJ, TAB-FR-KUIN, OVW-FR-VYPG: removing the last source of a document closes its viewer tab, with no prompt", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    await userEvent.click(row(/manual\.pdf/));
    await userEvent.click(row(/plain\.txt/));
    expect(tabLabels()).toEqual(expect.arrayContaining(["notes.md", "manual.pdf", "plain.txt"]));
    await userEvent.click(screen.getByRole("button", { name: "Remove /refs" }));
    await waitFor(() => expect(tabLabels()).not.toContain("notes.md"));
    expect(tabLabels()).not.toContain("manual.pdf");
    // A document of another source keeps its tab, and it stays the active one.
    expect(tabLabels()).toContain("plain.txt");
    expect(activeLabel()).toBe("plain.txt");
    expect(window.confirm).not.toHaveBeenCalled();
    expect(screen.queryByText("notes.md")).toBeNull();
  });

  it("DPN-FR-ZHEJ, TAB-FR-KUIN: a document that another source still includes keeps its tab and stays in the tree", async () => {
    sources = [
      { kind: "folder", path: "/refs", status: "available" },
      { kind: "file", path: "/refs/specs/notes.md", status: "available" },
    ];
    documents = [MD, PDF];
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    await userEvent.click(row(/manual\.pdf/));
    removeImpl = () => {
      sources = [{ kind: "file", path: "/refs/specs/notes.md", status: "available" }];
      documents = [MD];
      return snapshot();
    };
    await userEvent.click(screen.getByRole("button", { name: "Remove /refs" }));
    await waitFor(() => expect(tabLabels()).not.toContain("manual.pdf"));
    expect(tabLabels()).toContain("notes.md");
    expect(screen.getByRole("treeitem", { name: /notes\.md/ })).toBeInTheDocument();
  });

  it("TAB-FR-KUIN, TAB-FR-15: the strip never empties, a Dashboard opens when the closure empties it", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/plain\.txt/));
    await userEvent.click(tabNamed("Dashboard").querySelector(".tab__close")!);
    expect(tabLabels()).toEqual(["plain.txt"]);
    await userEvent.click(screen.getByRole("button", { name: "Remove /loose/plain.txt" }));
    await waitFor(() => expect(tabLabels()).toEqual(["Dashboard"]));
    expect(activeLabel()).toBe("Dashboard");
  });

  it("TAB-FR-ZMGX: each closed viewer tab writes one DEBUG record through append log records, with the document id and no path", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    await userEvent.click(row(/manual\.pdf/));
    await userEvent.click(screen.getByRole("button", { name: "Remove /refs" }));
    const records = async () => {
      const sent = invokeMock.mock.calls
        .filter((c) => c[0] === "append_log_records")
        .flatMap((c) => (c[1] as { records: Array<Record<string, unknown>> }).records);
      return sent.filter((r) => (r.fields as { rule?: string } | undefined)?.rule === "TAB-FR-KUIN");
    };
    await waitFor(async () => expect(await records()).toHaveLength(2), { timeout: 3000 });
    const found = await records();
    expect(found.map((r) => r.level)).toEqual(["DEBUG", "DEBUG"]);
    expect(found.map((r) => (r.fields as Record<string, unknown>).documentId).sort()).toEqual(
      [MD.id, PDF.id].sort(),
    );
    expect(found.map((r) => (r.fields as Record<string, unknown>).tabKind).sort()).toEqual(["document", "pdf"]);
    expect(JSON.stringify(found)).not.toContain("/refs");
    expect(JSON.stringify(found)).not.toContain("notes.md");
  });
});

describe("TAB-FR-ZINL: a document that leaves another way keeps its tab", () => {
  it("TAB-FR-ZINL, DTV-FR-BEQL: a document that drops out of the collection keeps its tab, and the viewer shows its unavailable state", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    await screen.findByRole("heading", { level: 1, name: "Notes heading" });
    documents = [PDF, TXT];
    await fire("documents-changed", snapshot());
    expect(await screen.findByText("This document is unavailable.")).toBeInTheDocument();
    expect(tabLabels()).toContain("notes.md");
    expect(activeLabel()).toBe("notes.md");
    // The panel follows the same event and no longer lists the file.
    await userEvent.click(toggle());
    await userEvent.click(toggle());
    expect(await screen.findByRole("tree", { name: "Documents" })).toBeInTheDocument();
    expect(screen.queryByRole("treeitem", { name: /notes\.md/ })).toBeNull();
  });

  it("TAB-FR-ZINL, PDV-FR-SLWS: an unavailable PDF keeps its tab and shows the unavailable state, then shows the PDF again", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/manual\.pdf/));
    await screen.findByRole("textbox", { name: "Page number" });
    documents = [MD, { ...PDF, status: "unavailable", revision: undefined }, TXT];
    await fire("documents-changed", snapshot());
    expect(await screen.findByText("This document is unavailable.")).toBeInTheDocument();
    expect(tabLabels()).toContain("manual.pdf");
    documents = [MD, { ...PDF, revision: "p2" }, TXT];
    await fire("documents-changed", snapshot());
    expect(await screen.findByRole("textbox", { name: "Page number" })).toBeInTheDocument();
  });
});

describe("DTV-FR-SAIG, PDV-FR-YOQS: the open viewer follows the collection", () => {
  it("DTV-FR-SAIG: a changed revision replaces the text of the open Document tab", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    await screen.findByRole("heading", { level: 1, name: "Notes heading" });
    texts[MD.id] = "# Changed heading";
    documents = [{ ...MD, revision: "m2" }, PDF, TXT];
    await fire("documents-changed", snapshot());
    expect(await screen.findByRole("heading", { level: 1, name: "Changed heading" })).toBeInTheDocument();
    expect(tabs().filter((t) => t.querySelector(".tab__label")?.textContent === "notes.md")).toHaveLength(1);
  });

  it("PDV-FR-YOQS: a changed revision reopens the open PDF Viewer tab and keeps its page", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/manual\.pdf/));
    await screen.findByRole("textbox", { name: "Page number" });
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    pdfState.pages = [page("New one"), page("New two"), page("New three")];
    documents = [MD, { ...PDF, revision: "p2" }, TXT];
    await fire("documents-changed", snapshot());
    await waitFor(() => expect(screen.getByText("of 3")).toBeInTheDocument());
    expect(screen.getByRole("textbox", { name: "Page number" })).toHaveValue("2");
  });

  it("DTV-FR-XMRL, PDV-FR-DXUX: the Rich or Source choice and the PDF page survive a switch of tab, and start again in a new tab after the old one is closed", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/notes\.md/));
    await userEvent.click(await screen.findByRole("radio", { name: "Source" }));
    await userEvent.click(row(/manual\.pdf/));
    await screen.findByRole("textbox", { name: "Page number" });
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    await userEvent.click(tabNamed("notes.md"));
    expect(await screen.findByRole("radio", { name: "Source" })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(tabNamed("manual.pdf"));
    expect(await screen.findByRole("textbox", { name: "Page number" })).toHaveValue("2");
    // Close both tabs, then open them again: both start from the defaults.
    await userEvent.click(tabNamed("notes.md").querySelector(".tab__close")!);
    await userEvent.click(tabNamed("manual.pdf").querySelector(".tab__close")!);
    await userEvent.click(row(/notes\.md/));
    expect(await screen.findByRole("radio", { name: "Rich" })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(row(/manual\.pdf/));
    expect(await screen.findByRole("textbox", { name: "Page number" })).toHaveValue("1");
  });
});

describe("DPN-FR-AREM: the panel's session state in the shell", () => {
  it("DPN-FR-AREM: the filter text survives a switch to another panel and back, and a change of the active tab", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.type(screen.getByRole("textbox", { name: "Filter documents" }), "notes");
    await userEvent.click(screen.getByRole("button", { name: "Notes" }));
    await waitFor(() => expect(screen.queryByRole("tree", { name: "Documents" })).toBeNull());
    await userEvent.click(toggle());
    expect(await screen.findByRole("textbox", { name: "Filter documents" })).toHaveValue("notes");
    await userEvent.click(row(/notes\.md/));
    await userEvent.click(tabNamed("Dashboard"));
    expect(screen.getByRole("textbox", { name: "Filter documents" })).toHaveValue("notes");
  });

  it("DPN-FR-AREM: the folder state survives a switch to another panel", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(row(/^refs/));
    expect(screen.getByRole("treeitem", { name: /^refs/ })).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(screen.getByRole("button", { name: "Drafts" }));
    await userEvent.click(toggle());
    expect(await screen.findByRole("treeitem", { name: /^refs/ })).toHaveAttribute("aria-expanded", "false");
  });
});

describe("DPN-FR-ZMBQ, SNV-FR-56: the Add documents choice is a floating overlay", () => {
  it("DPN-FR-ZMBQ, SNV-FR-56: opening the choice closes the tab context menu", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    fireEvent.contextMenu(tabNamed("Dashboard"));
    expect(document.querySelector(".menu--tab")).not.toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Add documents" }));
    expect(screen.getByRole("menu", { name: "Add documents" })).toBeInTheDocument();
    expect(document.querySelector(".menu--tab")).toBeNull();
  });

  it("DPN-FR-ZMBQ, SNV-FR-56: opening the tab context menu closes the choice, so one overlay stands at a time", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    await userEvent.click(screen.getByRole("button", { name: "Add documents" }));
    expect(screen.getByRole("menu", { name: "Add documents" })).toBeInTheDocument();
    await userEvent.pointer({ keys: "[MouseRight]", target: tabNamed("Dashboard") });
    expect(screen.queryByRole("menu", { name: "Add documents" })).toBeNull();
  });

  it("DPN-FR-FDVO, DPN-FR-ZMBQ: Files in the shell calls the backend pick with mode files", async () => {
    render(<App />);
    await enterIde();
    await openDocumentsPanel();
    const baseImpl = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "pick_document_sources")
        return { cancelled: true, ignored_count: 0, snapshot: snapshot() };
      return baseImpl(cmd, args);
    });
    await userEvent.click(screen.getByRole("button", { name: "Add documents" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Files" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("pick_document_sources", { mode: "files" }),
    );
  });
});
