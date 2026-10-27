import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import { resetLogBufferForTest } from "../logging";
import type { EmbeddingsStatus } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type Handler = (event: { payload: unknown }) => void;
let handlers: Handler[] = [];
const unlistened = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (_name: string, handler: Handler) => {
    handlers.push(handler);
    return Promise.resolve(() => {
      unlistened();
      handlers = handlers.filter((h) => h !== handler);
    });
  },
  emit: () => Promise.resolve(),
}));

import { useEmbeddingsStatus } from "./useEmbeddingsStatus";

const off: EmbeddingsStatus = { enabled: false, model: "BGESmallENV15", modelState: "not_downloaded" };
const on: EmbeddingsStatus = { enabled: true, model: "BGESmallENV15", modelState: "ready" };

let release: ((s: EmbeddingsStatus) => void) | null;

beforeEach(() => {
  resetLogBufferForTest();
  handlers = [];
  unlistened.mockReset();
  invokeMock.mockReset();
  release = null;
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "get_embeddings_status") return new Promise<EmbeddingsStatus>((r) => (release = r));
    if (cmd === "append_log_records") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});
afterEach(cleanup);

describe("useEmbeddingsStatus (EMB-FR-ZFGT)", () => {
  it("EMB-FR-ZFGT: reads the status, then follows the event", async () => {
    const { result } = renderHook(() => useEmbeddingsStatus());
    expect(result.current).toBeNull();
    await act(async () => release!(off));
    expect(result.current).toEqual(off);
    act(() => handlers.forEach((h) => h({ payload: on })));
    expect(result.current).toEqual(on);
  });

  it("EMB-FR-ZFGT: an event newer than the first read is not overwritten by it", async () => {
    const { result } = renderHook(() => useEmbeddingsStatus());
    await waitFor(() => expect(handlers).toHaveLength(1));
    act(() => handlers.forEach((h) => h({ payload: on })));
    await act(async () => release!(off));
    expect(result.current).toEqual(on);
  });

  it("stops listening when it unmounts", async () => {
    const { unmount } = renderHook(() => useEmbeddingsStatus());
    await waitFor(() => expect(handlers).toHaveLength(1));
    unmount();
    await waitFor(() => expect(unlistened).toHaveBeenCalledTimes(1));
  });
});
