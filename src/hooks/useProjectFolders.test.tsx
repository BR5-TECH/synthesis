import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { useProjectFolders } from "./useProjectFolders";
import { PROJECT_TREE_CHANGED } from "../events";
import { treeCommand } from "../test/treeCommands";
import type { PathSearch, TreeNode } from "../types";

// NFW-FR-04 / NFI-FR-04 / NTA-FR-04: the folder list of the creation windows
// comes from one `"search project paths"` read of the project index
// (ASC-FR-AVVY). The list is read again only when a tree change made it
// stale, so an open with a fresh list issues no call.

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

/** The project the index answers from. A test changes it to change the next read. */
let project: TreeNode;

/** Answer the folder search from `project`, as the index would. */
function answerFromProject() {
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "append_log_records") return undefined;
    const read = cmd === "search_project_paths" ? treeCommand(cmd, args, project) : undefined;
    if (read) return read;
    throw new Error(`unexpected invoke ${cmd}`);
  });
}

/** The folder searches issued so far. */
const searches = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "search_project_paths");

/** A search answer held open until the test releases it. */
function holdSearches() {
  let release: (paths: string[]) => void = () => {};
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd !== "search_project_paths") return Promise.reject(`unexpected invoke ${cmd}`);
    return new Promise<PathSearch>((resolve) => {
      release = (paths) => resolve({ paths, truncated: false });
    });
  });
  return (paths: string[]) => release(paths);
}

function fireTreeChanged() {
  act(() => {
    for (const h of [...(listeners[PROJECT_TREE_CHANGED] ?? [])]) {
      h({ payload: { changeCount: 1, changedFolders: [""] } });
    }
  });
}

const paths = (r: { current: { folders: { path: string }[] } }) =>
  r.current.folders.map((f) => f.path);

beforeEach(() => {
  invokeMock.mockReset();
  project = tree();
  answerFromProject();
  for (const k in listeners) delete listeners[k];
});

afterEach(cleanup);

