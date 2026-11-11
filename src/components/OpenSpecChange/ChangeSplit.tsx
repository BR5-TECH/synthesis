/**
 * The split of a change in its tab
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-BGDN,
 * OCW-FR-ANKT, OCW-FR-TZRT).
 *
 * The tab follows `"new change proposals changed"` for its change, offers
 * Confirm split once the split holds an accepted proposal and none pending,
 * and names the changes a confirmed split made.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { onNewChangeProposalsChanged } from "../../events";
import { logInfo, logWarn } from "../../logging";
import { openOpenSpecChange } from "../../state/openspec/changeRoutes";
import {
  noteChangeSplit,
  noteNewChangeChanged,
  registerNewChangeDiscussRoute,
  rereadNewChanges,
  useNewChangeProposals,
} from "../../state/openspec/newChangeProposals";
import type { NewChangeProposal } from "../../types";
import { Modal } from "../DraftsPanelParts";
import { refusalCode } from "./decisions";
import { newChangeMessage } from "./newChangeDecisions";

/**
 * OCW-FR-BGDN: follow the split of one change while its tab is mounted, and
 * lend the cards of its discussion column the tab's Discuss.
 */
export function useChangeSplit(
  streamId: string,
  change: string,
  discuss: (proposal: NewChangeProposal) => void,
) {
  const reading = useNewChangeProposals(streamId, change);

  useEffect(() => registerNewChangeDiscussRoute(streamId, change, discuss), [streamId, change, discuss]);

  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    void onNewChangeProposalsChanged((payload) => {
      if (payload.streamId !== streamId || payload.change !== change) return;
      logInfo(["frontend"], "the split of the open change moved on", {
        streamId,
        cause: payload.cause,
      });
      noteNewChangeChanged(payload);
      void rereadNewChanges(streamId, change);
    })
      .then((fn) => {
        if (cancelled) fn();
        else stop = fn;
      })
      .catch((e: unknown) => {
        logWarn(["frontend"], "could not follow the split of a change", {
          streamId,
          error: refusalCode(e).slice(0, 80),
        });
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [streamId, change]);

  return reading;
}

/** The names the accepted proposals created, oldest first. */
function acceptedNames(proposals: readonly NewChangeProposal[]): string[] {
  return proposals
    .filter((p) => p.state === "accepted")
    .sort((a, b) => (a.decidedAt ?? a.createdAt).localeCompare(b.decidedAt ?? b.createdAt))
    .map((p) => p.createdChange ?? p.name);
}

/** OCW-FR-TZRT: the marker of a confirmed split, in the header. */
export function SplitMarker({ streamId, names }: { streamId: string; names: readonly string[] }) {
  return (
    <span className="openspec-change__split-marker" role="note" data-testid="openspec-split-marker">
      Split into{" "}
      {names.map((name, at) => (
        <span key={name}>
          {at > 0 && ", "}
          <button
            type="button"
            className="btn btn--ghost btn--sm openspec-change__split-link"
            onClick={() => openOpenSpecChange({ kind: "stream", streamId }, name)}
          >
            {name}
          </button>
        </span>
      ))}
    </span>
  );
}

/** OCW-FR-ANKT: Confirm split, with its confirmation that names the changes. */
export function ConfirmSplitBar({
  streamId,
  change,
  proposals,
}: {
  streamId: string;
  change: string;
  proposals: readonly NewChangeProposal[];
}) {
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const opener = useRef<HTMLButtonElement | null>(null);
  const names = acceptedNames(proposals);

  // A refusal keeps the bar. The confirmation closed while the button was
  // disabled, so the focus is on the document body: give it back to the
  // button.
  useEffect(() => {
    if (error === null || busy) return;
    const active = document.activeElement;
    if (active === null || active === document.body) opener.current?.focus();
  }, [error, busy]);

  const confirm = () => {
    setConfirming(false);
    setBusy(true);
    setError(null);
    api.confirmChangeSplit(streamId, change).then(
      (split) => {
        setBusy(false);
        noteChangeSplit(streamId, change, split);
        logInfo(["frontend"], "the split of a change was confirmed", {
          streamId,
          accepted: split.accepted.length,
        });
        void rereadNewChanges(streamId, change);
      },
      (e: unknown) => {
        const code = refusalCode(e);
        setBusy(false);
        setError(newChangeMessage(code));
        logWarn(["frontend"], "the split of a change could not be confirmed", {
          streamId,
          code: code.slice(0, 80),
        });
      },
    );
  };

  return (
    <div className="openspec-review openspec-split" role="region" aria-label="Split" data-testid="openspec-split-bar">
      <span className="openspec-review__count t-ui-sm">
        The split has {names.length === 1 ? "1 accepted change" : `${names.length} accepted changes`}:{" "}
        {names.join(", ")}.
      </span>
      <span className="spacer" />
      <button
        ref={opener}
        type="button"
        className="btn btn--primary btn--sm"
        disabled={busy}
        onClick={() => setConfirming(true)}
      >
        Confirm split
      </button>
      {error !== null && (
        <p className="openspec-review__error t-ui-xs" role="alert">
          {error}
        </p>
      )}
      {confirming && (
        <Modal label="Confirm split" returnFocus={opener.current} onClose={() => setConfirming(false)}>
          <div className="modal__head">
            <div className="modal__title">Confirm split</div>
          </div>
          <div className="modal__body">
            <p className="t-ui-sm" data-testid="openspec-confirm-split-statement">
              Confirm that {change} is split into {names.join(", ")}? No more change is
              proposed in this split, and {change} itself is not changed.
            </p>
          </div>
          <div className="modal__actions">
            <button type="button" className="btn btn--ghost" onClick={() => setConfirming(false)}>
              Cancel
            </button>
            <button type="button" className="btn btn--primary" onClick={confirm}>
              Confirm split
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}

/** OCW-FR-ANKT: whether the tab offers Confirm split. */
export function offersConfirmSplit(
  proposals: readonly NewChangeProposal[],
  confirmed: boolean,
): boolean {
  return (
    !confirmed &&
    proposals.some((p) => p.state === "accepted") &&
    !proposals.some((p) => p.state === "pending")
  );
}
