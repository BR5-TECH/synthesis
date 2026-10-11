/**
 * The merge prompt of a temporary stream
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-YHCT,
 * WSS-FR-LWZB).
 *
 * It asks one question: merge this stream into its base branch now? It is a
 * floating overlay that opens without closing another, so the hook that shows
 * it waits until no other overlay is open. Its own elements carry
 * `data-merge-prompt`, so the check for other overlays does not count it.
 */

import { useEffect, useRef } from "react";

import type { WorkStreamSummary } from "../../types";
import { Icon } from "../icons";

export function MergePrompt({
  summary,
  onYes,
  onNo,
}: {
  summary: WorkStreamSummary;
  onYes: () => void;
  onNo: () => void;
}) {
  const yesRef = useRef<HTMLButtonElement>(null);
  const { stream } = summary;

  useEffect(() => {
    yesRef.current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onNo();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onNo]);

  return (
    <div
      className="scrim"
      role="presentation"
      data-merge-prompt=""
      onClick={(event) => {
        if (event.target === event.currentTarget) onNo();
      }}
    >
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="merge-prompt-title"
        data-testid="merge-prompt"
      >
        <div className="modal__head">
          <Icon.Graduate size={14} />
          <h2 className="modal__title" id="merge-prompt-title">
            Merge “{stream.name}”?
          </h2>
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Close"
            onClick={onNo}
          >
            <Icon.X size={12} />
          </button>
        </div>
        <div className="modal__body">
          <p className="t-ui-sm">
            All the work of the temporary stream “{stream.name}” is finished.
            Merge it into {stream.baseBranch} now? The stream is deleted after
            the merge.
          </p>
        </div>
        <div className="modal__actions">
          <button type="button" className="btn btn--ghost" onClick={onNo}>
            No
          </button>
          <button
            type="button"
            className="btn btn--primary"
            ref={yesRef}
            onClick={onYes}
          >
            Yes
          </button>
        </div>
      </div>
    </div>
  );
}
