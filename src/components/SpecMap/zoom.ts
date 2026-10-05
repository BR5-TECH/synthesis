/**
 * Semantic zoom and the stage transform (`SMZ-specification-map-zoom.md`).
 *
 * The detail level is explicit state. Each level is viewed inside one legible
 * scale window, and crossing an end of the window steps the level instead of
 * shrinking the type (SMZ-FR-GMEB, SMZ-FR-ZQLP).
 */
import type { MapView } from "../../state/specMap/session";
import type { Rect } from "./layout";

export const SCALE_MIN = 0.66;
export const SCALE_MAX = 1.3;
/** SMZ-FR-ZQLP: a wheel step toward the user, and away. */
export const WHEEL_IN = 1.06;
export const WHEEL_OUT = 0.94;
/** SMZ-FR-JWXA: the plus and minus buttons. */
export const BUTTON_STEP = 1.18;
export const FIRST_PAINT_SCALE = 0.8;

/** SMZ-FR-ZQLP / SMZ-FR-HCTF: one zoom step, which may step the level. */
export function zoomStep(view: MapView, factor: number, lastLevel: number): MapView {
  const requested = view.scale * factor;
  let level = view.level;
  let scale = requested;
  if (requested > SCALE_MAX) {
    if (level < lastLevel) {
      level += 1;
      scale = SCALE_MIN;
    } else {
      scale = SCALE_MAX;
    }
  } else if (requested < SCALE_MIN) {
    if (level > 0) {
      level -= 1;
      scale = SCALE_MAX;
    } else {
      scale = SCALE_MIN;
    }
  }
  const ratio = scale / view.scale;
  return { level, scale, tx: view.tx * ratio, ty: view.ty * ratio, touched: true };
}

/**
 * SMZ-FR-NUOB: level 0 at scale 0.8, top-aligned from the measured canvas
 * height rather than centred.
 */
export function firstPaintView(contentHeight: number, canvasHeight: number): MapView {
  return topAlignedView(0, FIRST_PAINT_SCALE, contentHeight, canvasHeight, false);
}

/**
 * SMZ-FR-NUOB / SMZ-FR-QOTA: a level framed with the top of its content near
 * the top of the canvas rather than centred.
 */
export function topAlignedView(
  level: number,
  scale: number,
  contentHeight: number,
  canvasHeight: number,
  touched: boolean,
): MapView {
  return {
    level,
    scale,
    tx: 0,
    ty: Math.max(0, (contentHeight / 2) * scale - canvasHeight / 2 + 26),
    touched,
  };
}

/** SMZ-FR-EPTR: the scale a chosen level opens at. */
export function centerScale(level: number, lastLevel: number): number {
  return level === lastLevel ? 0.85 : 0.8;
}

/**
 * SMZ-FR-EPTR: a view that centres `rect` horizontally, with its vertical
 * centre at most 240px below its top.
 */
export function centeredOn(rect: Rect, level: number, scale: number): MapView {
  const cx = rect.x + rect.w / 2;
  const cy = rect.y + Math.min(rect.h / 2, 240);
  return { level, scale, tx: -cx * scale, ty: -cy * scale, touched: true };
}

export interface CanvasRect {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function screenToWorld(
  clientX: number,
  clientY: number,
  rect: CanvasRect,
  view: MapView,
): { x: number; y: number } {
  return {
    x: (clientX - rect.left - rect.width / 2 - view.tx) / view.scale,
    y: (clientY - rect.top - rect.height / 2 - view.ty) / view.scale,
  };
}

/** A world point in canvas coordinates, the canvas's top-left being 0,0. */
export function worldToCanvas(
  x: number,
  y: number,
  view: MapView,
  canvasWidth: number,
  canvasHeight: number,
): { x: number; y: number } {
  return {
    x: canvasWidth / 2 + view.tx + x * view.scale,
    y: canvasHeight / 2 + view.ty + y * view.scale,
  };
}

export const HOVER_CARD_W = 280;
export const HOVER_CARD_H = 170;

/**
 * SMN-FR-MVWA: 18px right of and 14px below the node's top-left corner, clamped
 * to stay 8px inside the canvas.
 */
export function placeHoverCard(
  worldX: number,
  worldY: number,
  view: MapView,
  canvasWidth: number,
  canvasHeight: number,
): { x: number; y: number } {
  const p = worldToCanvas(worldX, worldY, view, canvasWidth, canvasHeight);
  return {
    x: Math.max(8, Math.min(p.x + 18, canvasWidth - HOVER_CARD_W - 8)),
    y: Math.max(8, Math.min(p.y + 14, canvasHeight - HOVER_CARD_H - 8)),
  };
}

/** SMZ-FR-JWXA: the percentage the zoom control shows. */
export const scaleLabel = (scale: number): string => `${Math.round(scale * 100)}%`;
