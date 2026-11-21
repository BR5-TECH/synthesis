import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, waitFor } from "@testing-library/react";

import { ProjectPicker } from "./ProjectPicker";
import { resetLogBufferForTest } from "../logging";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

type Size = { width: number; height: number };

const handlers = new Set<() => void>();
const unlistenMock = vi.fn();
const setSizeMock = vi.fn(async (_s: unknown) => {});
const setResizableMock = vi.fn(async (_v: boolean) => {});
const isFullscreenMock = vi.fn(async () => false);
const setFullscreenMock = vi.fn(async (_v: boolean) => {});
const innerSizeMock = vi.fn(async (): Promise<Size> => ({ width: 800, height: 600 }));
function subscribe(cb: () => void): Promise<() => void> {
  handlers.add(cb);
  return Promise.resolve(() => {
    unlistenMock();
    handlers.delete(cb);
  });
}
const onResizedMock = vi.fn(subscribe);

vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    width: number;
    height: number;
    constructor(width: number, height: number) {
      this.width = width;
      this.height = height;
    }
  }
  return {
    LogicalSize,
    getCurrentWindow: () => ({
      setSize: (s: unknown) => setSizeMock(s),
      setResizable: (v: boolean) => setResizableMock(v),
      setMaximizable: async () => {},
      isMaximized: async () => false,
      unmaximize: async () => {},
      isFullscreen: () => isFullscreenMock(),
      setFullscreen: async (v: boolean) => {
        await setFullscreenMock(v);
        // Leaving full-screen changes the size, so the platform fires a resize.
        if (v === false) fire();
      },
      innerSize: () => innerSizeMock(),
      scaleFactor: async () => 1,
      onResized: (cb: () => void) => onResizedMock(cb),
    }),
  };
});

function fire() {
  for (const h of [...handlers]) h();
}

async function flush() {
  for (let i = 0; i < 20; i += 1) await Promise.resolve();
}

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}

function centerCalls(): number {
  return invokeMock.mock.calls.filter((c) => c[0] === "center_picker").length;
}

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records" || cmd === "center_picker") return undefined;
    if (cmd === "list_recent_projects") return [];
    throw new Error(`unexpected ${cmd}`);
  });
  handlers.clear();
  unlistenMock.mockReset();
  setSizeMock.mockReset();
  setSizeMock.mockImplementation(async () => {});
  setResizableMock.mockClear();
  isFullscreenMock.mockReset();
  isFullscreenMock.mockResolvedValue(false);
  setFullscreenMock.mockClear();
  innerSizeMock.mockReset();
  innerSizeMock.mockResolvedValue({ width: 800, height: 600 });
  onResizedMock.mockReset();
  onResizedMock.mockImplementation(subscribe);
});

afterEach(() => {
  cleanup();
  resetLogBufferForTest();
});

describe("ProjectPicker — keeps its fixed size", () => {
  it("PPK-FR-TBNQ, SNV-FR-27: a frame restored after the full-screen exit is corrected", async () => {
    isFullscreenMock.mockResolvedValueOnce(true);
    render(<ProjectPicker onOpen={vi.fn()} />);
    // The chrome subscribes once for the full-screen exit, the guard once after.
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(centerCalls()).toBe(1));
    expect(setSizeMock).toHaveBeenCalledTimes(1);

    // macOS restores the frame of the project window at the end of the exit.
    innerSizeMock.mockResolvedValue({ width: 1440, height: 900 });
    fire();
    await waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(centerCalls()).toBe(2));
    const size = setSizeMock.mock.calls[1][0] as Size;
    expect(size).toMatchObject({ width: 800, height: 600 });
  });

  it("PPK-FR-TBNQ: the guard starts only after the window chrome is applied", async () => {
    const held = deferred<void>();
    setSizeMock.mockImplementationOnce(() => held.promise);
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(1));
    await flush();
    expect(onResizedMock).not.toHaveBeenCalled();
    held.resolve();
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));
  });

  it("PPK-FR-TBNQ: unmount removes the guard", async () => {
    const { unmount } = render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));
    // The first check of the guard reads the size once.
    await waitFor(() => expect(innerSizeMock).toHaveBeenCalledTimes(1));
    unmount();
    expect(unlistenMock).toHaveBeenCalledTimes(1);
    innerSizeMock.mockResolvedValue({ width: 1440, height: 900 });
    fire();
    await flush();
    expect(innerSizeMock).toHaveBeenCalledTimes(1);
    expect(setSizeMock).toHaveBeenCalledTimes(1);
  });

  it("PPK-FR-TBNQ: unmount before the subscription resolves removes the guard when it resolves", async () => {
    const held = deferred<void>();
    onResizedMock.mockImplementationOnce((cb) =>
      held.promise.then(() => {
        handlers.add(cb);
        return () => {
          unlistenMock();
          handlers.delete(cb);
        };
      }),
    );
    const { unmount } = render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => expect(onResizedMock).toHaveBeenCalledTimes(1));
    unmount();
    held.resolve();
    await waitFor(() => expect(unlistenMock).toHaveBeenCalledTimes(1));
    expect(handlers.size).toBe(0);
  });

  it("PPK-FR-TBNQ: unmount while the chrome is applied starts no guard", async () => {
    const held = deferred<void>();
    setSizeMock.mockImplementationOnce(() => held.promise);
    const { unmount } = render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(1));
    unmount();
    held.resolve();
    await waitFor(() => expect(centerCalls()).toBe(1));
    await flush();
    expect(onResizedMock).not.toHaveBeenCalled();
  });

  it("PPK-FR-TBNQ, PPK-FR-GMYH: the modal presentation starts no guard", async () => {
    render(<ProjectPicker onOpen={vi.fn()} presentation="modal" />);
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "list_recent_projects"),
      ).toBe(true),
    );
    await flush();
    expect(onResizedMock).not.toHaveBeenCalled();
    expect(setSizeMock).not.toHaveBeenCalled();
    expect(setResizableMock).not.toHaveBeenCalled();
    expect(centerCalls()).toBe(0);
  });
  it("PPK-FR-TBNQ: a frame restored before the guard subscribes is corrected", async () => {
    const held = deferred<void>();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "append_log_records") return undefined;
      if (cmd === "list_recent_projects") return [];
      if (cmd === "center_picker") {
        if (centerCalls() === 1) await held.promise;
        return undefined;
      }
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => expect(centerCalls()).toBe(1));
    // The platform restores a large frame while no guard listens.
    innerSizeMock.mockResolvedValue({ width: 1440, height: 900 });
    fire();
    held.resolve();
    await waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(2));
    expect(setSizeMock.mock.calls[1][0]).toMatchObject({ width: 800, height: 600 });
    await waitFor(() => expect(centerCalls()).toBe(2));
  });
});
