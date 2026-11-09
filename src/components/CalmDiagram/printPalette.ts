/**
 * CAD-FR-FONB: the colours and fonts of the print drawing of a CALM diagram.
 *
 * The print drawing always uses the light theme, because it goes on a white
 * page. The backend rasteriser has no CSS variables, so each token of the
 * canvas is here as its resolved light value. A test checks each value
 * against `src/styles/colors_and_type.css`, so the two cannot drift apart.
 */

/** The light value of each theme token that the canvas uses. */
export const PRINT_TOKENS = {
  "--bg-canvas": "#FFFFFF",
  "--bg-sunken": "#EFEBE3",
  "--bg-hover": "rgba(0,0,0,0.04)",
  "--fg-1": "#1A1816",
  "--fg-2": "#44403B",
  "--fg-3": "#6E6A63",
  "--fg-4": "#9A958B",
  "--border-1": "#E5E0D6",
  "--border-2": "#D7D1C3",
  "--border-3": "#BAB3A2",
  "--code-keyword": "#8B3FA8",
  "--code-string": "#1F7A4D",
  "--code-number": "#A8551A",
  "--code-type": "#1D6FA5",
  "--code-function": "#2F4FD0",
  "--code-meta": "#7D5700",
  "--ok": "#1F8F6B",
  "--warn": "#B07A00",
} as const;

export type PrintToken = keyof typeof PRINT_TOKENS;

/** The colour of a token in the print drawing. */
export const ink = (token: PrintToken): string => PRINT_TOKENS[token];

/**
 * CAD-FR-FONB, DXP-FR-DMUH: the faces of the print drawing. They are the faces
 * that the backend bundles; Arial and Courier New have the same widths, so a
 * measure in the window gives the widths of the raster.
 */
export const PRINT_SANS = "Liberation Sans, Arial, Helvetica, sans-serif";
export const PRINT_MONO = "Liberation Mono, Courier New, monospace";
