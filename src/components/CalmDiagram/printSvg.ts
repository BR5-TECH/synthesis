/**
 * CAD-FR-QHEJ, CAD-FR-FONB, CAD-FR-VBDT: the print drawing of a CALM diagram.
 *
 * The drawing is one self-contained SVG document of the layout that the inline
 * canvas shows (`CalmCanvas.tsx`), with no flow chosen and nothing selected.
 * It copies the metrics of `calm-diagram.css` and uses the light colours of
 * `printPalette.ts`. Every CALM text is escaped SVG text. The backend
 * rasterises the drawing for a PDF or a DOCX export (DXP-FR-WGLT).
 */
import { getBezierPath, Position } from "@xyflow/react";

import { SHOWN_INTERFACES } from "../../calm/layout";
import type { Box, CalmEdgeView, CalmLayout, CalmNodeView, CalmShape } from "../../calm/types";
import { besideEnd, routePath, sideOf } from "./geometry";
import { absoluteBoxes, exitPoint, type Side } from "./model";
import { GLYPHS } from "./nodes";
import { fitText, measureText, type Measure, type PrintFont } from "./printMeasure";
import { ink, PRINT_MONO, PRINT_SANS, type PrintToken } from "./printPalette";

/** CAD-FR-VBDT: `text` as SVG character data, with no markup in it. */
export function escapeXml(text: string): string {
  return (
    text
      // A control character is not allowed in XML 1.0, so it is left out.
      .replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F￾￿]/g, "")
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&apos;")
  );
}

/** A number for an attribute, with at most two decimals. */
const n = (value: number): string => String(Math.round(value * 100) / 100);

// The metrics of `calm-diagram.css`, in CSS pixels.
const NAME: PrintFont = { size: 13, bold: true };
const SMALL: PrintFont = { size: 10 };
const SMALL_ITALIC: PrintFont = { size: 10, italic: true };
const MONO: PrintFont = { size: 10, mono: true };
const SNUG = 1.35;
const NAME_LINE = NAME.size * SNUG;
const TYPE_LINE = MONO.size * SNUG;
const ROW = 18;
const GLYPH = 14;
const GAP = 6;
const LABEL_MAX = 220;

/** The baseline of a text of `font` in a line box at `top` of `height`. */
function baseline(top: number, height: number, font: PrintFont): number {
  // The ascent and the descent of Liberation Sans and Liberation Mono.
  const [ascent, descent] = font.mono ? [0.833, 0.3] : [0.905, 0.212];
  return top + (height - (ascent + descent) * font.size) / 2 + ascent * font.size;
}

/** One `<text>` element at the start (or the end) of `x`. */
function text(
  content: string,
  x: number,
  top: number,
  height: number,
  font: PrintFont,
  fill: PrintToken,
  anchor: "start" | "middle" | "end" = "start",
): string {
  const attrs = [
    `x="${n(x)}"`,
    `y="${n(baseline(top, height, font))}"`,
    `font-family="${font.mono ? PRINT_MONO : PRINT_SANS}"`,
    `font-size="${font.size}"`,
    font.bold ? `font-weight="bold"` : "",
    font.italic ? `font-style="italic"` : "",
    anchor === "start" ? "" : `text-anchor="${anchor}"`,
    `fill="${ink(fill)}"`,
  ]
    .filter(Boolean)
    .join(" ");
  return `<text ${attrs} xml:space="preserve">${escapeXml(content)}</text>`;
}

/** The spine colour of each shape (CLM-FR-DTYX). */
const SPINE: Record<CalmShape, PrintToken> = {
  actor: "--code-keyword",
  system: "--code-type",
  service: "--code-function",
  database: "--code-number",
  webclient: "--code-string",
  network: "--code-meta",
  dataAsset: "--code-number",
  generic: "--fg-4",
};

/** The border widths of a node card: top, right, and bottom; the left is 3. */
const BORDER: Record<CalmShape, number> = {
  actor: 1,
  system: 1.5,
  service: 1,
  database: 1,
  webclient: 1,
  network: 3,
  dataAsset: 1,
  generic: 1,
};

/**
 * A rounded rectangle as one path. The top corners are round; the bottom
 * corners are round too, or elliptic with the height `bottomRy`.
 */
