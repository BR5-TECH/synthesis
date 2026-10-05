// The artifact tree and the tabs over it
//
// Split out of one `types.ts` that had grown past four thousand lines.
// Every name is re-exported from `./index`, so `from "../types"` still
// resolves to the same set and no import site moved.

import type { DiscussionTarget } from "./comments";
import { DiffTarget } from "./diff";
import { SearchMode } from "./search";

export type ArtifactType =
  | "skill"
  | "agent"
  | "prompt"
  | "spec"
  | "flow"
  | "instructions"
  | "scenario"
  | "scratchpad";

export type NodeType =
  | "project"
  | "workstream"
  | "role"
  | "playbook"
  | ArtifactType;

export type Status = "draft" | "active" | "inactive" | "deleted";

export interface ArtifactRef {
  name: string;
  type: ArtifactType | NodeType;
  chip?: string;
}

export interface LibraryNode {
  name: string;
  type: NodeType;
  chip?: string;
  status?: Status;
  count?: number;
  expanded?: boolean;
  children?: LibraryNode[];
}

// Filesystem-derived Library tree (ASC-artifact-scanning.md / LIB-FR-02).
// Mirrors the Rust `scanning::TreeNode` wire shape (camelCase) produced by
// `load_project_tree`. Folders carry `hasArtifacts` + `children`; files carry
// `artifactType` + `typeSource` when classified.
export type NodeKind = "folder" | "file";
export type TypeSource = "inferred" | "assigned" | "inherited";
export type AssignScope = "file" | "folder";

export interface TreeNode {
  id: string;
  name: string;
  path: string;
  nodeKind: NodeKind;
  artifactType?: ArtifactType;
  typeSource?: TypeSource;
  /**
   * ASC-FR-19: the name the file declares for itself, present only for the
   * artifacts whose filename does not identify them — today a skill, which
   * always lives in a `SKILL.md`. Absent means "go by the basename".
   */
  displayName?: string;
  hasArtifacts?: boolean;
  children?: TreeNode[];
}

// Payload of the backend `"project tree changed"` event (ASC-FR-10 / ASC-FR-22).
export interface TreeChangedPayload {
  changeCount: number;
  /**
   * ASC-FR-22: the project-relative paths that no longer exist once this
   * coalesced burst is applied. A deleted folder contributes its own path
   * rather than enumerating what was beneath it, so a consumer treats a removed
   * folder as removing everything under it (TAB-FR-19). The source side of a
   * rename is a removed path like any other — the path is gone whatever took
   * its place, and nothing correlates it with the destination.
   *
   * Always present; a burst that removed nothing carries an empty list.
   * Optional here only so a payload from an older backend does not crash the
   * consumer that reads it.
   */
  removedPaths?: string[];
}

// Artifact contents contract (PST-FR-15 / EXC-FR-WCOM). `load_artifact_contents_by_id`
// returns the body plus the sha256 checksum the Editor keeps as its
// external-change baseline; `save_artifact_contents` returns the checksum of the
// bytes written (the new baseline after a save).
export interface ArtifactContents {
  body: string;
  checksum: string;
}

export interface SaveResult {
  checksum: string;
}

/**
 * One thing wrong with a Flow document (`../specifications/core/FGV-flow-graph-validation.md`
 * FGV-FR-04). `elementId` names the node or loop it sits on, `edgeId` names the
 * edge, and a violation about the document as a whole carries neither.
 */
export interface FlowViolation {
  code: string;
  message: string;
  elementId?: string;
  edgeId?: string;
}

/** What `"validate flow document"` returns (FGV-FR-02). */
export interface FlowValidationReport {
  valid: boolean;
  violations: FlowViolation[];
}

// Payload of the backend `"artifact changed externally"` event (PST-FR-16).
// The Editor compares `checksum` against its baseline to tell a real external
// change from its own save (EXC-FR-LKHZ).
export interface ArtifactChangedPayload {
  artifactId: string;
  checksum: string;
}

