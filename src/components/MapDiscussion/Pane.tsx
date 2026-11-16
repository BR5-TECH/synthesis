/**
 * SMP-FR-WNLR / SMP-FR-SWCL: the splitter and the Discussion column, at the
 * right edge of the body of a lens. The split is the session's (SMP-FR-AUFA):
 * the presets, the accelerator, and the dragged splitter of the draft column
 * (`DDS-draft-discussion.md` DDS-FR-PNXR), with the canvas side as the
 * document column.
 */
import { useCallback, useEffect, useRef, useSyncExternalStore } from "react";

import { columnFraction } from "../../state/specMap/discussion";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import { Splitter } from "../DraftDiscussion/Splitter";
import { MapDiscussionColumn, type MapColumnProps } from "./Column";

/** SMP-FR-SWCL: the width of the column, as a share of the body of the lens. */
export function columnWidth(fraction: number): string {
  return `${Math.round(fraction * 1000) / 10}%`;
}

export function DiscussionPane(props: MapColumnProps & { store: SpecMapSessionStore }) {
  const { store } = props;
  useSyncExternalStore(store.subscribe, store.getVersion);
  const { visible, ratio } = store.snapshot().discussion;

  // SMP-FR-SWCL: the accelerator of DDS-FR-PNXR cycles the presets while the
  // Map tab is the active tab. Only the active tab is mounted, so one listener
  // answers at a time, and it answers nothing while the column is hidden.
  useEffect(() => {
    if (!visible) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "\\" || !(e.metaKey || e.ctrlKey) || e.altKey) return;
      e.preventDefault();
      store.cycleDiscussionRatio();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [store, visible]);

  // A drag is measured against the body of the lens, which holds the canvas
  // side and the column.
  const boundsRef = useRef<HTMLElement | null>(null);
  const bindColumn = useCallback((element: HTMLDivElement | null) => {
    boundsRef.current = element?.parentElement ?? null;
  }, []);

  if (!visible) return null;
  return (
    <>
      <Splitter
        ratio={ratio}
        onChange={store.setDiscussionRatio}
        boundsRef={boundsRef}
        className="map-discussion-splitter"
        testId="map-discussion-splitter"
        label="Resize the map and discussion columns"
        side="Map"
      />
      <div
        ref={bindColumn}
        className="map-discussion"
        style={{ width: columnWidth(columnFraction(ratio)) }}
        data-testid="map-discussion"
      >
        <MapDiscussionColumn {...props} />
      </div>
    </>
  );
}
