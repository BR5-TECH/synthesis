/*
 * Map thread stand-in (MPR-map-plan-proposals.md), for UI auditing only.
 *
 * The threads of the Map tab live in memory. `open_map_discussion` always makes
 * a new thread (MPR-FR-MAXH), `find_map_discussion` answers the latest open
 * thread of the node (MPR-FR-MYLA), and `list_map_discussions` answers every
 * thread in the order it was opened (MPR-FR-NRNZ). `add_comment`,
 * `set_discussion_resolution` and `read_discussion` serve a Map thread here and
 * fall through for every other discussion. Starts with no thread.
 *
 * With `?mapProposal` (use it with `?deliveryGithub`) one whole-map thread
 * holds two plan proposals (MPV-map-proposal-review.md): an older one whose row
 * failed with `changed_on_tracker`, and a pending one with a changed, an added,
 * and a withdrawn issue. With `?mapProposalFail` every accept fails with
 * `changed_on_tracker`. With `?mapProposalParent` #49 also moves under the
 * new issue `rnew`, and waits for it. Accept, reject, undo, and the re-prepare request change
 * the proposals in memory and emit `map-plan-proposals-changed`.
 */

import { NOT_HANDLED } from "./mock-github-polling";

const ME = { kind: "human", login: "" };

type MapNode = { lens: string; kind: string; id: string; label: string } | null;

const threads: any[] = [];
let nextThread = 1;

const flag = (name: string) => new URLSearchParams(location.search).has(name);
const AGENT = { kind: "agent", agentId: "claude", handle: "claude", model: "opus-5", title: "Software architect" };
const row = (key: string, kind: string, fields: any, over: any = {}) => ({
  key,
  kind,
  fields,
  state: "waiting",
  blockedBy: null,
  error: null,
  result: null,
  ...over,
});
const BODY_BEFORE = [
  "## Description",
  "",
  "Card numbers are expected to be tokenized before they reach the service.",
  "",
  "## Acceptance criteria",
  "",
  "- Tokens are stored per merchant.",
  "- The `vault` and `proxy` endpoints are available to the checkout.",
  "",
  "> The card number never reaches the service, not even in its logs.",
  "",
  "## Flow",
  "",
  "```mermaid",
  "sequenceDiagram",
  "  Browser->>Vault: card number",
  "  Vault-->>Browser: token",
  "```",
  "",
].join("\n");
const BODY_AFTER = [
  "## Description",
  "",
  "Card numbers are tokenized in the browser before they reach the service.",
  "",
  "## User stories",
  "",
  "### Pay with a saved card",
  "",
  "As a returning buyer, I want to pay with a saved card, so that checkout takes one step.",
  "",
  "- **When** the buyer picks a saved card",
  "- **Then** the token is sent, and never the card number",
  "",
  "## Acceptance criteria",
  "",
  "- Tokens are stored per merchant, with a rotation date.",
  "- The `vault` and `proxy` endpoints are available to the checkout.",
  "",
  "> The card number never reaches the service, not even in its logs.",
  "",
  "## Flow",
  "",
  "```mermaid",
  "sequenceDiagram",
  "  Browser->>Vault: card number",
  "  Vault-->>Browser: token and its expiry",
  "  Browser->>Service: token, merchant id, amount, currency, and an idempotency key that the checkout makes once per attempt",
  "    Note over Service: the token is stored per merchant",
  "```",
  "",
].join("\n");

