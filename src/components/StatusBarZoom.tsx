import { useEditorZoomPercent } from "../hooks/useEditorZoom";
import { setEditorZoom, zoomOptions } from "../state/editorZoom";

/**
 * STB-FR-VRUS / STB-FR-OQTE / STB-FR-LBHP: the zoom control of the status bar.
 *
 * It shows the editor zoom level as a whole percent and offers each zoom step
 * (`EZM-editor-zoom.md` EZM-FR-COYK). A level between two steps, from a pinch,
 * shows as one more option in its place. On a tab that is not zoomable
 * (EZM-FR-JGRP) it renders the neutral disabled state of the indentation
 * control (STB-FR-22).
 */
export function StatusBarZoom({ zoomable }: { zoomable: boolean }) {
  const level = useEditorZoomPercent();
  if (!zoomable) {
    return (
      <span
        className="status-bar__inert"
        data-testid="zoom-inert"
        aria-label="Zoom"
        aria-disabled="true"
      >
        Zoom
      </span>
    );
  }
  return (
    <select
      className="status-bar__select status-bar__select--zoom"
      data-testid="zoom-select"
      aria-label="Editor zoom"
      title={`Editor zoom — ${level}%`}
      value={String(level)}
      onChange={(e) => setEditorZoom(Number(e.target.value))}
    >
      {zoomOptions(level).map((option) => (
        <option key={option} value={String(option)}>
          {option}%
        </option>
      ))}
    </select>
  );
}
