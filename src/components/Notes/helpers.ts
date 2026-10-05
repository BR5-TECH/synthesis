/**
 * The Notes panel's own types and small pure helpers.
 *
 * Kept beside the panel so the rules that decide a new note's scope, the Move
 * picker's list, and the timestamp titles stay readable on their own.
 */
import type {
  ArtifactType,
  Note,
  NoteScope,
  NotesEntity,
  NotesScopePosition,
  TreeNode,
} from "../../types";

/**
 * The panel's floating overlays. Exactly one may be open (NTS-FR-24), which is
 * why they share a single piece of state rather than a flag each: opening one
 * necessarily replaces any other.
 */
export type Overlay =
  | { kind: "menu"; noteId: string; x: number; y: number }
  | { kind: "move"; noteId: string; x: number; y: number }
  | { kind: "reminder"; noteId: string; x: number; y: number }
  | { kind: "confirmDelete"; noteId: string; x: number; y: number };

/**
 * An entry of the Move picker's list (NTS-FR-22), and what a group header needs
 * to route to its artifact (NTS-FR-11).
 *
 * `id` is the project-relative path, which is what disambiguates two artifacts
 * sharing a basename — both in the picker's rendering and in what its filter
 * matches. `artifactType` is what decides the surface the artifact opens in
 * (LIB-FR-03): without it a Flow would open in an Editor tab.
 */
export interface MoveTarget {
  id: string;
  name: string;
  artifactType?: ArtifactType;
}

/** How often a rendered relative timestamp is recomputed (NTS-FR-14). */
export const TIMESTAMP_REFRESH_MS = 60_000;

/**
 * What a note created from `position` attaches to.
 *
 * NTS-FR-04 has a new note follow the active tab's entity in every position.
 * That holds here for the entity and all-notes positions, but **not** for the
 * project-wide one: a note is now written before the user types into it, so a
 * note the current list cannot show is a note the user cannot then edit. In the
 * project-wide position the note is project-wide — the scope the panel is
 * showing — so the row it lands in is the row the caret goes to. This is a
 * deliberate divergence from NTS-FR-04's "in every scope position"; the
 * all-notes position keeps that rule because it shows both kinds.
 */
export function newNoteScope(
  position: NotesScopePosition,
  entity: NotesEntity | null,
): NoteScope {
  if (position === "project" || !entity) return { kind: "project" };
  return { kind: "entity", entityId: entity.id, entityPath: entity.id };
}

/**
 * Size the editor to its content, so a row in edit is as tall as the text it
 * holds — no scrollbar inside a two-line note, and no empty space under a
 * one-line one.
 */
export function autoGrow(el: HTMLTextAreaElement | null): void {
  if (!el) return;
  el.style.height = "auto";
  // jsdom reports 0 here (it lays nothing out); leaving the height alone in
  // that case keeps the element from collapsing under a test.
  if (el.scrollHeight > 0) el.style.height = `${el.scrollHeight}px`;
}

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "operation failed";
}

export function formatAbsolute(iso: string): string {
  const parsed = new Date(iso);
  return Number.isNaN(parsed.getTime()) ? iso : parsed.toLocaleString();
}

/**
 * NTS-FR-14: hovering a row's relative timestamp discloses the note's absolute
 * creation and last-edit instants.
 */
export function absoluteTitle(note: Note): string {
  return `Created ${formatAbsolute(note.createdAt)}\nLast edited ${formatAbsolute(note.updatedAt)}`;
}

/**
 * NTS-FR-22: the project's artifacts and Flows, flattened out of the
 * Library tree — the same set the Library's own **Notes** action applies to, so
 * a note can be moved onto anything it could have been created on.
 */
export function artifactTargets(node: TreeNode, out: MoveTarget[] = []): MoveTarget[] {
  // Files only: a folder can carry an artifact type too (a folder-scope
  // assignment, ASC-FR-05), and a note attaches to an artifact, not to a
  // directory.
  if (node.nodeKind === "file" && node.artifactType) {
    out.push({ id: node.id, name: node.name, artifactType: node.artifactType });
  }
  for (const child of node.children ?? []) artifactTargets(child, out);
  return out;
}
