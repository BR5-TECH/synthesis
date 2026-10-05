/**
 * How one persisted log record reads on screen
 * (`../../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-FXAL,
 * GLW-FR-GIWK, GLW-FR-HPWG).
 *
 * Every value here is **untrusted** — a model, an executor, or the application
 * wrote it (GLW-FR-RUNX) — so nothing is parsed, linked, or interpreted. What
 * this module produces is plain text, which React then escapes (GLW-FR-SHAF).
 */

import type { GraduationLogPageEntry } from "../../types";

/** GLW-FR-DDXJ: what the run-level marker reads as in place of a pass. */
export const RUN_LEVEL_MARKER = "run-level";
/** GLW-FR-FXAL: what a field the record holds as null reads as. */
export const ABSENT_FIELD = "—";

function text(record: Record<string, unknown>, key: string): string | null {
  const value = record[key];
  return typeof value === "string" && value.length > 0 ? value : null;
}

function number(record: Record<string, unknown>, key: string): number | null {
  const value = record[key];
  return typeof value === "number" ? value : null;
}

/** The instant, in the local time zone, from the instant the record carries. */
export function localInstant(at: string | null): string {
  if (!at) return ABSENT_FIELD;
  const parsed = new Date(at);
  if (Number.isNaN(parsed.getTime())) return at;
  // The day as well as the time: two chunks written on different days read
  // identically without it, and a log is read long after it was written.
  return parsed.toLocaleString(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

/**
 * GRS-FR-QVTA: decoded as UTF-8, with replacement characters for invalid byte
 * sequences, so a chunk that is not text still renders as lines.
 */
export function decodeSourceChunk(record: Record<string, unknown>): string {
  const encoded = text(record, "data_base64");
  if (!encoded) return "";
  try {
    const binary = atob(encoded);
    const bytes = new Uint8Array(binary.length);
    for (let at = 0; at < binary.length; at += 1) {
      bytes[at] = binary.charCodeAt(at);
    }
    // `fatal: false` is the default, which is exactly the replacement-character
    // behaviour the storage spec asks for.
    return new TextDecoder("utf-8").decode(bytes);
  } catch {
    // A chunk whose base64 does not decode is still a record the reader must
    // be told about, so the row stands and says so rather than vanishing.
    return "";
  }
}

/** GLW-FR-FXAL: the ten fields every source line carries. */
export interface SourceMeta {
  runId: string;
  phaseId: string;
  pass: string;
  origin: string;
  producer: string;
  agent: string;
  container: string;
  source: string;
  at: string;
  sequence: string;
}

export interface SourceRow {
  key: string;
  sequence: number;
  runLevel: boolean;
  meta: SourceMeta;
  /** The ten fields as one line, in the order GLW-FR-FXAL states them. */
  metaLine: string;
  text: string;
}

/**
 * GLW-FR-FXAL: one source chunk as its text lines, each carrying all ten of the
 * chunk's displayed fields.
 *
 * A chunk decoding to several lines shows the same ten on each of them.
 */
export function sourceRows(entry: GraduationLogPageEntry): SourceRow[] {
  const record = entry.record;
  const sequence = number(record, "sequence") ?? 0;
  const pass = number(record, "pass");
  const meta: SourceMeta = {
    runId: text(record, "run_id") ?? ABSENT_FIELD,
    phaseId: text(record, "phase_id") ?? ABSENT_FIELD,
    // GLW-FR-DDXJ: a record the file holds with no pass keeps its run-level
    // marker rather than being given a pass it was not written with.
    pass: pass === null ? RUN_LEVEL_MARKER : String(pass),
    origin: text(record, "origin") ?? ABSENT_FIELD,
    producer: text(record, "producer") ?? ABSENT_FIELD,
    agent: text(record, "agent") ?? ABSENT_FIELD,
    container: text(record, "container") ?? ABSENT_FIELD,
    source: text(record, "source") ?? ABSENT_FIELD,
    at: localInstant(text(record, "at")),
    sequence: String(sequence),
  };
  const metaLine = [
    meta.at,
    `#${meta.sequence}`,
    meta.source,
    meta.origin,
    meta.producer,
    meta.agent,
    meta.container,
    meta.phaseId,
    meta.pass,
    meta.runId,
  ].join(" · ");

  const decoded = decodeSourceChunk(record);
  const lines = decoded.length === 0 ? [""] : decoded.replace(/\n$/, "").split("\n");
  return lines.map((line, at) => ({
    key: `${sequence}-${at}`,
    sequence,
    runLevel: entry.presentation.runLevel,
    meta,
    metaLine,
    text: line,
  }));
}

/** GLW-FR-GIWK: one structured record as its event, level, instant, and fields. */
export interface StructuredRow {
  key: string;
  sequence: number;
  runLevel: boolean;
  at: string;
  level: string;
  event: string;
  fields: Array<[string, string]>;
  metaLine: string;
}

export function structuredRow(entry: GraduationLogPageEntry): StructuredRow {
  const record = entry.record;
  const sequence = number(record, "sequence") ?? 0;
  const pass = number(record, "pass");
  const raw = record.fields;
  const fields: Array<[string, string]> =
    raw && typeof raw === "object" && !Array.isArray(raw)
      ? Object.entries(raw as Record<string, unknown>).map(([key, value]) => [
          key,
          typeof value === "string" ? value : JSON.stringify(value ?? null),
        ])
      : [];
  return {
    key: String(sequence),
    sequence,
    runLevel: entry.presentation.runLevel,
    at: localInstant(text(record, "at")),
    level: text(record, "level") ?? ABSENT_FIELD,
    event: text(record, "event") ?? ABSENT_FIELD,
    fields,
    metaLine: [
      text(record, "origin") ?? ABSENT_FIELD,
      text(record, "producer") ?? ABSENT_FIELD,
      text(record, "phase_id") ?? ABSENT_FIELD,
      pass === null ? RUN_LEVEL_MARKER : String(pass),
      `#${sequence}`,
    ].join(" · "),
  };
}
