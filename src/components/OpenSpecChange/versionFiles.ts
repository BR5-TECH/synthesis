/**
 * The files of a version of a change, grouped by artifact and compared with
 * the current change (`../../../specifications/ui/OCW-openspec-change-workspace.md`
 * OCW-FR-RKCC, OCW-FR-DRQD, OCW-FR-ABAR). Pure functions, so the rules are
 * read and tested in one place.
 */
import { alignLines, type AlignedRow } from "../../diff/lineAlign";
import type { ChangeDetail, ChangeVersion, ChangeVersionFile } from "../../types";
import { artifactOfPath } from "./changeFiles";

/** The id of the group that holds the files no artifact of the schema claims. */
export const OTHER_FILES = "other files";

/** OCW-FR-DRQD: one artifact of a version, with the files the version holds for it. */
export interface VersionArtifact {
  id: string;
  files: ChangeVersionFile[];
}

/**
 * OCW-FR-DRQD: the files of a version, under the artifacts of the schema in
 * their order. A file that no artifact claims stands in a last group of its
 * own.
 */
export function versionArtifacts(
  detail: ChangeDetail,
  files: readonly ChangeVersionFile[],
): VersionArtifact[] {
  const groups: VersionArtifact[] = detail.artifacts.map((a) => ({ id: a.id, files: [] }));
  const other: VersionArtifact = { id: OTHER_FILES, files: [] };
  for (const file of files) {
    const id = artifactOfPath(detail, file.path);
    const group = groups.find((g) => g.id === id) ?? other;
    group.files.push(file);
  }
  return other.files.length > 0 ? [...groups, other] : groups;
}

/** OCW-FR-RKCC: the count of files a version changed. */
export function changedFileCount(version: ChangeVersion): number {
  return version.added.length + version.modified.length + version.deleted.length;
}

/**
 * OCW-FR-ABAR: how one file of the change differs between a version and now.
 * `notText` is a file that both hold and that is not text in one of them, so
 * no line diff can say whether it changed.
 */
export type CompareMark = "added" | "removed" | "changed" | "unchanged" | "notText";

/** OCW-FR-ABAR: one file of a comparison. */
export interface ComparedFile {
  path: string;
  mark: CompareMark;
  /** The file is not text in the version or in the current change. */
  notText: boolean;
  /** The line diff, version on the left and the current change on the right. */
  rows: AlignedRow[];
}

/** The text of every file of the current change, or null for a file that is not text. */
function currentTexts(detail: ChangeDetail): Map<string, string | null> {
  const texts = new Map<string, string | null>();
  for (const artifact of detail.artifacts) {
    for (const file of artifact.files) {
      texts.set(file.path, file.unreadable ? null : file.text);
    }
  }
  return texts;
}

/**
 * OCW-FR-ABAR: each file of the version beside the same file of the current
 * change, ordered by path. A file that only the current change holds is
 * `added`, one that only the version holds is `removed`, and one whose text
 * differs is `changed`. A file that is not text in either is named with no
 * diff.
 */
export function compareWithCurrent(
  files: readonly ChangeVersionFile[],
  detail: ChangeDetail,
): ComparedFile[] {
  const now = currentTexts(detail);
  const then = new Map<string, string | null>(
    files.map((f) => [f.path, f.binary || f.text == null ? null : f.text]),
  );
  const paths = [...new Set([...then.keys(), ...now.keys()])].sort();
  return paths.map((path) => {
    const inThen = then.has(path);
    const inNow = now.has(path);
    const oldText = then.get(path) ?? null;
    const newText = now.get(path) ?? null;
    const notText = (inThen && oldText === null) || (inNow && newText === null);
    const mark: CompareMark = !inThen
      ? "added"
      : !inNow
        ? "removed"
        : notText
          ? "notText"
          : oldText === newText
            ? "unchanged"
            : "changed";
    return {
      path,
      mark,
      notText,
      rows: notText ? [] : alignLines(inThen ? oldText : null, inNow ? newText : null),
    };
  });
}

/** OCW-FR-RKCC: the short form of a commit id. */
export function shortCommit(commitId: string): string {
  return commitId.slice(0, 7);
}
