/**
 * The isolation graduation agent launches run under.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type { IsolationSettings, IsolationState } from "../types";

// ---------------------------------------------------------------------------
// Isolation (GSS-global-settings-storage.md GSS-FR-YDKS..GSS-FR-NCAQ)
// ---------------------------------------------------------------------------

/**
 * GSS-FR-DPWS: the stored record with the facts of this machine. Boots no VM
 * and reaches no network, so the Isolation section renders offline and
 * instantly.
 */
export const loadIsolationSettings = () =>
  invoke<IsolationState>("load_isolation_settings");

/**
 * GSS-FR-OAKR: validate and persist the record. Rejects with
 * `isolation_unsupported`, `cpus_out_of_range`, `memory_out_of_range`,
 * `allowed_host_invalid: <entry>`, `allowed_host_duplicate: <entry>`, or
 * `allowed_hosts_too_many`, and on every one of them nothing is written.
 */
export const saveIsolationSettings = (settings: IsolationSettings) =>
  invoke<IsolationState>("save_isolation_settings", { settings });
