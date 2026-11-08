/**
 * The milestone list of the selected run
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-WQHM,
 * GRU-FR-NKDU, GRU-FR-CYDK, GRU-FR-JHVX, GRU-FR-ZCTP, GRU-FR-LMTX).
 *
 * It shows the milestones of the run's current turn, read from the `milestone`
 * records of the run's persisted activity stream. It reads no observability
 * record and no `message` summary, and it adds no control.
 */

import { useMemo } from "react";

import { readGraduationLogs } from "../../api/streams";
import {
  MILESTONE_KIND,
  isPrSummaryRun,
  milestoneStateLabel,
  milestonesFrom,
  turnHasEnded,
} from "../../state/graduation";
import type { GraduationLogCursor, GraduationRun } from "../../types";
import { TurnReadUnavailable, useTurnRead } from "./useTurnRead";

/** The records one page of the read holds. */
const PAGE_LIMIT = 200;
/** A bound on the pages one read walks, so a damaged cursor cannot loop. */
const MAX_PAGES = 50;

/** GRU-FR-JHVX: every `milestone` summary of one turn, in `sequence` order. */
async function readSummaries(runId: string, phaseId: string, pass: number): Promise<string[]> {
  const pages: string[][] = [];
  let cursor: GraduationLogCursor | null = null;
  for (let walked = 0; walked < MAX_PAGES; walked += 1) {
    const page = await readGraduationLogs({
      runId,
      phaseId,
      pass: { kind: "pass", pass },
      stream: "activity",
      cursor,
      limit: PAGE_LIMIT,
      kinds: [MILESTONE_KIND],
    });
    // GRU-FR-ELDK: a page that says the stream could not be read is a failed
    // read, so the list stays as it was rather than going empty.
    if (page.status === "unavailable" || page.status === "persistence_failed") {
      throw new TurnReadUnavailable(page.status);
    }
    pages.unshift(
      page.entries
        .map((entry) => (entry.record as { summary?: unknown }).summary)
        .filter((summary): summary is string => typeof summary === "string"),
    );
    if (page.oldestReached || !page.olderCursor) break;
    cursor = page.olderCursor;
  }
  return pages.flat();
}

function sameSummaries(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((summary, at) => summary === b[at]);
}

export function MilestoneList({ run }: { run: GraduationRun }) {
  // GRU-FR-JHVX / GRU-FR-ZCTP: the summaries of the run's current turn.
  const summaries = useTurnRead(run, readSummaries, sameSummaries, "milestones");
  const ended = turnHasEnded(run);
  const milestones = useMemo(
    () => (summaries === null ? null : milestonesFrom(summaries, ended)),
    [summaries, ended],
  );
  // GRU-FR-NKDU: a PR summary run and a turn with no valid plan render nothing.
  if (isPrSummaryRun(run) || milestones === null) return null;
  return (
    <section
      className="graduation__milestones"
      aria-label="Milestones"
      data-testid="graduation-milestones"
    >
      <h4 className="graduation__milestones-head t-eyebrow">Milestones</h4>
      <ol className="graduation__milestones-rows">
        {milestones.map((milestone, at) => (
          <li
            key={at}
            className="graduation__milestone"
            data-state={milestone.state}
            data-testid="graduation-milestone"
          >
            {/* GRU-FR-CYDK: the state in words, not by mark or colour alone. */}
            <span className="graduation__milestone-state">
              {milestoneStateLabel(milestone.state)}
            </span>
            {/* GRU-FR-LMTX: a model wrote the title, so it is plain text. */}
            <span className="graduation__milestone-title">{milestone.title}</span>
          </li>
        ))}
      </ol>
    </section>
  );
}
