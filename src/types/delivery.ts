/**
 * The shapes of the project's delivery: the delivery source and its settings
 * (`DSR-delivery-source.md`), the plan held on GitHub
 * (`GDP-github-delivery-plan.md`), and the per-machine layout
 * (`DMS-delivery-machine-state.md`). Field names are camelCase, as the backend
 * serializes them. Union discriminants and enumeration values are snake_case
 * strings.
 */
import type { FlowValidationReport } from "./core";
import type {
  FlowLeafCounts,
  FlowOverlap,
  ItemDeliveryState,
  FlowItemState,
} from "./flowPlan";
import type { GithubPollingConfiguration } from "./githubPolling";

/** DSR contract surface: a plan kind. */
export type DeliveryKind = "milestone" | "epic" | "feature" | "task" | "defect";
/** DSR contract surface: the kind of an issue. */
export type IssueKind = "epic" | "feature" | "task" | "defect";

/** DSR-FR-HOEW: the stored source choice. */
export type DeliverySourceChoice =
  | { kind: "automatic" }
  | { kind: "github" }
  | { kind: "file"; flowId: string };

/** DSR-FR-ZJMC: one read entry of the Type mapping. */
export interface DeliveryReadEntry {
  type: string;
  kind: IssueKind | "exclude";
}

/** DSR-FR-HOEW, DSR-FR-ZJMC: the delivery settings, with defaults applied. */
export interface DeliverySettings {
  source: DeliverySourceChoice;
  createTypes: { epic: string; feature: string; task: string; defect: string };
  readEntries: DeliveryReadEntry[];
  untyped: IssueKind | "exclude" | null;
}

/** DSR-FR-ZKGG. */
export type DeliveryNoneReason =
  | "no_source"
  | "github_project_unset"
  | "delivery_file_missing";

/** DSR-FR-AECK, DSR-FR-EDHP, DSR-FR-RHUM, DSR-FR-ZKGG: the resolved source. */
export type DeliverySource =
  | {
      kind: "github";
      /** MDL-FR-BZHY: `url` is empty until the first GitHub read stores it. */
      project: { nodeId: string; title: string; ownerLogin: string; number: number; url: string };
      repository: { host: string; owner: string; name: string };
      configuration: GithubPollingConfiguration;
    }
  | { kind: "file"; flowId: string; path: string }
  | { kind: "none"; reason: DeliveryNoneReason; flowId?: string | null };

/** DSR contract surface. */
export interface PublishResult {
  milestones: number;
  issues: number;
  relations: number;
  createdUrls: string[];
}

/** DSR-FR-JWYO. */
export interface DeliveryExportResult {
  flowId: string;
  path: string;
}

/** DSR-FR-FQZD: one milestone of the Dashboard's Delivery widget. */
export interface DeliveryMilestone {
  itemId: string;
  name: string;
  due: string | null;
  state: FlowItemState;
  leafCounts: FlowLeafCounts;
  blockedLeaves: number;
  current: boolean;
}

/** DSR contract surface: `"load delivery milestones (refresh)"`. */
export interface DeliveryMilestones {
  source: DeliverySource;
  planName: string | null;
  milestones: DeliveryMilestone[];
  readAt: string | null;
  stale: boolean;
}

/** GDP-FR-QGFO: the tracker facts of one item of a GitHub plan. */
export interface TrackerFacts {
  itemId: string;
  ref: { kind: "issue" | "milestone"; number: number; url: string };
  state: "open" | "closed";
  closeReason: "completed" | "not_planned" | null;
  updatedAt: string;
  status: string | null;
  claimEligible: boolean;
  externalBlockers: { number: number; title: string; url: string }[];
  /** GDP-FR-QGFO: the assignee logins of an issue, in GitHub order. Absent reads as none. */
  assignees?: string[];
}

/** GDP-FR-XWES: the notices of a GitHub plan. */
export interface GithubDeliveryNotices {
  unmappedTypes: { type: string | null; count: number }[];
  misplaced: { itemId: string; parentItemId: string }[];
  omittedRelations: {
    blockedItemId: string;
    blockingItemId: string;
    reason: "lineage" | "cycle";
  }[];
  skipped: number;
}

/**
 * GDP contract surface: `"load github delivery plan (refresh)"` and
 * `"apply github delivery edit (edit)"`. `document` is a Flow document of
 * version 3 (FGV-FR-01), as JSON.
 */