function roundedRect(
  b: Box,
  r: { tl: number; tr: number; br: number; bl: number },
  bottomRy?: number,
): string {
  const cap = (value: number) => Math.min(value, b.width / 2, b.height / 2);
  const tl = cap(r.tl);
  const tr = cap(r.tr);
  const [brx, bry] =
    bottomRy === undefined ? [cap(r.br), cap(r.br)] : [Math.min(r.br, b.width / 2), Math.min(bottomRy, b.height / 2)];
  const [blx, bly] =
    bottomRy === undefined ? [cap(r.bl), cap(r.bl)] : [Math.min(r.bl, b.width / 2), Math.min(bottomRy, b.height / 2)];
  const right = b.x + b.width;
  const bottom = b.y + b.height;
  return [
    `M ${n(b.x + tl)} ${n(b.y)}`,
    `H ${n(right - tr)}`,
    `A ${n(tr)} ${n(tr)} 0 0 1 ${n(right)} ${n(b.y + tr)}`,
    `V ${n(bottom - bry)}`,
    `A ${n(brx)} ${n(bry)} 0 0 1 ${n(right - brx)} ${n(bottom)}`,
    `H ${n(b.x + blx)}`,
    `A ${n(blx)} ${n(bly)} 0 0 1 ${n(b.x)} ${n(bottom - bly)}`,
    `V ${n(b.y + tl)}`,
    `A ${n(tl)} ${n(tl)} 0 0 1 ${n(b.x + tl)} ${n(b.y)}`,
    "Z",
  ].join(" ");
}

/** CLM-FR-DTYX: the outline of a node card of `shape`. */
function shapePath(shape: CalmShape, b: Box): string {
  switch (shape) {
    case "actor":
      return roundedRect(b, { tl: 999, tr: 999, br: 999, bl: 999 });
    case "system":
      return roundedRect(b, { tl: 3, tr: 3, br: 3, bl: 3 });
    case "database": {
      // `border-radius: 5px 5px 40% 40% / 5px 5px 14px 14px`.
      const rx = b.width * 0.4;
      return roundedRect(b, { tl: 5, tr: 5, br: rx, bl: rx }, 14);
    }
    case "dataAsset":
      return roundedRect(b, { tl: 3, tr: 14, br: 3, bl: 3 });
    default:
      return roundedRect(b, { tl: 5, tr: 5, br: 5, bl: 5 });
  }
}

/** A glyph of a shape, 14 px, in the spine colour (`nodes.tsx` ShapeGlyph). */
function glyph(shape: CalmShape, x: number, y: number, stroke: PrintToken): string {
  return `<g transform="translate(${n(x)} ${n(y)}) scale(${GLYPH / 16})"><path d="${GLYPHS[shape]}" fill="none" stroke="${ink(
    stroke,
  )}" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"/></g>`;
}

const SHIELD = "M8 1.5 13 3.5v4c0 3.2-2.2 5.6-5 7-2.8-1.4-5-3.8-5-7v-4z";
const DIAMOND = "M8 1.5 14.5 8 8 14.5 1.5 8z";

/** CLM-FR-ETEA, CLM-FR-ESMQ: the markers, right-aligned at `right`. */
function markers(
  node: CalmNodeView,
  right: number,
  top: number,
  height: number,
  measure: Measure,
): { svg: string; width: number } {
  const items: Array<{ path: string; count: number; color: PrintToken }> = [];
  if (node.controls.length > 0) items.push({ path: SHIELD, count: node.controls.length, color: "--ok" });
  if (node.decisions.length > 0) items.push({ path: DIAMOND, count: node.decisions.length, color: "--warn" });
  if (items.length === 0) return { svg: "", width: 0 };
  const widths = items.map((item) => 11 + 2 + measure(String(item.count), SMALL));
  const width = widths.reduce((a, b) => a + b, 0) + (items.length - 1) * 4;
  let x = right - width;
  const parts = items.map((item, index) => {
    const iconY = top + (height - 11) / 2;
    const part =
      `<g transform="translate(${n(x)} ${n(iconY)}) scale(${11 / 16})"><path d="${item.path}" fill="none" stroke="${ink(
        item.color,
      )}" stroke-width="1.5" stroke-linejoin="round"/></g>` +
      text(String(item.count), x + 13, top, height, SMALL, item.color);
    x += widths[index] + 4;
    return part;
  });
  return { svg: parts.join(""), width };
}

interface Drawing {
  defs: string[];
  body: string[];
  measure: Measure;
}

