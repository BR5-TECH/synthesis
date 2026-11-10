import type { FlowDocument } from "../../state/flowDocument";
import type { SyncConflict } from "../../types";

/** FSR-FR-IFQE: the name of a conflicting field, as the review shows it. */
export function fieldLabel(field: SyncConflict["field"]): string {
  switch (field) {
    case "name":
      return "Title";
    case "description":
      return "Description";
    case "type":
      return "Type";
    case "parent":
      return "Parent";
    case "milestone":
      return "Milestone";
    case "due":
      return "Due date";
    default:
      return "Item";
  }
}

/** The name of the plan item that links a tracker item, if one does. */
function linkedName(
  doc: FlowDocument,
  kind: "issue" | "milestone",
  number: number,
): string | undefined {
  const link = doc.sync?.links.find((l) => l.ref.kind === kind && l.ref.number === number);
  if (!link) return undefined;
  return doc.items.find((i) => i.id === link.itemId)?.name;
}

function numberOf(v: unknown): number | undefined {
  if (typeof v === "number") return v;
  if (v && typeof v === "object" && typeof (v as { number?: unknown }).number === "number") {
    return (v as { number: number }).number;
  }
  return undefined;
}

/**
 * FSR-FR-IFQE: a value of a conflict in words. A parent or a milestone shows
 * the item name and the number, a Type shows its name, and an empty side
 * reads as None.
 */
export function formatSyncValue(
  doc: FlowDocument,
  field: SyncConflict["field"],
  value: unknown,
): string {
  if (value === null || value === undefined || value === "") return "None";
  if (field === "parent" || field === "milestone") {
    const kind = field === "milestone" ? "milestone" : "issue";
    const n = numberOf(value);
    if (n !== undefined) {
      const name = linkedName(doc, kind, n);
      const mark = kind === "milestone" ? `milestone ${n}` : `#${n}`;
      return name ? `${name} (${mark})` : mark;
    }
    if (typeof value === "string") {
      return doc.items.find((i) => i.id === value)?.name ?? value;
    }
  }
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

/** FSR-FR-IFQE: a text long enough to show its first lines only. */
export function isLongText(text: string): boolean {
  return text.length > 140 || text.split("\n").length > 3;
}
