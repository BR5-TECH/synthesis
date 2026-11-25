/**
 * EDT-FR-JFGN: the legend toggle of a diagram overlay. It hides and shows the
 * syntax hints of the diagram editor overlay (EDT-FR-UOXS), or the details
 * panel of a CALM interactive canvas (`CAD-calm-architecture-diagrams.md`
 * CAD-FR-RLDM, CAD-FR-XHZP). All of them share one legend state (EDT-FR-MSCV).
 */
import { Icon } from "../icons";
import { setDiagramLegendHidden, useDiagramLegendHidden } from "../../state/diagramLegend";

/** EDT-FR-UOXS: the names of the toggle of the diagram editor overlay. */
export const HIDE_HINTS_LABEL = "Hide syntax hints";
export const SHOW_HINTS_LABEL = "Show syntax hints";
/** CAD-FR-XHZP: the names of the toggle of the CALM interactive canvas. */
export const HIDE_DETAILS_LABEL = "Hide details";
export const SHOW_DETAILS_LABEL = "Show details";

export function LegendToggle({ legend }: { legend: "hints" | "details" }) {
  const hidden = useDiagramLegendHidden();
  const label =
    legend === "hints"
      ? hidden
        ? SHOW_HINTS_LABEL
        : HIDE_HINTS_LABEL
      : hidden
        ? SHOW_DETAILS_LABEL
        : HIDE_DETAILS_LABEL;
  const Glyph = legend === "hints" ? Icon.PanelBottom : Icon.PanelRight;
  return (
    <button
      type="button"
      className="btn btn--ghost btn--icon btn--sm diagram-legend-toggle"
      aria-label={label}
      title={label}
      aria-pressed={!hidden}
      onClick={() => setDiagramLegendHidden(!hidden)}
    >
      <Glyph size={12} />
    </button>
  );
}