/** CLM-FR-DLUV, CLM-FR-DTYX, CLM-FR-ETEA, CLM-FR-DYAW: one node card. */
function drawNode(d: Drawing, node: CalmNodeView, b: Box, index: number): void {
  const shape = node.shape;
  const outline = shapePath(shape, b);
  const clip = `calm-clip-${index}`;
  d.defs.push(`<clipPath id="${clip}"><path d="${outline}"/></clipPath>`);
  const border = BORDER[shape];
  const parts: string[] = [];
  parts.push(`<path d="${outline}" fill="${ink("--bg-canvas")}" filter="url(#calm-shadow)"/>`);
  parts.push(`<g clip-path="url(#${clip})">`);
  // The borders: drawn inside the outline, as `box-sizing: border-box` does.
  const stroke = ink("--border-3");
  const inset = (by: number): Box => ({
    x: b.x + by,
    y: b.y + by,
    width: b.width - 2 * by,
    height: b.height - 2 * by,
  });
  if (shape === "network") {
    parts.push(`<path d="${shapePath(shape, inset(0.5))}" fill="none" stroke="${stroke}" stroke-width="1"/>`);
    parts.push(`<path d="${shapePath(shape, inset(2.5))}" fill="none" stroke="${stroke}" stroke-width="1"/>`);
  } else {
    const dash = shape === "generic" ? ` stroke-dasharray="4 3"` : "";
    parts.push(
      `<path d="${shapePath(shape, inset(border / 2))}" fill="none" stroke="${stroke}" stroke-width="${border}"${dash}/>`,
    );
  }
  if (shape === "webclient") {
    parts.push(`<rect x="${n(b.x)}" y="${n(b.y)}" width="${n(b.width)}" height="5" fill="${stroke}"/>`);
  }
  // The spine: the left border in the colour of the family of the type.
  parts.push(`<rect x="${n(b.x)}" y="${n(b.y)}" width="3" height="${n(b.height)}" fill="${ink(SPINE[shape])}"/>`);

  const top = b.y + (shape === "webclient" ? 5 : border) + 8;
  const left = b.x + 3 + (shape === "actor" ? 16 : 12);
  const right = b.x + b.width - border - 10;
  parts.push(glyph(shape, left, top + (NAME_LINE - GLYPH) / 2, SPINE[shape]));
  const marks = markers(node, right, top, NAME_LINE, d.measure);
  const nameX = left + GLYPH + GAP;
  const nameWidth = right - nameX - (marks.width > 0 ? marks.width + GAP : 0);
  parts.push(text(fitText(node.name, nameWidth, NAME, d.measure), nameX, top, NAME_LINE, NAME, "--fg-1"));
  parts.push(marks.svg);
  // The type is clipped by the card, as on the canvas.
  const typeTop = top + NAME_LINE + 1;
  parts.push(text(node.nodeType, nameX, typeTop, TYPE_LINE, MONO, "--fg-3"));
  let y = typeTop + TYPE_LINE;
  const shown = node.interfaces.slice(0, SHOWN_INTERFACES);
  const more = node.interfaces.length - shown.length;
  if (shown.length > 0) {
    y += 4;
    parts.push(
      `<rect x="${n(left)}" y="${n(y)}" width="${n(right - left)}" height="1" fill="${ink("--border-1")}"/>`,
    );
    y += 1 + 4;
    const bullet = "◦ ";
    const bulletWidth = d.measure(bullet, MONO);
    for (const iface of shown) {
      const id = String(iface["unique-id"] ?? "");
      parts.push(text(bullet, left, y, ROW, MONO, "--fg-4"));
      parts.push(text(fitText(id, right - left - bulletWidth, MONO, d.measure), left + bulletWidth, y, ROW, MONO, "--fg-2"));
      y += ROW;
    }
    if (more > 0) {
      parts.push(text(`+${more} more`, left, y, ROW, SMALL, "--fg-3"));
      y += ROW;
    }
  }
  if (node.alsoIn.length > 0) {
    const also = `Also in ${node.alsoIn.join(", ")}`;
    parts.push(text(fitText(also, right - left, SMALL_ITALIC, d.measure), left, y, ROW, SMALL_ITALIC, "--fg-3"));
  }
  parts.push("</g>");
  d.body.push(parts.join(""));
}

