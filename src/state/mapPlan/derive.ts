/**
 * What the review of a plan proposal shows, derived from the proposal alone
 * (`MPV-map-proposal-review.md`): the operation and the status of a row, the
 * status and the summary of a proposal, and the marks and the ghost cards the
 * Delivery board takes (`DBR-delivery-board.md` DBR-FR-AGIN, DBR-FR-WKNI).
 * Pure functions, so the summary row, the sheet, the board, and the tests read
 * one source.
 */
import type { MapPlanProposal, MapPlanRow, ProposalUpdateCounts, RowChange } from "../../types";

export type RowOp = "changed" | "added" | "withdrawn" | "draft";
export type RowStatus = "pending" | "applying" | "applied" | "rejected" | "failed";
export type ProposalMark = "proposed" | "withdrawn";

/** The order the summary counts the operations in (MPV-FR-WKNB). */
export const OP_ORDER: readonly RowOp[] = ["changed", "added", "withdrawn", "draft"];

/**
 * PVC-FR-HMJD: the word a badge and a summary show for an operation. A
 * `new_draft` row creates a change.
 */
export const OP_WORD: Readonly<Record<RowOp, string>> = {
  changed: "changed",
  added: "added",
  withdrawn: "withdrawn",
  draft: "change",
};

/** The close change of a row, or null. */
export function closeChangeOf(row: MapPlanRow): RowChange | null {
  if (row.kind !== "modify_issue") return null;
  return row.fields.changes.find((c) => c.field === "close") ?? null;
}

/** The reason of a close change: `completed` or `not_planned`. */
export function closeReasonOf(change: RowChange): "completed" | "not_planned" {
  const raw = typeof change.reason === "string" ? change.reason : String(change.after ?? "");
  return raw === "not_planned" || raw === "not planned" ? "not_planned" : "completed";
}

/** MPV-FR-UXHR: the operation of a row. */
export function rowOp(row: MapPlanRow): RowOp {
  switch (row.kind) {
    case "create_issue":
      return "added";
    case "new_draft":
      return "draft";
    case "modify_issue": {
      const close = closeChangeOf(row);
      return close !== null && closeReasonOf(close) === "not_planned" ? "withdrawn" : "changed";
    }
  }
}

/** MPV-FR-JXFA: the status of a row. `busy` is true while its decision runs. */
export function rowStatus(row: MapPlanRow, busy: boolean): RowStatus {
  if (busy) return "applying";
  switch (row.state) {
    case "waiting":
      return "pending";
    case "accepted":
      return "applied";
    case "rejected":
      return "rejected";
    case "failed":
      return "failed";
  }
}

/** The key of a row of a proposal in a set of busy rows. */
export function rowId(proposalId: string, rowKey: string): string {
  return `${proposalId}\u0000${rowKey}`;
}

/** The statuses of the rows of a proposal, in proposal order. */
export function rowStatuses(proposal: MapPlanProposal, busy: ReadonlySet<string>): RowStatus[] {
  return proposal.rows.map((row) => rowStatus(row, busy.has(rowId(proposal.id, row.key))));
}

/** MPV-FR-LOSO: the status of a proposal, from the statuses of its rows. */
export function proposalStatus(statuses: readonly RowStatus[]): RowStatus {
  if (statuses.includes("applying")) return "applying";
  if (statuses.includes("pending")) return "pending";
  if (statuses.includes("failed")) return "failed";
  if (statuses.length > 0 && statuses.every((s) => s === "rejected")) return "rejected";
  return "applied";
}

/** MPV-FR-GYLV: the count of rows that are decided. */
export function decidedCount(proposal: MapPlanProposal): number {
  return proposal.rows.filter((row) => row.state !== "waiting").length;
}

/** MPV-FR-ENVX: the reference of a row: `#18`, `new`, or null for a draft. */
export function rowRef(row: MapPlanRow): string | null {
  if (row.kind === "modify_issue") return `#${row.fields.number}`;
  if (row.kind === "create_issue") {
    return row.state === "accepted" && row.result?.issueNumber !== undefined
      ? `#${row.result.issueNumber}`
      : "new";
  }
  return null;
}

/** The title or the name of a row. */
export function rowTitle(row: MapPlanRow): string {
  return row.kind === "new_draft" ? row.fields.name : row.fields.title;
}

/** MPV-FR-WKNB: the rows counted by operation, zero counts left out. */
export function opCounts(proposal: MapPlanProposal): string {
  const counts = new Map<RowOp, number>();
  for (const row of proposal.rows) {
    const op = rowOp(row);
    counts.set(op, (counts.get(op) ?? 0) + 1);
  }
  return OP_ORDER.filter((op) => (counts.get(op) ?? 0) > 0)
    .map((op) => `${counts.get(op)} ${OP_WORD[op]}`)
    .join(", ");
}

