/**
 * Agent surfaces (`AUI-agent-ui-surfaces.md`): the record of a panel an agent
 * showed, and the author's one action on it.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "./invoke";
import type { AgentSurface, SurfaceAction, SurfaceMedia, SurfaceSubmitResult } from "../types";

/** AUI-FR-QBXZ: one agent surface. */
export const getAgentSurface = (surfaceId: string) =>
  invoke<AgentSurface>("get_agent_surface", { surfaceId });

/** AUI-FR-KBVY: the author's one action on an open surface. */
export const submitSurfaceAction = (surfaceId: string, action: SurfaceAction) =>
  invoke<SurfaceSubmitResult>("submit_surface_action", { surfaceId, action });

/** AUI-FR-JUDL: a `project:` or `attachment:` source that the surface names. */
export const readSurfaceMedia = (surfaceId: string, source: string) =>
  invoke<SurfaceMedia>("read_surface_media", { surfaceId, source });
