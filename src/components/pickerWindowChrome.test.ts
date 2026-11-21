import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  keepPickerWindowSize,
  MAX_SIZE_CORRECTIONS,
  PICKER_WINDOW_HEIGHT,
  PICKER_WINDOW_WIDTH,
} from "./pickerWindowChrome";
import { resetLogBufferForTest } from "../logging";

const log = vi.hoisted(() => ({ warn: vi.fn(), debug: vi.fn() }));
vi.mock("../logging", async (orig) => ({
  ...(await orig<typeof import("../logging")>()),
  logWarn: log.warn,
  logDebug: log.debug,
}));

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type Size = { width: number; height: number };

const handlers = new Set<() => void>();
const unlistenMock = vi.fn();
const setSizeMock = vi.fn(async (_s: unknown) => {});
const isFullscreenMock = vi.fn(async () => false);
const innerSizeMock = vi.fn(async (): Promise<Size> => ({ width: 800, height: 600 }));
const scaleFactorMock = vi.fn(async (): Promise<number> => 1);
let onResizedImpl: (cb: () => void) => Promise<() => void>;
let hasInnerSize = true;
let hasOnResized = true;
let noRuntime = false;

function defaultOnResized(cb: () => void): Promise<() => void> {
  handlers.add(cb);
  return Promise.resolve(() => {
    unlistenMock();
    handlers.delete(cb);
  });
}

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
    getCurrentWindow: () => {
      if (noRuntime) throw new TypeError("no Tauri runtime");
      return {
        setSize: (s: unknown) => setSizeMock(s),
        isFullscreen: () => isFullscreenMock(),
        innerSize: hasInnerSize ? () => innerSizeMock() : undefined,
        scaleFactor: () => scaleFactorMock(),
        onResized: hasOnResized
          ? (cb: () => void) => onResizedImpl(cb)
          : undefined,
      };
    },
  };
});

function fire() {
  for (const h of [...handlers]) h();
}

// Lets every pending promise callback run. The checks under test chain a
// small, fixed number of awaits, so this does not depend on real time.
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

function physical(width: number, height: number) {
  innerSizeMock.mockImplementation(async () => ({ width, height }));
}

function correctionsLogged(): number[] {
  return log.debug.mock.calls
    .filter((c) => c[1] === "picker window size changed; apply fixed size again")
    .map((c) => (c[2] as { corrections: number }).corrections);
}

function centerCalls(): number {
  return invokeMock.mock.calls.filter((c) => c[0] === "center_picker").length;
}

const stops: Array<() => void> = [];

// Starts the guard and waits until its first check (PPK-FR-TBNQ) is done, then
// clears the call counts so that each test sees only its own resizes.
async function start(): Promise<() => void> {
  const stop = await keepPickerWindowSize();
  stops.push(stop);
  await vi.waitFor(() => expect(isFullscreenMock).toHaveBeenCalled());
  await flush();
  isFullscreenMock.mockClear();
  innerSizeMock.mockClear();
  scaleFactorMock.mockClear();
  setSizeMock.mockClear();
  invokeMock.mockClear();
  log.debug.mockClear();
  return stop;
}

beforeEach(() => {
  resetLogBufferForTest();
  log.warn.mockReset();
  log.debug.mockReset();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records" || cmd === "center_picker") return undefined;
    throw new Error(`unexpected ${cmd}`);
  });
  handlers.clear();
  unlistenMock.mockReset();
  setSizeMock.mockReset();
  setSizeMock.mockImplementation(async () => {});
  isFullscreenMock.mockReset();
  isFullscreenMock.mockResolvedValue(false);
  innerSizeMock.mockReset();
  physical(800, 600);
  scaleFactorMock.mockReset();
  scaleFactorMock.mockResolvedValue(1);
  onResizedImpl = defaultOnResized;
  hasInnerSize = true;
  hasOnResized = true;
  noRuntime = false;
});

