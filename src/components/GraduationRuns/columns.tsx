/**
 * The selected run's path lists and pass history
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-RRNN,
 * GRU-FR-WJHV, GRU-FR-YYXN, GRU-FR-MCYF, GRU-FR-KWRB, GRU-FR-ZPTE).
 *
 * Side by side in a wide region, with a narrow paths column and a divider the
 * author drags, and stacked in a narrow one.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import { logError } from "../../logging";
import {
  loadAppPreferences,
  patchAppPreferences,
  peekAppPreferences,
} from "../../state/appPreferences";
import {
  clampPathsFraction,
  DEFAULT_PATHS_FRACTION,
  interruptionCause,
  MAX_PATHS_FRACTION,
  MIN_PATHS_FRACTION,
  pathsFractionForDrag,
  pathsFractionForKey,
  pathsPercent,
} from "../../state/graduation";
import type { GraduationRun } from "../../types";
import { hasReadableProgress } from "../../types/graduationObservability";
import { PathListSection } from "./changeSet";
import { PassHistory } from "./passes";

/** One empty list, so a run with no paths gives its section a stable input. */
const NO_PATHS: readonly string[] = [];

export function RunColumns({ run }: { run: GraduationRun }) {
  const changedPaths = run.checkpoint?.changedPaths ?? NO_PATHS;
  const hiddenPaths = run.checkpoint?.hiddenPaths ?? NO_PATHS;
  // GRU-FR-MCYF: a side with nothing in it takes no column, so the other takes
  // the width and no divider stands between them.
  const hasPaths = changedPaths.length > 0 || hiddenPaths.length > 0;
  const hasPasses =
    hasReadableProgress(run.observability) &&
    (run.observability?.passes?.length ?? 0) > 0;
  const split = hasPaths && hasPasses;

  // GRU-FR-KWRB: the paths column's width, as the author last set it. It is
  // read once and written only when a gesture ends, so a drag costs one write
  // rather than one per frame.
  // The cached record is read on first render, so a width the author set
  // paints at once rather than one frame after the default.
  const [fraction, setFraction] = useState(() =>
    clampPathsFraction(peekAppPreferences()?.graduationPathsWidthFraction),
  );
  const columnsRef = useRef<HTMLDivElement | null>(null);
  const draggingRef = useRef(false);
  /** Where the gesture under way has put the boundary. */
  const draggedRef = useRef(DEFAULT_PATHS_FRACTION);
  /** The width before the gesture under way, for a cancelled drag. */
  const startRef = useRef(DEFAULT_PATHS_FRACTION);
  /**
   * How far right of the boundary the author took hold of the divider, so the
   * boundary keeps that distance from the pointer rather than jumping to it.
   */
  const grabRef = useRef(0);
  /** Whether the author has set the width since this mounted. */
  const touchedRef = useRef(false);
  useEffect(() => {
    let live = true;
    void loadAppPreferences().then((prefs) => {
      // A width the author set while the record loaded is the newer one: the
      // load does not take it back.
      if (live && !touchedRef.current) {
        setFraction(clampPathsFraction(prefs.graduationPathsWidthFraction));
      }
    });
    return () => {
      live = false;
    };
  }, []);
  const persist = useCallback((next: number) => {
    touchedRef.current = true;
    setFraction(next);
    void patchAppPreferences({ graduationPathsWidthFraction: next }).catch(() => {
      logError(["frontend"], "the paths column width could not be stored", {});
    });
  }, []);
  const widthAt = useCallback((clientX: number) => {
    const box = columnsRef.current?.getBoundingClientRect();
    // A box with no width says nothing about where the pointer stands.
    if (!box || !(box.width > 0)) return null;
    return pathsFractionForDrag(clientX - grabRef.current - box.left, box.width);
  }, []);
  // A drag ends with the divider it was on: a pointer that comes back to a new
  // divider starts no width change it did not press for.
  useEffect(() => {
    if (!split) draggingRef.current = false;
  }, [split]);

  if (!hasPaths && !hasPasses) return null;
  const percent = pathsPercent(fraction);

  return (
    <div
      className="graduation__columns"
      ref={columnsRef}
      data-split={split ? "true" : undefined}
      // The exact share, so a drag follows the pointer; the divider announces
      // it as a whole percentage.
      style={
        {
          "--graduation-paths": `${+(clampPathsFraction(fraction) * 100).toFixed(2)}%`,
        } as React.CSSProperties
      }
    >
      {hasPaths && (
        <div className="graduation__columns-paths">
          {/* GRU-FR-RRNN / GRU-FR-TXLW: what the run changed, against its base
              commit. No per-path verdict: the application judges no path any
              more. Keyed by run, so another run's list opens by its own rule
              (GRU-FR-NUCJ). */}
          {changedPaths.length > 0 && (
            <PathListSection
              key={`changed-${run.id}`}
              label="Changed"
              paths={changedPaths}
              testId="graduation-changes"
            />
          )}

          {/* GRU-FR-WJHV: authored work an ignore rule hides is committed by
              nothing, so the region says how many there are and names them. */}
          {hiddenPaths.length > 0 && (
            <PathListSection
              key={`hidden-${run.id}`}
              label="Hidden by ignore rules"
              paths={hiddenPaths}
              omitted={run.checkpoint?.hiddenPathsOmitted ?? undefined}
              testId="graduation-hidden"
            />
          )}
        </div>
      )}

      {/* GRU-FR-ZPTE: the boundary the author drags, and the same width by
          keyboard alone. A separator that states its name and its width as a
          percentage. The stylesheet shows it only where the columns stand
          side by side. */}
      {split && (
        <div
          className="graduation__columns-divider"
          role="separator"
          aria-orientation="vertical"
          aria-label="Paths width"
          aria-valuenow={percent}
          aria-valuemin={pathsPercent(MIN_PATHS_FRACTION)}
          aria-valuemax={pathsPercent(MAX_PATHS_FRACTION)}
          aria-valuetext={`Paths width ${percent} percent`}
          title="Drag, or use the arrow keys, to set the paths width"
          tabIndex={0}
          data-testid="graduation-columns-divider"
          onPointerDown={(event) => {
            // The primary button alone starts a drag.
            if (event.button !== undefined && event.button !== 0) return;
            const box = columnsRef.current?.getBoundingClientRect();
            grabRef.current = box
              ? event.clientX - (box.left + fraction * box.width)
              : 0;
            touchedRef.current = true;
            draggingRef.current = true;
            draggedRef.current = fraction;
            startRef.current = fraction;
            event.currentTarget.setPointerCapture?.(event.pointerId);
          }}
          onPointerMove={(event) => {
            if (!draggingRef.current) return;
            const next = widthAt(event.clientX);
            if (next === null) return;
            // Held in state and in a ref while the pointer is down: the value
            // written when the gesture ends is the one the last move reached.
            draggedRef.current = next;
            setFraction(next);
          }}
          onPointerUp={(event) => {
            if (!draggingRef.current) return;
            draggingRef.current = false;
            event.currentTarget.releasePointerCapture?.(event.pointerId);
            persist(draggedRef.current);
          }}
          onPointerCancel={() => {
            if (!draggingRef.current) return;
            // A cancelled drag stores nothing, so the width it shows goes back
            // to the one that is stored.
            draggingRef.current = false;
            setFraction(startRef.current);
          }}
          onKeyDown={(event) => {
            const next = pathsFractionForKey(event.key, fraction);
            if (next === null) return;
            event.preventDefault();
            persist(next);
          }}
        />
      )}

      {/* GRU-FR-YYXN: one row per pass, with what it was asked to do and what
          the review decided. */}
      {hasPasses && (
        <PassHistory
          // The open row belongs to the run it was opened on, so a change of
          // selection starts the history again rather than carrying one run's
          // reading position onto another's.
          key={run.id}
          passes={run.observability?.passes ?? []}
          current={run.checkpoint?.pass ?? 1}
          stoppedCause={
            run.state === "interrupted" && run.interruption
              ? interruptionCause(run.interruption.reason)
              : null
          }
        />
      )}
    </div>
  );
}
