import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { flattenFolders, useProjectFolders } from "./useProjectFolders";
import { PROJECT_TREE_CHANGED } from "../events";
import type { TreeNode } from "../types";

// NFW-FR-04 + the NFW non-functional requirement: the New Folder window's parent
// list comes from the tree the Library already loaded, so opening it costs no
// scan — while still being complete when the Library is unmounted and nothing has
// reloaded on its behalf.

const invokeMock = vi.fn();
const listeners: Record<string, Array<(e: { payload: unknown }) => void>> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (name: string, handler: (e: { payload: unknown }) => void) => {
      (listeners[name] ??= []).push(handler);
      return () => {
        listeners[name] = (listeners[name] ?? []).filter((h) => h !== handler);
      };
    },
  ),
}));

function folder(path: string, children: TreeNode[] = []): TreeNode {
  return {
    id: path,
    name: path.split("/").pop() ?? "",
    path,
    nodeKind: "folder",
    hasArtifacts: true,
    children,
  };
}

function file(path: string): TreeNode {
  return {
    id: path,
    name: path.split("/").pop()!,
    path,
    nodeKind: "file",
  };
}

function tree(extra: TreeNode[] = []): TreeNode {
  return folder("", [
    folder("specifications", [folder("specifications/ui", [file("specifications/ui/x.md")])]),
    file("AGENTS.md"),
    ...extra,
  ]);
}

function fireTreeChanged() {
  act(() => {
    for (const h of [...(listeners[PROJECT_TREE_CHANGED] ?? [])]) {
      h({ payload: { changeCount: 1 } });
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd !== "load_project_tree") throw new Error(`unexpected invoke ${cmd}`);
    return tree([folder("scanned-only")]);
  });
  for (const k in listeners) delete listeners[k];
});

afterEach(cleanup);

describe("flattenFolders (NFW-FR-04)", () => {
  it("lists every folder, in tree order, and no files", () => {
    expect(flattenFolders(tree([folder("tooling")]))).toEqual([
      { path: "specifications", label: "specifications" },
      { path: "specifications/ui", label: "specifications/ui" },
      { path: "tooling", label: "tooling" },
    ]);
  });

  it("includes an empty folder — one the Library's default lens hides", () => {
    // The whole point: a folder with nothing in it is still a valid parent.
    expect(flattenFolders(folder("", [folder("empty")]))).toEqual([
      { path: "empty", label: "empty" },
    ]);
  });
});

