import { recordEdit } from "../state/editHistory";
import type { EditSessionStore } from "../state/editSessions";
import type { DraftChangeProposal, ProjectHandle } from "../types";

/** DCR-FR-33: a proposal of the outgoing content root. */
export const pendingProposal: DraftChangeProposal = {
  id: "dp1",
  draftId: "d-out",
  path: "spec.md",
  agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
  rationale: "why",
  threadId: "t1",
  commentId: "c1",
  state: "pending",
  candidateEdited: false,
  legacy: false,
  hunkCount: 1,
  counts: { pending: 1, accepted: 0, rejected: 0, discussing: 0 },
  ledger: [
    { id: "h1", kind: "replace", state: "pending", edited: false, revision: 0 },
  ],
  createdAt: "2026-01-01T00:00:00Z",
};

/** The Flow the Flow-tab tests open, and the single-node graph it holds. */
export const FLOW_ID = "workflows/h.flow";
export const FLOW_BODY = JSON.stringify({
  version: 1,
  nodes: [{ id: "n1", name: "A", position: { x: 0, y: 0 } }],
  edges: [],
});

/**
 * A project handle as an open/create returns it (PST-FR-01). `path` is the
 * project's identity anchor and `activeWorktreePath` the content root; for a
 * project outside a Git repository they are the same path.
 */
export const handle = (name: string, path: string): ProjectHandle => ({
  name,
  path,
  activeWorktreePath: path,
});

/** Seed an artifact with an unsaved edit, as an Editor would have left it. */
export function seedDirty(sessions: EditSessionStore, id: string, body: string) {
  sessions.adoptLoad(id, body, "ck1");
  const s = sessions.ensure(id);
  s.buffer = `${body} edited`;
  recordEdit(s.history, s.buffer, "wysiwyg", "body");
  sessions.update(id, { dirty: true });
}