describe("useProjectFolders", () => {
  it("NFW-FR-04: reads every folder with one folder search, files excluded, empty folders included", async () => {
    project = tree([folder("empty")]);
    const { result } = renderHook(() => useProjectFolders("proj::0"));

    act(() => result.current.ensureFresh());

    await waitFor(() =>
      // ASC-FR-AVVY: in tree order, each folder's child folders by name.
      expect(paths(result)).toEqual(["empty", "specifications", "specifications/ui"]),
    );
    expect(result.current.folders[0]).toEqual({ path: "empty", label: "empty" });
    // One read of the index, for folders, with no text and no limit.
    expect(searches()).toEqual([
      ["search_project_paths", { text: "", kind: "folder", limit: null }],
    ]);
  });

  it("reads nothing until a consumer asks", async () => {
    renderHook(() => useProjectFolders("proj::0"));
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]?.length).toBe(1));
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("reads once, and a fresh list opens the next window with no call", async () => {
    const { result } = renderHook(() => useProjectFolders("proj::0"));

    act(() => result.current.ensureFresh());
    await waitFor(() => expect(paths(result)).toContain("specifications"));
    expect(searches()).toHaveLength(1);

    act(() => result.current.ensureFresh());
    expect(searches()).toHaveLength(1);
  });

  it("reads again after a tree change, when a consumer asks", async () => {
    const { result } = renderHook(() => useProjectFolders("proj::0"));
    act(() => result.current.ensureFresh());
    await waitFor(() => expect(paths(result)).toContain("specifications"));
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]?.length).toBe(1));

    project = tree([folder("created-since")]);
    fireTreeChanged();
    act(() => result.current.ensureFresh());

    await waitFor(() => expect(paths(result)).toContain("created-since"));
    expect(searches()).toHaveLength(2);
  });

  it("does not read on a tree change by itself — only when a consumer asks", async () => {
    // A change while no window needs the list costs nothing.
    const { result } = renderHook(() => useProjectFolders("proj::0"));
    act(() => result.current.ensureFresh());
    await waitFor(() => expect(paths(result)).toContain("specifications"));
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]?.length).toBe(1));

    project = tree([folder("created-since")]);
    fireTreeChanged();

    expect(searches()).toHaveLength(1);
    // And the last list read is still what a render sees.
    expect(paths(result)).toEqual(["specifications", "specifications/ui"]);
  });

  it("keeps the last list read when a read fails, and retries on the next open", async () => {
    const { result } = renderHook(() => useProjectFolders("proj::0"));
    act(() => result.current.ensureFresh());
    await waitFor(() => expect(paths(result)).toContain("specifications"));
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]?.length).toBe(1));

    invokeMock.mockImplementation(async () => {
      throw "no project open";
    });
    fireTreeChanged();
    act(() => result.current.ensureFresh());
    await waitFor(() => expect(searches()).toHaveLength(2));
    // Degraded to what was last known rather than emptied — creating a folder in
    // one of those parents still works.
    expect(paths(result)).toEqual(["specifications", "specifications/ui"]);

    // The failed read left the list stale, so the next open reads again.
    project = tree([folder("after-failure")]);
    answerFromProject();
    act(() => result.current.ensureFresh());
    await waitFor(() => expect(paths(result)).toContain("after-failure"));
  });

  it("reads again when a tree change lands while a read is already in flight", async () => {
    // A read dispatched BEFORE the change resolves AFTER it, so its
    // (already-stale) list must not be recorded as fresh.
    const release = holdSearches();
    const { result } = renderHook(() => useProjectFolders("proj::0"));
    await waitFor(() => expect(listeners[PROJECT_TREE_CHANGED]?.length).toBe(1));

    act(() => result.current.ensureFresh());
    expect(searches()).toHaveLength(1);

    // The change arrives mid-flight, then the pre-change list lands.
    fireTreeChanged();
    await act(async () => {
      release(["specifications", "specifications/ui"]);
    });
    expect(paths(result)).toEqual(["specifications", "specifications/ui"]);

    // Still stale, so the next open really does read again.
    project = tree([folder("created-since")]);
    answerFromProject();
    act(() => result.current.ensureFresh());
    await waitFor(() => expect(paths(result)).toContain("created-since"));
  });

  it("coalesces two opens in quick succession into one read", async () => {
    const release = holdSearches();
    const { result } = renderHook(() => useProjectFolders("proj::0"));

    act(() => result.current.ensureFresh());
    act(() => result.current.ensureFresh());
    expect(searches()).toHaveLength(1);

    await act(async () => {
      release(["specifications"]);
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
    act(() => result.current.ensureFresh());
    await waitFor(() => expect(result.current.folders).toHaveLength(2));

    project = tree([folder("only-in-b")]);
    rerender({ key: "projB::0" });

    expect(result.current.folders).toEqual([]);
    // And it is stale again, so the next open reads the NEW root.
    act(() => result.current.ensureFresh());
    await waitFor(() => expect(paths(result)).toContain("only-in-b"));
  });

  it("discards a read that outlived its content root, and still reads the new one", async () => {
    // A read is in flight when the user switches project. The old project's
    // folders must not land in the new project's list, and the in-flight marker
    // must not block the read the new root needs.
    const release = holdSearches();
    const { result, rerender } = renderHook(
      ({ key }: { key: string }) => useProjectFolders(key),
      { initialProps: { key: "projA::0" } },
    );

    act(() => result.current.ensureFresh());
    expect(searches()).toHaveLength(1);

    rerender({ key: "projB::0" });
    expect(result.current.folders).toEqual([]);

    // Project A's read lands late, naming A's folders.
    await act(async () => {
      release(["only-in-a"]);
    });
    expect(result.current.folders).toEqual([]);

    // And B's own read goes out rather than being swallowed by A's stale marker.
    project = tree([folder("only-in-b")]);
    answerFromProject();
    act(() => result.current.ensureFresh());
    await waitFor(() => expect(paths(result)).toContain("only-in-b"));
    expect(paths(result)).not.toContain("only-in-a");
  });

  it("drops the list when the worktree changes within the same project", async () => {
    const { result, rerender } = renderHook(
      ({ key }: { key: string }) => useProjectFolders(key),
      { initialProps: { key: "projA::0" } },
    );
    act(() => result.current.ensureFresh());
    await waitFor(() => expect(result.current.folders).toHaveLength(2));
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
