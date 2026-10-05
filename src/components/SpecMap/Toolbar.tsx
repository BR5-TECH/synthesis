import type { Rollup } from "../../state/specMap/rollups";
import type { LevelName } from "../../state/specMap/types";
import { Icon } from "../icons";
import { CompletenessBar } from "./CompletenessBar";

interface ToolbarProps {
  levels: LevelName[];
  level: number;
  total: Rollup | null;
  showDeps: boolean;
  gapsOnly: boolean;
  /** SMP-FR-GJEW: the measured width of the Map tab. */
  width: number;
  /** SMP-FR-ONSD: no control acts while the map has nothing to show. */
  disabled: boolean;
  onLevel: (level: number) => void;
  onToggleDeps: () => void;
  onToggleGaps: () => void;
}

/**
 * SMP-FR-CVIB: the 38px toolbar above the canvas, left to right: the level
 * control, a divider, the completeness label, bar and counts, a spacer, and the
 * two toggles.
 */
export function Toolbar({
  levels,
  level,
  total,
  showDeps,
  gapsOnly,
  width,
  disabled,
  onLevel,
  onToggleDeps,
  onToggleGaps,
}: ToolbarProps) {
  // SMP-FR-GJEW: the two thresholds, against the tab's own width.
  const wide = width >= 1180;
  const labelled = width >= 1000;
  const verifiedPct =
    total && total.specs > 0 ? Math.round((total.verified / total.specs) * 100) : 0;

  return (
    <div className="smap-toolbar" role="toolbar" aria-label="Map controls">
      <div className="smap-levels" role="radiogroup" aria-label="Detail level">
        {levels.map((name, i) => (
          <button
            key={name.plural}
            type="button"
            role="radio"
            aria-checked={i === level}
            className="smap-levels__option"
            data-active={i === level || undefined}
            disabled={disabled}
            onClick={() => onLevel(i)}
          >
            {name.plural}
          </button>
        ))}
      </div>
      <span className="smap-toolbar__divider" aria-hidden="true" />
      {wide && <span className="smap-toolbar__label">Completeness</span>}
      {total && <CompletenessBar rollup={total} variant="project" narrow={!wide} />}
      {total && wide && (
        <span className="smap-toolbar__counts">
          {total.verified} verified · {total.built} built · {total.drafted} drafted · {total.gap} gaps
        </span>
      )}
      {total && !wide && labelled && (
        <span className="smap-toolbar__counts">
          {verifiedPct}% verified · {total.gap} gaps
        </span>
      )}
      <span className="smap-toolbar__spacer" />
      <button
        type="button"
        className="smap-filter"
        data-filter="deps"
        aria-pressed={showDeps}
        aria-label="Dependencies"
        title="Dependencies"
        disabled={disabled}
        onClick={onToggleDeps}
      >
        <Icon.Branch size={12} />
        {labelled && <span>dependencies</span>}
      </button>
      <button
        type="button"
        className="smap-filter"
        data-filter="gaps"
        aria-pressed={gapsOnly}
        aria-label="Gaps only"
        title="Gaps only"
        disabled={disabled}
        onClick={onToggleGaps}
      >
        <Icon.Warning size={12} />
        {labelled && <span>gaps only</span>}
      </button>
    </div>
  );
}
