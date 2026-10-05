import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { useLineEndings } from "./useLineEndings";
import type { LineEndings } from "../types";

// STB-FR-16–STB-FR-19 / SET-FR-10 / SET-FR-11 — the single shared project-wide
// line-ending preference, with two equal editors. `onChanged` is what marks
// every open Editor tab dirty (EDT-FR-41), so when it fires — and when it must
// not — is load-bearing.

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// SET-FR-11 / SWN-FR-01: the two controls that edit this one value are in two
// different windows, so the hook both announces its own write and listens for
// the other window's. The bus stands in for the second window.
const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<string, Array<(e: { payload?: unknown }) => void>>,
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(async (name: string, payload?: unknown) => {
    for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
  }),
  listen: vi.fn(
    async (name: string, handler: (e: { payload?: unknown }) => void) => {
      (eventHandlers[name] ??= []).push(handler);
      return () => {
        eventHandlers[name] = (eventHandlers[name] ?? []).filter(
          (h) => h !== handler,
        );
      };
    },
  ),
}));
function fireBusEvent(name: string, payload?: unknown) {
  for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
}

let stored: LineEndings;
let saveOutcome: "ok" | Error;
let saved: unknown[];

beforeEach(() => {
  invokeMock.mockReset();
  stored = "lf";
  saveOutcome = "ok";
  saved = [];
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "load_project_config") return { lineEndings: stored };
      if (cmd === "save_project_config") {
        if (saveOutcome instanceof Error) throw saveOutcome;
        saved.push(args?.config);
        stored = (args?.config as { lineEndings: LineEndings }).lineEndings;
        return undefined;
      }
      throw new Error(`unexpected invoke ${cmd}`);
    },
  );
});

afterEach(cleanup);