/** CLM-FR-DWLY: one group box, dashed for `deployed-in`. */
function drawGroup(d: Drawing, node: CalmNodeView, b: Box): void {
  const composed = node.group === "composed-of";
  const outline = roundedRect(
    { x: b.x + 0.75, y: b.y + 0.75, width: b.width - 1.5, height: b.height - 1.5 },
    { tl: 7, tr: 7, br: 7, bl: 7 },
  );
  const parts: string[] = [];
  parts.push(
    composed
      ? `<path d="${outline}" fill="${ink("--bg-sunken")}" fill-opacity="0.55" stroke="${ink("--border-2")}" stroke-width="1.5"/>`
      : `<path d="${outline}" fill="none" stroke="${ink("--border-3")}" stroke-width="1.5" stroke-dasharray="6 4"/>`,
  );
  const top = b.y + 1.5 + 8;
  const left = b.x + 1.5 + 12;
  const right = b.x + b.width - 1.5 - 12;
  parts.push(glyph(node.shape, left, top + (NAME_LINE - GLYPH) / 2, "--code-type"));
  // From the right: the markers, the kind chip, and the type; the name takes
  // what is left (`.calm-group__head`).
  const marks = markers(node, right, top, NAME_LINE, d.measure);
  let end = right - (marks.width > 0 ? marks.width + GAP : 0);
  const kind = node.group ?? "composed-of";
  const kindWidth = d.measure(kind, SMALL) + 12;
  const kindX = end - kindWidth;
  parts.push(
    `<rect x="${n(kindX)}" y="${n(top + (NAME_LINE - 16) / 2)}" width="${n(kindWidth)}" height="16" rx="3" fill="${ink("--bg-hover")}"/>`,
  );
  parts.push(text(kind, kindX + 6, top, NAME_LINE, SMALL, "--fg-3"));
  end = kindX - GAP;
  const typeWidth = d.measure(node.nodeType, MONO);
  parts.push(text(node.nodeType, end, top, NAME_LINE, MONO, "--fg-3", "end"));
  end -= typeWidth + GAP;
  const nameX = left + GLYPH + GAP;
  parts.push(text(fitText(node.name, Math.max(0, end - nameX), NAME, d.measure), nameX, top, NAME_LINE, NAME, "--fg-1"));
  parts.push(marks.svg);
  if (node.alsoIn.length > 0) {
    const also = `Also in ${node.alsoIn.join(", ")}`;
    parts.push(text(fitText(also, right - left, SMALL_ITALIC, d.measure), left, top + NAME_LINE, ROW, SMALL_ITALIC, "--fg-3"));
  }
  d.body.push(parts.join(""));
}

const POSITIONS: Record<Side, Position> = {
  left: Position.Left,
  right: Position.Right,
  top: Position.Top,
  bottom: Position.Bottom,
};

/** CLM-FR-ERXV: the label of an edge, in its layout box or centred at a point. */
function drawLabel(d: Drawing, edge: CalmEdgeView, box: Box | null, center: { x: number; y: number }): string {
  const protocolWidth = edge.protocol !== undefined ? d.measure(edge.protocol, MONO) + 8 + 2 : 0;
  const labelWidth = edge.label !== undefined ? d.measure(edge.label, SMALL) : 0;
  const parts = (edge.label !== undefined ? 1 : 0) + (edge.protocol !== undefined ? 1 : 0);
  const natural = labelWidth + protocolWidth + (parts === 2 ? 4 : 0) + 12;
  const width = Math.min(LABEL_MAX, box ? box.width : natural);
  const height = box ? box.height : 18;
  const x = box ? box.x : center.x - width / 2;
  const y = box ? box.y : center.y - height / 2;
  const inner = width - 12;
  const textWidth = Math.max(0, Math.min(labelWidth, inner - protocolWidth - (parts === 2 ? 4 : 0)));
  const shown = edge.label !== undefined ? fitText(edge.label, textWidth + 0.5, SMALL, d.measure) : "";
  const content = (shown ? d.measure(shown, SMALL) : 0) + protocolWidth + (parts === 2 ? 4 : 0);
  let cursor = x + 6 + Math.max(0, (inner - content) / 2);
  const out: string[] = [
    `<rect x="${n(x)}" y="${n(y)}" width="${n(width)}" height="${n(height)}" rx="3" fill="${ink("--bg-canvas")}"/>`,
  ];
  if (shown) {
    out.push(text(shown, cursor, y, height, SMALL, "--fg-2"));
    cursor += d.measure(shown, SMALL) + 4;
  }
  if (edge.protocol !== undefined) {
    const chipTop = y + (height - 16) / 2;
    out.push(
      `<rect x="${n(cursor + 0.5)}" y="${n(chipTop + 0.5)}" width="${n(protocolWidth - 1)}" height="15" rx="3" fill="${ink(
        "--bg-sunken",
      )}" stroke="${ink("--border-2")}" stroke-width="1"/>`,
    );
    out.push(text(edge.protocol, cursor + 5, chipTop + 1, 14, MONO, "--fg-2"));
  }
  return out.join("");
}

