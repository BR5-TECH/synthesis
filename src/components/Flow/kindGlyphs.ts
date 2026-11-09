/**
 * The glyph of each plan item kind (FLO layout notes). The glyph, not a colour,
 * says what an item is, so it reads the same in a monochrome screenshot.
 */
import type { FlowItemKind } from "../../state/flowDocument/types";

export const KIND_GLYPHS: Readonly<Record<FlowItemKind, string>> = {
  deliverable: "◆",
  milestone: "⚑",
  epic: "◈",
  feature: "▣",
  task: "☐",
  defect: "✱",
};

/** The glyph of a kind sent as plain text, or a neutral mark for an unknown one. */
export function kindGlyph(kind: string): string {
  return (KIND_GLYPHS as Record<string, string>)[kind] ?? "•";
}

/** The kind in words, for accessible names. */
export function kindLabel(kind: string): string {
  return kind.length > 0 ? kind[0].toUpperCase() + kind.slice(1) : kind;
}
