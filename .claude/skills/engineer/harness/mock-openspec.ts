/*
 * OpenSpec change stand-in (OSM-openspec-mode.md OSM-FR-MBTI,
 * OCW-openspec-change-workspace.md), for UI auditing only.
 *
 * One demo change, `add-logout-3f2a`, with all four artifacts and one delta of
 * each operation. It lives on the stream `s-1` (editor-work), where the seeded
 * completed propose run `g-50` of draft `d-os` wrote it, and in the active
 * worktree, for a Project panel click on `openspec/changes/add-logout-3f2a/`.
 *
 * Open the change from the Runs panel (select run g-50, then Open change) or
 * from the Drafts panel (the row "Add logout everywhere", then Open change).
 * `?openspecRead=<failure>` makes every read reject with that failure, for
 * example `?openspecRead=change_not_found` or
 * `?openspecRead=cli_unavailable:not_found` (see mock-openspec-failures.ts).
 * `?openspecArchived` makes every read of the demo change reject with
 * `change_archived: 2026-11-11-add-logout-3f2a`, so the tab states that the
 * change is archived (OCW-FR-AKVG, OCW-FR-GVZT).
 */

import { NOT_HANDLED } from "./mock-github-polling";
import { failureFlag, failureOf } from "./mock-openspec-failures";
import { SPLIT_DISCUSSION, openspecSplitInvoke } from "./mock-openspec-split";
import { REQUEST_DISCUSSION } from "./mock-openspec-runs";
import { PLAN_THREAD_ID, planDiscussion, planMode, planSetStands } from "./mock-openspec-plan";

export const OPENSPEC_DEMO_CHANGE = "add-logout-3f2a";
export const OPENSPEC_DEMO_DRAFT_ID = "d-os";
export const OPENSPEC_DEMO_RUN_ID = "g-50";
export const OPENSPEC_DEMO_STREAM = { id: "s-1", name: "editor-work" };

/** The Drafts panel row of the propose run's draft. */
export const OPENSPEC_DEMO_DRAFT = {
  id: OPENSPEC_DEMO_DRAFT_ID,
  name: "Add logout everywhere",
  status: "graduated",
  folder: "",
  updatedAt: "2026-09-07T10:00:00Z",
};

/** The draft record `open_draft` answers with. */
export const OPENSPEC_DEMO_RECORD = {
  id: OPENSPEC_DEMO_DRAFT_ID,
  name: "Add logout everywhere",
  promptPath: "Add logout everywhere.md",
  status: "graduated",
  createdAt: "2026-09-06T10:00:00Z",
  updatedAt: "2026-09-07T10:00:00Z",
};

/** GRD-FR-LKGC: the over-fields of the completed propose run. */
export const OPENSPEC_DEMO_RUN_FIELDS = {
  id: OPENSPEC_DEMO_RUN_ID,
  draftId: OPENSPEC_DEMO_DRAFT_ID,
  streamId: OPENSPEC_DEMO_STREAM.id,
  streamName: OPENSPEC_DEMO_STREAM.name,
  state: "completed",
  workTurns: 1,
  reviewTurns: 1,
  commits: ["5e1f0c2a9b8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f"],
  openspec: { phase: "propose", change: OPENSPEC_DEMO_CHANGE },
  input: {
    draftId: OPENSPEC_DEMO_DRAFT_ID,
    draftName: "Add logout everywhere",
    prompt: "Users need a way to log out from every page.",
    promptChecksum: "sha-p50",
    capturedAt: "2026-09-07T09:00:00Z",
  },
  observability: {
    observabilityVersion: 1,
    currentStage: "done",
    stageCondition: "complete",
    stageHistory: [],
    passes: [
      {
        pass: 1,
        status: "passed",
        task: "Propose the change add-logout-3f2a.",
        verdict: "ready",
        rationale: "The change is valid and complete.",
        findings: [],
        startedAt: "2026-09-07T09:02:00Z",
        endedAt: "2026-09-07T09:20:00Z",
      },
    ],
  },
};

