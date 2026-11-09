/**
 * CAD-FR-LLNQ – CAD-FR-NNHM: the diagram viewer overlay on a CALM fence. Its
 * frame, focus trap, and closing are the ones of the Mermaid viewer
 * (`EDT-editor.md` EDT-FR-KMSI – EDT-FR-EAOC). Its content is the interactive
 * CALM canvas with the flow selector and the details panel.
 */
import { useEffect, useRef } from "react";

import { logDebug } from "../../logging";
import {
  CalmFlowSelect,
  CalmStage,
  CalmZoomControls,
  useCalmInteraction,
} from "../CalmDiagram/CalmInteractive";
import { useCalmRead, type RenderedCalm } from "../CalmDiagram/useCalmRead";
import type { CalmLayout } from "../../calm/types";
import { OverlayFrame } from "./OverlayFrame";
import type { DiagramEditorRequest } from "./store";
import { useFence } from "./useFence";

/** CAD-FR-LLNQ: the accessible name of the viewer on a CALM fence. */
export const CALM_VIEWER_LABEL = "View CALM architecture";

export function CalmViewerOverlay({ request }: { request: DiagramEditorRequest }) {
  const shownSource = useRef<string | null>(null);
  const waitingRef = useRef<HTMLParagraphElement>(null);
  const { source, finish } = useFence(request, () => shownSource.current);
  shownSource.current = source;
  const { empty, settled, lastRendered } = useCalmRead(source);

  // EDT-FR-JRDD: a source that stops being a valid diagram closes the viewer.
  const invalid = empty || settled?.state === "error";
  useEffect(() => {
    if (!invalid) return;
    logDebug(["frontend"], "the CALM viewer has no valid diagram", { empty });
    finish("not-a-diagram");
  }, [invalid, empty, finish]);

  // EDT-FR-LVFQ: while a changed source reads, the last diagram stays.
  const shown: RenderedCalm | null =
    settled?.state === "rendered" ? settled : invalid ? null : lastRendered;
  if (!shown) {
    return (
      <OverlayFrame
        label={CALM_VIEWER_LABEL}
        className="diagram-viewer calm-viewer"
        scrimTestId="diagram-viewer-scrim"
        home={waitingRef}
        onClose={finish}
      >
        <p ref={waitingRef} className="diagram-editor__statement" tabIndex={-1}>
          Reading the architecture…
        </p>
      </OverlayFrame>
    );
  }
  return <CalmViewerBody layout={shown.layout} finish={finish} />;
}

function CalmViewerBody({
  layout,
  finish,
}: {
  layout: CalmLayout;
  finish: ReturnType<typeof useFence>["finish"];
}) {
  const ix = useCalmInteraction(layout);

  // EDT-FR-ELUJ: focus moves into the diagram area.
  useEffect(() => {
    ix.areaRef.current?.focus();
  }, [ix.areaRef]);

  return (
    <OverlayFrame
      label={CALM_VIEWER_LABEL}
      className="diagram-viewer calm-viewer"
      scrimTestId="diagram-viewer-scrim"
      home={ix.areaRef}
      selectable=".calm-details"
      onClose={finish}
      controls={
        <>
          <CalmFlowSelect ix={ix} />
          <CalmZoomControls ix={ix} />
        </>
      }
    >
      <CalmStage ix={ix} />
    </OverlayFrame>
  );
}
