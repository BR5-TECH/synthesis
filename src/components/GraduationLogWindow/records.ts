/**
 * How one persisted activity record reads on screen
 * (`../../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-KHGP,
 * GLW-FR-JOIG, GLW-FR-HGXL).
 *
 * The window reads four fields of a record and no other: `sequence`, `at`,
 * `kind`, and `summary`. A record that holds any further field shows none of it.
 *
 * Every value here is untrusted (GLW-FR-RUNX). Nothing is parsed, linked, or
 * interpreted. This module returns plain text, which React then escapes
 * (GLW-FR-SHAF).
 */

import type { GraduationLogPageEntry } from "../../types";

/** GLW-FR-HGXL: the four fields a row is made of. */
export interface ActivityRow {
  /** Where the row sits in the stream. It is also the key of the row. */
  sequence: number;
  at: string;
  kind: string;
  summary: string;
}

function textOf(record: Record<string, unknown>, key: string): string {
  const value = record[key];
  return typeof value === "string" ? value : "";
}

/** GLW-FR-RQEV: the sequence a record is ordered and deduplicated by. */
export function sequenceOf(entry: GraduationLogPageEntry): number {
  const value = entry.record.sequence;
  return typeof value === "number" ? value : 0;
}

/** GLW-FR-HGXL: the four fields of one entry, and no other field. */
export function activityRow(entry: GraduationLogPageEntry): ActivityRow {
  const { record } = entry;
  return {
    sequence: sequenceOf(entry),
    at: textOf(record, "at"),
    kind: textOf(record, "kind"),
    summary: textOf(record, "summary"),
  };
}
