/**
 * An artifact's indentation convention (EDT-FR-37–EDT-FR-39).
 *
 * Entirely client-side: it is detected from the Markdown body when the artifact
 * loads, it decides what a Tab keypress inserts in the raw-text surface, and it
 * is reported (and overridable) by the status bar's indentation control
 * (STB-FR-21 / STB-FR-23). It is not a property of the file on disk — detection
 * reads the content and changes nothing about it, and selecting a different
 * convention rewrites no existing line.
 */

/** Tabs, or spaces with a width (EDT-FR-37). */
export type Indentation =
  | { kind: "tabs" }
  | { kind: "spaces"; width: number };

/**
 * EDT-FR-37: what an artifact with no indented line anywhere in its body is
 * described as.
 */
export const DEFAULT_INDENTATION: Indentation = { kind: "spaces", width: 2 };

/** The widths the status-bar control offers. */
export const SPACE_WIDTHS = [2, 4, 8] as const;

/** The characters a single Tab keypress inserts under this convention. */
export function indentUnit(indentation: Indentation): string {
  return indentation.kind === "tabs" ? "\t" : " ".repeat(indentation.width);
}

/** The label the status bar renders (STB-FR-21). */
export function indentationLabel(indentation: Indentation): string {
  return indentation.kind === "tabs"
    ? "Tabs"
    : `Spaces: ${indentation.width}`;
}

export function sameIndentation(a: Indentation, b: Indentation): boolean {
  if (a.kind !== b.kind) return false;
  return a.kind === "tabs" || a.width === (b as { width: number }).width;
}

/**
 * EDT-FR-37: detect an artifact's convention from its Markdown body.
 *
 * Tabs win when the body indents with them at least as often as with spaces:
 * a file that is mostly tab-indented but carries a couple of space-aligned
 * continuation lines is a tab file, and inserting spaces into it would be the
 * wrong answer.
 *
 * The space width is the *smallest* positive leading-space count, because that
 * is the unit every deeper level is a multiple of — a Markdown list indented
 * 2/4/6 has a unit of 2, not an "average" of 4. Blank and whitespace-only lines
 * carry no indentation information and are skipped; a body with no indented
 * line at all is `DEFAULT_INDENTATION`.
 */
export function detectIndentation(body: string): Indentation {
  let tabLines = 0;
  let spaceLines = 0;
  let smallestWidth = Number.POSITIVE_INFINITY;

  for (const line of body.split("\n")) {
    // A whitespace-only line is trailing padding, not an indent level.
    if (line.trim() === "") continue;
    if (line.startsWith("\t")) {
      tabLines += 1;
      continue;
    }
    const leading = line.length - line.trimStart().length;
    if (leading > 0) {
      spaceLines += 1;
      if (leading < smallestWidth) smallestWidth = leading;
    }
  }

  if (tabLines > 0 && tabLines >= spaceLines) return { kind: "tabs" };
  if (spaceLines === 0) return DEFAULT_INDENTATION;
  // Clamp so a pathological file (a single 40-space-indented line) cannot make
  // Tab insert an absurd run of spaces.
  const width = Math.min(Math.max(smallestWidth, 1), 8);
  return { kind: "spaces", width };
}