const LIVE_EXPIRY = {
  name: "Session Expiration",
  text: "Sessions SHALL expire after 60 minutes.",
  scenarios: [
    { name: "Idle session", text: "- **WHEN** a session is idle for 60 minutes\n- **THEN** the session expires" },
  ],
};

function demoDetail(location: unknown) {
  return applyChangeEdits({
    name: OPENSPEC_DEMO_CHANGE,
    location,
    schema: "spec-driven",
    isComplete: false,
    completedTasks: 1,
    totalTasks: 4,
    artifacts: [
      {
        id: "proposal",
        status: "done",
        files: [
          {
            path: "proposal.md",
            text: "# Why\n\nUsers cannot log out without closing the app.\n\n## What Changes\n\n- Add a **Log out** entry to every page.\n- Shorten the idle session to 30 minutes.\n- Remove the *Remember me* option.\n\n## Impact\n\nAffected specs: `auth`, `ui`.",
            unreadable: false,
          },
        ],
      },
      {
        id: "specs",
        status: "done",
        files: [
          { path: "specs/auth/spec.md", text: "## MODIFIED Requirements", unreadable: false },
          { path: "specs/ui/spec.md", text: "## ADDED Requirements", unreadable: false },
        ],
      },
      {
        id: "design",
        status: "done",
        files: [
          {
            path: "design.md",
            // OCW-FR-FMDZ: one image inside the change folder that reads, one
            // inside that refuses (not .png), one outside, and one remote.
            text: "## Context\n\nThe session store already keeps the last activity time.\n\n![The session flow](diagram.png)\n\n![The old flow](images/flow.svg)\n\n![Outside the change](../../logo.png)\n\n![Remote image](https://example.com/flow.png)\n\n## Decisions\n\n1. The idle limit is a setting with a default of 30 minutes.\n2. Log out clears the session on the server first.",
            unreadable: false,
          },
          { path: "diagram.png", text: null, unreadable: true },
        ],
      },
      {
        id: "tasks",
        status: "ready",
        files: [
          {
            path: "tasks.md",
            text: "## 1. Implementation\n\n- [x] 1.1 Add the idle limit setting\n- [x] 1.2 Add the Log out entry\n- [ ] 1.3 Add the logout route to every page of the main window, with its keyboard shortcut\n- [ ] 1.4 Update the docs",
            unreadable: false,
          },
        ],
      },
    ],
    deltas: [
      {
        capability: "auth",
        capabilityExists: true,
        operation: "MODIFIED",
        requirement: {
          name: "Session Expiration",
          text: "Sessions SHALL expire after 30 minutes of no activity.",
          scenarios: [
            { name: "Idle session", text: "- **WHEN** a session is idle for 30 minutes\n- **THEN** the session expires" },
          ],
        },
        rename: null,
        live: LIVE_EXPIRY,
      },
      {
        capability: "auth",
        capabilityExists: true,
        operation: "ADDED",
        requirement: {
          name: "Logout",
          text: "Users SHALL be able to log out from every page.",
          scenarios: [
            { name: "Log out from the menu", text: "- **WHEN** the user picks Log out\n- **THEN** the session ends\n- **AND** the sign-in page shows" },
          ],
        },
        rename: null,
        live: null,
      },
      {
        capability: "auth",
        capabilityExists: true,
        operation: "REMOVED",
        requirement: { name: "Remember Me", text: "", scenarios: [] },
        rename: null,
        live: {
          name: "Remember Me",
          text: "Sessions SHALL survive an application restart when Remember me is set.",
          scenarios: [],
        },
      },
      {
        capability: "auth",
        capabilityExists: true,
        operation: "RENAMED",
        requirement: null,
        rename: { from: "Login", to: "Sign In" },
        live: { name: "Login", text: "Users SHALL log in with a password.", scenarios: [] },
      },
      {
        capability: "ui",
        capabilityExists: false,
        operation: "ADDED",
        requirement: {
          name: "Logout Entry",
          text: "The header of every page SHALL show a Log out entry.",
          scenarios: [],
        },
        rename: null,
        live: null,
      },
    ],
  });
}

const VALID = /^[a-z0-9]+(-[a-z0-9]+)*$/;

