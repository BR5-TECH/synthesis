import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { followTargetForTab } from "../state/selectionFollowsTab";
import { resetPanelReveals } from "../state/panelReveal";
import {
  getPdfViewState,
  getTextViewState,
  resetViewStateForTest,
  setPdfViewState,
  setTextViewState,
} from "../state/documentViewState";
import { targetForTab } from "../state/notificationAddress";
import type { DocumentEntry } from "../types";

// The viewer tabs of the Documents collection in the shell session
// (`TAB-tabs.md` TAB-FR-LKCT, TAB-FR-QXRF, TAB-FR-KUIN, TAB-FR-ZINL,
// TAB-FR-FJXM, TAB-FR-ZMGX, TAB-FR-19; `DPN-documents-panel.md` DPN-FR-CDFO,
// DPN-FR-ZHEJ; `DTV-document-text-viewer.md` DTV-FR-MICG, DTV-FR-UGVG,
// DTV-FR-THTI, DTV-FR-XMRL; `PDV-pdf-viewer.md` PDV-FR-IWDK, PDV-FR-INXM,
// PDV-FR-DXUX; `OVW-overview.md` OVW-FR-VYPG; `SNV-shell-navigation.md`
// SNV-FR-28, SNV-FR-66).
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const logDebugMock = vi.fn();
vi.mock("../logging", () => ({
  logDebug: (...args: unknown[]) => logDebugMock(...args),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

type EventHandler = (ev: { payload: unknown }) => void;
let eventHandlers: [string, EventHandler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    eventHandlers.push([name, handler]);
    return Promise.resolve(() => {
      eventHandlers = eventHandlers.filter(([, h]) => h !== handler);
    });
  },
}));
const fireEvent = (name: string, payload: unknown) =>
  eventHandlers.filter(([n]) => n === name).forEach(([, h]) => h({ payload }));

beforeEach(() => {
  resetPanelReveals();
  resetViewStateForTest();
  eventHandlers = [];
  invokeMock.mockReset();
  logDebugMock.mockReset();
  invokeMock.mockImplementation(async () => undefined);
});
afterEach(cleanup);

/** The calls that reach a command, without the two menu-state pushes of the shell. */
const backendCalls = () =>
  invokeMock.mock.calls.filter(
    (c) => c[0] !== "set_save_menu_state" && c[0] !== "set_find_menu_state",
  );

const entry = (patch: Partial<DocumentEntry> = {}): DocumentEntry => ({
  id: "doc-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  path: "/refs/specs/api-guide.md",
  name: "api-guide.md",
  format: "markdown",
  status: "available",
  revision: "r1",
  ...patch,
});
const pdfEntry = entry({
  id: "doc-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  path: "/refs/manual.pdf",
  name: "manual.pdf",
  format: "pdf",
});
const textEntry = entry({
  id: "doc-cccccccccccccccccccccccccccccccc",
  path: "/refs/old.txt",
  name: "old.txt",
  format: "text",
});

