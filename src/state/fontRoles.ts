/**
 * The three typographic roles of `OVW-overview.md` OVW-FR-13, and the one place
 * that turns a stored `FontSettings` record into the custom properties the
 * stylesheet reads.
 *
 * Every piece of text the application renders belongs to exactly one role, and
 * `ui` is what text belongs to unless one of the other two names it — so a
 * surface added later needs no typographic decision to render correctly
 * (OVW-FR-13). The stylesheet expresses that: `html, body` is the UI role,
 * `.doc` is the Rich Markdown role, and the source surfaces name the Source
 * code role. Nothing here knows about any individual surface.
 *
 * The functions are pure so the bounding (GLS-FR-19), the fallback composition
 * (GLS-FR-22), and the unset-means-built-in rule are testable without a DOM.
 */
import type { FontRole, FontRoleKey, FontSettings } from "../types";

/**
 * The application's built-in default for one role — the value a role renders in
 * when the user has chosen nothing (OVW-FR-14).
 *
 * `fallback` is the face stack `colors_and_type.css` declares for the role. It
 * is duplicated here for one reason: a chosen family is applied *in front of*
 * it rather than instead of it, so a family uninstalled since it was chosen
 * falls back to the built-in face on its own while the stored selection stays
 * exactly as stored (GLS-FR-22). Resolving that in CSS rather than by checking
 * the family against `list_system_fonts` is what makes reinstalling the font
 * restore the role with no further action.
 */
export interface FontRoleDefaults {
  /** Human name of the built-in face, as the Appearance section labels it. */
  readonly builtInLabel: string;
  readonly fallback: string;
  readonly sizePx: number;
  readonly lineHeight: number;
}

export const FONT_ROLE_DEFAULTS: Record<FontRoleKey, FontRoleDefaults> = {
  ui: {
    builtInLabel: "Inter (built-in)",
    fallback:
      '"Inter", ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
    sizePx: 13,
    lineHeight: 1.5,
  },
  rich: {
    builtInLabel: "Inter (built-in)",
    fallback:
      '"Inter", ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
    sizePx: 15,
    lineHeight: 1.65,
  },
  source: {
    builtInLabel: "JetBrains Mono (built-in)",
    fallback:
      '"JetBrains Mono", ui-monospace, "SF Mono", Menlo, Consolas, monospace',
    sizePx: 13,
    lineHeight: 1.65,
  },
};

/** The roles in the order the Appearance section presents them (GLS-FR-17). */
export const FONT_ROLE_ORDER: readonly FontRoleKey[] = ["ui", "rich", "source"];

export const FONT_ROLE_LABELS: Record<FontRoleKey, string> = {
  ui: "UI",
  rich: "Rich Markdown",
  source: "Source code",
};

/**
 * GLS-FR-19: the range the section enforces before committing, so a value that
 * would leave the application unreadable or unusable cannot be persisted.
 *
 * The lower bounds are the real constraint — a 2px UI or a 0.5 line height is
 * not a small app, it is one whose rows overlap and whose controls cannot be
 * hit. The upper bounds keep a surface's chrome from pushing its own content
 * off screen. Storage bounds nothing (GSS-FR-29); this is the only gate.
 */
export const FONT_SIZE_MIN = 9;
export const FONT_SIZE_MAX = 32;
export const LINE_HEIGHT_MIN = 1;
export const LINE_HEIGHT_MAX = 3;

/** Whether a size may be committed (GLS-FR-19). */
export function isFontSizeInRange(value: number): boolean {
  return (
    Number.isFinite(value) && value >= FONT_SIZE_MIN && value <= FONT_SIZE_MAX
  );
}

/** Whether a line height may be committed (GLS-FR-19). */
export function isLineHeightInRange(value: number): boolean {
  return (
    Number.isFinite(value) &&
    value >= LINE_HEIGHT_MIN &&
    value <= LINE_HEIGHT_MAX
  );
}

/**
 * The `font-family` value for a role: the chosen family in front of the
 * built-in stack (GLS-FR-22), or the built-in stack alone when none is chosen.
 *
 * The family is quoted, because a CSS family name is only an unquoted
 * identifier when it happens to look like one — `Times New Roman` unquoted is
 * three identifiers, and a name containing a `"` would end the string and let
 * whatever follows be read as further declarations. Quoting through
 * `JSON.stringify` escapes both cases, so a family name is data here and never
 * syntax.
 */
export function fontFamilyValue(
  role: FontRoleKey,
  family: string | undefined,
): string {
  const fallback = FONT_ROLE_DEFAULTS[role].fallback;
  const chosen = family?.trim();
  if (!chosen) return fallback;
  return `${JSON.stringify(chosen)}, ${fallback}`;
}

/** The custom-property names one role owns on the document root. */
export function fontRoleProperties(role: FontRoleKey): {
  family: string;
  size: string;
  lineHeight: string;
} {
  return {
    family: `--font-${role}-family`,
    size: `--font-${role}-size`,
    lineHeight: `--font-${role}-line-height`,
  };
}

/**
 * OVW-FR-14: apply the three roles to `root`, so every open surface is
 * re-typeset at once with no relaunch.
 *
 * An unset field is **removed** rather than written with a default. The
 * stylesheet already declares the built-in value, so removing the override is
 * what returns the role to it — and it means this module never has to keep its
 * idea of the default in step with the stylesheet's for the app to render
 * correctly. A stored value the section would not have committed (a size
 * written by something else, a record edited by hand) is clamped rather than
 * applied, so an out-of-range number on disk cannot leave the app unusable.
 */
export function applyFontRoles(
  root: HTMLElement,
  fonts: FontSettings | undefined,
): void {
  for (const role of FONT_ROLE_ORDER) {
    const setting: FontRole = fonts?.[role] ?? {};
    const props = fontRoleProperties(role);

    if (setting.family?.trim()) {
      root.style.setProperty(props.family, fontFamilyValue(role, setting.family));
    } else {
      root.style.removeProperty(props.family);
    }

    if (typeof setting.sizePx === "number" && Number.isFinite(setting.sizePx)) {
      const size = Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, setting.sizePx));
      root.style.setProperty(props.size, `${size}px`);
    } else {
      root.style.removeProperty(props.size);
    }

    if (
      typeof setting.lineHeight === "number" &&
      Number.isFinite(setting.lineHeight)
    ) {
      const lh = Math.min(
        LINE_HEIGHT_MAX,
        Math.max(LINE_HEIGHT_MIN, setting.lineHeight),
      );
      root.style.setProperty(props.lineHeight, `${lh}`);
    } else {
      root.style.removeProperty(props.lineHeight);
    }
  }
}

/**
 * `next` merged onto `fonts` for one role, as a whole `FontSettings` record.
 *
 * Setting a field to `undefined` clears it back to the built-in default, which
 * is what a reset does (GLS-FR-23) — so the merge is per-field replacement
 * rather than a spread that would let `undefined` mean "leave alone".
 */
export function withFontRole(
  fonts: FontSettings | undefined,
  role: FontRoleKey,
  next: FontRole,
): FontSettings {
  return { ...(fonts ?? {}), [role]: next };
}
