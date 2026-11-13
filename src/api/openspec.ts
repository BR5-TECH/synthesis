// OpenSpec mode (OSM-openspec-mode.md)

import { invoke } from "@tauri-apps/api/core";

import type { ChangeDetail, ChangeLocation, OpenSpecStatus } from "../types";

/**
 * OSM-FR-YXUC: the setting, the detection, the mode, and the CLI state. With
 * `recheck`, the backend first clears a kept `failed` CLI check
 * (OFP-FR-ITUD). Without it, the call names no argument.
 */
export const getOpenspecStatus = (recheck = false) =>
  recheck
    ? invoke<OpenSpecStatus>("get_openspec_status", { recheck: true })
    : invoke<OpenSpecStatus>("get_openspec_status");

/**
 * OSM-FR-MBTI: read one change in one location. It rejects with an
 * `OpenSpecFailure` (OFP-FR-QLQJ).
 */
export const readOpenspecChange = (location: ChangeLocation, name: string) =>
  invoke<ChangeDetail>("read_openspec_change", { location, name });

/** OSM-FR-BLEY: one image of a change folder in its stream's working copy. */
export const readChangeImage = (streamId: string, change: string, path: string) =>
  invoke<{ data: string; mediaType: string }>("read_change_image", { streamId, change, path });