/** OSM-FR-IHUO: whether `name` is a valid change name. */
export function validChangeName(name: string): boolean {
  return name.length >= 1 && name.length <= 64 && name !== "archive" && VALID.test(name);
}

/** The reads served so far, for `?openspecReadAfter`. */
let reads = 0;

/**
 * OCW-FR-RFGG / OCW-FR-PFUQ: the failure of a read, as an `OpenSpecFailure`
 * (mock-openspec-failures.ts). `?openspecReadAfter=<n>` serves the first `n`
 * reads, so the failure stands above the content.
 */
function readFailure(): Record<string, unknown> | null {
  const value = failureFlag("openspecRead");
  if (!value) return null;
  reads += 1;
  const after = Number(new URLSearchParams(location.search).get("openspecReadAfter") ?? "0");
  return reads > after ? failureOf(value) : null;
}

/** Handle one OpenSpec change command, or answer NOT_HANDLED. */
export function openspecInvoke(cmd: string, a: Record<string, any>): unknown {
  if (cmd !== "read_openspec_change") return NOT_HANDLED;
  const failure = readFailure();
  // eslint-disable-next-line no-throw-literal
  if (failure) throw failure;
  const name = String(a.name ?? "");
  // eslint-disable-next-line no-throw-literal
  if (!validChangeName(name)) throw "invalid_change_name";
  const where = a.location ?? { kind: "active_worktree" };
  if (where.kind === "stream" && where.streamId !== OPENSPEC_DEMO_STREAM.id) {
    // eslint-disable-next-line no-throw-literal
    throw "change_not_found";
  }
  // eslint-disable-next-line no-throw-literal
  if (name !== OPENSPEC_DEMO_CHANGE) throw "change_not_found";
  if (typeof location !== "undefined" && new URLSearchParams(location.search).has("openspecArchived")) {
    // eslint-disable-next-line no-throw-literal
    throw `change_archived: 2026-11-11-${OPENSPEC_DEMO_CHANGE}`;
  }
  return demoDetail(where);
}

// --- OCW-FR-TRLG / OCW-FR-COLR: the discussion and the edit proposal of the
// demo change on its stream (CMS-FR-UEXD, CEP-change-edit-proposals.md).
//
// The change on `s-1` holds one whole-change discussion, whose agent comment
// carries one pending change edit proposal of two hunks: one edits
// `proposal.md`, and one creates `specs/notify/spec.md`. Decide them in the
// Source of their files. `?changeAccept=stream_busy` makes every acceptance
// refuse with that code. `?changeCommit=fail` makes the close commit fail, so
// the Error toast offers Retry.

const PROPOSAL_FILE = "proposal.md";
const NEW_FILE = "specs/notify/spec.md";
const EDIT_BEFORE = "Users cannot log out without closing the app.";
const EDIT_AFTER = "Users cannot log out without closing the application window.";
const NEW_TEXT =
  "## ADDED Requirements\n\n### Requirement: Logout Notice\n\nThe app SHALL tell the user that the session ended.\n";

const AGENT = { kind: "agent", agentId: "claude", handle: "claude", model: "opus-5", title: "" };
const ME = { kind: "human", login: "" };

/** The text an accepted hunk wrote, by change-relative path. */
const written: Record<string, string> = {};

type HunkRow = {
  id: string;
  path: string;
  kind: "add" | "replace";
  newFile: boolean;
  state: string;
  edited: boolean;
  revision: number;
  decidedAt?: string;
};

const ledger: HunkRow[] = [
  { id: "h1", path: PROPOSAL_FILE, kind: "replace", newFile: false, state: "pending", edited: false, revision: 0 },
  { id: "h2", path: NEW_FILE, kind: "add", newFile: true, state: "pending", edited: false, revision: 0 },
];
const afterText: Record<string, string> = { h1: EDIT_AFTER, h2: NEW_TEXT };
let commitOwed = false;
let committedOnce = false;

/**
 * OCR-FR-LNKQ / OCR-FR-TCFQ: what the edit proposal of the demo change holds a
 * start for: `pending` holds an apply start and a rework start, and
 * `commit_owed` holds an apply start alone.
 */
