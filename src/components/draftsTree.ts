/**
 * The Drafts panel's tree vocabulary: what a folder path is, how one is taken
 * apart and put back together, and what makes a name unusable
 * (`../../specifications/ui/DRP-drafts-panel.md`).
 *
 * Pure functions over paths and counts, held apart from the panel because they
 * are what the panel *means* by a folder rather than anything about how it is
 * drawn — and because a rule about names is far easier to read, and to test,
 * away from the surface that enforces it.
 */
import type { DraftMatch, DraftSummary, DraftsStatusFilter } from "../types";
import type { SelectorPosition } from "./SelectorRow";

/** DRP-FR-07: the five positions of the status filter. */
export type DraftFilter = DraftsStatusFilter;

/**
 * DRP-FR-07 / SNV-FR-62: the status filter's five positions, in the order the
 * requirement names them — active (the default), archived, graduated, all
 * drafts, published. Active leads because a retired draft is not what the
 * author came to the panel for, and the other positions are one step away
 * rather than gone.
 *
 * Module-level so its identity is stable across renders: `SelectorRow`
 * re-measures its fit when the position list changes, and this list never
 * varies.
 */
export const STATUS_POSITIONS: SelectorPosition<DraftsStatusFilter>[] = [
  { value: "active", tag: "Active", title: "Active" },
  { value: "archived", tag: "Archived", title: "Archived" },
  // DRP-FR-07: `graduated` is a position of its own rather than a kind of
  // archive — a draft a graduation has already turned into specifications is
  // neither being worked on nor retired (DRS-FR-20). It admits `graduated`
  // alone: a publication to GitHub leaves a draft in the active position.
  { value: "graduated", tag: "Graduated", title: "Graduated" },
  { value: "all", tag: "All", title: "All drafts" },
  // DRP-FR-GBVS: the drafts published to GitHub, on any status.
  { value: "published", tag: "Published", title: "Published to GitHub" },
];

/**
 * DRP-FR-07: whether a status filter position admits this draft.
 * **All drafts** admits everything, and **graduated** admits `graduated`
 * alone. **Active** also admits the legacy `published`, because a publication
 * to GitHub leaves a draft in the author's working view, and `github_shadow`
 * (DRP-FR-YVXS). A shadow draft that reports `graduated` falls in the
 * graduated position by its own status.
 *
 * DRP-FR-GBVS: **published** admits a draft whose row carries a publication
 * record, on any status. The legacy `published` status alone does not admit
 * it.
 */
export function statusAdmits(
  filter: DraftFilter,
  draft: Pick<DraftSummary, "status" | "publication">,
): boolean {
  if (filter === "published") return !!draft.publication;
  const { status } = draft;
  if (filter === "all" || status === filter) return true;
  return filter === "active" && (status === "published" || status === "github_shadow");
}

/**
 * DRP non-functional requirements: how long the text filter rests after the last
 * keystroke before it is dispatched, so typing a word costs one search rather
 * than one per character.
 */
export const FILTER_DEBOUNCE_MS = 250;

/**
 * DRP-FR-27 / DRP-FR-PXMK: the only data a row drag carries. It is a type of
 * this panel's own, so no editable field and no other surface takes the drop:
 * a drag from a row is a move within this panel and nothing else.
 */
export const DRAG_TYPE = "application/x-synthesis-drafts-item";

/** DRP-FR-PXMK: the attribute that marks the region a row drag may land in. */
export const DRAG_ZONE_ATTRIBUTE = "data-drafts-drag-zone";

/** The implicit root (DRP-FR-20): the tree's container, never a row. */
export const ROOT = "";

/**
 * How the implicit root is named inside the Move to Folder… select. Its real
 * path is the empty string, which that control reads as "nothing selected".
 */
export const ROOT_OPTION = "(root)";

/**
 * What the panel can act on. The root is included because it takes a menu and a
 * drop, and excluded from everything that renames, moves, or deletes.
 */
export type TreeItem =
  | { kind: "draft"; id: string; name: string; folder: string }
  | { kind: "folder"; path: string };

