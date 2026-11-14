import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { Rollup } from "../../state/specMap/rollups";
import type { LevelName } from "../../state/specMap/types";
import { Icon } from "../icons";
import { CompletenessBar } from "./CompletenessBar";

/** SDL-FR-HZLB: the state of the **unplanned** toggle and its action. */
export interface UnplannedToggle {
  on: boolean;
  onToggle: () => void;
}

/**
 * SMP-FR-GJEW: the three forms of the toolbar, from the narrowest. `icons`
 * hides the counts and shows the toggles as icons, `short` hides the label,
 * narrows the bar and shortens the counts, `full` shows everything.
 */
type ToolbarForm = "icons" | "short" | "full";
const FORMS: ToolbarForm[] = ["icons", "short", "full"];

/** SMP-FR-GJEW: the form that the width of the Map tab gives. */
function formForWidth(width: number): ToolbarForm {
  if (width >= 1180) return "full";
  if (width >= 1000) return "short";
  return "icons";
}

interface ToolbarProps {
  /** SMP-FR-TKUS: the lens switch, at the start of the toolbar. */
  lensSwitch: ReactNode;
  levels: LevelName[];
  level: number;
  total: Rollup | null;
  showDeps: boolean;
  /** SME-FR-XLUQ: an OpenSpec index has no dependency, so no toggle shows. */
  depsToggle: boolean;
  gapsOnly: boolean;
  /** SDL-FR-HZLB: the **unplanned** toggle, or undefined where it does not show. */
  unplanned?: UnplannedToggle;
  /** SMP-FR-GJEW: the measured width of the Map tab. */
  width: number;
  /** SMP-FR-ONSD: no control acts while the map has nothing to show. */
  disabled: boolean;
  onLevel: (level: number) => void;
  onToggleDeps: () => void;
  onToggleGaps: () => void;
}

/**
 * SMP-FR-CVIB: the 38px toolbar above the canvas, left to right: the lens
 * switch, the level control, a divider, the completeness label, bar and
 * counts, a spacer, and the toggles.
 */
export function Toolbar({
  lensSwitch,
  levels,
  level,
  total,
  showDeps,
  depsToggle,
  gapsOnly,
  unplanned,
  width,
  disabled,
  onLevel,
  onToggleDeps,
  onToggleGaps,
}: ToolbarProps) {
  // SMP-FR-GJEW: the two thresholds, against the tab's own width, give the
  // widest form. The lens switch and the unplanned toggle can make that form
  // wider than the tab. Then the toolbar takes the next narrower form, so
  // that no control goes past the right edge (SMP-FR-CVIB).
  const ref = useRef<HTMLDivElement>(null);
  const fitKey = [
    width,
    levels.map((name) => name.plural).join(","),
    total ? [total.verified, total.built, total.drafted, total.gap, total.specs].join(",") : "",
    depsToggle,
    Boolean(unplanned),
  ].join("|");
  const [fit, setFit] = useState<{ key: string; index: number }>({ key: "", index: 2 });
  const widest = FORMS.indexOf(formForWidth(width));
  const index = fit.key === fitKey ? Math.min(fit.index, widest) : widest;
  const form = FORMS[index];
  useLayoutEffect(() => {
    const el = ref.current;
    // Measure before paint: a form that does not fit is never shown.
    if (!el || index === 0 || el.scrollWidth <= el.clientWidth) return;
    setFit({ key: fitKey, index: index - 1 });
  }, [fitKey, index]);
  const wide = form === "full";
  const labelled = form !== "icons";
  const verifiedPct =
    total && total.specs > 0 ? Math.round((total.verified / total.specs) * 100) : 0;

  return (
    <div ref={ref} className="smap-toolbar" role="toolbar" aria-label="Map controls" data-form={form}>
      {lensSwitch}
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
      {depsToggle && (
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
      )}
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
      {unplanned && (
        <button
          type="button"
          className="smap-filter"
          data-filter="unplanned"
          aria-pressed={unplanned.on}
          aria-label="Unplanned"
          title="Unplanned"
          disabled={disabled}
          onClick={unplanned.onToggle}
        >
          <Icon.Flag size={12} />
          {labelled && <span>unplanned</span>}
        </button>
      )}
    </div>
  );
}
