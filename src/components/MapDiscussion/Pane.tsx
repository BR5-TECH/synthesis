/**
 * SMP-FR-WNLR / SMP-FR-SWCL: the splitter and the Discussion column, at the
 * right edge of the body of a lens. The width is the session's
 * (SMP-FR-AUFA), and the canvas takes the remaining width.
 */
import { useSyncExternalStore } from "react";

import type { SpecMapSessionStore } from "../../state/specMap/session";
import { MapDiscussionColumn, type MapColumnProps } from "./Column";
import { ColumnSplitter } from "./Splitter";

export function DiscussionPane(props: MapColumnProps & { store: SpecMapSessionStore }) {
  const { store } = props;
  useSyncExternalStore(store.subscribe, store.getVersion);
  const { visible, width } = store.snapshot().discussion;
  if (!visible) return null;
  return (
    <>
      <ColumnSplitter width={width} onWidth={store.setDiscussionWidth} />
      <div className="map-discussion" style={{ width }} data-testid="map-discussion">
        <MapDiscussionColumn {...props} />
      </div>
    </>
  );
}
