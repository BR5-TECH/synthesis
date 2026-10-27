import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { resetLogBufferForTest } from "../logging";
import { resetAppPreferencesCache } from "../state/appPreferences";
import type { SearchMode } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { useSearchQueryMode } from "./useSearchQueryMode";

/** A promise the test settles itself, so no outcome depends on timing. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const PREFS = { theme: "system", mainWindowFullscreen: false };

let load: () => Promise<unknown>;
let save: (prefs: { searchQueryMode: SearchMode }) => Promise<void>;

beforeEach(() => {
  resetLogBufferForTest();
  resetAppPreferencesCache();
  invokeMock.mockReset();
  load = async () => PREFS;
  save = async () => undefined;
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "load_app_preferences") return load();
    if (cmd === "save_app_preferences") {
      return save((args as { preferences: { searchQueryMode: SearchMode } }).preferences);
    }
    if (cmd === "append_log_records") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});
afterEach(cleanup);

describe("useSearchQueryMode", () => {
  it("SCH-FR-13: a user with no stored mode starts in Semantic and stays there after the read", async () => {
    const read = deferred<unknown>();
    load = () => read.promise;
    const { result } = renderHook(() => useSearchQueryMode());
    expect(result.current.mode).toBe("semantic");
    await act(async () => read.resolve(PREFS));
    expect(result.current.mode).toBe("semantic");
  });

  it("SCH-FR-13: a stored literal mode wins over the Semantic default", async () => {
    load = async () => ({ ...PREFS, searchQueryMode: "smart_case" });
    const { result } = renderHook(() => useSearchQueryMode());
    await waitFor(() => expect(result.current.mode).toBe("smart_case"));
  });

  it("SCH-FR-13: an unreadable preference keeps Semantic after the read ends", async () => {
    const read = deferred<unknown>();
    load = () => read.promise;
    const { result } = renderHook(() => useSearchQueryMode());
    await act(async () => read.reject(new Error("unreadable")));
    expect(result.current.mode).toBe("semantic");
  });

  it("SCH-FR-12: a refused save rolls the mode back to the stored one", async () => {
    load = async () => ({ ...PREFS, searchQueryMode: "regex" });
    save = async () => {
      throw new Error("disk full");
    };
    const { result } = renderHook(() => useSearchQueryMode());
    await waitFor(() => expect(result.current.mode).toBe("regex"));
    act(() => result.current.selectMode("semantic"));
    expect(result.current.mode).toBe("semantic");
    await waitFor(() => expect(result.current.mode).toBe("regex"));
  });

  it("SCH-FR-12, SCH-FR-13: a mode chosen before the stored mode is read is kept", async () => {
    const read = deferred<unknown>();
    load = () => read.promise;
    const { result } = renderHook(() => useSearchQueryMode());
    act(() => result.current.selectMode("regex"));
    await act(async () => read.resolve({ ...PREFS, searchQueryMode: "smart_case" }));
    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "save_app_preferences")).toBe(true),
    );
    expect(result.current.mode).toBe("regex");
  });

  it("SCH-FR-12: a refused earlier save does not undo a later choice that was stored", async () => {
    const saves: ReturnType<typeof deferred<void>>[] = [];
    save = () => {
      const d = deferred<void>();
      saves.push(d);
      return d.promise;
    };
    const { result } = renderHook(() => useSearchQueryMode());
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("load_app_preferences"));
    act(() => result.current.selectMode("smart_case"));
    act(() => result.current.selectMode("regex"));
    await waitFor(() => expect(saves).toHaveLength(1));
    await act(async () => saves[0].reject(new Error("refused")));
    await waitFor(() => expect(saves).toHaveLength(2));
    await act(async () => saves[1].resolve());
    expect(result.current.mode).toBe("regex");
  });

  it("SCH-FR-12: a refused save made before the read ends rolls back to the stored mode", async () => {
    const read = deferred<unknown>();
    load = () => read.promise;
    save = async () => {
      throw new Error("refused");
    };
    const { result } = renderHook(() => useSearchQueryMode());
    act(() => result.current.selectMode("regex"));
    await act(async () => read.resolve({ ...PREFS, searchQueryMode: "smart_case" }));
    await waitFor(() => expect(result.current.mode).toBe("smart_case"));
  });
});
