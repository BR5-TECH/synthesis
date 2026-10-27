import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { resetPanelReveals } from "../state/panelReveal";
import { resetViewStateForTest } from "../state/documentViewState";
import type { DocumentEntry, DocumentsSnapshot, SearchHit } from "../types";

// Documents of the active worktree in the shell session (`TAB-tabs.md`
// TAB-FR-UEKW, TAB-FR-GVOS, TAB-FR-KUIN; `DPN-documents-panel.md` DPN-FR-CDFO;
// `LIB-library.md` LIB-FR-03; `OVW-overview.md` OVW-FR-VYPG;
// `SCH-search.md` SCH-FR-09).
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const { logWarnMock } = vi.hoisted(() => ({ logWarnMock: vi.fn() }));
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: logWarnMock,
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

const entry = (patch: Partial<DocumentEntry> = {}): DocumentEntry => ({
  id: "doc-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  path: "/work/proj/guides/intro.md",
  project_path: "guides/intro.md",
  name: "intro.md",
  format: "markdown",
  status: "available",
  revision: "r1",
  ...patch,
});
const textEntry = entry({
  id: "doc-cccccccccccccccccccccccccccccccc",
  path: "/work/proj/guides/notes.txt",
  project_path: "guides/notes.txt",
  name: "notes.txt",
  format: "text",
});
const pdfEntry = entry({
  id: "doc-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
  path: "/work/proj/guides/manual.pdf",
  project_path: "guides/manual.pdf",
  name: "manual.pdf",
  format: "pdf",
});

let snapshot: DocumentsSnapshot = { sources: [], documents: [] };
const BACKGROUND = new Set(["append_log_records", "set_save_menu_state", "set_find_menu_state"]);

beforeEach(() => {
  resetPanelReveals();
  resetViewStateForTest();
  invokeMock.mockReset();
  logWarnMock.mockReset();
  snapshot = { sources: [], documents: [entry(), textEntry, pdfEntry] };
  // The shell's background commands answer with nothing. Any other command
  // this file does not expect is refused, never thrown.
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_documents") return Promise.resolve(snapshot);
    if (BACKGROUND.has(cmd)) return Promise.resolve(undefined);
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});
afterEach(cleanup);

const editorTabs = <T extends { kind?: string }>(tabs: T[]) => tabs.filter((t) => t.kind === "editor");

describe("TAB-FR-UEKW: a document of the active worktree opens in the Editor", () => {
  it("TAB-FR-UEKW, DPN-FR-CDFO, OVW-FR-VYPG: a markdown and a text document with a project path open Editor tabs on that path", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(textEntry));

    const tabs = editorTabs(result.current.tabs);
    expect(tabs.map((t) => t.id)).toEqual(["art:guides/intro.md", "art:guides/notes.txt"]);
    expect(result.current.tabs.some((t) => t.kind === "document")).toBe(false);
    expect(result.current.activeTab).toBe("art:guides/notes.txt");
  });

  it("TAB-FR-UEKW, TAB-FR-04: a second open focuses the one Editor tab the tree opened too", () => {
    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.activateLibraryFile({
        id: "guides/intro.md",
        name: "intro.md",
        artifactType: "doc",
      }),
    );
    act(() => result.current.openDocument(entry()));
    act(() => result.current.openDocument(entry()));

    expect(editorTabs(result.current.tabs)).toHaveLength(1);
    expect(result.current.activeTab).toBe("art:guides/intro.md");
  });

  it("TAB-FR-UEKW, DTV-FR-MICG: a document with no project path still opens its Document tab", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry({ project_path: undefined })));
    expect(result.current.activeT).toMatchObject({ kind: "document", documentId: entry().id });
    expect(editorTabs(result.current.tabs)).toHaveLength(0);
  });

  it("TAB-FR-UEKW: a PDF with a project path opens its PDF Viewer tab, never the Editor", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(pdfEntry));
    expect(result.current.activeT).toMatchObject({ kind: "pdf", documentId: pdfEntry.id });
    expect(editorTabs(result.current.tabs)).toHaveLength(0);
  });

  it("TAB-FR-UEKW, SCH-FR-09: a document search hit inside the worktree opens the Editor", async () => {
    const { result } = renderHook(() => useShellSession());
    const hit = {
      id: entry().id,
      documentId: entry().id,
      name: "intro.md",
      path: "",
      group: "document",
      ordinal: 0,
    } as unknown as SearchHit;
    act(() => result.current.activateSearchHit(hit));
    await waitFor(() => expect(result.current.activeTab).toBe("art:guides/intro.md"));
  });

  it("TAB-FR-KUIN: a source removal leaves the Editor tab of a removed document open", () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(entry()));
    act(() => result.current.closeTabsForRemovedDocuments([entry().id]));
    expect(editorTabs(result.current.tabs).map((t) => t.id)).toEqual(["art:guides/intro.md"]);
  });
});

describe("TAB-FR-GVOS: a Document-typed PDF in the Project panel", () => {
  it("TAB-FR-GVOS, LIB-FR-03: opens the PDF Viewer tab of its document", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.activateLibraryFile({
        id: "guides/manual.pdf",
        name: "manual.pdf",
        artifactType: "doc",
      }),
    );
    await waitFor(() =>
      expect(result.current.activeT).toMatchObject({ kind: "pdf", documentId: pdfEntry.id }),
    );
    expect(editorTabs(result.current.tabs)).toHaveLength(0);
  });

  it("TAB-FR-GVOS: a PDF that is not in the collection opens nothing", async () => {
    snapshot = { sources: [], documents: [] };
    const { result } = renderHook(() => useShellSession());
    const before = result.current.tabs.length;
    act(() =>
      result.current.activateLibraryFile({
        id: "guides/other.pdf",
        name: "other.pdf",
        artifactType: "doc",
      }),
    );
    // The answer has been handled once the miss is logged.
    await waitFor(() =>
      expect(logWarnMock).toHaveBeenCalledWith(
        ["frontend"],
        "project file is not a document of the collection",
        { path: "guides/other.pdf" },
      ),
    );
    expect(result.current.tabs).toHaveLength(before);
  });

  it("LIB-FR-03: a Document-typed Markdown file opens in the Editor", () => {
    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.activateLibraryFile({
        id: "guides/intro.md",
        name: "intro.md",
        artifactType: "doc",
      }),
    );
    expect(result.current.activeT).toMatchObject({ kind: "editor", type: "doc" });
  });
});
