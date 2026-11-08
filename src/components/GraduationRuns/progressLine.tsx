/**
 * The progress line of the selected run
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-NPQY,
 * GRU-FR-YMMF, GRU-FR-SOIJ).
 *
 * While the run's current turn works, the line shows the newest `message`
 * summary of that turn, so the author sees what the agent does also where the
 * agent writes no milestone marker. It adds no control.
 */

import { readGraduationLogs } from "../../api/streams";
import { MESSAGE_KIND, turnHasEnded } from "../../state/graduation";
import type { GraduationRun } from "../../types";
import { TurnReadUnavailable, useTurnRead } from "./useTurnRead";

/**
 * GRU-FR-YMMF: the newest `message` summary of one turn, or "" where the turn
 * holds no `message` record yet.
 */
async function readNewestMessage(runId: string, phaseId: string, pass: number): Promise<string> {
  const page = await readGraduationLogs({
    runId,
    phaseId,
    pass: { kind: "pass", pass },
    stream: "activity",
    cursor: null,
    limit: 1,
    kinds: [MESSAGE_KIND],
  });
  // GRU-FR-YMMF: a page that says the stream could not be read is a failed
  // read, so the line stays as it was rather than going away.
  if (page.status === "unavailable" || page.status === "persistence_failed") {
    throw new TurnReadUnavailable(page.status);
  }
  const newest = page.entries[page.entries.length - 1];
  const summary = (newest?.record as { summary?: unknown } | undefined)?.summary;
  return typeof summary === "string" ? summary.trim() : "";
}

function sameText(a: string, b: string): boolean {
  return a === b;
}

/** The line of a turn that works, which reads and follows the stream. */
function LiveProgressLine({ run }: { run: GraduationRun }) {
  const summary = useTurnRead(run, readNewestMessage, sameText, "progress line");
  // GRU-FR-NPQY: a turn with no `message` record shows no line.
  if (!summary) return null;
  return (
    <p
      className="graduation__progress"
      aria-label="Agent progress"
      data-testid="graduation-progress"
    >
      <span className="graduation__progress-label">Now</span>
      {/* GRU-FR-SOIJ: a model wrote the summary, so it is plain text, cut to
          one line, with the full text as the tooltip. */}
      <span className="graduation__progress-text" title={summary}>
        {summary}
      </span>
    </p>
  );
}

export function ProgressLine({ run }: { run: GraduationRun }) {
  // GRU-FR-NPQY: a turn that has ended shows no line, and reads nothing.
  if (turnHasEnded(run)) return null;
  return <LiveProgressLine run={run} />;
}
