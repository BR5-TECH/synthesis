/**
 * Builders for the tests of the planning conversation of the Map tab
 * (`MPR-map-plan-proposals.md`, `MPV-map-proposal-review.md`).
 */
import type {
  Comment,
  CreateIssueFields,
  Discussion,
  MapNodeRef,
  MapPlanProposal,
  MapPlanRow,
  ModifyIssueFields,
  Participant,
  RowChange,
} from "../types";

export const HUMAN: Participant = { kind: "human", login: "raver119" };
export const AGENT: Participant = { kind: "agent", agentId: "a1", handle: "arch", model: "m" };

/** A node of the Specifications lens. */
export const SPEC_NODE: MapNodeRef = {
  lens: "specifications",
  kind: "spec",
  id: "SMP",
  label: "Specification map",
};

/** A node of the Delivery lens. */
export const ITEM_NODE: MapNodeRef = {
  lens: "delivery",
  kind: "item",
  id: "issue-47",
  label: "Decline message",
};

export function mapComment(
  id: string,
  author: Participant,
  body: string,
  over: Partial<Comment> = {},
): Comment {
  return {
    id,
    author,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

/** A Map thread (no node) or a node thread. */
export function mapThread(
  id: string,
  bodies: string[],
  node: MapNodeRef | null = null,
  over: Partial<Discussion> = {},
): Discussion {
  return {
    id,
    target: { kind: "map" },
    fragmentTarget: null,
    ...(node ? { mapNode: node } : {}),
    comments: bodies.map((body, at) => mapComment(`${id}-c${at}`, HUMAN, body)),
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

const ROW_BASE = { state: "waiting", blockedBy: null, error: null, result: null } as const;

export function createRow(
  key: string,
  title: string,
  over: Partial<MapPlanRow> = {},
  fields: Partial<CreateIssueFields> = {},
): MapPlanRow {
  return {
    ...ROW_BASE,
    key,
    kind: "create_issue",
    fields: { title, labels: [], ...fields },
    ...over,
  } as MapPlanRow;
}

export function modifyRow(
  key: string,
  number: number,
  title: string,
  changes: RowChange[],
  over: Partial<MapPlanRow> = {},
): MapPlanRow {
  const fields: ModifyIssueFields = { number, title, baseUpdatedAt: "2026-01-01T00:00:00Z", changes };
  return { ...ROW_BASE, key, kind: "modify_issue", fields, ...over } as MapPlanRow;
}

export function draftRow(
  key: string,
  name: string,
  body: string,
  over: Partial<MapPlanRow> = {},
): MapPlanRow {
  return { ...ROW_BASE, key, kind: "new_draft", fields: { name, body }, ...over } as MapPlanRow;
}

export function proposal(
  rows: MapPlanRow[],
  over: Partial<MapPlanProposal> = {},
): MapPlanProposal {
  return {
    id: "p1",
    discussionId: "t1",
    commentId: "c1",
    createdAt: "2026-01-01T00:00:00Z",
    rows,
    ...over,
  };
}

/** The same proposal with one row replaced by `over`. */
export function withRow(
  source: MapPlanProposal,
  key: string,
  over: Partial<MapPlanRow>,
): MapPlanProposal {
  return {
    ...source,
    rows: source.rows.map((r) => (r.key === key ? ({ ...r, ...over } as MapPlanRow) : r)),
  };
}