describe("loading (STB-FR-16 / PSS-FR-17)", () => {
  it("reads the persisted convention once a project is open", async () => {
    stored = "crlf";
    const { result } = renderHook(() => useLineEndings("~/dev/acme"));
    await waitFor(() => expect(result.current.lineEndings).toBe("crlf"));
    expect(invokeMock).toHaveBeenCalledWith("load_project_config");
  });

  it("reads nothing at all while no project is open", () => {
    const { result } = renderHook(() => useLineEndings(""));
    expect(result.current.lineEndings).toBeNull();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("reports no convention when the config cannot be read", async () => {
    // PSS-FR-10: a damaged `project.toml` is a typed error. The status bar is a
    // strip of ambient facts, so it renders no control rather than an error.
    invokeMock.mockImplementation(async () => {
      throw new Error("malformed project config");
    });
    const { result } = renderHook(() => useLineEndings("~/dev/acme"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(result.current.lineEndings).toBeNull();
  });

  it("rejects a value it does not recognise", async () => {
    invokeMock.mockImplementation(async () => ({ lineEndings: "cr" }));
    const { result } = renderHook(() => useLineEndings("~/dev/acme"));
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(result.current.lineEndings).toBeNull();
  });

  it("re-reads when the open project changes", async () => {
    const { result, rerender } = renderHook(
      ({ path }) => useLineEndings(path),
      { initialProps: { path: "~/dev/acme" } },
    );
    await waitFor(() => expect(result.current.lineEndings).toBe("lf"));

    stored = "crlf";
    rerender({ path: "~/dev/other" });
    await waitFor(() => expect(result.current.lineEndings).toBe("crlf"));
  });
});

describe("selecting (STB-FR-17 / SET-FR-10)", () => {
  it("persists immediately and only then reports the change", async () => {
    // The ordering is the contract: `onChanged` marks every open Editor tab
    // dirty (EDT-FR-41 / STB-FR-18). Firing it before the write lands would
    // dirty every tab for a conversion that might never happen.
    const order: string[] = [];
    let release: () => void = () => {};
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_config") return { lineEndings: "lf" };
      if (cmd === "save_project_config") {
        order.push("save");
        return new Promise<void>((r) => (release = () => r()));
      }
      throw new Error(`unexpected invoke ${cmd}`);
    });
    const onChanged = vi.fn(() => order.push("changed"));
    const { result } = renderHook(() =>
      useLineEndings("~/dev/acme", onChanged),
    );
    await waitFor(() => expect(result.current.lineEndings).toBe("lf"));

    act(() => result.current.selectLineEndings("crlf"));
    expect(order).toEqual(["save"]);
    expect(onChanged).not.toHaveBeenCalled();

    await act(async () => {
      release();
    });
    await waitFor(() => expect(order).toEqual(["save", "changed"]));
  });

  it("applies the selection optimistically so both controls agree at once", async () => {
    // STB-FR-19 / SET-FR-11: each entry point reflects the other's change
    // without a reload.
    const { result } = renderHook(() => useLineEndings("~/dev/acme"));
    await waitFor(() => expect(result.current.lineEndings).toBe("lf"));

    act(() => result.current.selectLineEndings("crlf"));
    expect(result.current.lineEndings).toBe("crlf");
    await waitFor(() => expect(saved).toEqual([{ lineEndings: "crlf" }]));
  });

  it("rolls back and reports nothing when the write fails", async () => {
    // A control showing a convention that will not survive a relaunch is bad;
    // marking every tab dirty for a conversion that will never happen is worse.
    saveOutcome = new Error("disk full");
    const onChanged = vi.fn();
    const { result } = renderHook(() =>
      useLineEndings("~/dev/acme", onChanged),
    );
    await waitFor(() => expect(result.current.lineEndings).toBe("lf"));

    act(() => result.current.selectLineEndings("crlf"));
    await waitFor(() => expect(result.current.lineEndings).toBe("lf"));
    expect(onChanged).not.toHaveBeenCalled();
  });

  it("ignores a selection of the value already in effect", async () => {
    // Re-selecting LF must not write, and above all must not dirty every open
    // tab for a conversion that is a no-op.
    const onChanged = vi.fn();
    const { result } = renderHook(() =>
      useLineEndings("~/dev/acme", onChanged),
    );
    await waitFor(() => expect(result.current.lineEndings).toBe("lf"));

    act(() => result.current.selectLineEndings("lf"));

    expect(saved).toEqual([]);
    expect(onChanged).not.toHaveBeenCalled();
  });

  // SET-FR-11 / SWN-FR-01: the Project settings window and the status bar are
  // in two different windows now, so "each reflects a change made from the
  // other without a reload" means hearing about it.
  it("adopts a convention the other window persisted, and marks tabs dirty for it", async () => {
    const onChanged = vi.fn();
    const { result } = renderHook(() =>
      useLineEndings("~/dev/acme", onChanged),
    );
    await waitFor(() => expect(result.current.lineEndings).toBe("lf"));

    act(() => {
      fireBusEvent("project-config:line-endings-changed", { value: "crlf" });
    });

    await waitFor(() => expect(result.current.lineEndings).toBe("crlf"));
    // STB-FR-18 / EDT-FR-41: a change made in the other window marks the
    // artifact of every open Editor tab dirty exactly as one made here does.
    expect(onChanged).toHaveBeenCalledTimes(1);
    // …and it reflected the write rather than repeating it.
    expect(saved).toEqual([]);
  });

  it("does nothing when the announcement is its own", async () => {
    // The bus reaches every window including the one that wrote it, where the
    // value is already applied — so the writer must not mark its own tabs dirty
    // a second time.
    const onChanged = vi.fn();
    const { result } = renderHook(() =>
      useLineEndings("~/dev/acme", onChanged),
    );
    await waitFor(() => expect(result.current.lineEndings).toBe("lf"));

    await act(async () => {
      result.current.selectLineEndings("crlf");
    });
    await waitFor(() => expect(result.current.lineEndings).toBe("crlf"));

    expect(onChanged).toHaveBeenCalledTimes(1);
  });
});
