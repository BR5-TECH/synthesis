/**
 * One read of the persisted activity stream for a run's current turn, kept
 * fresh while the run works
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-TBNE,
 * GRU-FR-ZCTP, GRU-FR-ELDK, GRU-FR-QKSY, GRU-FR-YMMF).
 *
 * The milestone list and the progress line both read through this hook, so
 * they read again at the same moments and keep what they show on a failed read.
 */

import { useEffect, useState } from "react";

import { onGraduationLogRecordsAppended } from "../../events";
import { logDebug } from "../../logging";
import { currentTurnOf } from "../../state/graduation";
import type { GraduationRun } from "../../types";
import { TERMINAL_RUN_STATES as TERMINAL } from "../../types";

/**
 * GRU-FR-ELDK / GRU-FR-YMMF: a page that says the stream could not be read. A
 * reader throws it, so the held value stays, and the log names the status.
 */
export class TurnReadUnavailable extends Error {
  constructor(readonly status: string) {
    super(`the activity stream answered ${status}`);
    this.name = "TurnReadUnavailable";
  }
}

/** The read of one turn: the run, its phase and its pass. */
export type TurnReader<T> = (runId: string, phaseId: string, pass: number) => Promise<T>;

/**
 * GRU-FR-ZCTP / GRU-FR-YMMF: the value `reader` gives for the run's current turn.
 *
 * Read when the run is selected, when its state, phase or pass changes, and on
 * an append of the activity stream while the run is not terminal. A read that
 * fails leaves the value as it was (GRU-FR-ELDK). A read that returns a value
 * `same` as the held one changes nothing on screen. One read runs at a time,
 * and an append that arrives during it asks for one more. `what` names the
 * value in the debug log of a failed read.
 *
 * `reader` and `same` must be stable (module-level functions).
 */
export function useTurnRead<T>(
  run: GraduationRun,
  reader: TurnReader<T>,
  same: (a: T, b: T) => boolean,
  what: string,
): T | null {
  const turn = currentTurnOf(run);
  const runId = run.id;
  const phaseId = turn?.phaseId ?? null;
  const pass = turn?.pass ?? null;
  const terminal = (TERMINAL as readonly string[]).includes(run.state);
  const [held, setHeld] = useState<{ key: string; value: T } | null>(null);
  const key = `${runId}|${phaseId}|${pass}`;

  useEffect(() => {
    if (phaseId === null || pass === null) return undefined;
    let cancelled = false;
    let running = false;
    let again = false;
    const read = () => {
      // An append that arrives after the cleanup reads nothing.
      if (cancelled) return;
      if (running) {
        again = true;
        return;
      }
      running = true;
      void reader(runId, phaseId, pass)
        .then((value) => {
          if (cancelled) return;
          // GRU-FR-ZCTP: a read that returns what is held changes nothing on
          // screen. An agent that streams appends many records each second.
          setHeld((prev) =>
            prev !== null && prev.key === key && same(prev.value, value)
              ? prev
              : { key, value },
          );
        })
        .catch((error: unknown) => {
          // The cause is a status or "read_failed", never the error text: a
          // backend error can carry a path or agent text.
          const cause = error instanceof TurnReadUnavailable ? error.status : "read_failed";
          logDebug(["frontend"], `a run's ${what} could not be read`, {
            runId,
            phaseId,
            pass,
            cause,
          });
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
    // `run.state` is in the list on purpose: a state change that keeps the
    // phase and the pass still reads again (GRU-FR-ZCTP).
  }, [runId, phaseId, pass, run.state, terminal, key, reader, same, what]);

  // GRU-FR-TBNE: a new turn starts empty, so what was read for another turn is
  // never shown.
  return held && held.key === key ? held.value : null;
}
