/**
 * MPV-FR-PIQU .. MPV-FR-JHCG: the footer of the detail, by the status of the
 * row: the decisions, the outcome, or the failure and the re-prepare request.
 */
import { useId } from "react";

import * as api from "../../api";
import { logWarn } from "../../logging";
import type { MapPlanProposal, MapPlanRow } from "../../types";
import type { RowStatus } from "../../state/mapPlan/derive";
import { Icon } from "../icons";
import type { MapPlanHost } from "./host";
import { StatusWord } from "./parts";
import { blockWord, closedWord, errorText } from "./words";
import { rowTitle } from "../../state/mapPlan/derive";

export interface FooterActions {
  accept: () => void;
  reject: () => void;
  reopen: () => void;
  reprepare: () => void;
  discuss: () => void;
}

function openLink(url: string) {
  api.openTrackerLink(url).catch(() => {
    logWarn(["frontend", "remote"], "tracker link did not open", {});
  });
}

/** MPV-FR-PIQU: the outcome of an applied row. */
function Applied({ row, host }: { row: MapPlanRow; host: MapPlanHost | null }) {
  if (row.kind === "new_draft") {
    const draftId = row.result?.draftId;
    return (
      <div className="pp-out">
        <span>Created the draft</span>
        {draftId && host?.openDraft && (
          <button type="button" className="btn btn--ghost btn--sm" onClick={() => host.openDraft?.(draftId)}>
            Open draft
          </button>
        )}
      </div>
    );
  }
  const number = row.kind === "modify_issue" ? row.fields.number : row.result?.issueNumber;
  const url = row.result?.issueUrl;
  const ref = number === undefined ? null : `${host?.repository ? host.repository : ""}#${number}`;
  return (
    <div className="pp-out">
      <span>Applied to GitHub</span>
      {ref !== null &&
        (url ? (
          <a
            className="mono"
            href={url}
            onClick={(e) => {
              e.preventDefault();
              openLink(url);
            }}
          >
            {ref}
            <Icon.ExternalLink size={11} aria-hidden="true" />
          </a>
        ) : (
          <span className="mono">{ref}</span>
        ))}
    </div>
  );
}

export function RowFooter({
  proposal,
  row,
  status,
  refusal,
  reprepareRefusal,
  reprepareBusy,
  proposer,
  host,
  actions,
}: {
  proposal: MapPlanProposal;
  row: MapPlanRow;
  status: RowStatus;
  refusal: string | null;
  reprepareRefusal: string | null;
  reprepareBusy: boolean;
  proposer: string | null;
  host: MapPlanHost | null;
  actions: FooterActions;
}) {
  const reasonId = useId();
  const refusalLine = refusal ? (
    <div className="pp-err" role="alert">
      <Icon.ErrorCircle size={12} aria-hidden="true" />
      <span>{refusal}</span>
    </div>
  ) : null;

  if (status === "applying") {
    return (
      <div className="pp-out">
        <StatusWord status="applying" />
        <span>Sending to GitHub</span>
      </div>
    );
  }
  if (status === "applied") return <Applied row={row} host={host} />;
  if (status === "rejected") {
    return (
      <>
        {refusalLine}
        <div className="pp-act">
          <span className="pp-out">Rejected. Nothing was sent to GitHub.</span>
          <span className="pp-act__spacer" />
          <button type="button" className="btn btn--ghost btn--sm" onClick={actions.reopen}>
            Undo
          </button>
        </div>
      </>
    );
  }
  if (status === "failed") {
    const agent = proposer ? `@${proposer}` : "the agent";
    const pending = proposal.rows.some((r) => r.state === "waiting");
    return (
      <>
        <div className="pp-err" role="alert" data-testid="pp-failure">
          <Icon.ErrorCircle size={12} aria-hidden="true" />
          <span>{errorText(row)}</span>
        </div>
        {reprepareRefusal && (
          <div className="pp-err" role="alert">
            <span>{reprepareRefusal}</span>
          </div>
        )}
        <div className="pp-act">
          <span className="pp-act__spacer" />
          {proposal.reprepareRequestedAt ? (
            <span className="pp-out" data-testid="pp-reprepare-asked">
              {agent} was asked to prepare it again
            </span>
          ) : (
            <button
              type="button"
              className="btn btn--default btn--sm"
              disabled={pending || reprepareBusy}
              aria-describedby={pending ? reasonId : undefined}
              onClick={actions.reprepare}
            >
              Ask {agent} to re-prepare
            </button>
          )}
          {pending && !proposal.reprepareRequestedAt && (
            <span id={reasonId} className="pp-sr-only">
              Decide every pending change first.
            </span>
          )}
        </div>
      </>
    );
  }

  // MPV-FR-FUQG / MPV-FR-YEIH: a row blocked by its parent cannot be accepted.
  const word = blockWord(row);
  const parentKey = row.blockedBy?.rowKey;
  const parent = parentKey ? proposal.rows.find((r) => r.key === parentKey) : undefined;
  const blocked = word === null ? null : parent ? `${word}: ${rowTitle(parent)}` : word;
  const close = row.kind === "modify_issue" ? row.fields.changes.find((c) => c.field === "close") : undefined;
  return (
    <>
      {refusalLine}
      {blocked && (
        <div className="pp-out" id={reasonId}>
          {blocked}
        </div>
      )}
      <div className="pp-act">
        <button type="button" className="btn btn--ghost btn--sm btn--link" onClick={actions.discuss}>
          Discuss
          <Icon.ArrowRight size={12} aria-hidden="true" />
        </button>
        <span className="pp-act__spacer" />
        <button type="button" className="btn btn--default btn--sm" onClick={actions.reject}>
          Reject
        </button>
        <button
          type="button"
          className="btn btn--primary btn--sm"
          disabled={blocked !== null}
          aria-describedby={blocked ? reasonId : undefined}
          aria-label={close ? `Accept: ${closedWord(close)}` : undefined}
          onClick={actions.accept}
        >
          Accept
        </button>
      </div>
    </>
  );
}
