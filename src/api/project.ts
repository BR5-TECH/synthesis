/**
 * Project lifecycle, user-global settings, and the windows the shell opens.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  SettingsWindowContext,
  SettingsWindowKind,
} from "../settingsWindow";
import type {
  AppPreferences,
  BrowseResult,
  CreateMode,
  FontFamily,
  InstalledAdapter,
  InstalledPlugin,
  LayoutPreferences,
  ProjectHandle,
  RecentProject,
} from "../types";

// --- Project lifecycle (project.rs) ---------------------------------------

export const openProjectAtPath = (path: string) =>
  invoke<ProjectHandle>("open_project_at_path", { path });

export const openProjectFromGitUrl = (url: string) =>
  invoke<ProjectHandle>("open_project_from_git_url", { url });

// SNV-FR-25 / PST-FR-14 / ASC-FR-14: drop the open project's in-memory state and
// stop its watcher. Invoked by the frontend *after* pending Editor changes have
// been written (EDT-FR-33), since a torn-down project can no longer be saved to.
export const closeProject = () => invoke<void>("close_project");

// SNV-FR-26: answer the quit the backend is holding — `true` once pending Editor
// changes have been written (EDT-FR-33), `false` to cancel the quit because a
// write was blocked (EDT-FR-32).
export const finishExit = (proceed: boolean) =>
  invoke<void>("finish_exit", { proceed });

// SNV-FR-28 / SNV-FR-30: set whether File → Save and File → Save All render
// enabled. The frontend owns the answer (it holds the buffers and knows the
// active tab); greying the native item is also what makes its accelerator inert.
export const setSaveMenuState = (save: boolean, saveAll: boolean) =>
  invoke<void>("set_save_menu_state", { save, saveAll });

/**
 * SNV-FR-43: whether Edit -> Find and Edit -> Find & Replace are enabled. Both
 * answer to one condition — an Editor is the active surface — so one flag
 * carries both. Greying the items is what makes ⌘F and ⌘R inert elsewhere.
 */
export const setFindMenuState = (enabled: boolean) =>
  invoke<void>("set_find_menu_state", { enabled });

export const createProject = (
  name: string,
  mode: CreateMode,
  targetPath: string,
) => invoke<ProjectHandle>("create_project", { name, mode, targetPath });

// --- User-global settings (settings.rs) -----------------------------------

export const listRecentProjects = () =>
  invoke<RecentProject[]>("list_recent_projects");

export const removeRecentProject = (path: string) =>
  invoke<void>("remove_recent_project", { path });

export const clearRecentProjects = () =>
  invoke<void>("clear_recent_projects");

export const pinRecentProject = (path: string) =>
  invoke<void>("pin_recent_project", { path });

export const unpinRecentProject = (path: string) =>
  invoke<void>("unpin_recent_project", { path });

export const loadAppPreferences = () =>
  invoke<AppPreferences>("load_app_preferences");

export const saveAppPreferences = (preferences: AppPreferences) =>
  invoke<void>("save_app_preferences", { preferences });

/**
 * The font families this machine has installed
 * (`FNT-font-enumeration.md` FNT-FR-01).
 *
 * FNT-FR-07: the command never fails — a platform whose font set cannot be
 * enumerated yields an empty list, which the Appearance section reads as "no
 * family beyond the built-in faces is offerable" rather than as an error to
 * surface. The result is already deduplicated by family and ordered
 * case-insensitively (FNT-FR-02 / FNT-FR-03), so a consumer renders it as
 * returned.
 */
export const listSystemFonts = () =>
  invoke<FontFamily[]>("list_system_fonts");

export const listInstalledPlugins = () =>
  invoke<InstalledPlugin[]>("list_installed_plugins");

export const installPlugin = (source: string) =>
  invoke<InstalledPlugin>("install_plugin", { source });

export const uninstallPlugin = (id: string) =>
  invoke<void>("uninstall_plugin", { id });

export const listAgentAdapters = () =>
  invoke<InstalledAdapter[]>("list_agent_adapters");

export const installAdapter = (source: string) =>
  invoke<InstalledAdapter>("install_adapter", { source });

// --- Layout preferences (layout.rs) ---------------------------------------

export const loadLayoutPreferences = () =>
  invoke<LayoutPreferences | null>("load_layout_preferences");

export const saveLayoutPreferences = (preferences: LayoutPreferences) =>
  invoke<void>("save_layout_preferences", { preferences });

// --- Window / dialog (window.rs, dialog.rs) -------------------------------

export const centerPicker = () => invoke<void>("center_picker");

// --- Settings child windows (settings_window.rs / SWN-settings-windows.md) --

/**
 * SWN-FR-13: open a settings window, focus it when it is the one already open
 * (SWN-FR-06), or switch to it when the other one is (SWN-FR-05, SWN-FR-07).
 *
 * `section` names a section within the window — the Agents section the chrome
 * roster routes to, the GitHub section a failed Git operation offers, the
 * section a notification's address carries. Omit it to open the window on the
 * section it presents by default.
 */
export const openSettingsWindow = (
  kind: SettingsWindowKind,
  section: string | null = null,
) => invoke<void>("open_settings_window", { kind, section });

/**
 * SWN-FR-08 through SWN-FR-12: a settings window's answer to the save sweep it
 * was asked to run before closing.
 *
 * `ok` is true once every pending change in every section has been written;
 * `section` names the one that failed, so the window can present it (SWN-FR-11).
 */
export const finishSettingsClose = (ok: boolean, section: string | null = null) =>
  invoke<void>("finish_settings_close", { ok, section });

/**
 * The open project behind a settings window (SWN-FR-02).
 *
 * A settings window is a webview of its own and shares no state with the main
 * window, so this is how it learns which project it is configuring. Every field
 * is fixed for the window's life — the parent is blocked, so neither the project
 * nor its active worktree can change while one is open (SET-FR-20) — and every
 * field is empty while no project is open, which is a state Global settings is
 * reachable in (SWN-FR-15).
 */
export const getSettingsWindowContext = () =>
  invoke<SettingsWindowContext>("get_settings_window_context");

export const browseForFolder = () =>
  invoke<BrowseResult>("browse_for_folder");

/**
 * The file counterpart of `browseForFolder`, on the same contract: a
 * cancellation is the `"cancelled"` sentinel rather than a rejection. Used by
 * the Agentic AI section to point at a CLI binary detection could not find
 * (AII-FR-05).
 */
export const browseForFile = () => invoke<BrowseResult>("browse_for_file");
