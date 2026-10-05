/**
 * Which pages and which records belong to the selected scope
 * (`../../../specifications/ui/GLW-graduation-log-window.md` GLW-FR-VRTC).
 *
 * The backend scopes every read. The window still checks what came back, so a
 * page for another run, phase, scope, or stream, and a record that carries
 * another run, phase, or pass, never reaches the viewport.
 */

import type { LogScopeEntry } from "../../state/graduation/logScopes";
import { scopeOf } from "../../state/graduation/logScopes";
import type {
  GraduationLogPage,
  GraduationLogPageEntry,
  GraduationLogStream,
} from "../../types";

export interface PageScope {
  runId: string;
  phaseId: string;
  entry: LogScopeEntry | null;
  stream: GraduationLogStream;
}

/** GLW-FR-VRTC: whether a page answers for the selected run, phase, scope, and stream. */
export function pageIsFor(page: GraduationLogPage, scope: PageScope): boolean {
  if (!scope.entry) return false;
  if (page.runId !== scope.runId) return false;
  if (page.phaseId !== scope.phaseId) return false;
  if (page.stream !== scope.stream) return false;
  const wanted = scopeOf(scope.entry);
  if (page.scope.kind !== wanted.kind) return false;
  return (
    wanted.kind !== "pass" ||
    (page.scope.kind === "pass" && page.scope.pass === wanted.pass)
  );
}

/**
 * GLW-FR-VRTC: whether one record belongs to the selection.
 *
 * A record that does not carry a field is judged on the fields it does carry.
 */
export function recordBelongs(
  entry: GraduationLogPageEntry,
  scope: PageScope,
): boolean {
  const { record } = entry;
  if (record.run_id !== undefined && record.run_id !== scope.runId) return false;
  if (record.phase_id !== undefined && record.phase_id !== scope.phaseId) {
    return false;
  }
  if (record.pass !== undefined && scope.entry) {
    const expected = scope.entry.kind === "run_level" ? null : scope.entry.pass;
    if (record.pass !== expected) return false;
  }
  return true;
}

/**
 * GLW-FR-VRTC: the page with only the records of the selection, or null where
 * the page answers for another selection altogether.
 */
export function acceptPage(
  page: GraduationLogPage,
  scope: PageScope,
): GraduationLogPage | null {
  if (!pageIsFor(page, scope)) return null;
  return {
    ...page,
    entries: (page.entries ?? []).filter((entry) => recordBelongs(entry, scope)),
  };
}
