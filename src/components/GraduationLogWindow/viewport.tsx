/**
 * What stands where the activity would be
 * (`../../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-NMOD,
 * GLW-FR-NYQF, GLW-FR-OATU, GLW-FR-VJNK, GLW-FR-KHGP).
 *
 * The five states are rendered distinctly, none as any of the others, and none
 * of them says that the run produced no output.
 */

import type { GraduationLogFailure, GraduationLogPageEntry } from "../../types";
import { ActivityLine } from "../AgentActivityLine";
import { activityRow } from "./records";

export function LoadingState() {
  return (
    <p className="glw__state" data-testid="glw-loading" role="status">
      Reading the agent activity…
    </p>
  );
}

/**
 * GLW-FR-NYQF: an empty entry states that this entry holds no agent activity.
 * It never says that the run produced no output, and it names no other stream.
 */
export function EmptyState({ entryLabel }: { entryLabel: string }) {
  return (
    <p className="glw__state" data-testid="glw-empty">
      {entryLabel} holds no agent activity yet.
    </p>
  );
}

/**
 * GLW-FR-VJNK: the stream could not be read whole. It names the typed read
 * failure and the sequence the read got as far as, and it says the run itself
 * is unaffected.
 */
export function UnavailableState({
  failure,
  message,
}: {
  failure: GraduationLogFailure | null;
  message?: string | null;
}) {
  return (
    <div className="glw__state glw__state--warning" data-testid="glw-unavailable" role="alert">
      <p>The agent activity could not be read whole. The run itself is unaffected.</p>
      {failure && (
        <p className="t-meta">
          {failure.code} · {failure.stream}
          {typeof failure.stoppedSequence === "number"
            ? ` · read as far as sequence ${failure.stoppedSequence}`
            : ""}
        </p>
      )}
      {message && <p className="t-meta">{message}</p>}
    </div>
  );
}

/**
 * GLW-FR-OATU: a write of the stream failed. It names the typed failure, says
 * the run is interrupted and that Continue retries the writes, and it offers no
 * control that acts on the run.
 */
export function PersistenceFailureState({
  failure,
}: {
  failure: GraduationLogFailure | null;
}) {
  return (
    <div
      className="glw__state glw__state--warning"
      data-testid="glw-persistence-failed"
      role="alert"
    >
      <p>The log could not be written and the run stopped.</p>
      {failure && (
        <p className="t-meta">
          {failure.code} · {failure.stream}
        </p>
      )}
      <p className="t-meta">The run is interrupted. Continue retries the writes.</p>
    </div>
  );
}

/** GLW-FR-NMOD: a search matched nothing. It is not an empty entry. */
export function NoMatchState({ query }: { query: string }) {
  return (
    <p className="glw__state" data-testid="glw-no-match">
      No row of this entry matches “{query}”.
    </p>
  );
}

/**
 * GLW-FR-KHGP / GLW-FR-JOIG: one row of three parts, with no expansion and no
 * other part. The summary is escaped plain text (GLW-FR-SHAF).
 */
function ActivityRowView({ entry }: { entry: GraduationLogPageEntry }) {
  const row = activityRow(entry);
  return (
    <div data-testid="glw-row" data-sequence={row.sequence}>
      <ActivityLine at={row.at} kind={row.kind}>
        <span className="runs-line__msg">{row.summary}</span>
      </ActivityLine>
    </div>
  );
}

export function LogRows({ entries }: { entries: GraduationLogPageEntry[] }) {
  return (
    <>
      {entries.map((entry) => {
        const row = activityRow(entry);
        return <ActivityRowView key={row.sequence} entry={entry} />;
      })}
    </>
  );
}
