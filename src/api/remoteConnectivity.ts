/**
 * The relay this machine reaches a remote client through.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type { RelayEndpoint } from "../types";

// ---------------------------------------------------------------------------
// Remote connectivity (GSS-global-settings-storage.md GSS-FR-ZKQT..GSS-FR-TXAO)
// ---------------------------------------------------------------------------

/**
 * GSS-FR-ZKQT: the relay endpoint this machine reaches. Reads the stored
 * record and reaches no network, so the Remote connectivity section renders
 * offline and instantly.
 */
export const loadRelayEndpoint = () =>
  invoke<RelayEndpoint>("load_relay_endpoint");

/**
 * GSS-FR-NLDC: persist the URL, and answer with the validation state that URL
 * carries.
 *
 * Changing the URL returns the record to `unverified`: the success that was
 * earned belonged to the URL that earned it.
 */
export const saveRelayEndpoint = (url: string) =>
  invoke<RelayEndpoint>("save_relay_endpoint", { url });

/**
 * GSS-FR-HPWE: read the relay's unauthenticated health answer, and commit the
 * success only where the relay answered and advertised what this application
 * needs.
 *
 * This is the one call in the Remote connectivity section that leaves the
 * machine, and it sends no credential. Rejects with `endpoint_empty`,
 * `endpoint_invalid`, `scheme_unsupported`, `unreachable`, `timed_out`,
 * `not_a_relay`, or `capability_missing`, and on every one of them the stored
 * record is exactly as it was.
 */
export const verifyRelayEndpoint = (url: string) =>
  invoke<RelayEndpoint>("verify_relay_endpoint", { url });