export function demoProposalHold(): "pending" | "commit_owed" | null {
  const record = proposalRecord();
  if (record.state === "pending") return "pending";
  return record.commitOwed ? "commit_owed" : null;
}

function proposalRecord() {
  const count = (state: string) => ledger.filter((h) => h.state === state).length;
  const undecided = ledger.some((h) => h.state === "pending" || h.state === "discussing");
  const accepted = count("accepted");
  return {
    id: "cep-1",
    streamId: OPENSPEC_DEMO_STREAM.id,
    change: OPENSPEC_DEMO_CHANGE,
    rationale: "Name the window, and tell the user when the session ends.",
    // OPU-FR-OQRA: with `?openspecPlan=update`, `cep-1` is the plan proposal.
    threadId: planMode() === "update" ? PLAN_THREAD_ID : "ocd-1",
    commentId: planMode() === "update" ? `${PLAN_THREAD_ID}-c2` : "ocd-1-c2",
    state: undecided ? "pending" : accepted > 0 ? "accepted" : "rejected",
    candidateEdited: ledger.some((h) => h.edited),
    commentOwed: false,
    commitOwed,
    hunkCount: ledger.length,
    createdAt: "2026-09-07T11:00:00Z",
    decidedAt: undecided ? undefined : new Date().toISOString(),
    counts: {
      pending: count("pending"),
      accepted,
      rejected: count("rejected"),
      discussing: count("discussing"),
      withdrawn: 0,
    },
    writtenPaths: ledger.filter((h) => h.state === "accepted").map((h) => h.path),
    hunks: ledger.map((h) => ({ ...h })),
    agent: AGENT,
  };
}

function applyChangeEdits<T extends { artifacts: { id: string; files: { path: string; text: string | null; unreadable: boolean }[] }[] }>(
  detail: T,
): T {
  for (const artifact of detail.artifacts) {
    for (const file of artifact.files) {
      if (written[file.path] !== undefined) file.text = written[file.path];
    }
  }
  if (written[NEW_FILE] !== undefined) {
    const specs = detail.artifacts.find((a) => a.id === "specs");
    specs?.files.push({ path: NEW_FILE, text: written[NEW_FILE], unreadable: false });
  }
  return detail;
}

const PROPOSAL_TEXT =
  "# Why\n\nUsers cannot log out without closing the app.\n\n## What Changes\n\n- Add a **Log out** entry to every page.\n- Shorten the idle session to 30 minutes.\n- Remove the *Remember me* option.\n\n## Impact\n\nAffected specs: `auth`, `ui`.";

function hunkDocuments() {
  const at = PROPOSAL_TEXT.indexOf(EDIT_BEFORE);
  return {
    hunks: [
      {
        id: "h1",
        kind: "replace",
        revision: 0,
        note: "Say which window.",
        before: EDIT_BEFORE,
        after: afterText.h1,
        anchor: { lead: "# Why\n\n", trail: "\n\n## What", hint_start: at, hint_end: at + EDIT_BEFORE.length },
        path: PROPOSAL_FILE,
        newFile: false,
        state: ledger[0].state,
      },
      {
        id: "h2",
        kind: "add",
        revision: 0,
        note: "A new capability for the notice.",
        before: null,
        after: afterText.h2,
        anchor: { lead: "", trail: "", hint_start: 0, hint_end: 0 },
        path: NEW_FILE,
        newFile: true,
        state: ledger[1].state,
      },
    ],
    resolutions: [
      { kind: "resolved", start: at, end: at + EDIT_BEFORE.length },
      { kind: "resolved", start: 0, end: 0 },
    ],
    checksum: `ck-${ledger.map((h) => h.state).join("-")}`,
  };
}

const TARGET = { kind: "change", streamId: OPENSPEC_DEMO_STREAM.id, change: OPENSPEC_DEMO_CHANGE };

