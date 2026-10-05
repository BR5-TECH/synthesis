import type { CommitFile, CommitFileStatus } from "../../types";

/** GIT-FR-MVBZ: a Unix-seconds instant as the reader's local date. */
export function formatCommitDate(seconds: number): string {
  return new Date(seconds * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

/** The absolute local date and time, for a title and an accessible label. */
export function formatCommitInstant(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString();
}

/** The same, for an ISO instant as GitHub reports it. */
export function formatIsoInstant(iso: string): string {
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? iso : at.toLocaleString();
}

export const ROOT_GROUP_LABEL = "Repository root";

export interface FileGroup {
  /** Project-relative folder path, or "" for the repository root. */
  folder: string;
  label: string;
  files: { file: CommitFile; name: string }[];
}

/**
 * GIT-FR-DXNC: the commit's flat file list grouped by folder.
 *
 * Folders sort by path with the root group first, and files by name inside a
 * group, so the same commit always reads the same way.
 */
export function groupFilesByFolder(files: CommitFile[]): FileGroup[] {
  const groups = new Map<string, FileGroup>();
  for (const file of files) {
    const cut = file.path.lastIndexOf("/");
    const folder = cut === -1 ? "" : file.path.slice(0, cut);
    const name = cut === -1 ? file.path : file.path.slice(cut + 1);
    let group = groups.get(folder);
    if (!group) {
      group = { folder, label: folder === "" ? ROOT_GROUP_LABEL : folder, files: [] };
      groups.set(folder, group);
    }
    group.files.push({ file, name });
  }
  const out = [...groups.values()];
  for (const g of out) g.files.sort((a, b) => a.name.localeCompare(b.name));
  out.sort((a, b) => {
    if (a.folder === "") return -1;
    if (b.folder === "") return 1;
    return a.folder.localeCompare(b.folder);
  });
  return out;
}

const STATUS_LETTER: Record<CommitFileStatus, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  type_changed: "T",
};

const STATUS_WORD: Record<CommitFileStatus, string> = {
  added: "added",
  modified: "modified",
  deleted: "deleted",
  renamed: "renamed",
  copied: "copied",
  type_changed: "type changed",
};

export const statusLetter = (s: CommitFileStatus) => STATUS_LETTER[s] ?? "?";
export const statusWord = (s: CommitFileStatus) => STATUS_WORD[s] ?? String(s);

/** WSS-FR-JMWA wording for a path set: the first few, then the count of the rest. */
export function summarizePaths(
  paths: string[],
  shown = 5,
): { listed: string[]; remaining: number } {
  return { listed: paths.slice(0, shown), remaining: Math.max(0, paths.length - shown) };
}