// The minimal shape `App.openArtifact` accepts. Both the filesystem Library
// tree (which supplies `id` + `artifactType`) and the Dashboard's canned rows
// (which supply `type`) satisfy it, so the open handler stays shared.
export interface OpenableArtifact {
  id?: string;
  name: string;
  type?: NodeType;
  artifactType?: ArtifactType;
  chip?: string;
}

/**
 * TAB-FR-37: which of the tab context menu's three mass closes is being run,
 * named by the range it takes relative to the context-clicked tab.
 *
 * Each takes every *unpinned* tab in that range and never the context-clicked
 * tab itself, so the range is only half of what decides the eligible set.
 */
export type TabCloseScope = "left" | "right" | "others";

/**
 * TAB-FR-40: what a mass close names when it was asked for from the **Home
 * affordance** rather than from a tab.
 *
 * The affordance is a control rather than a tab (per `SNV-shell-navigation.md`
 * SNV-FR-09), so it has no tab id to be the target — but it does have a
 * position, the head of the strip, and that is the whole of what a mass close
 * needs from it. The value is deliberately not a shape any tab id takes (those
 * are `dashboard`, or a `kind:…` / path-derived key), so it can never collide
 * with a real tab.
 */
export const HOME_TAB_TARGET = "\u0000home-affordance";

export interface Tab {
  id: string;
  label: string;
  /**
   * What the tab's own tooltip says, when the label alone does not tell it
   * apart. Two Diff tabs on one file under different comparisons carry
   * identical labels by design, and the tooltip is what distinguishes them
   * (DFV-FR-03). Absent -> the label is the tooltip.
   */
  tooltip?: string;
  chip?: string;
  type?: NodeType;
  /** `map` is the one Map tab of the project (`SMP-specification-map.md`). */
  kind?:
    | "editor"
    | "flow"
    | "diff"
    | "search"
    | "draft"
    | "conversation"
    | "map"
    | "document"
    | "pdf";
  dirty?: boolean;
  // The backend artifact id (project-relative path, ASC-FR-13) when this tab
  // was opened from a real filesystem node. Present -> the Editor loads/saves
  // live via the backend; absent (e.g. a Dashboard canned row) -> mock content.
  //
  // A Diff tab deliberately leaves this unset: it owns no savable content, so
  // File -> Save is greyed out while it is active (SNV-FR-28 / CHG-FR-20).
  artifactId?: string;
  // CHG-FR-18: what a Diff tab renders. Present only on `kind: "diff"` tabs.
  diff?: DiffTarget;
  /**
   * TAB-FR-17: the id of the draft a New Artifact tab is bound to. A draft is
   * not a file, so it is identified by its own id rather than by a path — which
   * is what lets a draft and an artifact of the same name coexist in the strip.
   */
  draftId?: string;
  /**
   * SCH-FR-08: the query and mode a Search results tab sweeps. Present only on
   * `kind: "search"` tabs. Like a Diff tab it owns no savable content, so it
   * carries no `artifactId` and File → Save stays greyed out (SNV-FR-28).
   */
  search?: { query: string; mode: SearchMode };
  /**
   * TAB-FR-23: the id of the conversation a **conversation tab** is bound to,
   * rather than a path — a conversation is not a file and outlives every file it
   * was about (`CVP-conversation-presentation.md` CVP-FR-01). The single-tab rule
   * binds it on those terms, and no path removal closes it (TAB-FR-24).
   * Absent while a note's conversation is in its opening state.
   */
  threadId?: string;
  /**
   * TAB-FR-QXRF: the id of the document a **Document tab** or a **PDF Viewer
   * tab** shows, rather than a path — a viewer tab names no file of the project
   * and holds no savable content, so no path removal reaches it (TAB-FR-19).
   */
  documentId?: string;
  /** CVP-FR-60: the note a conversation tab is about, before and after it opens. */
  noteId?: string;
  /** CVP-FR-45: the owner of the conversation, for the availability check. */
  ownerTarget?: DiscussionTarget;
  /** CVP-FR-57: the owner's name in `Chat: <owner>`. */
  ownerLabel?: string;
  /** CVP-FR-57: the conversation's subject, which follows the name. */
  subject?: string;
}
