/**
 * The boot of a window that "open project window" created
 * (`../specifications/core/PWN-project-windows.md` PWN-FR-KQZB).
 *
 * Such a window is a project window from its start. The backend marks it with
 * a query on its page, so the window knows it before it renders anything, and
 * shows a loading state instead of the Project picker while it mounts its
 * project.
 */

export interface ProjectWindowBoot {
  /** The name of the project that the window opens. */
  name: string;
}

/** Read the boot of this window from its query, or `null` for another window. */
export function readProjectWindowBoot(search: string): ProjectWindowBoot | null {
  const params = new URLSearchParams(search.startsWith("?") ? search.slice(1) : search);
  if (params.get("projectWindow") !== "1") return null;
  return { name: params.get("name") ?? "" };
}
