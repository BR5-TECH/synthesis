/**
 * GitHub publication — a draft's prompt as a GitHub issue
 * (`specifications/core/GHP-github-publication.md`).
 *
 * Every shape here must match its Rust counterpart in
 * `src-tauri/src/github_publication/records.rs` byte-for-byte.
 */

/** GHP-FR-PVOA: the whole typed error vocabulary publication answers with. */
export type PublicationErrorCode =
  | "no_project_open"
  | "draft_not_found"
  | "draft_archived"
  | "draft_locked_by_graduation"
  | "local_assets_unsupported"
  | "no_remote_configured"
  | "no_github_remote"
  | "issues_inaccessible"
  | "issues_disabled"
  | "issues_create_forbidden"
  | "token_unavailable"
  | "tls_untrusted"
  | "attempt_in_progress"
  | "no_attempt"
  | "github_unreachable"
  | "issue_create_failed"
  | "issue_update_failed"
  | "publication_store_write_failed"
  | "parent_issues_unreadable"
  | "issue_types_unreadable"
  | "milestones_unreadable"
  | "parent_issue_unavailable"
  | "issue_type_unavailable"
  | "milestone_unavailable"
  | "invalid_publication_choice"
  | "sub_issue_link_failed"
  | "invalid_publication_settings";

/** GHP-FR-AZPF: how a sub-issue's milestone is resolved. */
export type MilestonePolicy =
  | "inherit_parent"
  | "no_milestone"
  | "author_selected";

/**
 * GHP-FR-ATCH / GHP-FR-HSCH: what the author chose, resolved and saved before
 * the first GitHub mutation. A record or an attempt written before the choice
 * existed carries none and reads as a root issue.
 */
export interface PublicationChoice {
  kind: "root" | "sub_issue";
  parentRepositoryOwner?: string;
  parentRepositoryName?: string;
  parentIssueNumber?: number;
  issueType?: string;
  milestonePolicy?: MilestonePolicy;
  milestoneNumber?: number;
  milestoneTitle?: string;
}

/** GHP-FR-BWNI: the choice the chooser sends; a null parent is a root issue. */
export interface PublicationChoiceInput {
  parentIssueNumber: number | null;
  issueType: string | null;
  milestoneNumber: number | null;
}

/** GHP-FR-KVRH: the three project-public publication settings. */
export interface GithubPublicationSettings {
  parentIssueTypes: string[];
  subIssueType: string;
  subIssueMilestonePolicy: MilestonePolicy;
}

/** GHP-FR-DZLB: one metadata list and how its read ended. */
export interface MetadataList<T> {
  state: "loaded" | "failed";
  items: T[];
  errorCode: PublicationErrorCode | null;
  error: string | null;
}

export interface PublicationMilestone {
  number: number;
  title: string;
}

export interface PublicationIssueType {
  name: string;
}

/** GHP-FR-FTMC: one issue offered as a parent. */
export interface PublicationParentIssue {
  number: number;
  title: string;
  issueType: string;
  url: string;
  milestone: PublicationMilestone | null;
}

/** GHP-FR-MDLD: everything the publication chooser renders. */
export interface PublicationMetadata {
  repositoryOwner: string;
  repositoryName: string;
  settings: GithubPublicationSettings;
  parents: MetadataList<PublicationParentIssue>;
  issueTypes: MetadataList<PublicationIssueType>;
  milestones: MetadataList<PublicationMilestone>;
  /** GHP-FR-ETJD: `resolved` is null where the configured Type is unavailable. */
  subIssueType: { name: string; resolved: string | null };
}

/**
 * GHP-FR-JAWD: one successful publication.
 *
 * Append-only (GHP-FR-UZMX): an issue an earlier record names stays a valid
 * reference however many later publications the draft has.
 */
export interface PublicationRecord {
  provider: string;
  repositoryOwner: string;
  repositoryName: string;
  issueNumber: number;
  issueUrl: string;
  publishedAt: string;
  marker: string;
  /** GHP-FR-HSCH: absent in a record written before the choice existed. */
  choice?: PublicationChoice;
}

/** DRS-FR-VDQR: both values are non-terminal. */
export type PublicationAttemptState = "open" | "awaiting_choice";

/** GHP-FR-RUYT: what is on disk before the first GitHub request. */
export interface PublicationAttempt {
  marker: string;
  remoteName: string;
  remoteUrl: string;
  repositoryOwner: string;
  repositoryName: string;
  state: PublicationAttemptState;
  startedAt: string;
  updatedAt: string;
  /** GHP-FR-ATCH: absent in an attempt written before the choice existed. */
  choice?: PublicationChoice;
}

/** GHP-FR-MZPR / GHP-FR-LTAC: why a remote can or cannot receive an issue. */
export type PublicationRemoteEligibility =
  | "eligible"
  | "not_github"
  | "issues_inaccessible"
  | "issues_disabled"
  | "issues_create_forbidden"
  | "token_unavailable"
  | "tls_untrusted";

export interface PublicationRemote {
  name: string;
  url: string;
  kind: "github" | "other";
  repositoryOwner: string | null;
  repositoryName: string | null;
  eligibility: PublicationRemoteEligibility;
  /** GHP-FR-MZPR: rendered unchanged; `null` only when eligible. */
  reason: string | null;
}

/** GHP-FR-NDSB: how the reported selection was reached. */
export type PublicationSelectionOrigin =
  | "automatic"
  | "persisted"
  | "attempt_only"
  | "none";

export interface PublicationRemoteResolution {
  remotes: PublicationRemote[];
  selection: string | null;
  origin: PublicationSelectionOrigin;
  persistedChoice: { name: string; url: string } | null;
}

/** GHP-FR-CWTG: whether the action is offered, and why not where it is not. */
export interface PublicationEligibility {
  publishable: boolean;
  reasonCode: PublicationErrorCode | null;
  reason: string | null;
  /** GHP-FR-AKUM: every offending image reference; empty otherwise. */
  localAssets: string[];
}

export interface DraftPublicationView {
  current: PublicationRecord | null;
  /** Newest first — the order both surfaces render (GHP-FR-CWTG). */
  history: PublicationRecord[];
  attempt: PublicationAttempt | null;
  eligibility: PublicationEligibility;
}

/** GHP-FR-JAWD / GHP-FR-HRUN: what a publication attempt answers with. */
export type PublicationOutcome =
  | { kind: "published"; record: PublicationRecord }
  | {
      kind: "recoveryRequired";
      issueNumber: number;
      issueUrl: string;
      marker: string;
      /** GHP-FR-HRUN: which of these differ from the draft and the saved choice. */
      mismatches: PublicationMismatch[];
    };

export type PublicationMismatch =
  | "title"
  | "body"
  | "parent"
  | "type"
  | "milestone";

/** GHP-FR-YPGL: the two answers to a recovery choice. */
export type PublicationRecoveryChoice = "update_existing" | "publish_new";
