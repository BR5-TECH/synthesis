/**
 * SMP-FR-DUUU: the **Focus** chip of the composer. It holds the node the next
 * message is about, **Use selected node** to take the node selected on the
 * canvas of the active lens, and a control that clears the focus.
 */
import type { MapNodeRef } from "../../types";
import { sameMapNode } from "../../state/specMap/discussion";
import { Icon } from "../icons";

export function FocusChip({
  focus,
  selected,
  onUse,
  onClear,
}: {
  focus: MapNodeRef | null;
  /** The node selected on the canvas of the active lens, or null. */
  selected: MapNodeRef | null;
  onUse: (node: MapNodeRef) => void;
  onClear: () => void;
}) {
  const usable = selected !== null && !sameMapNode(selected, focus);
  return (
    <div className="mdc-focus" role="group" aria-label="Focus" data-testid="map-focus">
      <span className="mdc-focus__label">Focus</span>
      {focus ? (
        <span className="mdc-focus__chip" data-testid="map-focus-chip">
          <Icon.Node size={11} />
          <span className="mdc-focus__name">{focus.label}</span>
          <button
            type="button"
            className="mdc-focus__clear"
            aria-label="Clear the focus"
            title="Clear the focus"
            onClick={onClear}
          >
            <Icon.X size={10} />
          </button>
        </span>
      ) : (
        <span className="mdc-focus__none">The whole map</span>
      )}
      <button
        type="button"
        className="btn btn--ghost btn--sm mdc-focus__use"
        disabled={!usable}
        title={selected === null ? "Select a node on the canvas first" : undefined}
        onClick={() => selected && onUse(selected)}
      >
        Use selected node
      </button>
    </div>
  );
}
