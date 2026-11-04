/*
 * Decomposition stand-in (DCP-draft-change-proposals.md "New-draft proposals",
 * DCR-draft-change-review.md DCR-FR-RDLK … DCR-FR-LFDJ), for UI auditing only.
 *
 * `d-19` is an original draft with a decomposition in progress: proposal
 * `np-1` is accepted (it created `d-20`), and `np-2` is pending, so the card
 * with its controls is reachable at once. Decide `np-2` and the What next? row
 * stands. `d-21` is a superseded original, and `d-22` is one of its parts, so
 * both Drafts panel badges and both chrome tags are on screen without a turn.
 */

import { NOT_HANDLED } from "./mock-github-polling";

const ARCH = {
  kind: "agent",
  agentId: "ag-arch",
  handle: "arch",
  model: "claude-opus-5",
  title: "Architect",
};

export const DECOMPOSITION_SET = "set-19";
export const DECOMPOSITION_ORIGINAL = "Release pipeline overhaul";

/** DRS-FR-MXQF: the place of a part in its decomposition. */
export const sequenceOf = (
  setId: string,
  position: number,
  total: number,
  originalDraftId: string,
  originalName: string,
) => ({ setId, position, total, originalDraftId, originalName });

const proposals: any[] = [
  {
    id: "np-1",
    setId: DECOMPOSITION_SET,
    draftId: "d-19",
    name: "Release build matrix",
    position: 1,
    total: 3,
    agent: ARCH,
    rationale: "The build matrix stands alone: it has no dependency on the rest.",
    threadId: "disc-19",
    commentId: "disc-19-c2",
    state: "accepted",
    revision: 1,
    createdDraftId: "d-20",
    createdAt: "2026-09-12T10:04:00Z",
    decidedAt: "2026-09-12T10:10:00Z",
  },
  {
    id: "np-2",
    setId: DECOMPOSITION_SET,
    draftId: "d-19",
    // Long on purpose, so the card's title row has to wrap in the column.
    name: "Release notes generator for every supported distribution channel and platform",
    position: 2,
    total: 3,
    agent: ARCH,
    rationale: "The notes generator reads the build matrix, so it comes second.",
    threadId: "disc-19",
    commentId: "disc-19-c3",
    state: "pending",
    revision: 1,
    createdDraftId: null,
    createdAt: "2026-09-12T10:14:00Z",
    decidedAt: null,
  },
];

/** DRS-FR-WMIK: a pending new-draft proposal also raises the row's marker. */
export const hasPendingNewDraft = (draftId: string) =>
  proposals.some((p) => p.draftId === draftId && p.state === "pending");

const BODIES: Record<string, string> = {
  "np-1": "# Release build matrix\n\nBuild every target of the release on each push to `main`.\n",
  "np-2":
    "# Release notes generator\n\nCollect the merged pull requests since the last tag, and " +
    "group them by area.\n\n- One section per area.\n- One line per pull request.\n",
};

/** The discussion of `d-19`, in the legacy shape the rest of the mock seeds. */
export function decompositionDiscussion(human: unknown, agent: unknown) {
  const at = (m: string) => `2026-09-12T10:${m}:00Z`;
  return {
    id: "disc-19",
    scope: "draft",
    kind: "discussion",
    draftId: "d-19",
    anchor: null,
    comments: [
      {
        id: "disc-19-c1",
        author: human,
        body: "This draft is too large for one graduation. Split it into smaller drafts.",
        quotes: [],
        attachments: [],
        createdAt: at("02"),
      },
      {
        id: "disc-19-c2",
        author: agent,
        body: "The build matrix stands alone: it has no dependency on the rest.",
        quotes: [],
        attachments: [{ kind: "newDraftProposal", proposalId: "np-1", draftId: "d-19" }],
        createdAt: at("04"),
      },
      {
        id: "disc-19-c3",
        author: agent,
        body: "The notes generator reads the build matrix, so it comes second.",
        quotes: [],
        attachments: [{ kind: "newDraftProposal", proposalId: "np-2", draftId: "d-19" }],
        createdAt: at("14"),
      },
    ],
    locked: false,
    resolved: false,
    createdAt: at("02"),
    updatedAt: at("14"),
  };
}

export interface DecompositionHooks {
  /** Create the draft an accepted proposal names, and return its record. */
  createDraft: (name: string, sequence: unknown) => any;
  /** Put the superseded marker on the original draft, and return its record. */
  supersede: (draftId: string, marker: unknown) => any;
  fire: (name: string, payload?: unknown) => void;
}

export function decompositionInvoke(
  cmd: string,
  a: Record<string, any>,
  hooks: DecompositionHooks,
): unknown {
  switch (cmd) {
    case "list_new_draft_proposals":
      return proposals
        .filter((p) => p.draftId === a.draftId)
        .sort((x, y) => y.createdAt.localeCompare(x.createdAt))
        .map((p) => ({ ...p }));
    case "load_new_draft_proposal_body": {
      const body = BODIES[String(a.proposalId)];
      // eslint-disable-next-line no-throw-literal
      if (body === undefined) throw `no such proposal: ${a.proposalId}`;
      return { body };
    }
    case "accept_new_draft_proposal":
    case "reject_new_draft_proposal": {
      const p = proposals.find((x) => x.id === a.proposalId);
      // eslint-disable-next-line no-throw-literal
      if (!p) throw `no such proposal: ${a.proposalId}`;
      // eslint-disable-next-line no-throw-literal
      if (p.state !== "pending") throw "proposal_not_pending";
      const accept = cmd === "accept_new_draft_proposal";
      p.state = accept ? "accepted" : "rejected";
      p.decidedAt = new Date().toISOString();
      let createdDraft = null;
      if (accept) {
        createdDraft = hooks.createDraft(
          p.name,
          sequenceOf(p.setId, p.position, p.total, p.draftId, DECOMPOSITION_ORIGINAL),
        );
        p.createdDraftId = createdDraft.id;
      }
      const proposal = { ...p };
      queueMicrotask(() => {
        hooks.fire("new-draft-proposals-changed", { draftId: p.draftId, proposal, cause: "decided" });
        hooks.fire("drafts-changed", {});
      });
      return { proposal, createdDraft, commentId: null, originKind: "draft_discussion" };
    }
    case "confirm_draft_decomposition": {
      const draftId = String(a.draftId);
      const accepted = proposals.filter((p) => p.draftId === draftId && p.state === "accepted");
      const record = hooks.supersede(draftId, {
        setId: DECOMPOSITION_SET,
        confirmedAt: new Date().toISOString(),
        drafts: accepted.map((p) => ({ id: p.createdDraftId, name: p.name })),
      });
      queueMicrotask(() => {
        hooks.fire("new-draft-proposals-changed", { draftId, proposal: null, cause: "confirmed" });
        hooks.fire("drafts-changed", {});
      });
      return record;
    }
    default:
      return NOT_HANDLED;
  }
}