export interface GithubDeliveryPlan {
  document: unknown;
  tracker: TrackerFacts[];
  states: ItemDeliveryState[];
  overlaps: FlowOverlap[];
  notices: GithubDeliveryNotices;
  readAt: string;
  stale: boolean;
  error: DeliveryError | null;
}

/** GDP contract surface: one edit. `containerId` is null for the top level. */
export type GithubDeliveryEdit =
  | {
      kind: "create_item";
      itemKind: DeliveryKind;
      name: string;
      containerId: string | null;
      description?: string;
      due?: string;
    }
  | { kind: "rename"; itemId: string; name: string; updatedAt: string }
  | { kind: "set_description"; itemId: string; description: string; updatedAt: string }
  | { kind: "set_due"; itemId: string; due: string | null; updatedAt: string }
  | { kind: "set_kind"; itemId: string; itemKind: DeliveryKind; updatedAt: string }
  | { kind: "move"; itemId: string; containerId: string | null; updatedAt: string }
  | {
      kind: "add_edge";
      from: string;
      to: string;
      type: "blocks" | "relates";
      updatedAt: string;
    }
  | {
      kind: "remove_edge";
      from: string;
      to: string;
      type: "blocks" | "relates";
      updatedAt: string;
    }
  | {
      kind: "remove_item";
      itemId: string;
      mode: "remove_from_board" | "close_not_planned";
      updatedAt: string;
    }
  | { kind: "add_existing_issue"; number: number; containerId: string | null }
  | { kind: "set_references"; itemId: string; artifactIds: string[] }
  | { kind: "set_footprint"; itemId: string; footprint: string[] }
  | { kind: "link_draft"; itemId: string; draftId: string }
  | { kind: "unlink_draft"; itemId: string }
  | { kind: "set_plan_references"; references: string[] }
  /** FLO-FR-OGPF / GDP-FR-YBHD: the milestone order, first to last. */
  | { kind: "set_milestone_order"; milestoneIds: string[] };

/**
 * The typed error of the delivery operations (DSR, GDP, DMS). `code` is one of
 * the codes those specs name; `message` is displayable text.
 */
/** GDP-FR-QFQF: the full detail of one plan issue. */
export interface GithubPlanIssueDetail {
  number: number;
  url: string;
  title: string;
  body: string;
  issueType: string | null;
  state: string;
  closeReason: string | null;
  milestone: number | null;
  milestoneTitle: string | null;
  parent: number | null;
  subIssues: number[];
  /** Empty where the answer is stale. */
  labels: string[];
  status: string | null;
  blockedBy: number[];
  updatedAt: string;
}

/** GDP-FR-WQEN / GDP-FR-KRMB: the answer of `"read github plan issue (number)"`. */
export interface GithubPlanIssueRead {
  issue: GithubPlanIssueDetail;
  /** True where the GitHub read failed and the detail comes from the read cache. */
  stale: boolean;
  error: DeliveryError | null;
  /** The `read_at` of the cache, where stale. */
  readAt: string | null;
}

export interface DeliveryError {
  code: string;
  message: string;
  report?: FlowValidationReport;
  /** DSR-FR-ULON, GDP-FR-MSVS: the tracker items a failed call created. */
  createdUrls?: string[];
}

/** DMS-FR-DMWD: one item's entry in a layout scope. */
export interface DeliveryLayoutEntry {
  position?: { x: number; y: number };
  size?: { width: number; height: number };
  collapsed?: boolean;
}

/** DMS contract surface. */
export interface DeliveryLayout {
  items: Record<string, DeliveryLayoutEntry>;
}

/** DMS-FR-TXMH: a board status (per DBR-FR-QZPA). */
export type DeliveryBoardStatus = "backlog" | "ready" | "in progress" | "in review" | "done";

/** DMS contract surface: the statuses a scope's board hides, and Blocked only. */
export interface DeliveryView {
  hidden: DeliveryBoardStatus[];
  blockedOnly: boolean;
}

/** DMS-FR-DMWD: the layout scope of a Flow file or of a GitHub plan. */
export const flowLayoutScope = (flowId: string) => `flow:${flowId}`;
export const githubLayoutScope = (projectNodeId: string) => `github:${projectNodeId}`;
