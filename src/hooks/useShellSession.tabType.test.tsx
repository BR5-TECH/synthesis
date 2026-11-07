import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { resetPanelReveals } from "../state/panelReveal";
import { resetViewStateForTest } from "../state/documentViewState";
import { typeChip } from "../artifactTypes";
import type { DocumentEntry, OpenedArtifact, Tab } from "../types";

// The artifact type of an Editor tab, whichever surface opens it (`TAB-tabs.md`
// TAB-FR-SZGD; `PST-project-storage.md` PST-FR-QQAE).
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

const doc: DocumentEntry = {
  id: "doc-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  path: "/work/proj/guides/intro.md",
  project_path: "guides/intro.md",
  name: "intro.md",
  format: "markdown",
  status: "available",
  revision: "r1",
};
const TAB = "art:guides/intro.md";
const BACKGROUND = new Set(["append_log_records", "set_save_menu_state", "set_find_menu_state"]);

/** What `open_artifact_by_id` answers; a function so a test can hold it. */
let openAnswer: (id: string) => Promise<OpenedArtifact>;

beforeEach(() => {
  resetPanelReveals();
  resetViewStateForTest();
  invokeMock.mockReset();
  logWarnMock.mockReset();
  openAnswer = (id) => Promise.resolve({ key: id, kind: "markdown", artifactType: "doc" });
  // The shell's background commands answer with nothing. Any other command
  // this file does not expect is refused, never thrown.
  invokeMock.mockImplementation((cmd: string, args?: { id?: string }) => {
    if (cmd === "open_artifact_by_id") return openAnswer(args?.id ?? "");
    if (BACKGROUND.has(cmd)) return Promise.resolve(undefined);
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});
afterEach(cleanup);

const typeReads = () => invokeMock.mock.calls.filter(([cmd]) => cmd === "open_artifact_by_id");
const tab = (tabs: Tab[]) => tabs.find((t) => t.id === TAB);

/** A promise and the function that settles it. */
function held<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

describe("TAB-FR-SZGD: an Editor tab carries the type of its file", () => {
  it("TAB-FR-SZGD, PST-FR-QQAE, EXD-FR-UQNO: a DOC opened from the Documents panel gets its type and chip", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(doc));

    await waitFor(() => expect(tab(result.current.tabs)).toMatchObject({ type: "doc" }));
    expect(tab(result.current.tabs)).toMatchObject({ kind: "editor", chip: typeChip("doc") });
    expect(typeReads()).toEqual([["open_artifact_by_id", { id: "guides/intro.md" }]]);
  });

  it("TAB-FR-SZGD: a surface that opens a file by path alone gets the type too", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openArtifact({ id: "guides/intro.md", name: "intro.md" }));
    await waitFor(() => expect(tab(result.current.tabs)).toMatchObject({ type: "doc" }));
  });

  it("TAB-FR-SZGD: a read that fails leaves the tab with no type and logs a WARN", async () => {
    openAnswer = () => Promise.reject(new Error("not a file"));
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(doc));

    await waitFor(() =>
      expect(logWarnMock).toHaveBeenCalledWith(["frontend"], "tab type could not be resolved", {
        error: "not a file",
      }),
    );
    expect(tab(result.current.tabs)).toBeDefined();
    expect(tab(result.current.tabs)?.type).toBeUndefined();
  });

  it("TAB-FR-SZGD, PST-FR-QQAE: a file with no type keeps a tab with no type and no chip", async () => {
    const answered = held<OpenedArtifact>();
    openAnswer = () => answered.promise;
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(doc));
    await act(async () => {
      answered.resolve({ key: "guides/intro.md", kind: "text" });
      await answered.promise;
    });

    expect(tab(result.current.tabs)?.type).toBeUndefined();
    expect(tab(result.current.tabs)?.chip).toBeUndefined();
    expect(logWarnMock).not.toHaveBeenCalled();
  });

  it("TAB-FR-SZGD: a surface that gives the type reads nothing, and a second open reads nothing", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() =>
      result.current.activateLibraryFile({
        id: "guides/intro.md",
        name: "intro.md",
        artifactType: "doc",
      }),
    );
    act(() => result.current.openDocument(doc));
    expect(tab(result.current.tabs)).toMatchObject({ type: "doc" });
    expect(typeReads()).toHaveLength(0);
  });

  it("TAB-FR-SZGD: an answer for a tab that closed changes no tab", async () => {
    const answered = held<OpenedArtifact>();
    openAnswer = () => answered.promise;
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(doc));
    await act(async () => {
      await result.current.closeTab(TAB);
    });
    expect(tab(result.current.tabs)).toBeUndefined();
    const before = result.current.tabs;

    await act(async () => {
      answered.resolve({ key: "guides/intro.md", kind: "markdown", artifactType: "doc" });
      await answered.promise;
    });
    expect(tab(result.current.tabs)).toBeUndefined();
    expect(result.current.tabs).toEqual(before);
  });

  it("TAB-FR-SZGD: a late answer does not replace the type of a tab that has one", async () => {
    const answered = held<OpenedArtifact>();
    openAnswer = () => answered.promise;
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.openDocument(doc));
    await act(async () => {
      await result.current.closeTab(TAB);
    });
    act(() =>
      result.current.activateLibraryFile({
        id: "guides/intro.md",
        name: "intro.md",
        artifactType: "spec",
        chip: typeChip("spec"),
      }),
    );

    await act(async () => {
      answered.resolve({ key: "guides/intro.md", kind: "markdown", artifactType: "doc" });
      await answered.promise;
    });
    expect(tab(result.current.tabs)).toMatchObject({ type: "spec", chip: typeChip("spec") });
  });
});
