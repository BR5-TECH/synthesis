/**
 * TRM-FR-KZPU: the text of a terminal keeps its size when the display's pixel
 * ratio changes, or when the WebView resets the canvas state
 * (`../../specifications/ui/TRM-terminals.md`).
 *
 * ghostty-web 0.4.0 reads `devicePixelRatio` once, when it makes the renderer.
 * It applies its drawing scale with one `ctx.scale()` in `resize()`, and each
 * frame it compares only the canvas width and height. When the WebView loses
 * the drawing surface and makes it again (for example while the display
 * sleeps), the context goes back to the identity transform but the canvas
 * keeps its size. The library then draws the text at a fraction of its size
 * until the window changes its size.
 */
import type { Terminal } from "ghostty-web";
import { logInfo, logWarn } from "../logging";

/** The part of the frame buffer that the repair reads. */
interface FrameBuffer {
  getDimensions?: () => { cols: number; rows: number };
}

type Render = (buffer: FrameBuffer, forceAll?: boolean, ...rest: unknown[]) => void;

/** The private parts of the ghostty-web 0.4.0 renderer that the repair uses. */
interface RendererInternals {
  devicePixelRatio?: number;
  metrics?: { width: number; height: number };
  canvas?: { width: number; height: number };
  ctx?: { getTransform?: () => { a: number; d: number } };
  resize?: (cols: number, rows: number) => void;
  render?: Render;
}

export type CanvasRepairReason = "pixel_ratio" | "state_lost";

/** The transform scale and the pixel ratio are equal to this tolerance. */
const SCALE_TOLERANCE = 1e-3;

function currentRatio(): number {
  const ratio = typeof window === "undefined" ? 1 : window.devicePixelRatio;
  return Number.isFinite(ratio) && ratio > 0 ? ratio : 1;
}

/**
 * TRM-FR-KZPU: why the next frame of `renderer` must repair the canvas, or
 * null when the canvas is correct. A canvas whose size does not agree with the
 * pixel ratio is not a lost state: the library resizes it in the same frame,
 * for example in its own `Terminal.resize()`.
 */
export function canvasRepairNeeded(
  renderer: RendererInternals,
  buffer: FrameBuffer,
  ratio: number,
): CanvasRepairReason | null {
  if (renderer.devicePixelRatio !== ratio) return "pixel_ratio";
  const transform = renderer.ctx?.getTransform?.();
  const size = buffer.getDimensions?.();
  const { metrics, canvas } = renderer;
  if (!transform || !size || !metrics || !canvas) return null;
  if (canvas.width !== size.cols * metrics.width * ratio || canvas.height !== size.rows * metrics.height * ratio)
    return null;
  if (Math.abs(transform.a - ratio) <= SCALE_TOLERANCE && Math.abs(transform.d - ratio) <= SCALE_TOLERANCE) return null;
  return "state_lost";
}

/**
 * TRM-FR-KZPU: makes the renderer of `term` check its canvas before each frame
 * and repair it. Call it after `open()`, which makes the renderer. Where the
 * library has no such renderer, it changes nothing and, where `warn` is true,
 * logs a `WARN` record (TRM-FR-LNOZ). Returns whether it applied.
 */
export function keepCanvasScale(term: Pick<Terminal, "renderer">, terminalId: string, warn = true): boolean {
  const renderer = term.renderer as unknown as RendererInternals | undefined;
  const original = renderer?.render;
  const resize = renderer?.resize;
  if (!renderer || typeof original !== "function" || typeof resize !== "function") {
    if (warn) logWarn(["frontend"], "terminal canvas scale not kept", { terminalId, reason: "no_renderer_hook" });
    return false;
  }
  // TRM-FR-LNOZ: one record for each repair, not for each frame of a run of them.
  let lastRepair: CanvasRepairReason | null = null;
  renderer.render = function renderAtScale(this: RendererInternals, buffer, forceAll, ...rest) {
    const ratio = currentRatio();
    const reason = canvasRepairNeeded(this, buffer, ratio);
    if (reason && reason !== lastRepair) logInfo(["frontend"], "terminal canvas repaired", { terminalId, reason, ratio });
    lastRepair = reason;
    if (reason === "pixel_ratio") {
      // The library's own size check sees the new ratio and resizes in this frame.
      this.devicePixelRatio = ratio;
    } else if (reason === "state_lost") {
      const size = buffer.getDimensions?.();
      if (size) resize.call(this, size.cols, size.rows);
      forceAll = true;
    }
    return original.call(this, buffer, forceAll, ...rest);
  };
  return true;
}
