/**
 * The words of the plan proposal card (`MPV-map-proposal-review.md`). Pure
 * functions, so the card and its tests read one source for each sentence.
 */
import type { MapPlanRow, RowChange } from "../../types";

/** MPV-FR-OCYF: the kind of a row, in words. */
export function kindWord(row: MapPlanRow): string {
  switch (row.kind) {
    case "create_issue":
      return "New issue";
    case "modify_issue":
      return "Change issue";
    case "new_draft":
      return "New draft";
  }
}

/** MPV-FR-OCYF: the title or the name of a row. */
export function rowTitle(row: MapPlanRow): string {
  return row.kind === "new_draft" ? row.fields.name : row.fields.title;
}

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

/** MPV-FR-FTXV: the state of a row, in words. */
export function stateWord(row: MapPlanRow): string {
  const blocked = blockWord(row);
  if (blocked !== null) return blocked;
  switch (row.state) {
    case "waiting":
      return "Waiting for you";
    case "accepted":
      return "Accepted";
    case "rejected":
      return "Rejected";
    case "failed":
      return "Failed";
  }
}

/** MPV-FR-LYKS: the reason of a close, in words. */
export function closeReasonWord(change: RowChange): string {
  const raw = typeof change.reason === "string" ? change.reason : String(change.after ?? "");
  return raw === "not_planned" || raw === "not planned" ? "not planned" : "completed";
}

/** The close change of a row, or null. */
export function closeChangeOf(row: MapPlanRow): RowChange | null {
  if (row.kind !== "modify_issue") return null;
  return row.fields.changes.find((c) => c.field === "close") ?? null;
}

/** MPV-FR-WQKV: the text of the typed error of a failed row. */
export function errorText(row: MapPlanRow): string {
  const error = row.error;
  if (error === null) return "The row failed.";
  if (error.code === "changed_on_tracker") {
    return "The issue changed on GitHub after the proposal. Nothing was written.";
  }
  return error.message && error.message !== "" ? error.message : error.code;
}

/** The text of a typed error of a refused call. */
export function refusalText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object") {
    const { message, code } = e as { message?: unknown; code?: unknown };
    if (typeof message === "string" && message !== "") return message;
    if (typeof code === "string" && code !== "") return code;
  }
  return e instanceof Error ? e.message : "The decision was refused.";
}

/** The typed code of a refused call, for the log. Never the message. */
export function refusalCode(e: unknown): string {
  if (e && typeof e === "object") {
    const { code } = e as { code?: unknown };
    if (typeof code === "string") return code;
  }
  return e instanceof Error ? e.name : typeof e;
}

/** A value of a text field, or the word for none. */
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

/** The labels of the field names a change row shows. */
export const FIELD_WORD: Record<RowChange["field"], string> = {
  title: "Title",
  body: "Body",
  type: "Type",
  milestone: "Milestone",
  parent: "Parent",
  labels: "Labels",
  close: "Close",
  status: "Status",
};

/** The text fields, whose before is struck through and whose after is added. */
export const TEXT_FIELDS: ReadonlySet<RowChange["field"]> = new Set(["title", "body"]);
