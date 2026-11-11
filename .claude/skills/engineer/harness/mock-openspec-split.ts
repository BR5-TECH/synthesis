/*
 * The History, the statistics, and the split of the demo change
 * (CVH-change-version-history.md, CST-change-statistics.md,
 * NCP-new-change-proposals.md, OCW-openspec-change-workspace.md OCW-FR-EJGH
 * through OCW-FR-FMDZ), for UI auditing only.
 *
 * Open the change `add-logout-3f2a` on its stream (see mock-openspec.ts).
 * - History lists three versions. `?changeHistory=truncated` marks the list
 *   truncated, and `?changeHistory=fail` makes the list refuse.
 * - Information reads the statistics. `?changeStatistics=unavailable` reports
 *   every figure unavailable, and `?changeStatistics=fail` makes the read refuse.
 * - The discussion "Split the change" holds a pending new-change proposal
 *   (part 1 of 2). Accept it, and the tab offers Confirm split.
 *   `?newChangeAccept=stream_busy` makes the acceptance refuse with that code,
 *   and `?newChangeCommit=fail` makes its commit fail, so the Error toast
 *   offers Retry.
 */

import { NOT_HANDLED } from "./mock-github-polling";

const STREAM = "s-1";
const CHANGE = "add-logout-3f2a";
const AGENT = { kind: "agent", agentId: "claude", handle: "claude", model: "opus-5", title: "" };
const ME = { kind: "human", login: "" };

function query(name: string): string | null {
  if (typeof location === "undefined") return null;
  return new URLSearchParams(location.search).get(name);
}

function fire(name: string, payload: unknown) {
  queueMicrotask(() => {
    const bus = (globalThis as Record<string, unknown>).__fireBusEvent as
      | ((n: string, p?: unknown) => void)
      | undefined;
    bus?.(name, payload);
  });
}

function ours(a: Record<string, any>): boolean {
  return a.streamId === STREAM && a.change === CHANGE;
}

// --- CVH: three versions of the demo change, newest first.

const VERSIONS = [
  {
    commitId: "9c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d",
    summary: "Accept the edits to add-logout-3f2a",
    authorName: "Synthesis",
    committedAt: "2026-09-07T11:30:00Z",
    added: [],
    modified: ["proposal.md"],
    deleted: [],
  },
  {
    commitId: "5e1f0c2a9b8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f",
    summary: "Add the design of add-logout-3f2a",
    authorName: "claude",
    committedAt: "2026-09-07T09:20:00Z",
    added: ["design.md", "diagram.png"],
    modified: ["tasks.md"],
    deleted: [],
  },
  {
    commitId: "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b",
    summary: "Propose add-logout-3f2a",
    authorName: "claude",
    committedAt: "2026-09-07T09:05:00Z",
    added: ["proposal.md", "specs/auth/spec.md", "tasks.md"],
    modified: [],
    deleted: [],
  },
];

const OLD_PROPOSAL =
  "# Why\n\nUsers cannot log out.\n\n## What Changes\n\n- Add a **Log out** entry.\n\n![The log out entry](./images/logout.png)\n";
const OLD_TASKS = "## 1. Implementation\n\n- [ ] 1.1 Add the Log out entry\n";

function versionFiles(commitId: string) {
  const oldest = commitId === VERSIONS[2].commitId;
  const files = [
    { path: "proposal.md", text: OLD_PROPOSAL, bytes: OLD_PROPOSAL.length, binary: false },
    { path: "specs/auth/spec.md", text: "## MODIFIED Requirements", bytes: 24, binary: false },
    { path: "tasks.md", text: OLD_TASKS, bytes: OLD_TASKS.length, binary: false },
  ];
  if (!oldest) {
    files.splice(0, 0, { path: "design.md", text: "## Context\n\nThe session store keeps the last activity time.", bytes: 60, binary: false });
    files.splice(1, 0, { path: "diagram.png", text: undefined as unknown as string, bytes: 2048, binary: true });
  }
  return files.sort((a, b) => a.path.localeCompare(b.path));
}

// --- CST: the statistics of the demo change.

function statistics() {
  const mode = query("changeStatistics");
  // eslint-disable-next-line no-throw-literal
  if (mode === "fail") throw "read_failed";
  const none = { value: null, availability: "unavailable" };
  const m = (value: number) => (mode === "unavailable" ? none : { value, availability: "available" });
  return {
    streamId: STREAM,
    change: CHANGE,
    boundaryAt: mode === "unavailable" ? null : "2026-09-07T09:00:00Z",
    workingTimeMs: m(47 * 60_000),
    aiInteractions: m(9),
    versions: m(VERSIONS.length),
    acceptedProposals: m(1),
    rejectedProposals: { value: 0, availability: mode === "unavailable" ? "unavailable" : "incomplete" },
    conversationTokens: { input: m(48_210), output: m(6_031) },
    unreadableLines: 0,
  };
}

// --- NCP: the split of the demo change.

const proposal: any = {
  id: "ncp-1",
  setId: "split-1",
  streamId: STREAM,
  change: CHANGE,
  name: "add-logout-entry",
  paths: ["proposal.md", "specs/ui/spec.md", "tasks.md"],
  position: 1,
  total: 2,
  agent: AGENT,
  rationale: "The Log out entry of the UI is a change of its own.",
  threadId: "ocd-split",
  commentId: "ocd-split-c2",
  state: "pending",
  revision: 0,
  createdChange: undefined,
  commitOwed: false,
  createdAt: "2026-09-07T12:00:00Z",
  decidedAt: undefined,
};
let confirmedAt: string | null = null;

