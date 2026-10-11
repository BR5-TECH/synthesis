/**
 * Which window this webview is (`specifications/ui/SWN-settings-windows.md`).
 *
 * The Settings window is a native child window rather than a tab of the main
 * viewport (SWN-FR-01), and it is a webview of its own. Both windows boot the
 * same HTML entry point, so what a webview renders is decided by the query
 * string the backend built it with (`settings_window.rs`) — the main window
 * carries none, the Settings window carries a marker and, optionally, the
 * section address it was asked to open on (SWN-FR-HZWD).
 *
 * Pure and dependency-free so the routing that decides between two whole
 * application roots is a unit test rather than something only a running desktop
 * app can exercise.
 */

/** The two scopes of the window's side nav (SWN-FR-QXTV). */
export type SettingsScope = "global" | "project";

/**
 * The sections of the Global group, in the order the side nav lists them
 * (`GLS-global-settings.md` GLS-FR-03). A section is named by its key in every
 * route to it (SWN-FR-HZWD).
 */
export const GLOBAL_SECTION_KEYS = [
  "appearance",
  "navigation",
  "notifications",
  "recents",
  "plugins",
  "adapters",
  "github",
  "ai-api",
  "ai-agentic",
  "agents",
  "docker",
  "remote",
] as const;
export type GlobalSectionKey = (typeof GLOBAL_SECTION_KEYS)[number];

/** The sections of the Project group (`SET-project-settings.md` SET-FR-03). */
export const PROJECT_SECTION_KEYS = [
  "project",
  "template",
  "bindings",
  "github-polling",
  "adapters",
  "mcp",
  "plugins",
  "agents",
  "docker",
  "graduation",
] as const;
export type ProjectSectionKey = (typeof PROJECT_SECTION_KEYS)[number];

/** Whether `section` is a section the `scope` group has. */
export function isSectionOf(scope: SettingsScope, section: string): boolean {
  return (
    scope === "global" ? GLOBAL_SECTION_KEYS : PROJECT_SECTION_KEYS
  ).includes(section as never);
}

/** SWN-FR-MRNB: the first section of a group, which is where a route that names a scope and no section lands. */
export function firstSectionOf(scope: SettingsScope): string {
  return scope === "global" ? GLOBAL_SECTION_KEYS[0] : PROJECT_SECTION_KEYS[0];
}

/** The window's own title, matching `settings_window.rs` (SWN-FR-17). */
export const SETTINGS_WINDOW_TITLE = "Settings";

/** What a Settings webview boots knowing about itself. */
export interface SettingsWindowBoot {
  /** SWN-FR-HZWD: the section address the request named, or null for none. */
  section: string | null;
}

/**
 * Read the boot parameters out of a location search string, or `null` when this
 * is not the Settings window at all.
 *
 * Anything but the marker the backend writes yields `null` rather than a guess:
 * rendering the main shell inside a window sized and parented as the Settings
 * window would be far stranger than rendering nothing recognisable.
 */
export function readSettingsWindowBoot(
  search: string,
): SettingsWindowBoot | null {
  const params = new URLSearchParams(
    search.startsWith("?") ? search.slice(1) : search,
  );
  if (params.get("settings") !== "1") return null;
  const section = params.get("section");
  return { section: section && section.length > 0 ? section : null };
}

/**
 * What the Settings window knows about the project behind it, as
 * `get_settings_window_context` answers (SWN-FR-02).
 *
 * Every field is the empty string while no project is open — a state the window
 * is reachable in (SWN-FR-15), and the one in which its Project group is absent
 * (SWN-FR-16) — so a consumer tests for emptiness rather than for a null.
 */
export interface SettingsWindowContext {
  projectName: string;
  /** The project's identity anchor, which is the key an address names. */
  projectKey: string;
  /** The active worktree's directory. */
  contentRoot: string;
}

/**
 * A **section address**: which section of the Settings window to present, in
 * which group, and optionally what within it (SWN-FR-HZWD).
 *
 * One string rather than three parameters, because it travels as one — through
 * the backend's `open_settings_window`, into the window's boot URL, and back
 * out on the routing event — and every route that names a section names at most
 * one thing inside it. `global/agents` is the section alone; `global/agents:new`
 * is the section with a fresh persona editor open (`AGT-agents.md` AGT-FR-07);
 * and `global/agents:<id>` is the section with that agent's editor open
 * (AGT-FR-06). `project/graduation` names a Project section, so a section that
 * exists in both groups is never ambiguous.
 *
 * The scope is what stands before the first slash and the item is what stands
 * after the first colon, so an item carrying either of its own survives the
 * round trip intact.
 */
export const NEW_AGENT_ITEM = "new";

export interface SectionAddress {
  scope: SettingsScope;
  section: string;
  /** What within the section, or null when the address names the section alone. */
  item: string | null;
}

export function formatSectionAddress(
  scope: SettingsScope,
  section: string,
  item: string | null = null,
): string {
  return item === null ? `${scope}/${section}` : `${scope}/${section}:${item}`;
}

/** Read an address, or `null` when it names no known scope (SWN-FR-HZWD). */
export function parseSectionAddress(address: string): SectionAddress | null {
  const slash = address.indexOf("/");
  if (slash < 0) return null;
  const scope = address.slice(0, slash);
  if (scope !== "global" && scope !== "project") return null;
  const rest = address.slice(slash + 1);
  const colon = rest.indexOf(":");
  if (colon < 0) return rest ? { scope, section: rest, item: null } : null;
  const item = rest.slice(colon + 1);
  const section = rest.slice(0, colon);
  return section
    ? { scope, section, item: item.length > 0 ? item : null }
    : null;
}
