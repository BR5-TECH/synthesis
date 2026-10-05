import type { MouseEvent, ReactNode } from "react";

/**
 * The first-class empty state a vertical panel renders when it has nothing at
 * all to list (`../../specifications/ui/SNV-shell-navigation.md` SNV-FR-60).
 *
 * One block, composed in the same fixed order across every such surface: a
 * short line stating what the surface is empty of, a sentence naming what the
 * absent thing is, and at most one action — that surface's own primary
 * affordance, rendered as a button. It is centred both across the panel's width
 * and down the region the list would have occupied, and its sentence wraps on a
 * measure narrower than the panel, so an empty panel reads as a state that was
 * designed rather than as a run of text left in its top-left corner.
 *
 * One component rather than a block per panel, for the same reason
 * `.panel-controls` is one class: SNV-FR-60 is a claim about *every* vertical
 * panel, and five hand-rolled copies of it drift apart the moment one of them
 * is touched.
 *
 * A surface with nothing to offer in the state — a read-only one like Comments,
 * or one whose action would mean nothing there like Changes outside a
 * repository — passes no `action` and gets the block without one, rather than
 * with a disabled control.
 *
 * A list narrowed to nothing by a filter is **not** this state (SNV-FR-61): it
 * renders inside the list's own region with every control still present, which
 * is what [`PanelFilteredState`] below is for. Reaching for this component to
 * render a no-match message is the mistake SNV-FR-61 exists to name.
 */
export interface PanelEmptyStateProps {
  /** The short line stating what the surface is empty of. */
  line: string;
  /** The sentence naming what the absent thing is. */
  children: ReactNode;
  /**
   * The surface's own primary affordance. Omitted entirely by a surface that
   * has none — SNV-FR-60 asks for no action rather than a disabled one.
   */
  action?: {
    label: string;
    icon?: ReactNode;
    /** Given the click, so an action that opens a menu can anchor it to the button. */
    onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  };
}

export function PanelEmptyState({ line, children, action }: PanelEmptyStateProps) {
  return (
    <div className="panel-empty">
      <p className="panel-empty__line t-ui-sm">{line}</p>
      <p className="panel-empty__body t-ui-xs">{children}</p>
      {action && (
        <button
          type="button"
          className="btn btn--sm panel-empty__action"
          onClick={(event) => action.onClick(event)}
        >
          {action.icon}
          {action.label}
        </button>
      )}
    </div>
  );
}

/**
 * The class a panel puts on its `vpanel__body` while that body holds the block
 * above.
 *
 * The centring lives on the scroll container rather than on the block itself:
 * a percentage height on the block would overflow the container's padding and
 * raise a scrollbar over an empty panel, which is the one thing an empty panel
 * must not do.
 */
export const EMPTY_BODY_CLASS = "vpanel__body vpanel__body--empty";

/**
 * The state a vertical panel renders when its list holds entries and the
 * controls in the author's hands admit none of them (SNV-FR-61).
 *
 * Deliberately *not* [`PanelEmptyState`]: that one says nothing of the kind
 * exists yet and offers the way to make one, while this says the filters
 * already set admit nothing and leaves them above it to be changed. What the
 * two share is where they sit — centred across the panel's width and down the
 * region the list would have occupied — because a message dropped in the
 * region's top-left corner reads as a stray line of text rather than as the
 * answer to what the author just typed.
 *
 * One component rather than a line per panel, for the reason `PanelEmptyState`
 * is one: five surfaces render this state, and left to themselves they had
 * three different treatments of it.
 */
export function PanelFilteredState({ children }: { children: ReactNode }) {
  return <p className="panel-filtered t-ui-sm">{children}</p>;
}

/**
 * The class a panel puts on its `vpanel__body` while that body holds the
 * message above.
 *
 * The centring lives on the scroll container for the same reason
 * [`EMPTY_BODY_CLASS`]'s does: a percentage height on the message itself would
 * overflow the container's padding and raise a scrollbar over a panel that has
 * nothing to scroll.
 */
export const FILTERED_BODY_CLASS = "vpanel__body vpanel__body--filtered";
