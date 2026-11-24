/**
 * The project window list and the commands that change it
 * (`../specifications/core/PWN-project-windows.md`).
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "./invoke";
import type {
  CloseProjectWindowResult,
  OpenProjectWindowResult,
  ProjectHandle,
  ProjectWindow,
  ProjectWindowsSnapshot,
  CreateMode,
} from "../types";

// PWN-FR-TJEN.
export const listProjectWindows = () =>
  invoke<ProjectWindowsSnapshot>("list_project_windows");

// The entry of the window that makes the call, or null while it holds no project.
export const currentProjectWindow = () =>
  invoke<ProjectWindow | null>("current_project_window");

// PWN-FR-AYTY: the calling window holds this project now.
export const attachProjectToWindow = (projectId: string) =>
  invoke<void>("attach_project_to_window", { projectId });

// The calling window holds no project and shows the Project picker.
export const detachProjectFromWindow = () =>
  invoke<void>("detach_project_from_window");

// PWN-FR-NAQR / PWN-FR-RORD.
export const openProjectWindow = (path: string) =>
  invoke<OpenProjectWindowResult>("open_project_window", { path });

// PWN-FR-XUJP. CEX-FR-QZPN: in one order with the attention flag.
export const activateProjectWindow = (projectId: string) =>
  invoke<void>("activate_project_window", { projectId });

// PWN-FR-BYCL.
export const requestProjectWindowClose = (projectId: string) =>
  invoke<void>("request_project_window_close", { projectId });

// PWN-FR-YRXF / PWN-FR-QEGI.
export const closeProjectWindow = (projectId: string) =>
  invoke<CloseProjectWindowResult>("close_project_window", { projectId });

// PWN-FR-CFSW.
export const setProjectWindowAttention = (projectId: string, needsAttention: boolean) =>
  invoke<void>("set_project_window_attention", { projectId, needsAttention });

// PJT-FR-IOEV: resolve a project without mounting it, so the choice of This tab
// or New tab comes first.
export const resolveProjectAtPath = (path: string) =>
  invoke<ProjectHandle>("resolve_project_at_path", { path });

export const resolveProjectFromGitUrl = (url: string) =>
  invoke<ProjectHandle>("resolve_project_from_git_url", { url });

export const resolveNewProject = (name: string, mode: CreateMode, targetPath: string) =>
  invoke<ProjectHandle>("resolve_new_project", { name, mode, targetPath });