describe("TAB-FR-LKCT: the two viewer tab kinds", () => {
  it("DPN-FR-CDFO, TAB-FR-02, TAB-FR-LKCT, DTV-FR-MICG: a markdown document opens in a Document tab and a text document in a Document tab too", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(textEntry));
    const tabs = result.current.tabs.filter((t) => t.id !== "dashboard");
    expect(tabs.map((t) => t.kind)).toEqual(["document", "document"]);
    expect(tabs.map((t) => t.documentId)).toEqual([entry().id, textEntry.id]);
  });

  it("DPN-FR-CDFO, TAB-FR-LKCT, PDV-FR-IWDK: a pdf document opens in a PDF Viewer tab", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(pdfEntry));
    const tab = result.current.tabs.find((t) => t.kind === "pdf");
    expect(tab).toBeDefined();
    expect(tab!.documentId).toBe(pdfEntry.id);
  });

  it("DPN-FR-CDFO, DTV-FR-UGVG, PDV-FR-IWDK: the tab label is the file name and its tooltip is the full path", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(pdfEntry));
    const doc = result.current.tabs.find((t) => t.kind === "document")!;
    const pdf = result.current.tabs.find((t) => t.kind === "pdf")!;
    expect(doc.label).toBe("api-guide.md");
    expect(doc.tooltip).toBe("/refs/specs/api-guide.md");
    expect(pdf.label).toBe("manual.pdf");
    expect(pdf.tooltip).toBe("/refs/manual.pdf");
  });

  it("DPN-FR-CDFO, TAB-FR-LKCT: opening focuses the new tab", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(pdfEntry));
    expect(result.current.activeT?.kind).toBe("pdf");
    expect(result.current.activeTab).toBe(`doc:${pdfEntry.id}`);
  });

  it("DPN-FR-CDFO: an unavailable document opens its tab on the same terms", () => {
    const { result } = renderHook(() => useShellSession());
    const gone = entry({ status: "unavailable", revision: undefined });
    act(() => result.current.openDocument(gone));
    expect(result.current.activeT).toMatchObject({ kind: "document", documentId: gone.id });
  });

  it("TAB-FR-LKCT, DTV-FR-MICG: neither kind is an Editor tab: no artifact id, no Editor session, and no Editor tab for the same file", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    const tab = result.current.activeT!;
    expect(tab.kind).not.toBe("editor");
    expect(tab.artifactId).toBeUndefined();
    expect(result.current.tabs.filter((t) => t.kind === "editor")).toHaveLength(0);
    expect(result.current.sessions.dirtyIds()).toEqual([]);
    expect(backendCalls()).toEqual([]);
  });

  it("TAB-FR-LKCT, TAB-FR-QXRF: a viewer tab coexists with an Editor tab on the same file", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "docs/api-guide.md", name: "api-guide.md" }));
    act(() => result.current.openDocument(entry({ path: "/refs/docs/api-guide.md" })));
    const kinds = result.current.tabs.map((t) => t.kind);
    expect(kinds.filter((k) => k === "editor")).toHaveLength(1);
    expect(kinds.filter((k) => k === "document")).toHaveLength(1);
    expect(new Set(result.current.tabs.map((t) => t.id)).size).toBe(result.current.tabs.length);
  });
});

describe("TAB-FR-QXRF: one viewer tab per document", () => {
  it("TAB-FR-QXRF, DTV-FR-UGVG, PDV-FR-IWDK: reopening a document focuses the existing tab and opens no second one", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(pdfEntry));
    expect(result.current.activeTab).toBe(`doc:${pdfEntry.id}`);
    const before = result.current.tabs.length;
    act(() => result.current.openDocument(entry()));
    expect(result.current.tabs).toHaveLength(before);
    expect(result.current.activeTab).toBe(`doc:${entry().id}`);
    act(() => result.current.openDocument(pdfEntry));
    expect(result.current.tabs).toHaveLength(before);
    expect(result.current.activeTab).toBe(`doc:${pdfEntry.id}`);
    expect(result.current.tabs.filter((t) => t.documentId === entry().id)).toHaveLength(1);
  });

  it("TAB-FR-QXRF: two requests that land before a render add one tab", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => {
      result.current.openDocument(entry());
      result.current.openDocument(entry());
    });
    expect(result.current.tabs.filter((t) => t.kind === "document")).toHaveLength(1);
  });

  it("TAB-FR-QXRF: the tab is identified by the document id, so a rename of the file in a new entry does not open another", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(entry({ name: "renamed.md", path: "/refs/renamed.md" })));
    expect(result.current.tabs.filter((t) => t.kind === "document")).toHaveLength(1);
  });
});