const discussions: any[] = [
  {
    id: "ocd-1",
    target: TARGET,
    fragmentTarget: null,
    resolved: false,
    createdAt: "2026-09-07T10:50:00Z",
    updatedAt: "2026-09-07T11:00:00Z",
    comments: [
      {
        id: "ocd-1-c1",
        author: ME,
        body: "@claude the proposal does not say which window closes. Please fix it, and add a notice when the session ends.",
        quotes: [],
        attachments: [],
        createdAt: "2026-09-07T10:50:00Z",
      },
      {
        id: "ocd-1-c2",
        author: AGENT,
        body: "Name the window, and tell the user when the session ends.",
        quotes: [],
        attachments: [
          { kind: "changeProposal", proposalId: "cep-1", streamId: OPENSPEC_DEMO_STREAM.id, change: OPENSPEC_DEMO_CHANGE },
        ],
        createdAt: "2026-09-07T11:00:00Z",
      },
    ],
  },
];
// OPU-FR-QXUE / OPU-FR-MCSG: the plan thread, where `?openspecPlan=` seeds
// one. With `update`, the proposal comment of `cep-1` stands in it alone.
{
  const plan = planDiscussion();
  if (plan) {
    discussions.push(plan);
    if (planMode() === "update") discussions[0].comments[1].attachments = [];
  }
}
// NCP / OCW-FR-BGDN: the discussion that holds the pending new-change proposal.
discussions.push(SPLIT_DISCUSSION);
// CAR / OCW-FR-QCPB: the discussion that holds the pending request card.
discussions.push(REQUEST_DISCUSSION);

/** OCR-FR-UAWO / CAR-FR-TQJZ: the discussions of the demo change, for the run stand-in. */
export function changeDiscussionList(): any[] {
  return discussions;
}
let nextDiscussion = 2;

function fire(name: string, payload: unknown) {
  queueMicrotask(() => {
    const bus = (globalThis as Record<string, unknown>).__fireBusEvent as
      | ((n: string, p?: unknown) => void)
      | undefined;
    bus?.(name, payload);
  });
}

function query(name: string): string | null {
  if (typeof location === "undefined") return null;
  return new URLSearchParams(location.search).get(name);
}

function isChangeTarget(t: any): boolean {
  return !!t && t.kind === "change";
}

function ownDiscussion(id: unknown) {
  return discussions.find((d) => d.id === id);
}

/** CEP-FR-KGOL: a decision on the plan proposal while its question set stands. */
function refuseWhilePlanQuestions() {
  // eslint-disable-next-line no-throw-literal
  if (planSetStands()) throw "plan_questions_pending";
}

/** CEP-FR-RWTQ / CEP-FR-FOJW: a decision, and the close it may make. */
function decide(hunkId: string, state: "accepted" | "rejected") {
  refuseWhilePlanQuestions();
  const row = ledger.find((h) => h.id === hunkId);
  // eslint-disable-next-line no-throw-literal
  if (!row) throw "hunk_not_found";
  // eslint-disable-next-line no-throw-literal
  if (row.state === "accepted" || row.state === "rejected") throw "hunk_already_decided";
  row.state = state;
  row.decidedAt = new Date().toISOString();
  if (state === "accepted") {
    written[row.path] = row.newFile
      ? afterText[row.id]
      : PROPOSAL_TEXT.replace(EDIT_BEFORE, afterText[row.id]);
  }
  const resolved = !ledger.some((h) => h.state === "pending" || h.state === "discussing");
  let committed = false;
  let commentId: string | undefined;
  if (resolved) {
    const anyAccepted = ledger.some((h) => h.state === "accepted");
    const home = ownDiscussion(proposalRecord().threadId) ?? discussions[0];
    commentId = `${home.id}-c${home.comments.length + 1}`;
    home.comments.push({
      id: commentId,
      author: ME,
      body: `Decided: ${ledger.map((h) => `${h.path} ${h.state}`).join(", ")}.`,
      quotes: [],
      attachments: [],
      createdAt: new Date().toISOString(),
    });
    fire("discussion-changed", home);
    if (anyAccepted) {
      committed = query("changeCommit") !== "fail";
      commitOwed = !committed;
      committedOnce = committed;
    }
  }
  const proposal = proposalRecord();
  fire("change-edit-proposals-changed", {
    streamId: OPENSPEC_DEMO_STREAM.id,
    change: OPENSPEC_DEMO_CHANGE,
    proposal,
    cause: committed ? "committed" : "decided",
  });
  return { proposal, committed, resolved, commentId, originKind: "change_discussion" };
}

