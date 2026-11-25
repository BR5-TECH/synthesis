// The text size of a terminal over pixel ratio changes and a lost canvas state (`TRM-terminals.md` TRM-FR-KZPU, TRM-FR-LNOZ).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Terminal } from "ghostty-web";
import { canvasRepairNeeded, keepCanvasScale } from "./terminalCanvas";
import { FakeRenderer } from "../test/ghosttyMock";
import { logInfo, logWarn } from "../logging";

vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

const SIZE = { cols: 80, rows: 24 };

const ORIGINAL_RATIO = Object.getOwnPropertyDescriptor(window, "devicePixelRatio");

function setRatio(ratio: number) {
  Object.defineProperty(window, "devicePixelRatio", { configurable: true, value: ratio });
}

function restoreRatio() {
  if (ORIGINAL_RATIO) Object.defineProperty(window, "devicePixelRatio", ORIGINAL_RATIO);
  else delete (window as { devicePixelRatio?: number }).devicePixelRatio;
}

/** A renderer at the pixel ratio of the window, after its first frame, with the patch. */
function patched() {
  const renderer = new FakeRenderer("#000000");
  const term = { renderer } as unknown as Pick<Terminal, "renderer">;
  expect(keepCanvasScale(term, "t1")).toBe(true);
  renderer.frame(SIZE);
  renderer.resized.length = 0;
  renderer.rendered.length = 0;
  vi.mocked(logInfo).mockClear();
  return renderer;
}

const sizeOf = (renderer: FakeRenderer) => ({ width: renderer.canvas.width, height: renderer.canvas.height });

const repairs = () => vi.mocked(logInfo).mock.calls.filter((c) => c[1] === "terminal canvas repaired");

