/**
 * Change information
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-ZZCS).
 *
 * A centred modal of one change's statistics, on the terms of the draft
 * Information modal (`../DraftInformationModal.tsx`): every figure comes from
 * the one read, an unavailable figure is stated as not captured, an incomplete
 * one carries the partial marker (DFI-FR-HSYB), a failed read is stated with a
 * retry and keeps the figures before it (DFI-FR-CVAX), the footer states where
 * the record begins (DFI-FR-NTPG), and dismissing it changes nothing
 * (DFI-FR-EBIL). It re-reads on each `"change statistics changed"` for its
 * change. Run time and Run tokens count the apply and rework runs on it.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { onChangeStatisticsChanged } from "../../events";
import { logWarn } from "../../logging";
import type { ChangeStatistics } from "../../types";
import { formatBoundary, formatCount, formatDuration, Row } from "../DraftInformationModal";
import { Modal } from "../DraftsPanelParts";
import { Icon } from "../icons";
import { refusalCode } from "./decisions";

/** DFI-FR-HSYB: a figure the read does not carry is one that was not captured. */
const NOT_CAPTURED = { value: null, availability: "unavailable" } as const;

export function ChangeInformationModal({
  streamId,
  change,
  returnFocus,
  onClose,
}: {
  streamId: string;
  change: string;
  returnFocus?: HTMLElement | null;
  onClose: () => void;
}) {
  const [statistics, setStatistics] = useState<ChangeStatistics | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  /** The newest read; an older answer that lands later is dropped. */
  const seq = useRef(0);

  const read = useCallback(async () => {
    const mine = ++seq.current;
    try {
      const next = await api.readChangeStatistics(streamId, change);
      if (mine !== seq.current) return;
      setStatistics(next);
      setFailure(null);
    } catch (e) {
      if (mine !== seq.current) return;
      // DFI-FR-CVAX: a failed read is stated, with a retry, and the figures
      // before it stay.
      const code = refusalCode(e);
      logWarn(["frontend"], "a change's statistics could not be read", {
        streamId,
        code: code.slice(0, 80),
      });
      setFailure(code);
    }
  }, [streamId, change]);

  useEffect(() => {
    void read();
  }, [read]);

  // OCW-FR-ZZCS / DFI-FR-ZGBU terms: re-read on each event for this change.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onChangeStatisticsChanged((payload) => {
      if (payload.streamId === streamId && payload.change === change) void read();
    })
      .then((off) => {
        if (cancelled) off();
        else unlisten = off;
      })
      .catch((e: unknown) =>
        logWarn(["frontend"], "a change's statistics subscription was refused", {
          streamId,
          reason: refusalCode(e).slice(0, 80),
        }),
      );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [streamId, change, read]);

  return (
    <Modal label={`Information for ${change}`} returnFocus={returnFocus} onClose={onClose} className="draft-info">
      <div className="modal__head">
        <div className="modal__title">{change}</div>
        <button className="btn btn--icon" aria-label="Close" onClick={onClose}>
          <Icon.X size={12} />
        </button>
      </div>
      <div className="modal__body draft-info__body" data-testid="change-information">
        {failure && (
          <div className="draft-info__failure" role="alert">
            <span className="t-ui-sm">This change's statistics could not be read.</span>
            <button className="btn btn--ghost btn--sm" onClick={() => void read()}>
              Retry
            </button>
          </div>
        )}
        {statistics && (
          <>
            <Row label="Working time" measure={statistics.workingTimeMs} format={formatDuration} />
            <Row label="AI interactions" measure={statistics.aiInteractions} format={formatCount} />
            <Row label="Versions" measure={statistics.versions} format={formatCount} />
            <Row label="Accepted proposals" measure={statistics.acceptedProposals} format={formatCount} />
            <Row label="Rejected proposals" measure={statistics.rejectedProposals} format={formatCount} />
            <div className="draft-info__group t-ui-sm">Conversation tokens</div>
            <Row label="Input" measure={statistics.conversationTokens.input} format={formatCount} indent />
            <Row label="Output" measure={statistics.conversationTokens.output} format={formatCount} indent />
            {/* OCW-FR-ZZCS / CST-FR-HJGG / CST-FR-CGRU: the agent time and the
                tokens of the apply and rework runs on the change. A record
                from before these figures reads as not captured. The run
                figures stand apart from the conversation tokens above them. */}
            <div className="draft-info__section" data-testid="change-information-runs">
              <Row label="Run time" measure={statistics.runTimeMs ?? NOT_CAPTURED} format={formatDuration} />
            </div>
            <div className="draft-info__group t-ui-sm">Run tokens</div>
            <Row label="Input" measure={statistics.runTokens?.input ?? NOT_CAPTURED} format={formatCount} indent />
            <Row label="Output" measure={statistics.runTokens?.output ?? NOT_CAPTURED} format={formatCount} indent />
            {/* DFI-FR-NTPG: where the captured record begins, or that it is empty. */}
            <div className="draft-info__footer t-ui-xs">
              {statistics.boundaryAt
                ? `Statistics cover activity captured since ${formatBoundary(statistics.boundaryAt)}.`
                : "Nothing has been captured for this change yet."}
            </div>
          </>
        )}
      </div>
    </Modal>
  );
}
