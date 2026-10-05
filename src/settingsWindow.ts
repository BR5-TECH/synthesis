/**
 * Which window this webview is (`specifications/ui/SWN-settings-windows.md`).
 *
 * Global settings and Project settings are native child windows rather than
 * tabs of the main viewport (SWN-FR-01), and each is a webview of its own. All
 * three windows boot the same HTML entry point, so what a webview renders is
 * decided by the query string the backend built it with (`settings_window.rs`)
 * — the main window carries none, a settings window carries its kind and,
 * optionally, the section it was asked to open on (SWN-FR-13).
 *
 * Pure and dependency-free so the routing that decides between two whole
 * application roots is a unit test rather than something only a running desktop
 * app can exercise.
 */

/** The two settings windows, named as the backend names them. */
export type SettingsWindowKind = "global" | "project";

/** What a settings webview boots knowing about itself. */
export interface SettingsWindowBoot {
  kind: SettingsWindowKind;
  /** SWN-FR-13: the section the request named, or null for the default one. */
  section: string | null;
}

/**
 * Read the boot parameters out of a location search string, or `null` when this
 * is not a settings window at all.
 *
 * An unrecognised `settings` value yields `null` rather than a guess: rendering
 * the main shell inside a window sized and parented as a settings window would
 * be far stranger than rendering nothing recognisable, and the backend is the
 * only thing that ever writes this value.
 */
export function readSettingsWindowBoot(
  search: string,
): SettingsWindowBoot | null {
  const params = new URLSearchParams(
    search.startsWith("?") ? search.slice(1) : search,
  );
  const kind = params.get("settings");
  if (kind !== "global" && kind !== "project") return null;
  const section = params.get("section");
  return { kind, section: section && section.length > 0 ? section : null };
}

/** The window's own title, matching `settings_window.rs` (SWN-FR-17). */
export function settingsWindowTitle(kind: SettingsWindowKind): string {
  return kind === "global" ? "Global settings" : "Project settings";
}

/**
 * What a settings window knows about the project behind it, as
 * `get_settings_window_context` answers (SWN-FR-02).
 *
 * Every field is the empty string while no project is open — a state Global
 * settings is reachable in (SWN-FR-15) — so a consumer tests for emptiness
 * rather than for a null.
 */
export interface SettingsWindowContext {
  projectName: string;
  /** The project's identity anchor, which is the key an address names. */
  projectKey: string;
  /** The active worktree's directory. */
  contentRoot: string;
}

/**
 * A **section address**: which section of a settings window to present, and
 * optionally what within it (SWN-FR-13).
 *
 * One string rather than two parameters, because it travels as one — through
 * the backend's `open_settings_window`, into the window's boot URL, and back
 * out on the routing event — and every route that names a section names at most
 * one thing inside it. `agents` is the section alone; `agents:new` is the
 * section with a fresh persona editor open (`AGT-agents.md` AGT-FR-07); and
 * `agents:<id>` is the section with that agent's editor open (AGT-FR-06).
 *
 * The separator is the first colon, so an item carrying one of its own survives
 * the round trip intact.
 */
export const NEW_AGENT_ITEM = "new";

export interface SectionAddress {
  section: string;
  /** What within the section, or null when the address names the section alone. */
  item: string | null;
}

export function formatSectionAddress(
  section: string,
  item: string | null = null,
): string {
  return item === null ? section : `${section}:${item}`;
}

export function parseSectionAddress(address: string): SectionAddress {
  const at = address.indexOf(":");
  if (at < 0) return { section: address, item: null };
  const item = address.slice(at + 1);
  return { section: address.slice(0, at), item: item.length > 0 ? item : null };
}