/** CLM-FR-ERXV, CLM-FR-ESCS: one edge; its line, and then its labels. */
function drawEdge(
  d: Drawing,
  layout: CalmLayout,
  edge: CalmEdgeView,
  boxes: Record<string, Box>,
  labels: string[],
): void {
  const empty = { x: 0, y: 0, width: 0, height: 0 };
  const from = boxes[edge.source] ?? empty;
  const to = boxes[edge.target] ?? empty;
  const route = Object.prototype.hasOwnProperty.call(layout.routes, edge.id) ? layout.routes[edge.id] : undefined;
  let path: string;
  let start: { x: number; y: number; side: Side };
  let end: { x: number; y: number; side: Side };
  let center: { x: number; y: number };
  if (route && route.points.length >= 2) {
    const first = route.points[0];
    const last = route.points[route.points.length - 1];
    path = routePath(route.points);
    start = { ...first, side: sideOf(from, first) };
    end = { ...last, side: sideOf(to, last) };
    center = { x: (first.x + last.x) / 2, y: (first.y + last.y) / 2 };
  } else {
    start = exitPoint(from, to);
    end = exitPoint(to, from);
    const [bezier, labelX, labelY] = getBezierPath({
      sourceX: start.x,
      sourceY: start.y,
      sourcePosition: POSITIONS[start.side],
      targetX: end.x,
      targetY: end.y,
      targetPosition: POSITIONS[end.side],
    });
    path = bezier;
    center = { x: labelX, y: labelY };
  }
  const dash = edge.kind === "interacts" ? ` stroke-dasharray="5 4"` : "";
  d.body.push(
    `<path d="${path}" fill="none" stroke="${ink("--fg-3")}" stroke-width="1.4"${dash} marker-end="url(#calm-arrow)"/>`,
  );
  if (edge.label !== undefined || edge.protocol !== undefined) {
    labels.push(drawLabel(d, edge, route?.label ?? null, center));
  }
  for (const [names, point] of [
    [edge.sourceInterfaces, start],
    [edge.targetInterfaces, end],
  ] as const) {
    if (names.length === 0) continue;
    const at = besideEnd(point);
    labels.push(text(names.join(", "), at.x, at.y, 14, MONO, "--fg-3", at.align));
  }
}

/** The shared definitions: the arrow of React Flow and the card shadow. */
const DEFS = [
  `<marker id="calm-arrow" viewBox="-10 -10 20 20" markerWidth="16" markerHeight="16" markerUnits="strokeWidth" orient="auto-start-reverse" refX="0" refY="0"><polyline points="-5,-4 0,0 -5,4 -5,-4" fill="${ink(
    "--fg-3",
  )}" stroke="${ink("--fg-3")}" stroke-width="1" stroke-linecap="round" stroke-linejoin="round"/></marker>`,
  `<filter id="calm-shadow" x="-5%" y="-5%" width="110%" height="120%"><feDropShadow dx="0" dy="1" stdDeviation="0" flood-color="#000000" flood-opacity="0.05"/></filter>`,
];

/**
 * CAD-FR-QHEJ: the print drawing of `layout`, or `null` for a graph with no
 * nodes (CAD-FR-VBDT). `measure` gives the width of a text.
 */
export function calmPrintSvg(layout: CalmLayout, measure: Measure = measureText): string | null {
  if (layout.graph.nodes.length === 0) return null;
  const boxes = absoluteBoxes(layout);
  const d: Drawing = { defs: [...DEFS], body: [], measure };
  // As on the canvas: the groups under the nodes, the edges over the nodes,
  // and the labels over everything.
  layout.graph.nodes.forEach((node) => {
    if (node.group) drawGroup(d, node, boxes[node.id]);
  });
  layout.graph.nodes.forEach((node, index) => {
    if (!node.group) drawNode(d, node, boxes[node.id], index);
  });
  const labels: string[] = [];
  for (const edge of layout.graph.edges) drawEdge(d, layout, edge, boxes, labels);
  const width = Math.max(1, Math.ceil(layout.width));
  const height = Math.max(1, Math.ceil(layout.height));
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`,
    `<defs>${d.defs.join("")}</defs>`,
    `<rect width="${width}" height="${height}" fill="${ink("--bg-canvas")}"/>`,
    ...d.body,
    ...labels,
    "</svg>",
  ].join("\n");
}
