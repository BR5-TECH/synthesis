/**
 * The shapes of the planning conversation of the Map tab
 * (`MPR-map-plan-proposals.md`, Contract surface). Field names are the camelCase
 * names of the wire.
 */

/** MPR-FR-CEKS: one node of the Map that a thread is about. */
export interface MapNodeRef {
  lens: "delivery" | "specifications";
  kind: "item" | "milestone" | "index" | "spec";
  id: string;
  /** The name shown to the author. It takes no part in a match. */
  label: string;
}

/** MPR-FR-QUYY: the four states of a row. */
export type MapPlanRowState = "waiting" | "accepted" | "rejected" | "failed";

/** MPR-FR-YLKL: why a row cannot be accepted yet, or for good. */
export interface MapPlanBlock {
  rowKey: string;
  reason: "waiting" | "rejected" | "failed";
}

/** MPR contract: a typed error of a row. */
export interface MapPlanRowError {
  code: string;
  message: string;
}

/** MPR contract: what an accepted row produced. */
export interface MapPlanRowResult {
  issueNumber?: number;
  issueUrl?: string;
  draftId?: string;
  appliedChanges?: unknown;
}

/** MPR-FR-YCHW: the fields of a new issue. */
export interface CreateIssueFields {
  title: string;
  body?: string;
  type?: string;
  milestone?: { number: number; title: string };
  /** An existing issue by number, or another row of the proposal by key. */
  parent?: { issue: number } | { row: string };
  labels: string[];
}

/** MPR-FR-GKOL: one changed field, as it was and as it would be. */
export interface RowChange {
  field: "title" | "body" | "type" | "milestone" | "parent" | "labels" | "close" | "status";
  before: unknown;
  after: unknown;
  /** Labels only: the names the change adds. */
  added?: string[];
  /** Labels only: the names the change removes. */
  removed?: string[];
  /** A close only: `completed` or `not_planned`. */
  reason?: string;
}

/** MPR-FR-FBSH: the fields of a change to a plan issue. */
export interface ModifyIssueFields {
  number: number;
  title: string;
  baseUpdatedAt: string;
  changes: RowChange[];
}

/** MPR-FR-XPXP: the fields of a new draft. */
export interface NewDraftFields {
  name: string;
  body: string;
}

interface MapPlanRowBase {
  key: string;
  state: MapPlanRowState;
  blockedBy: MapPlanBlock | null;
  error: MapPlanRowError | null;
  result: MapPlanRowResult | null;
}

/** MPR-FR-TQVZ: one row of a proposal. */
export type MapPlanRow =
  | (MapPlanRowBase & { kind: "create_issue"; fields: CreateIssueFields })
  | (MapPlanRowBase & { kind: "modify_issue"; fields: ModifyIssueFields })
  | (MapPlanRowBase & { kind: "new_draft"; fields: NewDraftFields });

/** MPR-FR-TQVZ: one batch of rows that belongs to one comment. */
export interface MapPlanProposal {
  id: string;
  discussionId: string;
  commentId: string;
  createdAt: string;
  rows: MapPlanRow[];
}

/** MPR-FR-SGYA: the payload of `"map plan proposals changed"`. */
export interface MapPlanProposalsChangedPayload {
  discussionId: string;
}

/** CMS-FR-OIUO: the attachment of a human comment that sets the focus of a turn. */
export interface MapNodeAttachment {
  kind: "mapNode";
  node: MapNodeRef;
}

/** CMS-FR-OIUO: the reference of an agent comment to one plan proposal. */
export interface MapPlanProposalReference {
  kind: "mapPlanProposal";
  proposalId: string;
}
