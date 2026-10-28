/**
 * DDS-FR-PNXR / DDS-FR-XQMF: the presets, the accelerator hint, and the control
 * that hides and shows the discussion column. The New Artifact tab sets them in
 * its action row, and the Editor tab at the trailing end of its toolbar
 * (`EDT-editor.md` EDT-FR-16).
 */
import { Icon } from "../icons";
import { RatioControl } from "./RatioControl";
import type { SplitRatio } from "./ratio";

export function SplitControls({
  ratio,
  hidden,
  onRatioChange,
  onToggle,
  toggleTestId,
}: {
  ratio: SplitRatio;
  hidden: boolean;
  onRatioChange: (ratio: SplitRatio) => void;
  onToggle: () => void;
  /** The test id the toggle carries in its host tab. */
  toggleTestId: string;
}) {
  return (
    <>
      {/* DDS-FR-PNXR: the three presets, and the accelerator that cycles them.
          They arrange two columns, so they stand down while one of them is
          hidden. */}
      {!hidden && (
        <>
          <RatioControl ratio={ratio} onChange={onRatioChange} />
          <kbd className="dds-kbd t-meta" title="Cycle the split">
            ⌘\
          </kbd>
        </>
      )}

      {/* DDS-FR-XQMF: the way to the whole window for the document, and the
          way back to the conversation. One control in both states, so hiding
          the column never puts the discussion out of reach. */}
      <button
        type="button"
        className="btn btn--ghost btn--icon-sm dds-discussion-toggle"
        aria-pressed={!hidden}
        data-testid={toggleTestId}
        title={hidden ? "Show the discussion" : "Hide the discussion"}
        aria-label={hidden ? "Show the discussion" : "Hide the discussion"}
        onClick={onToggle}
      >
        <Icon.Comment size={13} />
      </button>
    </>
  );
}
