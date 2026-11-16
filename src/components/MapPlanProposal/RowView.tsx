/**
 * MPV-FR-OCYF .. MPV-FR-QOPR, MPV-FR-XAOG: one row of the plan proposal card.
 * A row is a group named by its kind, its title, and its state. It decides
 * alone: its two controls act on this row and no other.
 */
import { useId } from "react";

import * as api from "../../api";
import { logWarn } from "../../logging";
import { Icon } from "../icons";
import type { MapPlanRow } from "../../types";
import { ChangeView } from "./ChangeView";
import { useMapPlanHost } from "./host";
import {
  blockWord,
  closeChangeOf,
  closeReasonWord,
  errorText,
  kindWord,
  plainValue,
  rowTitle,
  stateWord,
} from "./words";

export interface RowViewProps {
  row: MapPlanRow;
  /** MPV-FR-FUQG: the title of the parent row of the same card, or null. */
  parentTitle: string | null;
  busy: boolean;
  /** The typed text of a refused call for this row, or null. */
  refusal: string | null;
  onAccept: () => void;
  onReject: () => void;
}

function CreateFacts({ row, parentTitle }: { row: MapPlanRow & { kind: "create_issue" }; parentTitle: string | null }) {
  const f = row.fields;
  const facts: { name: string; value: string }[] = [];
  if (f.type) facts.push({ name: "Type", value: f.type });
  if (f.milestone) facts.push({ name: "Milestone", value: f.milestone.title });
  if (f.parent) {
    facts.push({
      name: "Parent",
      value: "issue" in f.parent ? `#${f.parent.issue}` : (parentTitle ?? "another row"),
    });
  }
  if (f.labels.length > 0) facts.push({ name: "Labels", value: f.labels.join(", ") });
  return (
    <>
      {facts.length > 0 && (
        <ul className="mpv-facts">
          {facts.map((fact) => (
            <li key={fact.name} className="mpv-fact">
              <span className="mpv-fact__name">{fact.name}</span> {fact.value}
            </li>
          ))}
        </ul>
      )}
      {f.body ? (
        <details className="mpv-body">
          <summary>Body</summary>
          <div className="mpv-body__text">{f.body}</div>
        </details>
      ) : null}
    </>
  );
}

/** MPV-FR-NMYT: the item a change row names, a chip where the item is on the canvas. */
function ItemChip({ number }: { number: number }) {
  const host = useMapPlanHost();
  const label = `#${number}`;
  if (host === null || !host.itemOnCanvas(number)) {
    return <span className="mpv-item">{label}</span>;
  }
  return (
    <button
      type="button"
      className="mpv-item mpv-item--chip"
      aria-label={`Select issue ${label} on the canvas`}
      title="Select on the canvas"
      onClick={() => host.selectItem(number)}
    >
      {label}
    </button>
  );
}

/** MPV-FR-FTXV: what an accepted row produced. */
function Outcome({ row }: { row: MapPlanRow }) {
  const host = useMapPlanHost();
  const result = row.result;
  if (row.state !== "accepted" || result === null) return null;
  if (row.kind === "create_issue" && result.issueNumber !== undefined) {
    const label = `#${result.issueNumber}`;
    const url = result.issueUrl;
    return (
      <p className="mpv-outcome">
        Created{" "}
        {url ? (
          <a
            href={url}
            className="mpv-outcome__link"
            onClick={(e) => {
              e.preventDefault();
              api.openTrackerLink(url).catch(() => {
                logWarn(["frontend", "remote"], "tracker link did not open", {});
              });
            }}
          >
            {label}
          </a>
        ) : (
          <span>{label}</span>
        )}
      </p>
    );
  }
  if (row.kind === "new_draft" && result.draftId !== undefined) {
    const draftId = result.draftId;
    return (
      <p className="mpv-outcome">
        Draft created
        {host?.openDraft && (
          <>
            {" "}
            <button type="button" className="mpv-outcome__link" onClick={() => host.openDraft?.(draftId)}>
              Open draft
            </button>
          </>
        )}
      </p>
    );
  }
  return null;
}

export function RowView({ row, parentTitle, busy, refusal, onAccept, onReject }: RowViewProps) {
  const ids = useId();
  const title = rowTitle(row);
  const word = stateWord(row);
  const blocked = blockWord(row);
  const close = closeChangeOf(row);
  const waiting = row.state === "waiting";
  const reasonId = `${ids}-reason`;
  const blockText = blocked === null ? null : `${blocked}${parentTitle && row.blockedBy?.reason === "waiting" ? `: ${parentTitle}` : ""}`;
  const acceptName = close ? `Accept, close as ${closeReasonWord(close)}` : "Accept";
  return (
    <li
      className="mpv-row"
      role="group"
      aria-label={`${kindWord(row)} ${title}, ${word}`}
      data-state={row.state}
      data-kind={row.kind}
      data-blocked={blocked !== null || undefined}
      data-busy={busy || undefined}
      aria-busy={busy || undefined}
    >
      <div className="mpv-row__head">
        <span className="mpv-row__kind">{kindWord(row)}</span>
        {row.kind === "modify_issue" && <ItemChip number={row.fields.number} />}
        <span className="mpv-row__title">{title}</span>
        <span className="mpv-row__state" data-state={row.state} data-blocked={blocked !== null || undefined}>
          {row.state === "failed" && <Icon.ErrorCircle size={12} />}
          {row.state === "accepted" && <Icon.Check size={12} />}
          {word}
        </span>
      </div>

      {row.kind === "create_issue" && <CreateFacts row={row} parentTitle={parentTitle} />}
      {row.kind === "new_draft" && (
        <details className="mpv-body">
          <summary>Prompt</summary>
          <div className="mpv-body__text">{plainValue(row.fields.body, "empty")}</div>
        </details>
      )}
      {row.kind === "modify_issue" && (
        <dl className="mpv-changes">
          {row.fields.changes.map((change) => (
            <ChangeView key={change.field} change={change} />
          ))}
        </dl>
      )}

      {blockText !== null && (
        <p className="mpv-block" id={reasonId} data-testid="mpv-block">
          {blockText}
        </p>
      )}
      <Outcome row={row} />
      {row.state === "failed" && (
        <p className="mpv-error" role="alert" data-testid="mpv-row-error">
          {errorText(row)}
        </p>
      )}
      {refusal !== null && (
        <p className="mpv-error" role="alert" data-testid="mpv-row-refusal">
          {refusal}
        </p>
      )}

      {waiting && (
        <div className="mpv-row__actions">
          <button
            type="button"
            className="btn btn--sm btn--primary"
            aria-label={acceptName}
            aria-describedby={blockText !== null ? reasonId : undefined}
            disabled={busy || blocked !== null}
            onClick={onAccept}
          >
            Accept
          </button>
          <button type="button" className="btn btn--sm" disabled={busy} onClick={onReject}>
            Reject
          </button>
          {busy && (
            <span className="mpv-busy" role="status">
              Working…
            </span>
          )}
        </div>
      )}
    </li>
  );
}
