/**
 * PWN-FR-EKKX / PWN-FR-NEYF: one open project window, as the list
 * `"project windows changed"` publishes it.
 */
export interface ProjectWindow {
  /** PWN-FR-MVWL: the project's anchor path. */
  projectId: string;
  /** The full project name. */
  name: string;
  windowLabel: string;
  needsAttention: boolean;
}

/** PWN-FR-NEYF: the whole list, as `"project windows changed"` publishes it. */
export interface ProjectWindowsSnapshot {
  windows: ProjectWindow[];
  activeProjectId: string | null;
}

/** The result of `"open project window"`. */
export interface OpenProjectWindowResult {
  projectId: string;
  windowLabel: string;
  created: boolean;
}

/** The result of `"close project window"`. */
export interface CloseProjectWindowResult {
  /** The window holds no project now and shows the Project picker. */
  lastClosed: boolean;
}
