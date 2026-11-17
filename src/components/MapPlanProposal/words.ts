/**
 * The words of the review of a plan proposal (`MPV-map-proposal-review.md`).
 * Pure functions, so the surface and its tests read one source for each
 * sentence.
 */
import type { MapPlanProposal, MapPlanRow, RowChange } from "../../types";
import { closeReasonOf } from "../../state/mapPlan/derive";

/** MPV-FR-FUQG / MPV-FR-YEIH: the block of a waiting row in words, or null. */
export function blockWord(row: MapPlanRow): string | null {
  if (row.state !== "waiting" || row.blockedBy === null) return null;
  switch (row.blockedBy.reason) {
    case "waiting":
      return "Waiting for its parent";
    case "rejected":
      return "Its parent was rejected";
    case "failed":
      return "Its parent failed";
  }
}

/** MPV-FR-LYKS: the state after a close change, in words. */
export function closedWord(change: RowChange): string {
  return closeReasonOf(change) === "not_planned" ? "closed as not planned" : "closed as completed";
}

/** MPV-FR-WQKV: the text of the typed error of a failed row. */
export function errorText(row: MapPlanRow): string {
  const error = row.error;
  if (error === null) return "The change failed. Nothing was sent.";
  if (error.code === "changed_on_tracker") {
    return "GitHub refused the edit: the issue changed after it was read. Nothing was sent.";
  }
  return error.message && error.message !== "" ? error.message : error.code;
}

/** A field value as plain words, or the word for none. */
export function plainValue(value: unknown, none = "none"): string {
  if (value === null || value === undefined || value === "") return none;
  if (typeof value === "string" || typeof value === "number") return String(value);
  if (typeof value === "object") {
    const { title, name, number, issue } = value as Record<string, unknown>;
    if (typeof title === "string") return title;
    if (typeof name === "string") return name;
    if (typeof issue === "number") return `#${issue}`;
    if (typeof number === "number") return `#${number}`;
  }
  return none;
}

/** MPV-FR-CCDM: the label of a field, in the order the grid shows them. */
export const FIELD_ORDER: readonly { field: RowChange["field"]; label: string }[] = [
  { field: "title", label: "Title" },
  { field: "type", label: "Type" },
  { field: "milestone", label: "Milestone" },
  { field: "parent", label: "Parent" },
  { field: "status", label: "Status" },
  { field: "labels", label: "Labels" },
  { field: "close", label: "State" },
];

/** MPV-FR-PBAD: the count of rows, `4 issues` or `1 issue`. */
export function issueCount(proposal: MapPlanProposal): string {
  const n = proposal.rows.length;
  return `${n} ${n === 1 ? "issue" : "issues"}`;
}

/** MPV-FR-IZLJ: the line of the review bar when no row is pending. */
export function decidedLine(applied: number, rejected: number, failed: number): string {
  return `All rows decided: ${applied} applied, ${rejected} rejected${failed > 0 ? `, ${failed} failed` : ""}`;
}
