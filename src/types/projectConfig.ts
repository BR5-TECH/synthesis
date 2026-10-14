// Project-public config (PSS-project-settings-storage.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import type { GraduationConcurrencyLimit } from "./graduation";

// ---------------------------------------------------------------------------
// Project-public config (PSS-project-settings-storage.md PSS-FR-17)
// ---------------------------------------------------------------------------

/**
 * The project's line-ending convention. Project-public, so it is shared by
 * everyone who checks the project out, and it is what every artifact write is
 * normalised to (PST-FR-23). Edited from two places that share one value: the
 * status bar (STB-FR-16) and the Project settings tab (SET-FR-10).
 */
export type LineEndings = "lf" | "crlf";

/** The project-public config record as a read returns it (PSS-FR-17). */
export interface ProjectConfig {
  lineEndings: LineEndings;
  /**
   * PSS-FR-21: the project's optional Markdown **draft template** — the text a
   * new draft's prompt is created holding (DRS-FR-39).
   *
   * `null` is the **unset** state, which is not the same thing as an empty
   * template: a project with no template configured creates drafts on the
   * empty-prompt terms of DRS-FR-06, and SET-FR-19 returns a project to that
   * state by deleting every character rather than by recording `""`.
   */
  draftTemplate: string | null;
  /**
   * PSS-FR-TQMV: how long one agent turn may run, in milliseconds. `null` is
   * the unset state, in which each loop uses its own default (PSS-FR-ZLCF).
   */
  executionTimeoutMs?: number | null;
  /**
   * PSS-FR-JRWC: how many graduation runs may hold a project slot at once,
   * stream runs and direct runs together. A read always names it, and a store
   * that holds none reads as one (PSS-FR-KMBT).
   */
  graduationConcurrencyLimit: GraduationConcurrencyLimit;
  /**
   * PSS-FR-QBNE: how many passes a run that is not a merge run may take, 1 to
   * 10. `null` is the unset state, which takes the default of 2 (PSS-FR-YDCA).
   */
  graduationPassBudget?: number | null;
  /**
   * PSS-FR-HVSM: how long one graduation turn may run, in milliseconds. A
   * whole multiple of 1,800,000 from 1,800,000 to 172,800,000. `null` is the
   * unset state, which takes the default of 3 hours.
   */
  graduationStageTimeLimitMs?: number | null;
  /**
   * PSS-FR-RKTW: how much working and reviewing time one run may use, in
   * milliseconds. A whole multiple of 1,800,000 from 1,800,000 to
   * 345,600,000. `null` is the unset state, which takes the default of 5 hours.
   */
  graduationTotalTimeLimitMs?: number | null;
}

/**
 * What a `"save project config"` carries: only the sections this write means to
 * persist.
 *
 * PSS-FR-17 requires a whole-store write to carry every section it does not
 * name through unchanged, and the payload is only a partial view of what
 * `project.toml` holds. An **omitted** key therefore means "leave this exactly
 * as it stands" — which is what lets the status bar persist a line-ending
 * convention without disturbing a configured template, and the Draft template
 * section persist a template without disturbing the convention. A
 * `draftTemplate` of `""` is the one value that *removes* the stored template
 * (PSS-FR-21).
 */
export interface ProjectConfigPatch {
  lineEndings?: LineEndings;
  draftTemplate?: string;
  /** PSS-FR-TQMV: omitted leaves it as stored; `null` clears it to unset. */
  executionTimeoutMs?: number | null;
  /** PSS-FR-ZVSD: absent leaves the stored limit as it stands. */
  graduationConcurrencyLimit?: GraduationConcurrencyLimit;
  /**
   * PSS-FR-OWGI: for each graduation loop setting, omitted leaves it as
   * stored, `null` clears it to unset, and a valid value stores it. A value
   * that is not valid is refused whole (PSS-FR-LFUZ).
   */
  graduationPassBudget?: number | null;
  graduationStageTimeLimitMs?: number | null;
  graduationTotalTimeLimitMs?: number | null;
}
