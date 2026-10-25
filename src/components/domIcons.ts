/**
 * Icon glyphs as path data, so a control drawn outside React (an editor
 * widget) shows the same glyph as the `Icon` set in `icons.tsx`.
 */

const SVG_NAMESPACE = "http://www.w3.org/2000/svg";

/** EDT-FR-17, EDT-FR-UTHM: the `</>` code glyph. */
export const CODE_GLYPH = "m8 8-4 4 4 4M16 8l4 4-4 4M13.5 6l-3 12";

/** EDT-FR-UTHM: a small flowchart — two boxes that a connector joins. */
export const DIAGRAM_GLYPH =
  "M5 3h4a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z" +
  "M15 13h4a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2h-4a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2z" +
  "M7 11v4a2 2 0 0 0 2 2h4";

/**
 * An icon as a DOM element, with the attributes of the `I` wrapper in
 * `icons.tsx`. It is decoration: the control that holds it has the name.
 */
export function iconElement(glyph: string, size = 14): SVGSVGElement {
  const svg = document.createElementNS(SVG_NAMESPACE, "svg");
  svg.setAttribute("class", "icon");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.6");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  const path = document.createElementNS(SVG_NAMESPACE, "path");
  path.setAttribute("d", glyph);
  svg.appendChild(path);
  return svg;
}
