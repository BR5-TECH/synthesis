// The shell's link opener (`EDT-editor.md` EDT-FR-GEHE, EDT-FR-YXUD, EDT-FR-HYVP).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { useFileLinkOpener } from "./useFileLinkOpener";
import { resetLogBufferForTest } from "../../logging";
import { resetToasts, visibleToasts } from "../../state/toasts";

const EXISTING = new Set(["docs/guide.md", "src/main.rs", "docs/manual.pdf"]);

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "open_artifact_by_id") {
      const { id } = args as { id: string };
      if (!EXISTING.has(id)) return Promise.reject(new Error("not a file"));
      return { key: id, kind: "file", artifactType: id.endsWith(".pdf") ? "doc" : undefined };
    }
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
});

afterEach(() => {
  vi.clearAllMocks();
});

function setup(roots: readonly string[] = ["/p"]) {
  const open = vi.fn();
  const hook = renderHook(({ r }) => useFileLinkOpener(r, open), { initialProps: { r: roots } });
  return { open, hook };
}

describe("useFileLinkOpener", () => {
  it("EDT-FR-GEHE: a file that exists opens as a Project panel click opens it", async () => {
    const { open, hook } = setup();
    act(() => hook.result.current.openFile("docs/manual.pdf", null));
    await waitFor(() =>
      expect(open).toHaveBeenCalledWith({ id: "docs/manual.pdf", name: "manual.pdf", artifactType: "doc" }),
    );
  });

  it("EDT-FR-YXUD: the fragment of a Markdown file waits for its tab, and is taken one time", async () => {
    const { open, hook } = setup();
    act(() => hook.result.current.openFile("docs/guide.md", "Setup"));
    await waitFor(() => expect(open).toHaveBeenCalled());
    expect(hook.result.current.takePendingFragment("docs/other.md")).toBeNull();
    expect(hook.result.current.takePendingFragment("docs/guide.md")).toBe("Setup");
    expect(hook.result.current.takePendingFragment("docs/guide.md")).toBeNull();
  });

  it("EDT-FR-YXUD: a fragment on a file that is not Markdown does not wait", async () => {
    const { open, hook } = setup();
    act(() => hook.result.current.openFile("src/main.rs", "x"));
    await waitFor(() => expect(open).toHaveBeenCalled());
    expect(hook.result.current.takePendingFragment("src/main.rs")).toBeNull();
  });

  it("EDT-FR-HYVP, NTF-FR-WQRN: a file that does not exist opens no tab and shows the toast", async () => {
    const { open, hook } = setup();
    act(() => hook.result.current.openFile("docs/missing.md", "a"));
    await waitFor(() =>
      expect(visibleToasts()).toEqual([
        expect.objectContaining({ key: "link-target-missing", level: "Warn" }),
      ]),
    );
    expect(open).not.toHaveBeenCalled();
    expect(hook.result.current.takePendingFragment("docs/missing.md")).toBeNull();
  });

  it("EDT-FR-HYVP: a link to the project folder itself names no file", () => {
    const { open, hook } = setup();
    act(() => hook.result.current.openFile("", null));
    expect(visibleToasts()[0]).toMatchObject({ key: "link-target-missing" });
    expect(open).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("open_artifact_by_id", expect.anything());
  });

  it("EDT-FR-AWFH: the roots drop empty and repeated paths, and the opener stays while they hold", () => {
    const { hook } = setup(["", "/p", "/p"]);
    const first = hook.result.current;
    expect(first.roots).toEqual(["/p"]);
    hook.rerender({ r: ["/p", ""] });
    expect(hook.result.current).toBe(first);
    hook.rerender({ r: ["/w", "/p"] });
    expect(hook.result.current.roots).toEqual(["/w", "/p"]);
  });

  it("EDT-FR-YXUD: of two links, the later one wins, also when the earlier file answers last", async () => {
    let releaseFirst: (value: unknown) => void = () => {};
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "append_log_records") return undefined;
      if (cmd === "open_artifact_by_id") {
        const { id } = args as { id: string };
        if (id === "docs/guide.md") return new Promise((resolve) => (releaseFirst = resolve));
        return { key: id, kind: "file" };
      }
      return Promise.reject(new Error(`unexpected invoke ${cmd}`));
    });
    const { open, hook } = setup();
    act(() => hook.result.current.openFile("docs/guide.md", "first"));
    act(() => hook.result.current.openFile("docs/other.md", "second"));
    await waitFor(() => expect(open).toHaveBeenCalledTimes(1));
    await act(async () => releaseFirst({ key: "docs/guide.md", kind: "file" }));
    expect(open).toHaveBeenCalledTimes(1);
    expect(open).toHaveBeenCalledWith(expect.objectContaining({ id: "docs/other.md" }));
    expect(hook.result.current.takePendingFragment("docs/guide.md")).toBeNull();
    expect(hook.result.current.takePendingFragment("docs/other.md")).toBe("second");
  });

  it("EDT-FR-YXUD: a fragment still waits while the active tab shows its file", async () => {
    const { open, hook } = setup();
    act(() => hook.result.current.openFile("docs/guide.md", "Setup"));
    await waitFor(() => expect(open).toHaveBeenCalled());
    hook.result.current.activeFileChanged("docs/guide.md");
    expect(hook.result.current.takePendingFragment("docs/guide.md")).toBe("Setup");
  });

  it("EDT-FR-YXUD: a fragment for a file that no tab showed is dropped when another file is active", async () => {
    const { open, hook } = setup();
    act(() => hook.result.current.openFile("docs/guide.md", "Setup"));
    await waitFor(() => expect(open).toHaveBeenCalled());
    // The file went to a surface of another file, or of none.
    hook.result.current.activeFileChanged(null);
    expect(hook.result.current.takePendingFragment("docs/guide.md")).toBeNull();
  });
});
