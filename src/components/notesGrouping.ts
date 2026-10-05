/**
 * The Notes panel's client-side view of a loaded note set: filtering
 * (NTS-FR-12), ordering (NTS-FR-15) and grouping by entity (NTS-FR-10).
 *
 * Pure over the list the backend returned, so none of it issues a call and all
 * of it is unit-testable without rendering the panel. The backend already sorts
 * most-recently-edited first (NTC-FR-09); these helpers re-establish that order
 * themselves rather than depending on it, so a group's position never turns on
 * the transport.
 */
import type { NoteListItem } from "../types";

/** What a group of the all-notes position is attached to (NTS-FR-10). */
export type NoteGroupKey =
  | { kind: "entity"; entityId: string; name: string }
  | { kind: "project" }
  | { kind: "unresolved" };

export interface NoteGroup {
  key: NoteGroupKey;
  /** The header's text. Only an entity header is a click-through (NTS-FR-11). */
  label: string;
  items: NoteListItem[];
}

/**
 * The entity's last-known project-relative path, which is what an Unresolved
 * row renders in place of a name (NTS-FR-23).
 */
export function lastKnownPath(item: NoteListItem): string {
  return item.note.scope.kind === "entity" ? item.note.scope.entityPath : "";
}

/** What the filter matches an entity-scoped note on when it has no live name. */
function entityLabel(item: NoteListItem): string {
  return item.entityName ?? lastKnownPath(item);
}

/**
 * NTS-FR-12: does this note survive the filter? The match is case-insensitive
 * over the note's body and, in the all-notes position only, over its entity's
 * name — the position is passed in rather than inferred, because the entity and
 * project-wide positions render no entity name for it to match.
 */
export function matchesFilter(
  item: NoteListItem,
  text: string,
  matchEntityName: boolean,
): boolean {
  const needle = text.trim().toLowerCase();
  if (!needle) return true;
  if (item.note.body.toLowerCase().includes(needle)) return true;
  return matchEntityName && entityLabel(item).toLowerCase().includes(needle);
}

/**
 * NTS-FR-15: most-recently-edited first, by `updatedAt` (NTC-FR-03).
 *
 * The tiebreak matches `notes.rs::sort_most_recent_first` field for field —
 * newer `createdAt` first, then id — because `updated_at` is stored to
 * whole-second resolution, so ties are ordinary rather than exotic. Two
 * orderings that disagree would make a row jump on reload for no reason the
 * user did.
 */
export function sortMostRecentFirst(items: NoteListItem[]): NoteListItem[] {
  return [...items].sort((a, b) => {
    if (a.note.updatedAt !== b.note.updatedAt) {
      return a.note.updatedAt < b.note.updatedAt ? 1 : -1;
    }
    if (a.note.createdAt !== b.note.createdAt) {
      return a.note.createdAt < b.note.createdAt ? 1 : -1;
    }
    return a.note.id < b.note.id ? -1 : a.note.id > b.note.id ? 1 : 0;
  });
}

/**
 * A React key for a group. The label is not one: two entities in different
 * folders can share a basename, and colliding them into a single key makes
 * React reconcile two distinct groups as one.
 */
export function groupKey(group: NoteGroup): string {
  return group.key.kind === "entity" ? `entity:${group.key.entityId}` : group.key.kind;
}

function groupKeyOf(item: NoteListItem): string {
  if (item.note.scope.kind === "project") return "\u0000project";
  if (item.unresolved) return "\u0000unresolved";
  return `entity:${item.note.scope.entityId}`;
}

/**
 * NTS-FR-10: one group per entity that has notes, a **Project** group, and an
 * **Unresolved** group for notes whose entity no longer resolves.
 *
 * Groups are ordered by their most-recently-edited note, most-recent first —
 * which falls out of walking an already-sorted list and keeping first-encounter
 * order — and the rows inside each keep that same order (NTS-FR-15).
 */
export function groupByEntity(items: NoteListItem[]): NoteGroup[] {
  const groups = new Map<string, NoteGroup>();
  for (const item of sortMostRecentFirst(items)) {
    const id = groupKeyOf(item);
    const existing = groups.get(id);
    if (existing) {
      existing.items.push(item);
      continue;
    }
    if (item.note.scope.kind === "project") {
      groups.set(id, { key: { kind: "project" }, label: "Project", items: [item] });
    } else if (item.unresolved) {
      groups.set(id, {
        key: { kind: "unresolved" },
        label: "Unresolved",
        items: [item],
      });
    } else {
      const name = item.entityName ?? lastKnownPath(item);
      groups.set(id, {
        key: { kind: "entity", entityId: item.note.scope.entityId, name },
        label: name,
        items: [item],
      });
    }
  }
  return [...groups.values()];
}

/**
 * The all-notes position's rendered shape: filter first, then group, so a group
 * left with no matching note is not rendered at all (NTS-FR-12).
 *
 * `exemptId` names one note the filter may not remove — the blank note a create
 * has just written, whose empty body matches no text and which would otherwise
 * be hidden the instant it appears.
 */
export function groupsForFilter(
  items: NoteListItem[],
  text: string,
  exemptId?: string | null,
): NoteGroup[] {
  return groupByEntity(
    items.filter((i) => i.note.id === exemptId || matchesFilter(i, text, true)),
  );
}