describe("useProjectFolders", () => {
  it("takes its list from a published tree without scanning", async () => {
    const { result } = renderHook(() => useProjectFolders("proj::0"));

    act(() => result.current.publishTree(tree()));
    expect(result.current.folders.map((f) => f.path)).toEqual([
      "specifications",
      "specifications/ui",
    ]);

    // A published list is fresh, so a consumer opening now issues nothing.
    act(() => result.current.ensureFresh());
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("scans once when nothing has been published yet", async () => {
    const { result } = renderHook(() => useProjectFolders("proj::0"));

    act(() => result.current.ensureFresh());

    await waitFor(() =>
      expect(result.current.folders.map((f) => f.path)).toContain("scanned-only"),
    );
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_project_tree"),
    ).toHaveLength(1);

    // The scan's result is itself a publish, so the next open scans nothing.
    act(() => result.current.ensureFresh());
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_project_tree"),
    ).toHaveLength(1);
  });

  it("re-scans after a tree change nobody published a reload for", async () => {
    // The Library is what normally republishes; while it is unmounted (another
    // vertical-panel surface is showing) a structural change would otherwise
    // leave the parent list missing a folder that now exists.
    const { result } = renderHook(() => useProjectFolders("proj::0"));
    act(() => result.current.publishTree(tree()));
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]?.length).toBe(1));

    fireTreeChanged();
    act(() => result.current.ensureFresh());

    await waitFor(() =>
      expect(result.current.folders.map((f) => f.path)).toContain("scanned-only"),
    );
  });

  it("does not scan on a tree change by itself — only when a consumer asks", async () => {
    // The event alone must not trigger a walk: the Library is already reloading
    // on the same event, and a second scan would double the work and show a
    // second "Indexing project…" operation in the status bar.
    const { result } = renderHook(() => useProjectFolders("proj::0"));
    act(() => result.current.publishTree(tree()));
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]?.length).toBe(1));

    fireTreeChanged();

    expect(invokeMock).not.toHaveBeenCalled();
    // And the last published list is still what a render sees.
    expect(result.current.folders.map((f) => f.path)).toEqual([
      "specifications",
      "specifications/ui",
    ]);
  });

  it("keeps the last published list when a scan fails", async () => {
    const { result } = renderHook(() => useProjectFolders("proj::0"));
    act(() => result.current.publishTree(tree()));
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]?.length).toBe(1));

    invokeMock.mockImplementation(async () => {
      throw "no project open";
    });
    fireTreeChanged();
    act(() => result.current.ensureFresh());

    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    // Degraded to what was last known rather than emptied — creating a folder in
    // one of those parents still works.
    expect(result.current.folders.map((f) => f.path)).toEqual([
      "specifications",
      "specifications/ui",
    ]);
  });

  it("re-scans when a tree change lands while a scan is already in flight", async () => {
    // The race that silently produces a wrong answer: a scan dispatched BEFORE the
    // change resolves AFTER it, so its (already-stale) tree would otherwise be
    // recorded as fresh and the next open would trust an incomplete parent list.
    let release: (tree: TreeNode) => void = () => {};
    invokeMock.mockImplementation(
      () => new Promise<TreeNode>((resolve) => (release = resolve)),
    );
    const { result } = renderHook(() => useProjectFolders("proj::0"));
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]?.length).toBe(1));

    act(() => result.current.ensureFresh());
    expect(invokeMock).toHaveBeenCalledTimes(1);

    // The change arrives mid-flight, then the pre-change tree lands.
    fireTreeChanged();
    await act(async () => {
      release(tree());
    });
    expect(result.current.folders.map((f) => f.path)).toEqual([
      "specifications",
      "specifications/ui",
    ]);

    // Still stale, so the next open really does scan again.
    invokeMock.mockImplementation(async () => tree([folder("scanned-only")]));
    act(() => result.current.ensureFresh());
    await waitFor(() =>
      expect(result.current.folders.map((f) => f.path)).toContain("scanned-only"),
    );
  });

  it("coalesces two opens in quick succession into one scan", async () => {
    let release: (tree: TreeNode) => void = () => {};
    invokeMock.mockImplementation(
      () => new Promise<TreeNode>((resolve) => (release = resolve)),
    );
    const { result } = renderHook(() => useProjectFolders("proj::0"));

    act(() => result.current.ensureFresh());
    act(() => result.current.ensureFresh());
    expect(invokeMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      release(tree());
    });
  });

  it("drops the list when the content root changes", async () => {
    // This hook lives outside the subtree a project or worktree switch remounts,
    // so nothing else would clear it: the previous project's folders would stay on
    // offer, marked fresh, as parents for a folder in the NEW project.
    const { result, rerender } = renderHook(
      ({ key }: { key: string }) => useProjectFolders(key),
      { initialProps: { key: "projA::0" } },
    );
    act(() => result.current.publishTree(tree()));
    expect(result.current.folders).toHaveLength(2);

    rerender({ key: "projB::0" });

    expect(result.current.folders).toEqual([]);
    // And it is stale again, so the next open reads the NEW root.
    act(() => result.current.ensureFresh());
    await waitFor(() =>
      expect(result.current.folders.map((f) => f.path)).toContain("scanned-only"),
    );
  });

  it("discards a scan that outlived its content root, and still scans the new one", async () => {
    // The nastiest interleaving: a scan is in flight when the user switches
    // project. Two things must not happen — the old project's folders must not be
    // written into the new project's list (they would be offered as parents for a
    // folder in a project they do not belong to), and the in-flight marker must
    // not block the scan the new root needs (the window would open with no parents
    // at all and nothing to refill it).
    let release: (tree: TreeNode) => void = () => {};
    invokeMock.mockImplementation(
      () => new Promise<TreeNode>((resolve) => (release = resolve)),
    );
    const { result, rerender } = renderHook(
      ({ key }: { key: string }) => useProjectFolders(key),
      { initialProps: { key: "projA::0" } },
    );

    act(() => result.current.ensureFresh());
    expect(invokeMock).toHaveBeenCalledTimes(1);

    rerender({ key: "projB::0" });
    expect(result.current.folders).toEqual([]);

    // Project A's scan lands late, naming A's folders.
    await act(async () => {
      release(tree([folder("only-in-a")]));
    });
    expect(result.current.folders).toEqual([]);

    // And B's own scan goes out rather than being swallowed by A's stale marker.
    invokeMock.mockImplementation(async () => tree([folder("only-in-b")]));
    act(() => result.current.ensureFresh());
    await waitFor(() =>
      expect(result.current.folders.map((f) => f.path)).toContain("only-in-b"),
    );
    expect(result.current.folders.map((f) => f.path)).not.toContain("only-in-a");
  });

  it("drops the list when the worktree changes within the same project", async () => {
    const { result, rerender } = renderHook(
      ({ key }: { key: string }) => useProjectFolders(key),
      { initialProps: { key: "projA::0" } },
    );
    act(() => result.current.publishTree(tree()));
    rerender({ key: "projA::1" });
    expect(result.current.folders).toEqual([]);
  });

  it("detaches its listener on unmount", async () => {
    const { unmount } = renderHook(() => useProjectFolders("proj::0"));
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]?.length).toBe(1));
    unmount();
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]).toHaveLength(0));
  });
});
