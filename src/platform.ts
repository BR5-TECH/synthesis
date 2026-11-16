/**
 * The platform of this window, on the document root as `data-platform`.
 *
 * On macOS the system draws the titlebar and the project tab bar above the top
 * chrome (`../specifications/ui/PJT-project-tabs.md` PJT-FR-PBQT). The styles of
 * the top chrome read this attribute to meet that bar.
 */
export type Platform = "macos" | "other";

/** The platform that a `navigator.platform` value names. */
export function platformOf(navigatorPlatform: string): Platform {
  return /mac/i.test(navigatorPlatform) ? "macos" : "other";
}

/** Put the platform on the document root. */
export function markPlatform(root: HTMLElement, navigatorPlatform: string): void {
  root.dataset.platform = platformOf(navigatorPlatform);
}