/** Handle one change discussion or change edit proposal command, or answer NOT_HANDLED. */
export function openspecDiscussionInvoke(cmd: string, a: Record<string, any>): unknown {
  switch (cmd) {
    case "list_discussions":
      if (!isChangeTarget(a.target)) return NOT_HANDLED;
      if (a.target.streamId !== OPENSPEC_DEMO_STREAM.id || a.target.change !== OPENSPEC_DEMO_CHANGE) return [];
      return discussions.map((d) => ({ ...d }));
    case "open_discussion": {
      if (!isChangeTarget(a.target)) return NOT_HANDLED;
      const at = new Date().toISOString();
      const created = {
        id: `ocd-${nextDiscussion++}`,
        target: a.target,
        fragmentTarget: a.fragmentTarget ?? null,
        resolved: false,
        createdAt: at,
        updatedAt: at,
        comments: [
          { id: `ocd-c-${Date.now()}`, author: ME, body: String(a.body ?? ""), quotes: [], attachments: a.attachments ?? [], createdAt: at },
        ],
      };
      discussions.push(created);
      return { ...created };
    }
    case "add_comment": {
      const d = ownDiscussion(a.discussionId);
      if (!d) return NOT_HANDLED;
      const at = new Date().toISOString();
      d.comments.push({ id: `ocd-c-${Date.now()}`, author: ME, body: String(a.body ?? ""), quotes: a.quotes ?? [], attachments: a.attachments ?? [], createdAt: at });
      d.updatedAt = at;
      return { ...d };
    }
    case "set_discussion_resolution": {
      const d = ownDiscussion(a.discussionId);
      if (!d) return NOT_HANDLED;
      d.resolved = !!a.resolved;
      return { ...d };
    }
    case "read_discussion": {
      const d = ownDiscussion(a.discussionId);
      return d ? { ...d } : NOT_HANDLED;
    }
    case "list_change_edit_proposals":
      if (a.streamId !== OPENSPEC_DEMO_STREAM.id || a.change !== OPENSPEC_DEMO_CHANGE) return [];
      return [proposalRecord()];
    case "load_change_edit_proposal_hunks":
      // eslint-disable-next-line no-throw-literal
      if (a.proposalId !== "cep-1") throw "proposal_not_found";
      return hunkDocuments();
    case "accept_change_edit_hunk": {
      const busy = query("changeAccept");
      // eslint-disable-next-line no-throw-literal
      if (busy) throw busy;
      return decide(String(a.hunkId), "accepted");
    }
    case "reject_change_edit_hunk":
      return decide(String(a.hunkId), "rejected");
    case "set_change_edit_hunk_discussing": {
      refuseWhilePlanQuestions();
      const row = ledger.find((h) => h.id === a.hunkId);
      // eslint-disable-next-line no-throw-literal
      if (!row) throw "hunk_not_found";
      row.state = a.discussing ? "discussing" : "pending";
      return proposalRecord();
    }
    case "edit_change_edit_hunk": {
      refuseWhilePlanQuestions();
      afterText[String(a.hunkId)] = String(a.after ?? "");
      const row = ledger.find((h) => h.id === a.hunkId);
      if (row) row.edited = true;
      return { checksum: `ck-edit-${Date.now()}` };
    }
    case "decline_change_edit_proposal": {
      refuseWhilePlanQuestions();
      for (const row of ledger) if (row.state === "pending" || row.state === "discussing") row.state = "rejected";
      return { proposal: proposalRecord(), committed: false, resolved: true, originKind: "change_discussion" };
    }
    case "complete_change_edit_decision": {
      if (commitOwed) {
        commitOwed = false;
        committedOnce = true;
      }
      return { proposal: proposalRecord(), committed: committedOnce, resolved: true, originKind: "change_discussion" };
    }
    default:
      // CVH, CST, and NCP: the History, the statistics, and the split.
      return openspecSplitInvoke(cmd, a);
  }
}
