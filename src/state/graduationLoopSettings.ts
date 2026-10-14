/**
 * The three graduation loop settings of a project, as the frontend shows them
 * (`../../specifications/core/PSS-project-settings-storage.md` PSS-FR-QBNE,
 * PSS-FR-HVSM, PSS-FR-RKTW; `../../specifications/ui/SET-project-settings.md`
 * SET-FR-PSLD, SET-FR-RTDF).
 *
 * The bounds, the defaults, the value text and the words for a refusal live
 * here, so the Settings sliders and the Runs surface say one value in one way.
 */

import type { ProjectConfig, ProjectConfigPatch } from "../types";

/** One half hour, the step of both time limits (PSS-FR-HVSM, PSS-FR-RKTW). */
export const HALF_HOUR_MS = 1_800_000;
const MS_PER_MINUTE = 60_000;

/** The keys of the three settings in `ProjectConfig` and `ProjectConfigPatch`. */
export type GraduationLoopKey =
  | "graduationPassBudget"
  | "graduationStageTimeLimitMs"
  | "graduationTotalTimeLimitMs";

/**
 * SWN-FR-HZWD: the section address item that names each slider, for example
 * `project/graduation:total-time-limit`.
 */
export const PASS_BUDGET_ITEM = "pass-budget";
export const STAGE_TIME_LIMIT_ITEM = "stage-time-limit";
export const TOTAL_TIME_LIMIT_ITEM = "total-time-limit";

/** What one slider is: its key, its address item, its bounds and its words. */
export interface GraduationLoopSetting {
  key: GraduationLoopKey;
  item: string;
  label: string;
  min: number;
  max: number;
  step: number;
  /** The value a slider shows while the store holds none (SET-FR-RTDF). */
  defaultValue: number;
  /** SET-FR-HLPL: what the setting bounds. */
  helper: string;
  /** The value as text, for the label and for assistive technology. */
  format: (value: number) => string;
  /** The typed error code a refusal of this setting starts with (PSS-FR-LFUZ). */
  refusalCode: string;
  /** The refusal in words. */
  refusal: string;
}

/** SET-FR-RTDF: "1 pass", "2 passes". */
export function formatPasses(passes: number): string {
  return passes === 1 ? "1 pass" : `${passes} passes`;
}

/**
 * SET-FR-RTDF / GRU-FR-WKTL: a duration as hours and minutes, for example
 * "3 h 30 min", "30 min" or "48 h". A duration is rounded to the nearest
 * minute. A duration that is more than zero but less than half a minute reads
 * as "less than 1 min", so that time used is never said as no time.
 */
export function formatDurationMs(ms: number): string {
  const minutesTotal = Math.round(Math.max(0, ms) / MS_PER_MINUTE);
  if (minutesTotal === 0) return ms > 0 ? "less than 1 min" : "0 min";
  const hours = Math.floor(minutesTotal / 60);
  const minutes = minutesTotal % 60;
  if (hours === 0) return `${minutes} min`;
  if (minutes === 0) return `${hours} h`;
  return `${hours} h ${minutes} min`;
}

/** SET-FR-PSLD: the three sliders, in the order the section shows them. */
export const GRADUATION_LOOP_SETTINGS: readonly GraduationLoopSetting[] = [
  {
    key: "graduationPassBudget",
    item: PASS_BUDGET_ITEM,
    label: "Passes per run",
    min: 1,
    max: 10,
    step: 1,
    defaultValue: 2,
    helper:
      "How many work and review passes a run can do before it stops for you. It does not apply to merge runs.",
    format: formatPasses,
    refusalCode: "graduation_pass_budget_invalid",
    refusal: "The passes per run must be a whole number from 1 to 10. Nothing was saved.",
  },
  {
    key: "graduationStageTimeLimitMs",
    item: STAGE_TIME_LIMIT_ITEM,
    label: "Time per turn",
    min: HALF_HOUR_MS,
    max: 96 * HALF_HOUR_MS,
    step: HALF_HOUR_MS,
    defaultValue: 6 * HALF_HOUR_MS,
    helper:
      "How long one graduation turn can run. It bounds each work, review, merge and rebase turn.",
    format: formatDurationMs,
    refusalCode: "graduation_stage_time_limit_invalid",
    refusal:
      "The time per turn must be a whole number of half hours from 30 min to 48 h. Nothing was saved.",
  },
  {
    key: "graduationTotalTimeLimitMs",
    item: TOTAL_TIME_LIMIT_ITEM,
    label: "Total time per run",
    min: HALF_HOUR_MS,
    max: 192 * HALF_HOUR_MS,
    step: HALF_HOUR_MS,
    defaultValue: 10 * HALF_HOUR_MS,
    helper:
      "How much time one run can use, also a merge run. It counts only working and reviewing time. Time that the run waits for you or for its queue is not counted.",
    format: formatDurationMs,
    refusalCode: "graduation_total_time_limit_invalid",
    refusal:
      "The total time per run must be a whole number of half hours from 30 min to 96 h. Nothing was saved.",
  },
];

/** The stored value of one setting, or `null` where the store holds none. */
export function storedLoopValue(
  config: ProjectConfig,
  key: GraduationLoopKey,
): number | null {
  const value = config[key];
  return typeof value === "number" ? value : null;
}

/** The text of an error value, whatever shape the bridge gave it. */
function errorText(error: unknown): string {
  return String(error instanceof Error ? error.message : (error ?? ""));
}

/** The code of a refusal alone, which is what a log record names. */
export function loopRefusalCode(error: unknown): string {
  return errorText(error).split(":")[0].trim() || "unknown";
}

/** SET-FR-WTOS: a refused write of one setting, in words. */
export function loopRefusalMessage(
  setting: GraduationLoopSetting,
  error: unknown,
): string {
  const raw = errorText(error);
  if (raw.startsWith(setting.refusalCode)) return setting.refusal;
  return raw
    ? `The setting was not saved: ${raw}`
    : "The setting was not saved. Nothing was changed.";
}

/** PSS-FR-OWGI: the patch that stores one setting, or clears it with `null`. */
export function loopPatch(
  key: GraduationLoopKey,
  value: number | null,
): Pick<ProjectConfigPatch, GraduationLoopKey> {
  return { [key]: value };
}
