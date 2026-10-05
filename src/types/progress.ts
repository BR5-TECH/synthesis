// Progress reporting (PRG-progress-reporting.md / STB-status-bar.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

// ---------------------------------------------------------------------------
// Progress reporting (PRG-progress-reporting.md / STB-status-bar.md)
// ---------------------------------------------------------------------------

/**
 * An operation is **in flight** while `"running"`, and **terminated** in each of
 * the other three states (PRG contract surface).
 */
export type OperationState = "running" | "finished" | "failed" | "cancelled";

/**
 * PRG-FR-KXQW: where an activity's owning surface is and which of its targets to
 * select. Supplied by the producer. A consumer never derives one from `kind` or
 * `label` (PRG-FR-TBZN).
 */
export type Activation =
  | { type: "graduation_run"; runId: string }
  | { type: "discussion"; discussionId: string }
  | { type: "git_push"; branch: string };

/**
 * One reported long-running operation (PRG-FR-01).
 *
 * `completed` and `total` are both present on a determinate operation and both
 * absent on an indeterminate one (PRG-FR-05) — which is what STB-FR-08 keys the
 * determinate-vs-busy rendering on. `kind` names the producing module but is
 * never required to render one: an unrecognised kind is displayed generically
 * from `label` and progress alone (PRG-FR-12 / STB-FR-09), so it is deliberately
 * an open `string` rather than a union.
 */
export interface Operation {
  /** Unique for the lifetime of the running application, never reused (PRG-FR-03). */
  id: string;
  kind: string;
  label: string;
  state: OperationState;
  completed?: number;
  total?: number;
  /** Monotonic registration ordinal — the ordering key of PRG-FR-02. */
  sequence: number;
  /**
   * PRG-FR-KXQW: the producer's destination, absent when the operation has none.
   * Typed as a loose `type` string at the boundary: a build may meet a `type` it
   * does not know, and the row is then not actionable (STB-FR-RWPD).
   */
  activation?: Activation;
}
