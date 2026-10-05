// Diff payload (GTC-git.md `get_diff`)
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import { ArtifactType } from "./core";

// ---------------------------------------------------------------------------
// Diff payload (GTC-git.md `get_diff`)
// ---------------------------------------------------------------------------

/**
 * What a `get_diff` call is asking about. `previousPath` is a renamed entry's
 * pre-rename path: both halves of a rename must be in view or the backend
 * cannot pair them, and the diff would read as a whole-file addition,
 * contradicting the `+n −n` the row that opened the tab already showed.
 */
export type DiffScope =
  | { kind: "path"; path: string; previousPath?: string }
  | { kind: "staged" }
  | {
      kind: "branch";
      path: string;
      targetBranch: string;
      previousPath?: string;
    };

export interface DiffLine {
  kind: "add" | "del" | "context";
  oldLineno?: number;
  newLineno?: number;
  content: string;
}

export interface DiffHunk {
  header: string;
  lines: DiffLine[];
}

/** Unified-diff hunks for text; an untouched-binary marker for non-text. */
export interface DiffPayload {
  isBinary: boolean;
  hunks: DiffHunk[];
}

/**
 * Both sides of a comparison for one file, whole (GTC-FR-16 / DFV-FR-25) — what
 * the side-by-side, final, and rich modes render from.
 *
 * `null` on a side means the comparison has no version of the file there: an
 * added file has no `old`, a deleted one has no `new`. That is deliberately
 * distinct from `""`, a file that exists and is empty, which the tab renders
 * differently (DFV-FR-15).
 */
export interface FileRevisions {
  old: string | null;
  new: string | null;
  isBinary: boolean;
}

/**
 * DFV-FR-08: how a Diff tab lays a comparison out. User-global (GSS-FR-24), so
 * every open Diff tab renders in the same one (DFV-FR-24).
 */
export type DiffVisualizationMode = "unified" | "side_by_side" | "final";

/** DFV-FR-16: a file's literal text, or — for Markdown — its rendered document. */
export type DiffRenderingMode = "source" | "rich";

/**
 * What a Diff tab shows: one file, under one comparison (CHG-FR-19). The pair
 * is the tab's identity, so the same file under a different comparison opens a
 * second tab rather than replacing the first.
 */
export interface DiffTarget {
  /** Project-relative path of the file being diffed. */
  path: string;
  /** Basename, for the tab label. */
  name: string;
  scope: DiffScope;
  /** Short label for the comparison, shown in the Diff tab's header. */
  comparisonLabel: string;
  /**
   * DFV-FR-17: the file's resolved artifact type (ASC-FR-06), which is what
   * decides whether a rich rendering applies to it. Resolved rather than
   * inferred from the name, so a user's type assignment overrides the path
   * convention here exactly as it does everywhere else. Absent for a file the
   * scan classifies as nothing.
   */
  artifactType?: ArtifactType;
}

// A recently-edited Dashboard item (DSH-FR-09), as served by
// `list_recently_edited_artifacts` (PST-FR-12). `kind` is the routing hint
// (DSH-FR-06): "flow" opens the Flow tab, "markdown" opens the Editor, and
// "text" — a file the scan surfaces with no artifact type — opens the Editor as
// a plain text file (PST-FR-24 / ESH-FR-ATDS).
export type ArtifactKind = "markdown" | "flow" | "text";

/** What `open_artifact_by_id` answers (PST-FR-08 / PST-FR-24). */
export interface OpenedArtifact {
  key: string;
  kind: ArtifactKind;
}
