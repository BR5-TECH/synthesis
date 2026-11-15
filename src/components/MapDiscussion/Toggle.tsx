/**
 * SMP-FR-WNLR / SMP-FR-GJEW: the **Discussion** toggle of the Map toolbar. It
 * shows and hides the Discussion column. Below the width where the toggles show
 * only their icon, the name is its tooltip.
 */
import { useSyncExternalStore } from "react";

import type { SpecMapSessionStore } from "../../state/specMap/session";
import { Icon } from "../icons";

export function DiscussionToggle({
  store,
  labelled,
}: {
  store: SpecMapSessionStore;
  /** False where the toolbar shows icons only. */
  labelled: boolean;
}) {
  useSyncExternalStore(store.subscribe, store.getVersion);
  const shown = store.snapshot().discussion.visible;
  return (
    <button
      type="button"
      className="smap-filter"
      data-filter="discussion"
      aria-pressed={shown}
      aria-label="Discussion"
      title="Discussion"
      onClick={() => store.setDiscussionVisible(!shown)}
    >
      <Icon.Comment size={12} />
      {labelled && <span>discussion</span>}
    </button>
  );
}
