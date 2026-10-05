/**
 * The tab records and identity rules the shell shares between its own body and
 * the action groups split out of it. Held apart from `useShellSession` so the
 * definition of "the Dashboard tab" and of a tab's identity has one home that
 * every group reads, rather than being reachable only through the hook.
 */
import { isMarkdownFile } from "../../state/syntaxHighlight";
import type { EditMode } from "../../state/editHistory";
import type { Tab } from "../../types";

/**
 * The Dashboard tab record. Defined once because three different rules put it
 * in the strip and they must agree on what "the Dashboard tab" is: the tab a
 * project opens on (DSH-FR-01 / OVW-FR-04), the tab the Home affordance
 * reopens (SNV-FR-41), and the tab the strip falls back to when the last one
 * closes (TAB-FR-15 / DSH-FR-10).
 *
 * Frozen because one record is shared by every strip in the process. Tab
 * records are treated as immutable everywhere (`tabsView` rebuilds rather than
 * assigns), so nothing is giving anything up — and an in-place write would
 * otherwise leak across shells and across tests, where it would be invisible to
 * the `toEqual` comparisons the tab assertions are built on.
 */
export const DASHBOARD_TAB: Tab = Object.freeze({
  id: "dashboard",
  label: "Dashboard",
});

/**
 * SMP-FR-KQTD / TAB-FR-KXMW: the Map tab record. Its id names no path and no
 * draft, so no path closure reaches it, and one fixed id is what keeps it to
 * one per project.
 */
export const SPEC_MAP_TAB: Tab = Object.freeze({
  id: "map:specifications",
  label: "Map — specifications",
  kind: "map",
});

/**
 * TAB-FR-23: a conversation tab's id, derived from the conversation rather than
 * from a path — which is what lets it coexist with the Editor, Diff, and History
 * detail tabs of the file that conversation is about without any of them
 * contending for the strip.
 */
export function conversationTabId(threadId: string): string {
  return `conv:${threadId}`;
}

/**
 * TAB-FR-QXRF: a Document tab's and a PDF Viewer tab's id, derived from the
 * document id rather than from a path. One id for both kinds is what keeps a
 * document to one viewer tab, and a path removal (TAB-FR-19) cannot reach it.
 * The `doc:` prefix keeps it apart from the `art:` id of an Editor tab on the
 * same file, so the two coexist.
 */
export function documentTabId(documentId: string): string {
  return `doc:${documentId}`;
}

/**
 * CMP-FR-11 / CMT-FR-36: the surface an arriving thread has to land on — the one
 * that renders the rail (CMT-FR-02).
 *
 * For a Markdown file that is WYSIWYG, whatever mode the tab was left in; for a
 * source file it is the one surface it has, so the activation says nothing about
 * the mode at all rather than asking for a rich surface the file does not have
 * (ESH-FR-SSDV).
 */
export function railSurfaceFor(artifactId: string): { mode?: EditMode } {
  return isMarkdownFile(artifactId) ? { mode: "wysiwyg" } : {};
}

/**
 * TAB-FR-19: the project-relative path a tab's content lives at, or null for a
 * tab that is not about a file at all (Dashboard, Search, settings) or that is
 * identified by something other than a path (a draft, TAB-FR-17).
 *
 * A Diff tab carries its path inside `diff` rather than as `artifactId` —
 * deliberately, since it owns no savable content — so it needs its own branch
 * here or a removed file would leave its Diff tab behind.
 *
 * TAB-FR-24: a conversation tab carries neither, being identified by the
 * conversation it is bound to (TAB-FR-23), so no path removal reaches it — a
 * conversation whose artifact has been removed renders its unavailable-owner
 * state in the tab it is already in rather than being taken off the strip.
 */
export const tabPath = (t: Tab): string | null =>
  t.artifactId ?? t.diff?.path ?? null;

/**
 * TAB-FR-15 / DSH-FR-10: the strip never empties, however it emptied. Closing
 * the last tab puts the Dashboard in the leading position, so the viewport
 * always has something to render and the author always lands somewhere that
 * offers a way onward.
 */
export const neverEmpty = (next: Tab[]): Tab[] =>
  next.length === 0 ? [DASHBOARD_TAB] : next;