const proposals: any[] = [];
if (flag("mapProposal")) {
  const at = "2026-11-17T09:00:00Z";
  proposals.push(
    {
      id: "pp-old",
      discussionId: "map-1",
      commentId: "map-1-c2",
      createdAt: "2026-11-17T08:40:00Z",
      reprepareRequestedAt: "2026-11-17T08:45:00Z",
      rows: [
        row(
          "r42",
          "modify_issue",
          { number: 42, title: "Tokenize card numbers", baseUpdatedAt: at, changes: [{ field: "body", before: BODY_BEFORE, after: BODY_AFTER }] },
          { state: "failed", error: { code: "changed_on_tracker", message: "The item changed on the tracker." } },
        ),
      ],
    },
    {
      id: "pp-new",
      discussionId: "map-1",
      commentId: "map-1-c4",
      createdAt: at,
      rows: [
        row("r42", "modify_issue", {
          number: 42,
          title: "Tokenize card numbers",
          baseUpdatedAt: at,
          changes: [{ field: "body", before: BODY_BEFORE, after: BODY_AFTER }],
        }),
        row("r49", "modify_issue", {
          number: 49,
          title: "Decline message is wrong",
          baseUpdatedAt: at,
          changes: [
            { field: "title", before: "Decline mesage is wrong", after: "Decline message is wrong" },
            { field: "milestone", before: { number: 3, title: "Beta" }, after: { number: 4, title: "GA" } },
            { field: "status", before: "Todo", after: "In Progress" },
            { field: "labels", before: ["legacy"], after: ["payments"], added: ["payments"], removed: ["legacy"] },
            // With `?mapProposalParent`, #49 moves under the new issue of this proposal (MPV-FR-CGOG).
            ...(flag("mapProposalParent") ? [{ field: "parent", before: { issue: 12 }, after: { row: "rnew" } }] : []),
          ],
        }, flag("mapProposalParent") ? { blockedBy: { rowKey: "rnew", reason: "waiting" } } : {}),
        row("rnew", "create_issue", {
          title: "Payout retry limits",
          type: "Task",
          milestone: { number: 4, title: "GA" },
          labels: ["payments"],
          body: "## Description\n\nA failed payout is retried within configured limits.\n\n- Maximum attempts can be configured.\n- A failed payout notifies the merchant.\n",
        }),
        row("r48", "modify_issue", {
          number: 48,
          title: "Refund emails",
          baseUpdatedAt: at,
          changes: [{ field: "close", before: "open", after: "closed", reason: "not_planned" }],
        }),
      ],
    },
  );
  const human = (id: string, body: string, createdAt: string) => ({ id, author: ME, body, quotes: [], attachments: [], createdAt });
  const agentSays = (id: string, body: string, proposalId: string, createdAt: string) => ({
    id,
    author: AGENT,
    body,
    quotes: [],
    attachments: [{ kind: "mapPlanProposal", proposalId }],
    createdAt,
  });
  threads.push({
    id: "map-1",
    target: { kind: "map" },
    fragmentTarget: null,
    mapNode: null,
    comments: [
      human("map-1-c1", "What is left for the beta? Propose a change.", "2026-11-17T08:39:00Z"),
      agentSays("map-1-c2", "Card tokenization lacks its user story. I propose to add it.", "pp-old", "2026-11-17T08:40:00Z"),
      human("map-1-c3", "The request failed.", "2026-11-17T08:58:00Z"),
      agentSays(
        "map-1-c4",
        "The request failed because #42 changed on GitHub. I read its current content and prepared the change again. I also fixed #49, added a retry limits task, and withdrew #48.",
        "pp-new",
        at,
      ),
    ],
    resolved: false,
    createdAt: "2026-11-17T08:39:00Z",
    updatedAt: at,
  });
  nextThread = 2;
}

/*
 * MPV-FR-HGLN: with `?planProposal` (use it with `?deliveryGithub`), the
 * discussions of other tabs hold one plan proposal each: the draft discussion
 * `disc-1` (New Artifact tab), the file discussion `disc-edt-1` (Editor tab of
 * EDT-editor.md), `disc-flow-1` (Flow tab of release.flow.md), and the
 * note conversation `disc-note-2`. Each has a
 * new issue with a Type, a change of #42, and a new draft. `mock-core.ts` adds
 * the agent comment of each (`planProposalComments`), and
 * `window.__planProposalArrive(discussionId)` appends one more (MPV-FR-UWQL).
 */