const PROPOSED_FILES = [
  { path: "proposal.md", text: "# Why\n\nEvery page needs a Log out entry.\n" },
  { path: "specs/ui/spec.md", text: "## ADDED Requirements\n\n### Requirement: Logout Entry\n\nThe header of every page SHALL show a Log out entry.\n" },
  { path: "tasks.md", text: "## 1. Implementation\n\n- [ ] 1.1 Add the entry to the header\n" },
];

/** The discussion whose agent comment carries the new-change proposal. */
export const SPLIT_DISCUSSION: any = {
  id: "ocd-split",
  target: { kind: "change", streamId: STREAM, change: CHANGE },
  fragmentTarget: null,
  resolved: false,
  createdAt: "2026-09-07T11:55:00Z",
  updatedAt: "2026-09-07T12:00:00Z",
  comments: [
    {
      id: "ocd-split-c1",
      author: ME,
      body: "@claude this change is too big. Please split it.",
      quotes: [],
      attachments: [],
      createdAt: "2026-09-07T11:55:00Z",
    },
    {
      id: "ocd-split-c2",
      author: AGENT,
      body: "The Log out entry of the UI is a change of its own.",
      quotes: [],
      attachments: [{ kind: "newChangeProposal", proposalId: "ncp-1", streamId: STREAM, change: CHANGE }],
      createdAt: "2026-09-07T12:00:00Z",
    },
  ],
};

function split() {
  return {
    setId: "split-1",
    accepted: proposal.state === "accepted" ? [proposal.createdChange ?? proposal.name] : [],
    confirmedAt: confirmedAt ?? undefined,
  };
}

function decide(accept: boolean, feedback: unknown) {
  // eslint-disable-next-line no-throw-literal
  if (proposal.state !== "pending") throw "already_decided";
  // eslint-disable-next-line no-throw-literal
  if (confirmedAt) throw "split_confirmed";
  const busy = accept ? query("newChangeAccept") : null;
  // eslint-disable-next-line no-throw-literal
  if (busy) throw busy;
  const at = new Date().toISOString();
  proposal.state = accept ? "accepted" : "rejected";
  proposal.decidedAt = at;
  let committed = false;
  if (accept) {
    proposal.createdChange = proposal.name;
    committed = query("newChangeCommit") !== "fail";
    proposal.commitOwed = !committed;
  }
  const commentId = `ocd-split-c${SPLIT_DISCUSSION.comments.length + 1}`;
  const note = typeof feedback === "string" && feedback !== "" ? ` ${feedback}` : "";
  SPLIT_DISCUSSION.comments.push({
    id: commentId,
    author: ME,
    body: `${accept ? "Accepted" : "Rejected"} part 1 of 2, ${proposal.name}.${note}`,
    quotes: [],
    attachments: [],
    createdAt: at,
  });
  SPLIT_DISCUSSION.updatedAt = at;
  fire("discussion-changed", SPLIT_DISCUSSION);
  fire("new-change-proposals-changed", { streamId: STREAM, change: CHANGE, proposal: { ...proposal }, cause: "decided" });
  return { proposal: { ...proposal }, committed, commentId, originKind: "change_discussion" };
}

/** Handle one History, statistics, or split command of the demo change, or answer NOT_HANDLED. */
export function openspecSplitInvoke(cmd: string, a: Record<string, any>): unknown {
  switch (cmd) {
    case "list_change_versions": {
      const mode = query("changeHistory");
      // eslint-disable-next-line no-throw-literal
      if (mode === "fail") throw "read_failed";
      // eslint-disable-next-line no-throw-literal
      if (!ours(a)) throw "change_not_found";
      return { versions: VERSIONS.map((v) => ({ ...v })), truncated: mode === "truncated" };
    }
    case "read_change_image": {
      // OSM-FR-BLEY: a 1x1 PNG for an image path of the demo change.
      // eslint-disable-next-line no-throw-literal
      if (!ours(a) || !/\.png$/.test(String(a.path))) throw "file_not_found";
      return {
        data: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        mediaType: "image/png",
      };
    }
    case "read_change_version":
      // eslint-disable-next-line no-throw-literal
      if (!ours(a) || !VERSIONS.some((v) => v.commitId === a.commitId)) throw "unknown_version";
      return { files: versionFiles(String(a.commitId)) };
    case "read_change_statistics":
      return statistics();
    case "record_change_working_interval":
      return null;
    case "list_new_change_proposals":
      if (!ours(a)) return { proposals: [], split: null };
      return { proposals: [{ ...proposal }], split: split() };
    case "load_new_change_proposal_files":
      // eslint-disable-next-line no-throw-literal
      if (a.proposalId !== proposal.id) throw "proposal_not_found";
      return { files: PROPOSED_FILES.map((f) => ({ ...f })) };
    case "accept_new_change_proposal":
      return decide(true, a.feedback);
    case "reject_new_change_proposal":
      return decide(false, a.feedback);
    case "complete_new_change_decision":
      proposal.commitOwed = false;
      fire("new-change-proposals-changed", { streamId: STREAM, change: CHANGE, proposal: { ...proposal }, cause: "committed" });
      return { proposal: { ...proposal }, committed: true, originKind: "change_discussion" };
    case "confirm_change_split": {
      // eslint-disable-next-line no-throw-literal
      if (confirmedAt) throw "split_confirmed";
      // eslint-disable-next-line no-throw-literal
      if (proposal.state === "pending") throw "new_change_proposal_pending";
      // eslint-disable-next-line no-throw-literal
      if (proposal.state !== "accepted") throw "nothing_accepted";
      confirmedAt = new Date().toISOString();
      fire("new-change-proposals-changed", { streamId: STREAM, change: CHANGE, cause: "confirmed" });
      return split();
    }
    default:
      return NOT_HANDLED;
  }
}
