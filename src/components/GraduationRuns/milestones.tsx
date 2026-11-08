/**
 * The milestone list of the selected run
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-WQHM,
 * GRU-FR-NKDU, GRU-FR-CYDK, GRU-FR-JHVX, GRU-FR-ZCTP, GRU-FR-LMTX).
 *
 * It shows the milestones of the run's current turn, read from the `milestone`
 * records of the run's persisted activity stream. It reads no observability
 * record and no `message` summary, and it adds no control.
 */

import { useEffect, useMemo, useState } from "react";

import { readGraduationLogs } from "../../api/streams";
import { onGraduationLogRecordsAppended } from "../../events";
import { logDebug } from "../../logging";
import {
  MILESTONE_KIND,
  currentTurnOf,
  isPrSummaryRun,
  milestoneStateLabel,
  milestonesFrom,
  turnHasEnded,
} from "../../state/graduation";
import type { GraduationLogCursor, GraduationRun } from "../../types";
import { TERMINAL_RUN_STATES as TERMINAL } from "../../types";

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
      throw new Error(`the activity stream answered ${page.status}`);
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

/**
 * GRU-FR-JHVX / GRU-FR-ZCTP: the summaries of the run's current turn.
 *
 * Read when the run is selected, when its state, phase or pass changes, and on
 * an append of the activity stream while the run is not terminal. A read that
 * fails leaves the list as it was (GRU-FR-ELDK). One read runs at a time, and an
 * append that arrives during it asks for one more.
 */
function useMilestoneSummaries(run: GraduationRun): string[] | null {
  const turn = currentTurnOf(run);
  const runId = run.id;
  const phaseId = turn?.phaseId ?? null;
  const pass = turn?.pass ?? null;
  const terminal = (TERMINAL as readonly string[]).includes(run.state);
  const [held, setHeld] = useState<{ key: string; summaries: string[] } | null>(null);
  const key = `${runId}|${phaseId}|${pass}`;

  useEffect(() => {
    if (phaseId === null || pass === null) return undefined;
    let cancelled = false;
    let running = false;
    let again = false;
    const read = () => {
      if (running) {
        again = true;
        return;
      }
      running = true;
      void readSummaries(runId, phaseId, pass)
        .then((summaries) => {
          if (!cancelled) setHeld({ key, summaries });
        })
        .catch(() => {
          logDebug(["frontend"], "a run's milestones could not be read", { runId, phaseId });
        })
        .finally(() => {
          running = false;
          if (again && !cancelled) {
            again = false;
            read();
          }
        });
    };
    read();
    let unlisten: (() => void) | null = null;
    if (!terminal) {
      void onGraduationLogRecordsAppended((payload) => {
        if (payload.runId === runId && payload.stream === "activity") read();
      }).then((off) => {
        if (cancelled) off();
        else unlisten = off;
      });
    }
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [runId, phaseId, pass, run.state, terminal, key]);

  // GRU-FR-TBNE: a new turn starts with an empty list, so what was read for
  // another turn is never shown.
  return held && held.key === key ? held.summaries : null;
}

export function MilestoneList({ run }: { run: GraduationRun }) {
  const summaries = useMilestoneSummaries(run);
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
