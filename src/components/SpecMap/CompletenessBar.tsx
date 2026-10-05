import { segments, type Rollup } from "../../state/specMap/rollups";
import { COMPLETENESS_STATES } from "../../state/specMap/types";

export type BarVariant = "project" | "card" | "box" | "mini" | "hover" | "inspector" | "row";

export const completenessLabel = (r: Rollup): string =>
  `${r.verified} verified, ${r.built} built, ${r.drafted} drafted, ${r.gap} gaps`;

/**
 * SMN-FR-OJAY / SMN-FR-CMLN: four segments in state order, each as wide as its
 * share of the spec nodes. The variant sets only the size.
 */
export function CompletenessBar({
  rollup,
  variant,
  narrow = false,
}: {
  rollup: Rollup;
  variant: BarVariant;
  narrow?: boolean;
}) {
  const share = segments(rollup);
  return (
    <span
      className="smap-bar"
      data-variant={variant}
      data-narrow={narrow || undefined}
      role="img"
      aria-label={completenessLabel(rollup)}
    >
      {COMPLETENESS_STATES.map((state) => (
        <span
          key={state}
          className="smap-bar__segment"
          data-state={state}
          style={{ width: `${share[state]}%` }}
        />
      ))}
    </span>
  );
}

/** SMN-FR-QHVM: the counts row of a root card. The gaps count hides at zero. */
export function Counts({ rollup }: { rollup: Rollup }) {
  return (
    <div className="smap-counts">
      <span className="smap-count" data-state="verified">
        {rollup.verified} verified
      </span>
      <span className="smap-count" data-state="built">
        {rollup.built} built
      </span>
      <span className="smap-count" data-state="drafted">
        {rollup.drafted} drafted
      </span>
      <span className="smap-counts__spacer" />
      {rollup.gap > 0 && (
        <span className="smap-count" data-state="gap">
          {rollup.gap} gaps
        </span>
      )}
    </div>
  );
}
