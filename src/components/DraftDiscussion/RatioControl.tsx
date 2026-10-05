/**
 * DDS-FR-PNXR: the three split presets, as one segmented control.
 *
 * A radio group rather than three buttons, because the three are one choice and
 * exactly one of them stands at a time. That is what makes it one stop in the
 * tab order with the arrow keys moving between the presets, which is how a
 * segmented control is expected to behave.
 */
import {
  PRESET_LABELS,
  PRESET_TITLES,
  RATIO_PRESETS,
  type SplitRatio,
} from "./ratio";

export function RatioControl({
  ratio,
  onChange,
}: {
  ratio: SplitRatio;
  onChange: (ratio: SplitRatio) => void;
}) {
  return (
    <div
      className="dds-ratio"
      role="radiogroup"
      aria-label="Split between the document and the discussion"
    >
      {RATIO_PRESETS.map((preset) => (
        <button
          key={preset}
          type="button"
          role="radio"
          className="dds-ratio__option t-ui-xs"
          // A dragged split matches no preset, so none of the three reads as
          // chosen — which is true, and better than rounding the drag onto
          // whichever preset it is nearest and claiming the author picked it.
          aria-checked={ratio === preset}
          data-active={ratio === preset}
          title={PRESET_TITLES[preset]}
          onClick={() => onChange(preset)}
        >
          {PRESET_LABELS[preset]}
        </button>
      ))}
    </div>
  );
}
