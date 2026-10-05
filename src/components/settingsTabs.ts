/**
 * The tab strip a settings section uses where it holds more than one subject.
 *
 * Three sections have one: the AI API providers and the agentic CLI vendors of
 * Global settings (`../../specifications/ui/AII-ai-integrations.md` AII-FR-03),
 * and the vendor tabs of the Project settings Docker section
 * (`../../specifications/ui/SET-project-settings.md` SET-FR-21). They are the
 * same control, so they are one definition: a row of ghost buttons standing on
 * a hairline, the selected one lit by an accent underline that covers the
 * hairline under it.
 *
 * Kept here rather than in one of the surfaces because a second surface that
 * imports the first only to borrow a style couples two sections that share
 * nothing else.
 */
import type { CSSProperties } from "react";

/** The row the tabs stand in. Spread it to change only the spacing below. */
export const SETTINGS_TABLIST_STYLE: Readonly<CSSProperties> = Object.freeze({
  display: "flex",
  gap: 2,
  flexWrap: "wrap",
  borderBottom: "1px solid var(--border-1)",
  marginBottom: 18,
} as CSSProperties);

/**
 * One tab. Pair it with `className="btn btn--ghost btn--sm"`, which is where
 * the height, the padding, and the hover come from.
 */
export function settingsTabStyle(selected: boolean): CSSProperties {
  return {
    borderRadius: "4px 4px 0 0",
    // Pulls the tab down onto the strip's hairline so a selected underline
    // replaces it rather than stacking two lines.
    marginBottom: -1,
    borderBottom: selected ? "2px solid var(--accent)" : "2px solid transparent",
    color: selected ? "var(--fg-1)" : undefined,
  };
}
