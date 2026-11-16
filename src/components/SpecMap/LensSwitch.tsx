import type { MapLens } from "../../state/specMap/session";

const LENSES: { lens: MapLens; label: string }[] = [
  { lens: "delivery", label: "Delivery" },
  { lens: "specifications", label: "Specifications" },
];

/**
 * SMP-FR-TKUS: the two-way lens switch at the start of the toolbar of each
 * lens. While the default lens is not known, no option is checked, and a
 * click still chooses a lens. It is the segmented control of the design
 * system (MDL layout notes).
 */
export function LensSwitch({
  lens,
  onLens,
}: {
  lens: MapLens | null;
  onLens: (lens: MapLens) => void;
}) {
  return (
    <div className="dm-seg smap-lens" role="radiogroup" aria-label="Lens">
      {LENSES.map((option) => (
        <button
          key={option.lens}
          type="button"
          role="radio"
          aria-checked={lens === option.lens}
          className="dm-seg__btn"
          data-active={lens === option.lens || undefined}
          onClick={() => onLens(option.lens)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
