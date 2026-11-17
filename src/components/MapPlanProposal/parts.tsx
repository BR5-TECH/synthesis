/**
 * The small parts the summary row and the sheet share
 * (`MPV-map-proposal-review.md`): the status dot and word, the operation
 * badge, the kind chip, and the reference.
 */
import type { MapPlanRow } from "../../types";
import { rowOp, rowRef, type RowOp, type RowStatus } from "../../state/mapPlan/derive";
import { KindChip } from "../Flow/boardItems";
import type { MapPlanHost } from "./host";

/** MPV-FR-FDDK: a status as a dot and its lowercase word, and an optional note in monospace. */
export function StatusWord({ status, note }: { status: RowStatus; note?: string | null }) {
  return (
    <span className="pp-st" data-s={status} data-testid="pp-status">
      <i aria-hidden="true" />
      {status}
      {note ? <span className="pp-st__time">{note}</span> : null}
    </span>
  );
}

/** MPV-FR-FDDK / MPV-FR-ENVX: a status as a dot alone, with the word as its name. */
export function StatusDot({ status }: { status: RowStatus }) {
  return (
    <span className="pp-st" data-s={status} role="img" aria-label={status} title={status}>
      <i />
    </span>
  );
}

const OP_CLASS: Readonly<Record<RowOp, string>> = {
  changed: "badge badge--accent",
  added: "badge badge--ok",
  withdrawn: "badge badge--danger",
  draft: "badge",
};

/** MPV-FR-SNCR: the operation badge of a row. */
export function OpBadge({ row }: { row: MapPlanRow }) {
  const op = rowOp(row);
  return (
    <span className={`pp-op ${OP_CLASS[op]}`} data-op={op}>
      {op}
    </span>
  );
}

/** MPV-FR-SNCR: the kind chip of a row, or nothing where the plan names no kind. */
export function RowKind({ row, host }: { row: MapPlanRow; host: MapPlanHost | null }) {
  if (!host) return null;
  const kind =
    row.kind === "modify_issue"
      ? host.kindOfIssue(row.fields.number)
      : row.kind === "create_issue"
        ? host.kindOfType(row.fields.type ?? null)
        : null;
  return kind ? <KindChip kind={kind} /> : null;
}

/** MPV-FR-ENVX: the reference of a row in monospace, or nothing for a draft. */
export function RowRef({ row }: { row: MapPlanRow }) {
  const ref = rowRef(row);
  return ref === null ? null : <span className="pp-ref">{ref}</span>;
}
