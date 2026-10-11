/**
 * The visible parts of one agent-activity row: local time, kind, and summary
 * (`../../specifications/ui/RUN-runs.md` RUN-FR-03, RUN-FR-11, RUN-FR-YQAE).
 *
 * The Agent Output section and the graduation log window share this module.
 * Because of this, a row has the same time, the same kind label, and the same
 * level treatment on both surfaces
 * (`../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-KHGP).
 */
import type { ReactNode } from "react";

/** The level treatment of a kind. */
export type ActivityLevel = "info" | "ok" | "warn" | "err" | "step";

/**
 * RUN-FR-03: which level treatment a kind takes.
 *
 * The kinds are the backend's (EAC-FR-33). This maps them onto the five levels
 * the row already has, so a stream reads at a glance: what went wrong is red,
 * what the agent decided is accented, and its narration is plain.
 */
const LEVEL: Record<string, ActivityLevel> = {
  invocation: "step",
  task: "step",
  started: "step",
  reasoning: "info",
  message: "info",
  tool_call: "step",
  tool_result: "info",
  command: "step",
  file_change: "ok",
  retry: "warn",
  usage: "info",
  finished: "ok",
  error: "err",
  diagnostic: "warn",
  unrecognized: "warn",
};

/** RUN-FR-03: the level of a kind. A kind that is not known reads as `info`. */
export function activityLevel(kind: string): ActivityLevel {
  return LEVEL[kind] ?? "info";
}

/** RUN-FR-03: the short label of a kind. It is wider than the level. */
export function kindLabel(kind: string): string {
  return kind.replace(/_/g, " ");
}

/** RUN-FR-11: the local wall-clock time of the instant a record carries. */
export function clockOf(at: string): string {
  const when = new Date(at);
  if (Number.isNaN(when.getTime())) return "";
  return when.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

interface ActivityLineProps {
  /** The instant the record carries. */
  at: string;
  kind: string;
  /** The tooltip of the kind. The Agent Output section names the channel. */
  title?: string;
  /** The one-line summary. The caller decides what holds it. */
  children: ReactNode;
}

/** One row: time, kind, and the summary the caller passes. */
export function ActivityLine({ at, kind, title, children }: ActivityLineProps) {
  return (
    <div className="runs-line">
      <span className="runs-line__ts">{clockOf(at)}</span>
      <span className="runs-line__lvl" data-level={activityLevel(kind)} title={title}>
        {kindLabel(kind)}
      </span>
      {children}
    </div>
  );
}
