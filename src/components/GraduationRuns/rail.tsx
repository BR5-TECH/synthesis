/**
 * The history rail of the graduation section
 * (`../../../specifications/ui/GRH-graduation-history.md`).
 *
 * Two controls pinned above one list: the text filter and the view selector
 * (GRH-FR-WIMY, GRH-FR-BDMB), then every run the project has made in the
 * project's own run order (GRH-FR-NCJK). The rail is a way into a run and never
 * a way to change one — what a row's own controls do is `GRU-graduation-runs.md`
 * GRU-FR-XQVG's, and the section owns every call they report.
 *
 * The controls sit **outside** the scrolling box rather than at the top of it,
 * so they stay put while the list scrolls (GRH-FR-OFHS: both are reachable
 * whatever the list is showing).
 */

import { PanelEmptyState, PanelFilteredState } from "../PanelEmptyState";
import { SelectorRow } from "../SelectorRow";
import { Icon } from "../icons";
import { runTitle, stateLabelOf, targetName } from "../../state/graduation";
import {
  holdsAStream,
  isArchived,
  VIEW_POSITIONS,
  type GraduationView,
} from "../../state/graduationRail";
import type { GraduationRun } from "../../types";
import { RowActions } from "./rowActions";

export interface GraduationRailProps {
  /** Every run the project has made, before the view and the text narrow it. */
  runs: GraduationRun[];
  /** GRH-FR-BDMB: what the view in force and the text filter admit. */
  listed: GraduationRun[];
  selected: string | null;
  view: GraduationView;
  onView: (view: GraduationView) => void;
  filter: string;
  onFilter: (text: string) => void;
  /** GRU-FR-XQVG: a call this section started is in flight. */
  busy: boolean;
  onSelect: (runId: string) => void;
  /** GRH-FR-FYTH: where a queued run stands in its own stream's queue. */
  positionOf: (run: GraduationRun) => string | null;
  onPause: (run: GraduationRun) => void;
  onContinue: (run: GraduationRun) => void;
  onAutoStart: (run: GraduationRun, enabled: boolean) => void;
  onArchive: (run: GraduationRun, archived: boolean) => void;
  /** GRT-FR-AMHS: open the restart confirmation for a discarded run. */
  onRestart: (run: GraduationRun) => void;
}

export function GraduationRail({
  runs,
  listed,
  selected,
  view,
  onView,
  filter,
  onFilter,
  busy,
  onSelect,
  positionOf,
  onPause,
  onContinue,
  onAutoStart,
  onArchive,
  onRestart,
}: GraduationRailProps) {
  return (
    <div className="graduation__history">
      {/* GRH-FR-BDMB / SNV-FR-58: the text filter first, the view selector
          under it and directly above the list it narrows. */}
      <div className="panel-controls graduation__filters">
        <div className="search-input">
          <Icon.Search size={12} />
          <input
            type="search"
            value={filter}
            placeholder="Find a run…"
            aria-label="Filter runs"
            onChange={(event) => onFilter(event.target.value)}
          />
        </div>
        <SelectorRow
          label="Run view"
          positions={VIEW_POSITIONS}
          value={view}
          onChange={onView}
        />
      </div>

      <div className="graduation__rail">
        {/* SNV-FR-60 / SNV-FR-61: a project that has graduated nothing is an
            empty state; a filter that admits nothing is not. The second one
            renders in the list's own region with both controls above it still
            holding what the author typed and picked, because their next move is
            to change them rather than to start a run. */}
        {listed.length === 0 &&
          (runs.length === 0 ? (
            <div data-testid="graduation-empty">
              <PanelEmptyState line="Nothing graduating">
                A graduation begins from a draft's Graduate action.
              </PanelEmptyState>
            </div>
          ) : (
            <PanelFilteredState>No run matches this filter.</PanelFilteredState>
          ))}

        {/* GRU-FR-XQVG: a plain list rather than a listbox. Each row carries
            two targets — the button that opens the run and the cluster that
            acts on it — and a listbox owns options and nothing else. The
            selection is carried by `aria-current` on the row that is open. */}
        <div role="list" aria-label="Graduation run history">
          {listed.map((entry) => (
            <RailRow
              key={entry.id}
              run={entry}
              selected={entry.id === selected}
              position={positionOf(entry)}
              busy={busy}
              onSelect={onSelect}
              onPause={() => onPause(entry)}
              onContinue={() => onContinue(entry)}
              onAutoStart={(enabled) => onAutoStart(entry, enabled)}
              onArchive={(archived) => onArchive(entry, archived)}
              onRestart={() => onRestart(entry)}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

interface RailRowProps {
  run: GraduationRun;
  selected: boolean;
  position: string | null;
  busy: boolean;
  onSelect: (runId: string) => void;
  onPause: () => void;
  onContinue: () => void;
  onAutoStart: (enabled: boolean) => void;
  onArchive: (archived: boolean) => void;
  onRestart: () => void;
}

/**
 * GRH-FR-FYTH: one run, named for its source draft (or, for a merge run, by
 * its `Merge <stream>` title), with the stream it runs in, its state in words and — in a queue — its place in that queue.
 */
function RailRow({
  run,
  selected,
  position,
  busy,
  onSelect,
  onPause,
  onContinue,
  onAutoStart,
  onArchive,
  onRestart,
}: RailRowProps) {
  const filed = isArchived(run);
  return (
    <div
      role="listitem"
      className={
        selected ? "graduation__row graduation__row--selected" : "graduation__row"
      }
      data-archived={filed ? "true" : undefined}
    >
      <button
        type="button"
        className="graduation__row-open"
        // GRH-FR-ODLT: which run the region beside the rail is rendering.
        // `aria-current` rather than `aria-selected`, which is meaningful only
        // on an option, a tab or a row of a grid.
        aria-current={selected ? "true" : undefined}
        // The row's name is the run's title and nothing else — a draft's name,
        // or a merge run's `Merge <stream>` (GRU-FR-HDPQ): the state and the
        // stream beside it are about the run rather than what to call it.
        aria-label={runTitle(run)}
        onClick={() => onSelect(run.id)}
        data-testid="graduation-row"
      >
        <span className="graduation__name">{runTitle(run)}</span>
        <span className="graduation__row-meta">
          {/* GRH-FR-YMIM: a run holding a work stream stays in In-flight even
              when it is filed away, so the mark says which row is holding one.
              The word is on the row's state beside it; this is only what makes
              it visible at a glance. */}
          {holdsAStream(run) && (
            <span className="graduation__marker" aria-hidden="true">
              ▸
            </span>
          )}
          <span className="graduation__tag t-meta">{stateLabelOf(run)}</span>
          <span className="graduation__stream t-meta">{targetName(run)}</span>
          {position && <span className="graduation__matched">{position}</span>}
        </span>
      </button>
      <RowActions
        run={run}
        busy={busy}
        onPause={onPause}
        onContinue={onContinue}
        onAutoStart={onAutoStart}
        onArchive={onArchive}
        onRestart={onRestart}
      />
    </div>
  );
}
