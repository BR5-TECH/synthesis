// The versions and the statistics of one OpenSpec change on a work stream
// (CVH-change-version-history.md and CST-change-statistics.md, contract
// surfaces).

import type { Measure, TokenPair } from "./statistics";

/** CVH-FR-VZGR: one version of a change, which is one commit of the stream branch. */
export interface ChangeVersion {
  /** The full hexadecimal commit id. */
  commitId: string;
  /** The first line of the commit message. */
  summary: string;
  authorName: string;
  /** RFC 3339 UTC. */
  committedAt: string;
  /** CVH-FR-URCF: change-relative paths. */
  added: string[];
  modified: string[];
  deleted: string[];
}

/** CVH-FR-VZGR / CVH-FR-DAAQ: what `"list change versions"` returns. */
export interface ChangeVersionList {
  /** Most recent first. */
  versions: ChangeVersion[];
  /** CVH-FR-DAAQ: the list holds the 500 newest versions alone. */
  truncated: boolean;
}

/** CVH-FR-VDZZ: one file of the change folder as one version holds it. */
export interface ChangeVersionFile {
  /** Change-relative. */
  path: string;
  /** Present where the bytes decode as UTF-8. */
  text?: string | null;
  bytes: number;
  /** True where `text` is absent. */
  binary: boolean;
}

/** CVH-FR-VDZZ: what `"read change version"` returns. */
export interface ChangeVersionFiles {
  files: ChangeVersionFile[];
}

/** CVH contract: the typed refusals of the two reads. */
export const CHANGE_VERSION_ERRORS = {
  noProjectOpen: "no_project_open",
  unknownStream: "unknown_stream",
  invalidChangeName: "invalid_change_name",
  changeNotFound: "change_not_found",
  unknownVersion: "unknown_version",
  readFailed: "read_failed",
} as const;

/** CST-FR-WPHC: what `"read change statistics"` returns. */
export interface ChangeStatistics {
  streamId: string;
  change: string;
  /** CST-FR-NCLF: null where the change has no activation boundary. */
  boundaryAt: string | null;
  workingTimeMs: Measure;
  aiInteractions: Measure;
  versions: Measure;
  acceptedProposals: Measure;
  rejectedProposals: Measure;
  conversationTokens: TokenPair;
  unreadableLines: number;
}

/** CST-FR-PSUF: what `"change statistics changed"` carries. */
export interface ChangeStatisticsChangedPayload {
  streamId: string;
  change: string;
}
