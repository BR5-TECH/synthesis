// Agent activity (AGV-agent-activity.md)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

/**
 * AGV-FR-02: one thing an agent CLI did, as it did it.
 *
 * Every field arrives masked — the backend is the only place that holds the
 * credentials to mask against — so nothing here is redacted, shortened, or
 * inspected on the way to the screen (AGV-FR-04).
 */
export interface AgentActivityRecord {
  /** Assigned by the store, ascending within a run and never reused. */
  seq: number;
  /** RFC 3339 UTC, taken when the line arrived. */
  at: string;
  /** `stdout`, `stderr`, or `executor` for the invocation and the task. */
  channel: string;
  kind: AgentActivityKind;
  /** One line, for a row. */
  summary: string;
  /** The whole event, verbatim, as the vendor wrote it. */
  payload: string;
  /** Whether `payload` is a prefix of what arrived. */
  payloadTruncated: boolean;
}

/**
 * EAC-FR-33: what one line meant, in terms every vendor shares.
 *
 * Open rather than closed: a vendor this build has never seen is reported as
 * `unrecognized` rather than dropped, and a kind added to the backend must
 * render as itself here rather than disappearing.
 */
export type AgentActivityKind =
  | "invocation"
  | "task"
  | "started"
  | "reasoning"
  | "message"
  | "tool_call"
  | "tool_result"
  | "command"
  | "file_change"
  | "retry"
  | "usage"
  | "finished"
  | "error"
  | "diagnostic"
  | "unrecognized";

/** AGV-FR-11: what a consumer is told when a run's activity changed. */
export interface AgentActivityState {
  runId: string;
  /** Every record ever appended for this run. */
  total: number;
  /** Records no longer held in memory. They are still in the run's file. */
  dropped: number;
  latestSeq: number;
}

/** AGV-FR-10: one page of a run's activity, ascending by `seq`. */
export interface AgentActivityPage extends AgentActivityState {
  records: AgentActivityRecord[];
}

/** GRD-FR-VLFO: the project's queue, in enqueue order. */