afterEach(() => {
  for (const stop of stops.splice(0)) stop();
});

describe("keepPickerWindowSize", () => {
  it("PPK-FR-TBNQ, PPK-FR-09, PPK-FR-12: a resize to another size applies the fixed size and centres again", async () => {
    await start();
    physical(1024, 768);
    fire();
    await vi.waitFor(() => expect(centerCalls()).toBe(1));
    expect(setSizeMock).toHaveBeenCalledTimes(1);
    const size = setSizeMock.mock.calls[0][0] as Size;
    expect(size.width).toBe(PICKER_WINDOW_WIDTH);
    expect(size.height).toBe(PICKER_WINDOW_HEIGHT);
    expect(setSizeMock.mock.invocationCallOrder[0]).toBeLessThan(
      invokeMock.mock.invocationCallOrder[
        invokeMock.mock.calls.findIndex((c) => c[0] === "center_picker")
      ],
    );
    expect(correctionsLogged()).toEqual([1]);
  });

  it("PPK-FR-TBNQ: a resize to the fixed size applies nothing", async () => {
    await start();
    fire();
    await vi.waitFor(() => expect(scaleFactorMock).toHaveBeenCalledTimes(1));
    await flush();
    expect(setSizeMock).not.toHaveBeenCalled();
    expect(centerCalls()).toBe(0);
  });

  it("PPK-FR-TBNQ: the fixed size is compared in logical pixels", async () => {
    await start();
    scaleFactorMock.mockResolvedValue(2);
    physical(1600, 1200);
    fire();
    await vi.waitFor(() => expect(scaleFactorMock).toHaveBeenCalledTimes(1));
    await flush();
    expect(setSizeMock).not.toHaveBeenCalled();

    scaleFactorMock.mockResolvedValue(1);
    fire();
    await vi.waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(1));
  });

  it.each([
    [799, 600, false],
    [801, 600, false],
    [800, 599, false],
    [801, 601, false],
    [798, 600, true],
    [802, 600, true],
    [800, 598, true],
    [800, 602, true],
  ])("PPK-FR-TBNQ: logical %dx%d is corrected: %s", async (w, h, corrected) => {
    await start();
    physical(w, h);
    fire();
    await vi.waitFor(() => expect(scaleFactorMock).toHaveBeenCalledTimes(1));
    await flush();
    expect(setSizeMock).toHaveBeenCalledTimes(corrected ? 1 : 0);
  });

  it.each([0, -1, Number.NaN])(
    "PPK-FR-TBNQ: a scale factor of %s counts as 1",
    async (scale) => {
      await start();
      scaleFactorMock.mockResolvedValue(scale);
      fire();
      await vi.waitFor(() => expect(scaleFactorMock).toHaveBeenCalledTimes(1));
      await flush();
      expect(setSizeMock).not.toHaveBeenCalled();
    },
  );

  it("PPK-FR-TBNQ: after 3 corrections that do not hold, it stops and logs one warning", async () => {
    expect(MAX_SIZE_CORRECTIONS).toBe(3);
    await start();
    physical(1024, 768);
    // The platform refuses the size: each correction causes a new resize.
    setSizeMock.mockImplementation(async () => {
      fire();
    });
    fire();
    await vi.waitFor(() => expect(log.warn).toHaveBeenCalledTimes(1));
    expect(setSizeMock).toHaveBeenCalledTimes(3);
    expect(log.warn.mock.calls[0][2]).toEqual({
      width: 1024,
      height: 768,
      corrections: 3,
    });

    fire();
    fire();
    await flush();
    expect(setSizeMock).toHaveBeenCalledTimes(3);
    expect(log.warn).toHaveBeenCalledTimes(1);
  });

  it("PPK-FR-TBNQ: a resize to the fixed size resets the count and the warning", async () => {
    await start();
    const resizeTo = async (w: number, h: number, checks: number) => {
      physical(w, h);
      fire();
      await vi.waitFor(() =>
        expect(scaleFactorMock).toHaveBeenCalledTimes(checks),
      );
      await flush();
    };
    await resizeTo(1024, 768, 1);
    await resizeTo(1024, 768, 2);
    await resizeTo(800, 600, 3);
    await resizeTo(1024, 768, 4);
    expect(correctionsLogged()).toEqual([1, 2, 1]);

    await resizeTo(1024, 768, 5);
    await resizeTo(1024, 768, 6);
    expect(log.warn).not.toHaveBeenCalled();
    await resizeTo(1024, 768, 7);
    expect(log.warn).toHaveBeenCalledTimes(1);

    await resizeTo(800, 600, 8);
    for (let i = 9; i <= 12; i += 1) await resizeTo(1024, 768, i);
    expect(log.warn).toHaveBeenCalledTimes(2);
  });

  it("PPK-FR-TBNQ: a resize in OS full-screen applies nothing and keeps the count", async () => {
    await start();
    physical(1024, 768);
    fire();
    await vi.waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(1));
    fire();
    await vi.waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(2));

    isFullscreenMock.mockResolvedValue(true);
    const reads = innerSizeMock.mock.calls.length;
    fire();
    await vi.waitFor(() => expect(isFullscreenMock).toHaveBeenCalledTimes(3));
    await flush();
    expect(innerSizeMock).toHaveBeenCalledTimes(reads);
    expect(setSizeMock).toHaveBeenCalledTimes(2);

    isFullscreenMock.mockResolvedValue(false);
    fire();
    await vi.waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(3));
    expect(correctionsLogged()).toEqual([1, 2, 3]);
  });

  it("PPK-FR-TBNQ: resizes during a check merge into one follow-up check", async () => {
    await start();
    const first = deferred<Size>();
    innerSizeMock.mockImplementationOnce(() => first.promise);
    fire();
    await vi.waitFor(() => expect(innerSizeMock).toHaveBeenCalledTimes(1));
    fire();
    fire();
    fire();
    first.resolve({ width: 800, height: 600 });
    await vi.waitFor(() => expect(scaleFactorMock).toHaveBeenCalledTimes(2));
    await flush();
    expect(innerSizeMock).toHaveBeenCalledTimes(2);
    expect(setSizeMock).not.toHaveBeenCalled();
  });

  it("PPK-FR-TBNQ: stop during a check applies nothing and removes the listener", async () => {
    const stop = await start();
    const first = deferred<Size>();
    innerSizeMock.mockImplementationOnce(() => first.promise);
    fire();
    await vi.waitFor(() => expect(innerSizeMock).toHaveBeenCalledTimes(1));
    fire();
    stop();
    expect(unlistenMock).toHaveBeenCalledTimes(1);
    first.resolve({ width: 1024, height: 768 });
    await flush();
    expect(setSizeMock).not.toHaveBeenCalled();
    expect(innerSizeMock).toHaveBeenCalledTimes(1);
    fire();
    await flush();
    expect(innerSizeMock).toHaveBeenCalledTimes(1);
  });

  it("PPK-FR-TBNQ: a failed centring still counts the correction", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "append_log_records") return undefined;
      throw new Error(`refused ${cmd}`);
    });
    await start();
    physical(1024, 768);
    fire();
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("center_picker"));
    await flush();
    expect(setSizeMock).toHaveBeenCalledTimes(1);
    expect(correctionsLogged()).toEqual([1]);
    expect(log.debug).not.toHaveBeenCalledWith(
      ["frontend"],
      "picker window size check failed",
      expect.anything(),
    );
    for (let i = 2; i <= 4; i += 1) {
      fire();
      await vi.waitFor(() => expect(scaleFactorMock).toHaveBeenCalledTimes(i));
      await flush();
    }
    expect(setSizeMock).toHaveBeenCalledTimes(3);
    expect(log.warn).toHaveBeenCalledTimes(1);
  });

  it("PPK-FR-TBNQ: a failed size read is logged and applies nothing", async () => {
    await start();
    innerSizeMock.mockRejectedValueOnce(new Error("window gone"));
    fire();
    await vi.waitFor(() =>
      expect(log.debug).toHaveBeenCalledWith(
        ["frontend"],
        "picker window size check failed",
        { error: "window gone" },
      ),
    );
    expect(setSizeMock).not.toHaveBeenCalled();
  });

  it("PPK-FR-TBNQ: a failed setSize counts toward the limit", async () => {
    await start();
    physical(1024, 768);
    setSizeMock.mockRejectedValue(new Error("refused"));
    for (let i = 1; i <= 4; i += 1) {
      fire();
      await vi.waitFor(() =>
        expect(scaleFactorMock).toHaveBeenCalledTimes(i),
      );
      await flush();
    }
    expect(setSizeMock).toHaveBeenCalledTimes(3);
    expect(log.warn).toHaveBeenCalledTimes(1);
  });

  it("PPK-FR-TBNQ: without an innerSize API it applies nothing", async () => {
    hasInnerSize = false;
    await start();
    fire();
    await vi.waitFor(() => expect(isFullscreenMock).toHaveBeenCalledTimes(1));
    await flush();
    expect(setSizeMock).not.toHaveBeenCalled();
    expect(log.debug).not.toHaveBeenCalled();
  });

  it("PPK-FR-TBNQ: without an onResized API it returns a stop that does nothing", async () => {
    hasOnResized = false;
    const stop = await keepPickerWindowSize();
    expect(() => stop()).not.toThrow();
  });

  it("PPK-FR-TBNQ: a failed subscription is logged and returns a stop that does nothing", async () => {
    onResizedImpl = () => Promise.reject(new Error("no events"));
    const stop = await keepPickerWindowSize();
    expect(log.debug).toHaveBeenCalledWith(
      ["frontend"],
      "picker window resize subscription failed",
      { error: "no events" },
    );
    expect(() => stop()).not.toThrow();
  });
  it("PPK-FR-TBNQ, PPK-FR-12: stop while the size is set skips the centring", async () => {
    const stop = await start();
    const held = deferred<void>();
    setSizeMock.mockImplementationOnce(() => held.promise);
    physical(1024, 768);
    fire();
    await vi.waitFor(() => expect(setSizeMock).toHaveBeenCalledTimes(1));
    stop();
    held.resolve();
    await flush();
    expect(centerCalls()).toBe(0);
  });

  it("PPK-FR-TBNQ: outside the Tauri runtime it returns a stop that does nothing", async () => {
    noRuntime = true;
    const stop = await keepPickerWindowSize();
    expect(() => stop()).not.toThrow();
  });
  it("PPK-FR-TBNQ: a wrong size before the subscription is corrected without a resize", async () => {
    physical(1440, 900);
    stops.push(await keepPickerWindowSize());
    await vi.waitFor(() => expect(centerCalls()).toBe(1));
    expect(setSizeMock).toHaveBeenCalledTimes(1);
    expect(correctionsLogged()).toEqual([1]);
  });

  it("PPK-FR-TBNQ: the first check applies nothing at the fixed size", async () => {
    stops.push(await keepPickerWindowSize());
    await vi.waitFor(() => expect(scaleFactorMock).toHaveBeenCalledTimes(1));
    await flush();
    expect(setSizeMock).not.toHaveBeenCalled();
    expect(centerCalls()).toBe(0);
  });

  it("PPK-FR-TBNQ: a failed full-screen read is logged and applies nothing", async () => {
    await start();
    isFullscreenMock.mockRejectedValueOnce(new Error("no state"));
    physical(1024, 768);
    fire();
    await vi.waitFor(() =>
      expect(log.debug).toHaveBeenCalledWith(
        ["frontend"],
        "picker window size check failed",
        { error: "no state" },
      ),
    );
    expect(setSizeMock).not.toHaveBeenCalled();
  });
});