const PLAN_DISCUSSIONS = ["disc-1", "disc-edt-1", "disc-flow-1", "disc-note-2"];
const planRows = (at: string) => [
  row("pnew", "create_issue", {
    title: "Retry failed cleanup",
    type: "Task",
    milestone: { number: 4, title: "GA" },
    labels: ["cli"],
    body: "## Description\n\nThe cleanup command retries a worktree that is locked.\n",
  }),
  row("p42", "modify_issue", {
    number: 42,
    title: "Tokenize card numbers",
    baseUpdatedAt: at,
    changes: [{ field: "status", before: "Todo", after: "In Progress" }],
  }),
  row("pdraft", "new_draft", { name: "Cleanup dry run", body: "Show what the cleanup would remove, and remove nothing." }),
];
const planComment = (discussionId: string, n: number, proposalId: string, createdAt: string) => ({
  id: `${discussionId}-pp${n}`,
  author: AGENT,
  body: "I propose a task for the retry, a status change of #42, and a draft for the dry run.",
  quotes: [],
  attachments: [{ kind: "mapPlanProposal", proposalId }],
  createdAt,
});
if (flag("planProposal")) {
  for (const id of PLAN_DISCUSSIONS) {
    const at = "2026-11-17T09:30:00Z";
    proposals.push({ id: `pp-${id}`, discussionId: id, commentId: `${id}-pp1`, createdAt: at, rows: planRows(at) });
  }
}

/** The seeded plan proposal comment of a discussion of another tab, if any. */
export function planProposalComments(discussionId: string): any[] {
  if (!flag("planProposal") || !PLAN_DISCUSSIONS.includes(discussionId)) return [];
  return [planComment(discussionId, 1, `pp-${discussionId}`, "2026-11-17T09:30:00Z")];
}

/** A new pending proposal of a discussion of another tab, and its comment. */
export function arrivingPlanProposal(discussionId: string): any {
  const at = new Date().toISOString();
  const n = proposals.filter((p) => p.discussionId === discussionId).length + 1;
  const proposalId = `pp-${discussionId}-${n}`;
  proposals.push({ id: proposalId, discussionId, commentId: `${discussionId}-pp${n}`, createdAt: at, rows: planRows(at) });
  fireChanged(discussionId);
  return planComment(discussionId, n, proposalId, at);
}

/*
 * With `?mapProposal`, `window.__mapProposalArrive()` appends an agent comment
 * that updates the proposal (MPV-FR-QZUD) to `map-1`, adds its pending
 * proposal `pp-upd`, and emits `discussion-changed` (MPV-FR-UWQL).
 */
(window as any).__mapProposalArrive = () => {
  const t = threads.find((x) => x.id === "map-1");
  if (!t) return false;
  const at = new Date().toISOString();
  const id = `map-1-c${t.comments.length + 1}`;
  proposals.push({
    id: "pp-upd",
    discussionId: "map-1",
    commentId: id,
    createdAt: at,
    rows: [
      row("r51", "modify_issue", {
        number: 51,
        title: "Payout report",
        baseUpdatedAt: at,
        changes: [{ field: "status", before: "Todo", after: "In Progress" }],
      }),
    ],
  });
  t.comments = [
    ...t.comments,
    {
      id,
      author: AGENT,
      body: "I updated the proposal.",
      quotes: [],
      attachments: [{ kind: "mapPlanProposal", proposalId: "pp-upd", update: { changed: 1, added: 1, withdrawn: 1 } }],
      createdAt: at,
    },
  ];
  t.updatedAt = at;
  (window as any).__fireBusEvent?.("discussion-changed", structuredClone(t));
  fireChanged("map-1");
  return true;
};

function fireChanged(discussionId: string) {
  queueMicrotask(() => (window as any).__fireBusEvent?.("map-plan-proposals-changed", { discussionId }));
}