describe("TAB-FR-KUIN: closing when a removal takes the document out", () => {
  it("DPN-FR-ZHEJ, TAB-FR-KUIN, OVW-FR-VYPG: closes the viewer tabs of the removed documents and no other tab", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "docs/keep.md", name: "keep.md" }));
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(pdfEntry));
    act(() => result.current.openDocument(textEntry));
    act(() => result.current.closeTabsForRemovedDocuments([entry().id, pdfEntry.id]));
    const ids = result.current.tabs.map((t) => t.id);
    expect(ids).not.toContain(`doc:${entry().id}`);
    expect(ids).not.toContain(`doc:${pdfEntry.id}`);
    expect(ids).toContain(`doc:${textEntry.id}`);
    expect(ids).toContain("art:docs/keep.md");
    expect(ids).toContain("dashboard");
  });

  it("TAB-FR-KUIN: a document that stays through another source keeps its tab, because it is not reported", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.closeTabsForRemovedDocuments([]));
    act(() => result.current.closeTabsForRemovedDocuments(["doc-ffffffffffffffffffffffffffffffff"]));
    expect(result.current.tabs.map((t) => t.id)).toContain(`doc:${entry().id}`);
  });

  it("TAB-FR-KUIN: the close needs no prompt, writes nothing, and never calls the backend", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(pdfEntry));
    const confirm = vi.fn();
    vi.stubGlobal("confirm", confirm);
    act(() => result.current.closeTabsForRemovedDocuments([pdfEntry.id]));
    expect(confirm).not.toHaveBeenCalled();
    expect(backendCalls()).toEqual([]);
    expect(result.current.tabs.some((t) => t.kind === "pdf")).toBe(false);
    vi.unstubAllGlobals();
  });

  it("TAB-FR-KUIN, TAB-FR-15: the never-empty invariant holds, a Dashboard opens when the closure empties the strip", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    await act(async () => {
      await result.current.closeTab("dashboard");
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual([`doc:${entry().id}`]);
    act(() => result.current.closeTabsForRemovedDocuments([entry().id]));
    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(result.current.activeTab).toBe("dashboard");
  });

  it("TAB-FR-KUIN: when the active viewer tab closes the focus falls to the last remaining tab, and an inactive tab's closure leaves the focus", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(pdfEntry));
    act(() => result.current.openDocument(textEntry));
    act(() => result.current.activateTab(`doc:${entry().id}`));
    act(() => result.current.closeTabsForRemovedDocuments([pdfEntry.id]));
    expect(result.current.activeTab).toBe(`doc:${entry().id}`);
    act(() => result.current.closeTabsForRemovedDocuments([entry().id]));
    expect(result.current.activeTab).toBe(`doc:${textEntry.id}`);
  });

  it("TAB-FR-ZMGX: writes one DEBUG record per closed viewer tab, naming the document id, the kind of tab, and the rule, and no path or file name", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(pdfEntry));
    logDebugMock.mockClear();
    act(() => result.current.closeTabsForRemovedDocuments([entry().id, pdfEntry.id]));
    expect(logDebugMock).toHaveBeenCalledTimes(2);
    const records = logDebugMock.mock.calls.map((c) => c[2]);
    expect(records).toEqual([
      { documentId: entry().id, tabKind: "document", rule: "TAB-FR-KUIN" },
      { documentId: pdfEntry.id, tabKind: "pdf", rule: "TAB-FR-KUIN" },
    ]);
    for (const call of logDebugMock.mock.calls) expect(call[0]).toEqual(["frontend"]);
    const text = JSON.stringify(logDebugMock.mock.calls);
    expect(text).not.toContain("/refs");
    expect(text).not.toContain("api-guide");
    expect(text).not.toContain("manual.pdf");
  });

  it("TAB-FR-ZMGX, TAB-FR-FJXM: no record is produced for a viewer tab that the user closes", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    logDebugMock.mockClear();
    await act(async () => {
      expect(await result.current.closeTab(`doc:${entry().id}`)).toBe(true);
    });
    expect(logDebugMock).not.toHaveBeenCalled();
  });
});

describe("TAB-FR-FJXM: closing a viewer tab ends the tab and nothing else", () => {
  it("TAB-FR-FJXM: a close writes nothing, is never refused, and changes no source reference", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(pdfEntry));
    invokeMock.mockClear();
    await act(async () => {
      expect(await result.current.closeTab(`doc:${entry().id}`)).toBe(true);
      expect(await result.current.closeTab(`doc:${pdfEntry.id}`)).toBe(true);
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard"]);
    expect(backendCalls()).toEqual([]);
  });

  it("TAB-FR-FJXM, TAB-FR-38: a pinned viewer tab closes with the author's own close, and a mass close treats it as any other tab", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(pdfEntry));
    act(() => result.current.setTabPinned(`doc:${entry().id}`, true));
    await act(async () => {
      await result.current.closeTabGroup("others", `doc:${pdfEntry.id}`);
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual([`doc:${entry().id}`, `doc:${pdfEntry.id}`]);
    await act(async () => {
      expect(await result.current.closeTab(`doc:${entry().id}`)).toBe(true);
    });
    expect(result.current.tabs.map((t) => t.id)).toEqual([`doc:${pdfEntry.id}`]);
  });
});

describe("TAB-FR-ZINL: a document that leaves another way keeps its tab", () => {
  it("TAB-FR-ZINL: the tab is not closed by a project tree removal or by any event of the collection, because only a source removal closes it", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    await waitFor(() => expect(eventHandlers.some(([n]) => n === "project-tree-changed")).toBe(true));
    act(() => fireEvent("project-tree-changed", { changeCount: 1, removedPaths: ["/refs/specs/api-guide.md", "refs"] }));
    // The shell itself does not follow the collection's event: the viewer does,
    // and shows its unavailable state.
    act(() => fireEvent("documents-changed", { sources: [], documents: [] }));
    expect(result.current.tabs.map((t) => t.id)).toContain(`doc:${entry().id}`);
    expect(logDebugMock.mock.calls.filter((c) => c[2]?.rule === "TAB-FR-KUIN")).toHaveLength(0);
  });
});

