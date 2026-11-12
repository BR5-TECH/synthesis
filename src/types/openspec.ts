// OpenSpec mode (OSM-openspec-mode.md, contract surface)

import type { Participant } from "./comments";

/** PSS-FR-IMDN: the stored OpenSpec mode setting. `auto` is the unset state. */
export type OpenSpecModeSetting = "auto" | "off";

/** OSM-FR-SKUP: why a found CLI did not report a usable version. */
export type OpenSpecCliFailure = "not_executable" | "timed_out" | "no_version";

/** OSM-FR-SKUP: the state of the OpenSpec CLI on this machine. */
export type OpenSpecCliState =
  | { state: "available"; path: string; version: string }
  | { state: "not_found" }
  | { state: "unsupported"; path: string; version: string }
  | { state: "failed"; path: string; reason: OpenSpecCliFailure };

/** OSM-FR-YXUC: what `"get openspec status"` returns. */
export interface OpenSpecStatus {
  setting: OpenSpecModeSetting;
  usesOpenspec: boolean;
  active: boolean;
  cli: OpenSpecCliState;
}

/** OSM-FR-MFVN: where a change is read. The frontend names two of the three kinds. */
export type ChangeLocation =
  | { kind: "active_worktree" }
  | { kind: "stream"; streamId: string };

/** OSM-FR-LLIO: the status of one artifact of a change's schema. */
export type ChangeArtifactStatus = "done" | "ready" | "blocked" | "skipped";

/** OSM-FR-LLIO: one file of an artifact, with its change-relative path. */
export interface ChangeFile {
  path: string;
  text: string | null;
  unreadable: boolean;
}

/** OSM-FR-LLIO: one artifact of the schema, in build order. */
export interface ChangeArtifact {
  id: string;
  status: ChangeArtifactStatus;
  files: ChangeFile[];
}

/** OSM contract: one scenario of a requirement, as raw text. */
export interface Scenario {
  name: string;
  text: string;
}

/** OSM contract: one requirement, with the name OpenSpec matches deltas on. */
export interface Requirement {
  name: string;
  text: string;
  scenarios: Scenario[];
}

/** OSM-FR-NMHS: the operation of a delta. */
export type DeltaOperation = "ADDED" | "MODIFIED" | "REMOVED" | "RENAMED";

/** OSM-FR-NMHS / OSM-FR-PYHP: one delta, with the live requirement it replaces. */
export interface ChangeDelta {
  capability: string;
  /** Whether the live specs hold this capability. */
  capabilityExists: boolean;
  operation: DeltaOperation;
  requirement: Requirement | null;
  rename: { from: string; to: string } | null;
  live: Requirement | null;
}

/** OSM-FR-MBTI: what `"read openspec change"` returns. */
export interface ChangeDetail {
  name: string;
  location: ChangeLocation;
  schema: string;
  isComplete: boolean;
  completedTasks: number;
  totalTasks: number;
  artifacts: ChangeArtifact[];
  deltas: ChangeDelta[];
}

/** OSM contract: the codes a read of a change rejects with. */
export type OpenSpecChangeReadError =
  | "no_open_project"
  | "openspec_mode_inactive"
  | "cli_unavailable"
  | "unknown_stream"
  | "invalid_change_name"
  | "change_not_found"
  /** OSM-FR-GMKV: rejected as `change_archived: <archived_as>`. */
  | "change_archived"
  | "cli_timed_out"
  | "cli_failed"
  | "cli_output_unreadable";

/**
 * GRD-FR-LKGC / OCR contract (`OpenSpecRunData`): the `openspec` data of a
 * propose run, an apply run, or a rework run.
 */
export interface GraduationOpenspec {
  phase: "propose" | "apply" | "rework";
  change: string;
  /** OCR-FR-TBGA: a rework run's instruction. */
  instruction?: string | null;
  /** OCR-FR-TBGA: a rework run's request thread. */
  threadId?: string | null;
  /** OCR-FR-TBGA: the participant a rework run's finish writes as. */
  requester?: Participant | null;
  /** OCR-FR-GWGE: the proposal a rework run's finish recorded. */
  proposalId?: string | null;
}

/** DRS-FR-VHMB: the change a draft's propose run writes, and its stream. */
export interface DraftOpenspecChange {
  name: string;
  streamId: string;
}
