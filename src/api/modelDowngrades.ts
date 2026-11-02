/**
 * The model downgrade suppressions (`GSS-global-settings-storage.md`
 * GSS-FR-PCPW, GSS-FR-UUET), read and written for `NTF-notifications.md`
 * NTF-FR-CYTU and NTF-FR-LHVU.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type { ModelDowngradeSuppression, SuppressionDuration } from "../types";

/** GSS-FR-PCPW: the rules of the open project that are in force. */
export const loadModelDowngradeSuppressions = () =>
  invoke<ModelDowngradeSuppression[]>("load_model_downgrade_suppressions");

/** GSS-FR-UUET: save a rule for one pair and one duration. */
export const saveModelDowngradeSuppression = (
  requestedModel: string,
  servedModel: string,
  duration: SuppressionDuration,
) =>
  invoke<ModelDowngradeSuppression>("save_model_downgrade_suppression", {
    requestedModel,
    servedModel,
    duration,
  });