/** A stable key per item, for selection, in-flight locks and inline errors. */
export function itemKey(item: TreeItem): string {
  return item.kind === "draft" ? `draft:${item.id}` : `folder:${item.path}`;
}

/** The basename of a drafts-root-relative folder path. */
export function folderName(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut === -1 ? path : path.slice(cut + 1);
}

/** The containing folder of a drafts-root-relative path; `""` for top-level. */
export function folderParent(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut === -1 ? ROOT : path.slice(0, cut);
}

/** Join a folder path with a name below it, treating `""` as the root. */
export function folderJoin(folder: string, name: string): string {
  return folder === ROOT ? name : `${folder}/${name}`;
}

/** Every ancestor of a folder path, root excluded, nearest last. */
export function ancestorsOf(path: string): string[] {
  const out: string[] = [];
  const parts = path.split("/");
  for (let i = 1; i < parts.length; i += 1) out.push(parts.slice(0, i).join("/"));
  return out;
}

/** Whether `path` is `folder` or sits anywhere inside its subtree. */
export function isWithin(path: string, folder: string): boolean {
  if (folder === ROOT) return true;
  return path === folder || path.startsWith(`${folder}/`);
}

/**
 * DRP-FR-23 / DRP-FR-24: what the panel refuses before any call is made, on the
 * same rules the New Folder window applies (NFW-FR-08). A malformed name costs
 * no round trip.
 *
 * Returns the reason, or `null` when the name is acceptable to send.
 */
export function folderNameProblem(name: string): string | null {
  const trimmed = name.trim();
  if (trimmed === "") return "A folder needs a name.";
  if (trimmed.includes("/") || trimmed.includes("\\"))
    return "A folder name cannot contain a path separator.";
  if (trimmed === "." || trimmed === "..")
    return "That name is reserved by the filesystem.";
  return null;
}

/**
 * DRP-FR-13 / DRP-FR-17: how a search result narrows the tree.
 *
 * `null` means no filter is in force — the text field is empty, or its result
 * has not landed yet — and every draft is shown unannotated. Otherwise the map
 * holds exactly the drafts the text was found in, against why each matched.
 */
export type DraftMatches = Map<string, DraftMatch["matchedIn"]> | null;

export function toMatchMap(hits: DraftMatch[]): Map<string, DraftMatch["matchedIn"]> {
  return new Map(hits.map((h) => [h.draftId, h.matchedIn]));
}

/**
 * DRP-FR-25: what the folder-delete confirmation says is about to move. A count
 * of zero is left out rather than stated, so a folder holding only subfolders
 * does not read "The 0 drafts and 1 folder…".
 */
export function countPhrase(drafts: number, folders: number): string {
  const parts: string[] = [];
  if (drafts > 0) parts.push(`${drafts} ${drafts === 1 ? "draft" : "drafts"}`);
  if (folders > 0)
    parts.push(`${folders} ${folders === 1 ? "folder" : "folders"}`);
  if (parts.length === 0) return "Nothing it holds moves";
  // The verb comes with the phrase rather than being fixed in the sentence:
  // eliding a zero count can leave a singular subject ("The 1 folder it holds"),
  // and a hardcoded plural verb would then disagree with it.
  const plural = parts.length > 1 || drafts > 1 || folders > 1;
  return `The ${parts.join(" and ")} it holds ${plural ? "move" : "moves"}`;
}

/**
 * Where a context menu opens: the pointer for a right-click, and the focused
 * row's own corner for the keyboard route, so both land somewhere the author
 * was already looking.
 */
export interface MenuAnchor {
  x: number;
  y: number;
}

/**
 * The point a keyboard-opened menu hangs from — the row's leading edge, just
 * below it, which is where a right-click on that row would most likely have
 * landed.
 */
export function anchorOfRow(row: HTMLElement): MenuAnchor {
  const rect = row.getBoundingClientRect();
  return { x: rect.left + 8, y: rect.bottom };
}
