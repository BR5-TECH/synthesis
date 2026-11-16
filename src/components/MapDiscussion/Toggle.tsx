/**
 * SMP-FR-WNLR / SMP-FR-GJEW: the **Discussion** toggle of the Map toolbar. It
 * shows and hides the Discussion column. Below the width where the toggles show
 * only their icon, the name is its tooltip.
 *
 * SMP-FR-VDND: the split presets and the accelerator hint of the draft column
 * (`DDS-draft-discussion.md` DDS-FR-PNXR) lead the toggle while the column
 * shows, and stand down while it is hidden. A narrow tab hides the hint, and a
 * narrower one the presets too, so the toggle stays in view.
 */
import { useSyncExternalStore } from "react";

import type { SpecMapSessionStore } from "../../state/specMap/session";
import { RatioControl } from "../DraftDiscussion/RatioControl";
import { Icon } from "../icons";

export function DiscussionToggle({
  store,
  labelled,
  presets = true,
  hint = true,
}: {
  store: SpecMapSessionStore;
  /** False where the toolbar shows icons only. */
  labelled: boolean;
  /** SMP-FR-VDND: false below a Map tab width of 1000px. */
  presets?: boolean;
  /** SMP-FR-VDND: false below a Map tab width of 1180px. */
  hint?: boolean;
}) {
  useSyncExternalStore(store.subscribe, store.getVersion);
  const { visible: shown, ratio } = store.snapshot().discussion;
  return (
    <>
      {shown && presets && (
        <span className="mdc-split" data-testid="map-discussion-split">
          <RatioControl ratio={ratio} onChange={store.setDiscussionRatio} />
          {hint && (
            <kbd className="dds-kbd t-meta" title="Cycle the split">
              ⌘\
            </kbd>
          )}
        </span>
      )}
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
    </>
  );
}