describe("keepCanvasScale", () => {
  beforeEach(() => {
    vi.mocked(logInfo).mockClear();
    vi.mocked(logWarn).mockClear();
    setRatio(2);
  });
  afterEach(restoreRatio);

  it("TRM-FR-KZPU: a frame with a correct canvas passes through, with no resize and no record", () => {
    const renderer = patched();
    renderer.frame(SIZE);
    expect(renderer.resized).toEqual([]);
    expect(renderer.rendered).toEqual([false]);
    expect(renderer.ctx.getTransform()).toEqual({ a: 2, d: 2 });
    expect(repairs()).toEqual([]);
  });

  it("TRM-FR-KZPU, TRM-FR-LNOZ: a lost canvas state gets its scale back on the next frame, with a full redraw and one INFO", () => {
    const renderer = patched();
    renderer.ctx.resetTransform();
    renderer.frame(SIZE);
    expect(renderer.resized).toEqual([SIZE]);
    expect(renderer.rendered).toEqual([true]);
    expect(renderer.ctx.getTransform()).toEqual({ a: 2, d: 2 });
    expect(repairs()).toEqual([[["frontend"], "terminal canvas repaired", { terminalId: "t1", reason: "state_lost", ratio: 2 }]]);
    renderer.frame(SIZE);
    expect(renderer.resized).toEqual([SIZE]);
    expect(renderer.rendered).toEqual([true, false]);
    expect(repairs()).toHaveLength(1);
  });

  it("TRM-FR-KZPU, TRM-FR-LNOZ: a second loss after a correct frame is repaired again, with its own INFO", () => {
    const renderer = patched();
    renderer.ctx.resetTransform();
    renderer.frame(SIZE);
    renderer.frame(SIZE);
    renderer.ctx.resetTransform();
    renderer.frame(SIZE);
    expect(renderer.resized).toEqual([SIZE, SIZE]);
    expect(renderer.ctx.getTransform()).toEqual({ a: 2, d: 2 });
    expect(repairs()).toHaveLength(2);
  });

  it("TRM-FR-LNOZ: a loss that stays over many frames is repaired on each frame and gives one INFO", () => {
    const renderer = patched();
    for (let i = 0; i < 3; i++) {
      renderer.ctx.resetTransform();
      renderer.frame(SIZE);
    }
    expect(renderer.resized).toHaveLength(3);
    expect(repairs()).toHaveLength(1);
  });

  it("TRM-FR-KZPU: a canvas that the library resizes in this frame is not a lost state", () => {
    const renderer = patched();
    // As `Terminal.resize()`: the canvas is set to one pixel for each CSS pixel, which resets the transform.
    renderer.canvas.width = 100 * renderer.metrics.width;
    renderer.canvas.height = 24 * renderer.metrics.height;
    expect(renderer.ctx.getTransform()).toEqual({ a: 1, d: 1 });
    renderer.frame({ cols: 100, rows: 24 });
    expect(renderer.resized).toEqual([{ cols: 100, rows: 24 }]);
    expect(renderer.rendered).toEqual([true]);
    expect(renderer.ctx.getTransform()).toEqual({ a: 2, d: 2 });
    expect(repairs()).toEqual([]);
  });

  it("TRM-FR-KZPU: a canvas whose height alone does not agree with the pixel ratio is not a lost state", () => {
    const renderer = patched();
    renderer.frame({ cols: 80, rows: 30 });
    expect(renderer.resized).toEqual([{ cols: 80, rows: 30 }]);
    expect(renderer.ctx.getTransform()).toEqual({ a: 2, d: 2 });
    expect(repairs()).toEqual([]);
  });

  it("TRM-FR-KZPU, TRM-FR-LNOZ: a pixel ratio change and a lost state in one frame give one repair at the new ratio", () => {
    const renderer = patched();
    setRatio(1);
    renderer.ctx.resetTransform();
    renderer.frame(SIZE);
    expect(sizeOf(renderer)).toEqual({ width: 80 * 8, height: 24 * 16 });
    expect(renderer.ctx.getTransform()).toEqual({ a: 1, d: 1 });
    expect(repairs().map((c) => (c[2] as { reason: string }).reason)).toEqual(["pixel_ratio"]);
  });

  it("TRM-FR-LNOZ: frames that need a repair for a new reason give a new INFO", () => {
    const renderer = patched();
    renderer.ctx.resetTransform();
    renderer.frame(SIZE);
    setRatio(3);
    renderer.frame(SIZE);
    expect(renderer.ctx.getTransform()).toEqual({ a: 3, d: 3 });
    expect(repairs().map((c) => (c[2] as { reason: string }).reason)).toEqual(["state_lost", "pixel_ratio"]);
  });

  it("TRM-FR-KZPU: a frame that the caller forces stays forced when it needs no repair", () => {
    const renderer = patched();
    renderer.render?.({ getDimensions: () => SIZE }, true);
    expect(renderer.rendered).toEqual([true]);
    expect(renderer.resized).toEqual([]);
  });

  it("TRM-FR-KZPU, TRM-FR-LNOZ: a lower pixel ratio applies on the next frame, with one INFO", () => {
    const renderer = patched();
    setRatio(1);
    renderer.frame(SIZE);
    expect(renderer.devicePixelRatio).toBe(1);
    expect(sizeOf(renderer)).toEqual({ width: 80 * 8, height: 24 * 16 });
    expect(renderer.ctx.getTransform()).toEqual({ a: 1, d: 1 });
    expect(renderer.rendered).toEqual([true]);
    expect(repairs()).toEqual([[["frontend"], "terminal canvas repaired", { terminalId: "t1", reason: "pixel_ratio", ratio: 1 }]]);
    renderer.frame(SIZE);
    expect(repairs()).toHaveLength(1);
  });

  it("TRM-FR-KZPU: a higher pixel ratio applies on the next frame", () => {
    setRatio(1);
    const renderer = patched();
    setRatio(2);
    renderer.frame(SIZE);
    expect(sizeOf(renderer)).toEqual({ width: 80 * 8 * 2, height: 24 * 16 * 2 });
    expect(renderer.ctx.getTransform()).toEqual({ a: 2, d: 2 });
    expect(repairs()[0][2]).toEqual({ terminalId: "t1", reason: "pixel_ratio", ratio: 2 });
  });

  it("TRM-FR-KZPU: a pixel ratio that the window does not give counts as 1", () => {
    const renderer = patched();
    setRatio(Number.NaN);
    renderer.frame(SIZE);
    expect(renderer.devicePixelRatio).toBe(1);
    expect(renderer.ctx.getTransform()).toEqual({ a: 1, d: 1 });
  });

  it("TRM-FR-KZPU: the other arguments of a frame reach the library unchanged", () => {
    const renderer = patched();
    const calls: unknown[][] = [];
    const term = { renderer: { ...renderer, render: (...args: unknown[]) => calls.push(args), resize: () => {} } };
    keepCanvasScale(term as unknown as Pick<Terminal, "renderer">, "t2");
    const buffer = { getDimensions: () => SIZE };
    (term.renderer.render as (...args: unknown[]) => void)(buffer, false, 7, "scrollback", 0.5);
    expect(calls).toEqual([[buffer, false, 7, "scrollback", 0.5]]);
  });

  it("TRM-FR-LNOZ: a library with no renderer, or with no render or resize, gives one WARN where asked and is not patched", () => {
    const render = () => {};
    const resize = () => {};
    const renderOnly = { render };
    const resizeOnly = { resize };
    expect(keepCanvasScale({ renderer: undefined } as unknown as Pick<Terminal, "renderer">, "t1")).toBe(false);
    expect(keepCanvasScale({ renderer: renderOnly } as unknown as Pick<Terminal, "renderer">, "t2")).toBe(false);
    expect(keepCanvasScale({ renderer: resizeOnly } as unknown as Pick<Terminal, "renderer">, "t3", false)).toBe(false);
    expect(renderOnly.render).toBe(render);
    expect(resizeOnly).toEqual({ resize });
    expect(vi.mocked(logWarn).mock.calls).toEqual([
      [["frontend"], "terminal canvas scale not kept", { terminalId: "t1", reason: "no_renderer_hook" }],
      [["frontend"], "terminal canvas scale not kept", { terminalId: "t2", reason: "no_renderer_hook" }],
    ]);
  });
});

describe("canvasRepairNeeded", () => {
  const renderer = (transform: { a: number; d: number } | null) => ({
    devicePixelRatio: 2,
    metrics: { width: 8, height: 16 },
    canvas: { width: 80 * 8 * 2, height: 24 * 16 * 2 },
    ctx: transform ? { getTransform: () => transform } : {},
  });
  const buffer = { getDimensions: () => SIZE };

  it("TRM-FR-KZPU: a scale within the tolerance of the pixel ratio needs no repair", () => {
    expect(canvasRepairNeeded(renderer({ a: 2.0004, d: 1.9996 }), buffer, 2)).toBeNull();
    expect(canvasRepairNeeded(renderer({ a: 2, d: 1 }), buffer, 2)).toBe("state_lost");
  });

  it("TRM-FR-KZPU: a renderer or a buffer that gives no transform, size, or metrics needs no repair", () => {
    expect(canvasRepairNeeded(renderer(null), buffer, 2)).toBeNull();
    expect(canvasRepairNeeded(renderer({ a: 1, d: 1 }), {}, 2)).toBeNull();
    expect(canvasRepairNeeded({ ...renderer({ a: 1, d: 1 }), metrics: undefined }, buffer, 2)).toBeNull();
    expect(canvasRepairNeeded({ ...renderer({ a: 1, d: 1 }), canvas: undefined }, buffer, 2)).toBeNull();
  });
});
