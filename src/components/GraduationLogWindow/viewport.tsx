/**
 * What stands where the output would be
 * (`../../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-NMOD,
 * GLW-FR-NYQF, GLW-FR-OATU, GLW-FR-VJNK).
 *
 * The five states are rendered distinctly, none as any of the others, and none
 * of them says that the run produced no output.
 */

import type { GraduationLogFailure, GraduationLogPageEntry, GraduationLogStream } from "../../types";
import { RUN_LEVEL_MARKER, sourceRows, structuredRow } from "./records";

/** The word the toggle reads for one stream, used inside the empty state. */
export function streamLabel(stream: GraduationLogStream): string {
  return stream === "source" ? "Source" : "Structured";
}

function otherStream(stream: GraduationLogStream): GraduationLogStream {
  return stream === "source" ? "structured" : "source";
}

export function LoadingState() {
  return (
    <p className="glw__state" data-testid="glw-loading" role="status">
      Reading the log…
    </p>
  );
}

/**
 * GLW-FR-NYQF: an empty scope states that this entry and this stream hold
 * nothing, and names the other stream as the one the toggle reads. It never
 * says the run produced no output.
 */
export function EmptyState({
  entryLabel,
  stream,
}: {
  entryLabel: string;
  stream: GraduationLogStream;
}) {
  return (
    <p className="glw__state" data-testid="glw-empty">
      {entryLabel} wrote nothing to the {streamLabel(stream)} stream. The{" "}
      {streamLabel(otherStream(stream))} stream is read with the toggle above.
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
      <p>This stream could not be read whole. The run itself is unaffected.</p>
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
 * GLW-FR-OATU: a write of this stream failed. It names the typed failure, says
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

/** GLW-FR-NMOD: a search matched nothing. It is not an empty scope. */
export function NoMatchState({ query }: { query: string }) {
  return (
    <p className="glw__state" data-testid="glw-no-match">
      No line of this stream matches “{query}”.
    </p>
  );
}

/**
 * GLW-FR-FXAL: one source chunk as its text lines, each carrying all ten of the
 * chunk's displayed fields.
 */
function SourceEntry({ entry }: { entry: GraduationLogPageEntry }) {
  return (
    <>
      {sourceRows(entry).map((row) => (
        <div
          className="glw__row"
          key={row.key}
          data-testid="glw-row"
          data-run-level={row.runLevel ? "true" : undefined}
        >
          {/* GLW-FR-DDXJ: the row keeps a visible origin and producer marker,
              so a run-level record reads as the run-level record it is. */}
          <span className="glw__meta t-meta">{row.metaLine}</span>
          {/* GLW-FR-SHAF: escaped plain text with its line structure kept, and
              nothing else interpreted. */}
          <span className="glw__text">{row.text}</span>
        </div>
      ))}
    </>
  );
}

/** GLW-FR-GIWK: one structured record as its event, level, instant, and fields. */
function StructuredEntry({ entry }: { entry: GraduationLogPageEntry }) {
  const row = structuredRow(entry);
  return (
    <div
      className="glw__row"
      data-testid="glw-row"
      data-run-level={row.runLevel ? "true" : undefined}
    >
      <span className="glw__meta t-meta">
        {row.at} · {row.level} · {row.event}
      </span>
      <span className="glw__text">
        {row.fields.map(([key, value]) => `${key}=${value}`).join("  ")}
      </span>
      <span className="glw__meta t-meta">{row.metaLine}</span>
    </div>
  );
}

export function LogRows({
  entries,
  stream,
}: {
  entries: GraduationLogPageEntry[];
  stream: GraduationLogStream;
}) {
  return (
    <>
      {entries.map((entry, at) => {
        const key = `${String(entry.record.sequence ?? at)}`;
        return stream === "source" ? (
          <SourceEntry key={key} entry={entry} />
        ) : (
          <StructuredEntry key={key} entry={entry} />
        );
      })}
    </>
  );
}

/** GLW-FR-DDXJ: what the run-level entry is called, in words. */
export const RUN_LEVEL_LABEL = `Run-level (${RUN_LEVEL_MARKER}) output of this stage`;
