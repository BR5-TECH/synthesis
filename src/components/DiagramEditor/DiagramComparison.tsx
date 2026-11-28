/**
 * The comparison pop-up (`EDT-editor.md` EDT-FR-BRVQ – EDT-FR-PQAS;
 * `DCR-draft-change-review.md` DCR-FR-ESIY – DCR-FR-UKMP).
 *
 * A third diagram overlay beside the editor and the viewer. It shows the
 * diagram of the document and the diagram of one diagram hunk side by side, and
 * holds the decision controls of that hunk. It edits no fence and holds no
 * source pane: opening, closing, zooming, and panning are view state, so it
 * changes no buffer, marks nothing as changed, and adds no undo step.
 *
 * The review that owns the hunk gives it the content and gives it again when
 * the hunk changes (DCR-FR-GAFN), so the pop-up is mounted once per opening and
 * keeps the focus where it was while the content is replaced.
 */
import { useEffect, useRef, useState, useSyncExternalStore } from "react";

import { logDebug } from "../../logging";
import {
  CURRENT_HEADING,
  CURRENT_PANE_LABEL,
  ComparisonPane,
  DIAGRAM_REMOVED,
  NO_DIAGRAM_YET,
  PROPOSED_HEADING,
  PROPOSED_PANE_LABEL,
} from "./ComparisonPane";
import { OverlayFrame } from "./OverlayFrame";
import {
  closeComparison,
  comparisonRequest,
  holdComparison,
  isComparisonHeld,
  subscribeComparison,
  type ComparisonRequest,
} from "./store";

/** EDT-FR-NZLC: the accessible name of the pop-up. */
export const DIAGRAM_COMPARISON_LABEL = "Compare Mermaid diagrams";

/** Each window of the main shell mounts one host. */
export function DiagramComparisonHost() {
  const request = useSyncExternalStore(subscribeComparison, comparisonRequest);
  if (!request) return null;
  return <DiagramComparisonOverlay key={request.id} request={request} />;
}

function DiagramComparisonOverlay({ request }: { request: ComparisonRequest }) {
  // The store replaces the request at each update of the content.
  const { content } = request;
  const bodyRef = useRef<HTMLDivElement>(null);
  const acceptRef = useRef<HTMLButtonElement>(null);
  const footRef = useRef<HTMLDivElement>(null);
  const homeRef = useRef<HTMLElement | null>(null);
  const [pending, setPending] = useState<"accept" | "reject" | null>(null);
  const mounted = useRef(true);

  const close = (reason: "close" | "escape") => closeComparison(reason);

  const closeButton = () =>
    bodyRef.current
      ?.closest<HTMLElement>('[role="dialog"]')
      ?.querySelector<HTMLElement>('button[aria-label="Close"]') ?? null;

  // The first decision control takes the focus when the pop-up opens, else the
  // Close control (EDT-FR-HCWP). It is also where a key that reaches the window
  // outside the pop-up sends the focus back.
  useEffect(() => {
    const first = footRef.current?.querySelector<HTMLElement>("button:not([disabled])");
    (first ?? closeButton())?.focus();
    // Opened once; later content does not move the focus (DCR-FR-GAFN).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  useEffect(() => {
    homeRef.current = acceptRef.current ?? closeButton();
  });

  // DCR-FR-UKMP, EDT-FR-HCWP: focus returns to the Compare button, and to the
  // document column where the button is gone because the hunk is decided.
  const latest = useRef(request);
  latest.current = request;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      const { opener, fallback } = latest.current;
      const target = opener?.isConnected ? opener : fallback?.isConnected ? fallback : null;
      if (!target) return;
      if (target === fallback && !target.hasAttribute("tabindex")) {
        target.setAttribute("tabindex", "-1");
      }
      target.focus({ preventScroll: true });
    };
  }, []);

  /** DCR-FR-OLDZ: the decision of the chip, then the close on success. */
  const decide = async (how: "accept" | "reject") => {
    if (pending !== null) return;
    setPending(how);
    // PCR-FR-TNQA: the confirmation of a write opens above the pop-up, and the
    // pop-up stays under it.
    holdComparison(true);
    let decided = false;
    try {
      decided = await (how === "accept" ? content.onAccept() : content.onReject());
    } catch {
      logDebug(["frontend"], "a decision of the diagram comparison failed", { how });
    } finally {
      if (isComparisonHeld()) holdComparison(false);
      if (mounted.current) setPending(null);
    }
    // A refused decision keeps the pop-up open, with its error beside it.
    if (decided && comparisonRequest()?.id === request.id) closeComparison("decided");
  };

  const off = content.busy || pending !== null;
  const blocked = content.acceptBlocked;

  return (
    <OverlayFrame
      label={DIAGRAM_COMPARISON_LABEL}
      className="diagram-comparison"
      scrimTestId="diagram-comparison-scrim"
      home={homeRef}
      suspended={isComparisonHeld}
      onClose={close}
    >
      <div ref={bodyRef} className="diagram-comparison__body">
        <div className="diagram-comparison__panes">
          <ComparisonPane
            label={CURRENT_PANE_LABEL}
            heading={CURRENT_HEADING}
            source={content.oldSource}
            emptyStatement={NO_DIAGRAM_YET}
          />
          <ComparisonPane
            label={PROPOSED_PANE_LABEL}
            heading={PROPOSED_HEADING}
            source={content.newSource}
            emptyStatement={DIAGRAM_REMOVED}
          />
        </div>
        <div ref={footRef} className="diagram-comparison__foot">
          {content.error !== null && (
            // DCR-FR-16: the typed error beside the controls.
            <span className="diagram-comparison__error t-ui-xs" role="alert">
              {content.error}
            </span>
          )}
          {blocked !== null && (
            <span className="diagram-comparison__note t-ui-xs" role="status">
              {blocked}
            </span>
          )}
          <button
            ref={acceptRef}
            type="button"
            className="btn btn--primary btn--sm"
            disabled={off || blocked !== null}
            title={blocked ?? "Accept this edit"}
            onClick={() => void decide("accept")}
          >
            Accept
          </button>
          {content.canReject && (
            <button
              type="button"
              className="btn btn--sm"
              disabled={off}
              title="Reject this edit"
              onClick={() => void decide("reject")}
            >
              Reject
            </button>
          )}
        </div>
      </div>
    </OverlayFrame>
  );
}