/** MPV-FR-WKNB: the summary of a proposal. */
export function summaryOf(proposal: MapPlanProposal): string {
  const counts = opCounts(proposal);
  const only = proposal.rows.length === 1 ? proposal.rows[0] : null;
  return only !== null && only.kind === "modify_issue" ? `#${only.fields.number}, ${counts}` : counts;
}

/**
 * MPV-FR-QZUD: the summary of an update comment: the counts of the update, with
 * a count of zero left out. `removed` counts the rows that the agent withdrew.
 */
export function updateSummaryOf(update: ProposalUpdateCounts): string {
  const parts = [
    update.changed > 0 ? `${update.changed} changed` : null,
    update.added > 0 ? `${update.added} added` : null,
    update.withdrawn > 0 ? `${update.withdrawn} removed` : null,
  ].filter((part): part is string => part !== null);
  return parts.length > 0 ? `Updated: ${parts.join(", ")}` : "Updated";
}

/**
 * MPV-FR-CAQL: the row to select after a decision on `afterKey`: the next
 * `pending` row after it, else the first `pending` row, else `afterKey`.
 */
export function nextPendingKey(
  proposal: MapPlanProposal,
  statuses: readonly RowStatus[],
  afterKey: string,
): string {
  const at = proposal.rows.findIndex((row) => row.key === afterKey);
  const pending = (i: number) => statuses[i] === "pending";
  for (let i = at + 1; i < proposal.rows.length; i++) if (pending(i)) return proposal.rows[i].key;
  for (let i = 0; i < proposal.rows.length; i++) if (pending(i)) return proposal.rows[i].key;
  return afterKey;
}

/** MPV-FR-CAQL: the row the sheet selects when it opens. */
export function firstSelection(proposal: MapPlanProposal, statuses: readonly RowStatus[]): string | null {
  const at = statuses.indexOf("pending");
  return proposal.rows[at >= 0 ? at : 0]?.key ?? null;
}

/** Whether a row is `pending` or `applying`: it still marks the board. */
function live(status: RowStatus): boolean {
  return status === "pending" || status === "applying";
}

/** DBR-FR-AGIN: the proposal mark of each plan item id. */
export function mapMarks(
  proposals: readonly MapPlanProposal[],
  busy: ReadonlySet<string>,
): Map<string, ProposalMark> {
  const marks = new Map<string, ProposalMark>();
  for (const proposal of proposals) {
    const statuses = rowStatuses(proposal, busy);
    proposal.rows.forEach((row, i) => {
      if (row.kind !== "modify_issue" || !live(statuses[i])) return;
      marks.set(`issue-${row.fields.number}`, rowOp(row) === "withdrawn" ? "withdrawn" : "proposed");
    });
  }
  return marks;
}

/** DBR-FR-WKNI: an `added` row that shows as a ghost card. */
export interface GhostCard {
  /** The plan item id of the ghost card. */
  id: string;
  proposalId: string;
  rowKey: string;
  title: string;
  type: string | null;
  /** The plan item id of the milestone, or null for the **No milestone** column. */
  milestoneId: string | null;
}

/** The plan item id of the ghost card of a row. */
export function ghostItemId(proposalId: string, rowKey: string): string {
  return `proposal-ghost-${proposalId}-${rowKey}`;
}

/** DBR-FR-WKNI: the ghost cards of the `pending` and `applying` `added` rows. */
export function ghostCards(
  proposals: readonly MapPlanProposal[],
  busy: ReadonlySet<string>,
): GhostCard[] {
  const ghosts: GhostCard[] = [];
  for (const proposal of proposals) {
    const statuses = rowStatuses(proposal, busy);
    proposal.rows.forEach((row, i) => {
      if (row.kind !== "create_issue" || !live(statuses[i])) return;
      ghosts.push({
        id: ghostItemId(proposal.id, row.key),
        proposalId: proposal.id,
        rowKey: row.key,
        title: row.fields.title,
        type: row.fields.type ?? null,
        milestoneId: row.fields.milestone ? `milestone-${row.fields.milestone.number}` : null,
      });
    });
  }
  return ghosts;
}

/** DBR-FR-PMGS: the plan item id of a row, for the selected ring, or null. */
export function rowItemId(proposalId: string, row: MapPlanRow): string | null {
  if (row.kind === "modify_issue") return `issue-${row.fields.number}`;
  if (row.kind === "create_issue") {
    return row.state === "accepted" && row.result?.issueNumber !== undefined
      ? `issue-${row.result.issueNumber}`
      : ghostItemId(proposalId, row.key);
  }
  return null;
}