describe("TAB-FR-19: path removals do not reach a viewer tab", () => {
  it("TAB-FR-19, TAB-FR-QXRF: a document tab carries no path of a project file, so removing a folder with the same name closes nothing", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry({ path: "docs/api-guide.md" })));
    await waitFor(() => expect(eventHandlers.some(([n]) => n === "project-tree-changed")).toBe(true));
    act(() => fireEvent("project-tree-changed", { changeCount: 1, removedPaths: ["docs", "docs/api-guide.md"] }));
    expect(result.current.tabs.map((t) => t.id)).toContain(`doc:${entry().id}`);
  });
});

describe("SNV-FR-28: a viewer tab is not savable", () => {
  it("TAB-FR-LKCT, DTV-FR-THTI, PDV-FR-INXM, SNV-FR-28: File → Save stays unavailable while a Document tab or a PDF Viewer tab is active", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    expect(result.current.saveEnabled).toBe(false);
    await act(async () => {
      await result.current.requestSave();
    });
    act(() => result.current.openDocument(pdfEntry));
    expect(result.current.saveEnabled).toBe(false);
    expect(result.current.saveAllEnabled).toBe(false);
    await act(async () => {
      await result.current.requestSave();
    });
    expect(backendCalls()).toEqual([]);
    expect(result.current.findEnabled).toBe(false);
    expect(result.current.historyEnabled).toBe(false);
  });

  it("DTV-FR-THTI, SNV-FR-28: a dirty Editor elsewhere does not enable Save for the viewer tab, but still enables Save All", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "docs/a.md", name: "a.md" }));
    act(() => {
      result.current.sessions.adoptLoad("docs/a.md", "x", "ck");
      result.current.sessions.update("docs/a.md", { buffer: "y", dirty: true });
    });
    act(() => result.current.openDocument(entry()));
    expect(result.current.saveEnabled).toBe(false);
    expect(result.current.saveAllEnabled).toBe(true);
  });
});

describe("SNV-FR-66: a viewer tab selects nowhere", () => {
  it("SNV-FR-66, OVW-FR-VYPG: activating a Document tab or a PDF Viewer tab names no item of any vertical panel", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(pdfEntry));
    for (const tab of result.current.tabs.filter((t) => t.documentId)) {
      expect(followTargetForTab(tab)).toBeNull();
      expect(targetForTab(tab)).toBeNull();
    }
  });

  it("SNV-FR-66: activating a viewer tab leaves the panel surface and its pending reveal as they were", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.setPanelSurface("documents"));
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(pdfEntry));
    act(() => result.current.activateTab(`doc:${entry().id}`));
    expect(result.current.panelSurface).toBe("documents");
    expect(result.current.panelReveal).toBeNull();
  });
});

describe("OVW-FR-VYPG: the viewer state belongs to the tab", () => {
  it("DTV-FR-XMRL, PDV-FR-DXUX: the Rich or Source choice and the page, zoom, and search are dropped when the tab closes by any route", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(pdfEntry));
    act(() => setTextViewState(entry().id, { mode: "source" }));
    act(() => setPdfViewState(pdfEntry.id, { page: 4, zoomIndex: 6, search: "q" }));
    expect(getTextViewState(entry().id).mode).toBe("source");
    // The user's own close.
    await act(async () => {
      await result.current.closeTab(`doc:${entry().id}`);
    });
    await waitFor(() => expect(getTextViewState(entry().id)).toEqual({}));
    // The automatic closure.
    expect(getPdfViewState(pdfEntry.id).page).toBe(4);
    act(() => result.current.closeTabsForRemovedDocuments([pdfEntry.id]));
    await waitFor(() => expect(getPdfViewState(pdfEntry.id)).toEqual({}));
  });

  it("DTV-FR-XMRL, PDV-FR-DXUX: a tab that is still open keeps its state", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => setTextViewState(entry().id, { mode: "source" }));
    act(() => result.current.openDocument(pdfEntry));
    await waitFor(() => expect(result.current.tabs).toHaveLength(3));
    expect(getTextViewState(entry().id).mode).toBe("source");
  });
});