/** MPR-FR-QUYY / MPR-FR-MVRX / MPR-FR-GIJN: one decision on a seeded proposal. */
function decide(cmd: string, a: Record<string, any>): unknown {
  const p = proposals.find((x) => x.id === a.proposalId && x.discussionId === a.discussionId);
  if (!p) return Promise.reject({ code: "proposal_not_found", message: "The proposal is gone." });
  if (cmd === "request_map_plan_reprepare") {
    p.reprepareRequestedAt = new Date().toISOString();
    fireChanged(p.discussionId);
    return structuredClone(p);
  }
  const r = p.rows.find((x: any) => x.key === a.rowKey);
  if (!r) return Promise.reject({ code: "row_not_found", message: "The row is gone." });
  if (cmd === "reopen_map_plan_row") {
    if (r.state !== "rejected") return Promise.reject({ code: "row_not_rejected", message: "Only a rejected row can be reopened." });
    r.state = "waiting";
  } else if (r.state !== "waiting") {
    return Promise.reject({ code: "row_decided", message: "The row is decided." });
  } else if (cmd === "reject_map_plan_row") {
    r.state = "rejected";
  } else if (flag("mapProposalFail")) {
    r.state = "failed";
    r.error = { code: "changed_on_tracker", message: "The item changed on the tracker." };
  } else {
    r.state = "accepted";
    const number = r.kind === "modify_issue" ? r.fields.number : 90;
    r.result =
      r.kind === "new_draft"
        ? { draftId: "d-1" }
        : { issueNumber: number, issueUrl: `https://github.com/acme/platform/issues/${number}` };
  }
  fireChanged(p.discussionId);
  return new Promise((resolve) => setTimeout(() => resolve(structuredClone(p)), 600));
}

const sameNode = (a: MapNode, b: MapNode) =>
  (a ?? null) === null || (b ?? null) === null
    ? (a ?? null) === (b ?? null)
    : a!.lens === b!.lens && a!.kind === b!.kind && a!.id === b!.id;

const own = (id: unknown) => threads.find((t) => t.id === id);

function comment(thread: any, a: Record<string, any>, at: string) {
  return {
    id: `${thread.id}-c${thread.comments.length + 1}`,
    author: ME,
    body: String(a.body ?? ""),
    quotes: a.quotes ?? [],
    attachments: a.attachments ?? [],
    createdAt: at,
  };
}

/** Handle one Map thread command, or answer NOT_HANDLED. */
export function mapDiscussionsInvoke(cmd: string, a: Record<string, any>): unknown {
  switch (cmd) {
    case "list_map_discussions":
      return threads.map((t) => ({ ...t }));
    case "find_map_discussion": {
      const open = threads.filter((t) => !t.resolved && sameNode(t.mapNode ?? null, a.mapNode ?? null));
      const latest = [...open].sort((x, y) => x.updatedAt.localeCompare(y.updatedAt)).pop();
      return latest ? { ...latest } : null;
    }
    case "open_map_discussion": {
      const at = new Date().toISOString();
      const made: any = {
        id: `map-${nextThread++}`,
        target: { kind: "map" },
        fragmentTarget: null,
        mapNode: a.mapNode ?? null,
        comments: [],
        resolved: false,
        createdAt: at,
        updatedAt: at,
      };
      made.comments.push(comment(made, a, at));
      threads.push(made);
      return { ...made };
    }
    case "add_comment": {
      const t = own(a.discussionId);
      if (!t) return NOT_HANDLED;
      const at = new Date().toISOString();
      t.comments = [...t.comments, comment(t, a, at)];
      t.updatedAt = at;
      return { ...t };
    }
    case "set_discussion_resolution": {
      const t = own(a.discussionId);
      if (!t) return NOT_HANDLED;
      t.resolved = !!a.resolved;
      t.updatedAt = new Date().toISOString();
      return { ...t };
    }
    case "read_discussion": {
      const t = own(a.discussionId);
      return t ? { ...t } : NOT_HANDLED;
    }
    case "list_map_plan_proposals":
      return own(a.discussionId) || proposals.some((p) => p.discussionId === a.discussionId)
        ? structuredClone(proposals.filter((p) => p.discussionId === a.discussionId)) : NOT_HANDLED;
    case "accept_map_plan_row":
    case "reject_map_plan_row":
    case "reopen_map_plan_row":
    case "request_map_plan_reprepare":
      return decide(cmd, a);
    default:
      return NOT_HANDLED;
  }
}
