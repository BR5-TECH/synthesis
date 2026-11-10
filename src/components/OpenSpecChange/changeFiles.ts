/**
 * Where the hunks of a pending proposal fall among the artifacts and the files
 * of a change (`../../../specifications/ui/OCW-openspec-change-workspace.md`
 * OCW-FR-ABUR, OCW-FR-COLR, OCW-FR-RRXG). Pure functions, so the rules are
 * read and tested in one place.
 */
import {
  isUndecided,
  type ChangeArtifact,
  type ChangeDetail,
  type ChangeEditProposal,
  type ChangeFile,
} from "../../types";

/** OCW-FR-ABUR: how one file is shown. */
export type FileMode = "rich" | "source";

/** OCW-FR-RRXG: a file of the change, or a file a hunk would create. */
export interface ShownFile extends ChangeFile {
  /** OCW-FR-RRXG: a new-file hunk would create it. It is not on disk yet. */
  isNew: boolean;
}

/** The final part of a path. */
function basename(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut === -1 ? path : path.slice(cut + 1);
}

/**
 * OCW-FR-RRXG: the artifact a change-relative path belongs to. A file that
 * the change holds belongs to the artifact that lists it. A new file belongs
 * to the artifact whose id is its first folder (`specs/...`), or the artifact
 * whose id is its name without the extension (`design.md`).
 */
export function artifactOfPath(detail: ChangeDetail, path: string): string | null {
  const holder = detail.artifacts.find((a) => a.files.some((f) => f.path === path));
  if (holder) return holder.id;
  const first = path.includes("/") ? path.slice(0, path.indexOf("/")) : null;
  if (first !== null) {
    const byFolder = detail.artifacts.find((a) => a.id === first);
    if (byFolder) return byFolder.id;
  }
  const stem = basename(path).replace(/\.[^.]*$/, "");
  return detail.artifacts.find((a) => a.id === stem)?.id ?? null;
}

/** The undecided ledger rows of a proposal, or none where it is not pending. */
function undecidedRows(proposal: ChangeEditProposal | undefined) {
  if (!proposal || proposal.state !== "pending") return [];
  return proposal.hunks.filter((h) => isUndecided(h.state));
}

/** OCW-FR-RRXG: how many undecided hunks each artifact holds. */
export function undecidedByArtifact(
  detail: ChangeDetail,
  proposal: ChangeEditProposal | undefined,
): Map<string, number> {
  const counts = new Map<string, number>();
  for (const row of undecidedRows(proposal)) {
    const id = artifactOfPath(detail, row.path);
    if (id !== null) counts.set(id, (counts.get(id) ?? 0) + 1);
  }
  return counts;
}

/** OCW-FR-COLR: the paths that hold an undecided hunk. */
export function pathsWithHunks(proposal: ChangeEditProposal | undefined): Set<string> {
  return new Set(undecidedRows(proposal).map((row) => row.path));
}

/**
 * OCW-FR-RRXG: the files an artifact shows. The files of the change, and after
 * them each file that an undecided new-file hunk would create, marked as new.
 */
export function shownFiles(
  detail: ChangeDetail,
  artifact: ChangeArtifact,
  proposal: ChangeEditProposal | undefined,
): ShownFile[] {
  const files: ShownFile[] = artifact.files.map((f) => ({ ...f, isNew: false }));
  const known = new Set(files.map((f) => f.path));
  for (const row of undecidedRows(proposal)) {
    if (!row.newFile || known.has(row.path)) continue;
    if (artifactOfPath(detail, row.path) !== artifact.id) continue;
    known.add(row.path);
    files.push({ path: row.path, text: "", unreadable: false, isNew: true });
  }
  return files;
}

/**
 * OCW-FR-ABUR / OCW-FR-COLR: the mode of one file. The author's choice where
 * the tab holds one, and otherwise Source for a file with a hunk and Rich for
 * every other file. A new file has no Rich rendering yet.
 */
export function modeOf(
  file: ShownFile,
  chosen: Readonly<Record<string, FileMode>>,
  withHunks: ReadonlySet<string>,
): FileMode {
  if (file.isNew) return "source";
  return chosen[file.path] ?? (withHunks.has(file.path) ? "source" : "rich");
}
