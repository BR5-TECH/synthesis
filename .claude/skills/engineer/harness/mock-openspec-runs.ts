/*
 * OpenSpec change runs stand-in (OCR-openspec-change-runs.md,
 * CAR-change-action-requests.md, OSM-openspec-mode.md OSM-FR-PEAK;
 * OCW-openspec-change-workspace.md OCW-FR-DTJD through OCW-FR-NWRS), for UI
 * auditing only.
 *
 * The demo change `add-logout-3f2a` on `s-1` has two runs: the rework run
 * `g-52`, completed with a proposal, and the newer apply run `g-51`, working
 * in its Tasks stage with 2 of 4 tasks done and a check state. Its discussion
 * `ocd-req` holds one pending request card of the agent `claude` to rework the
 * change.
 *
 * While the edit proposal `cep-1` of the change is pending, every start and
 * every confirmation is refused with `proposal_pending`. Once it is closed with
 * its commit owed, an apply start is refused still (OCR-FR-LNKQ, OCR-FR-TCFQ).
 * Decide its hunks to start runs.
 *
 * Flags:
 * - `?openspecRuns=idle` makes `g-51` completed, so no run holds Apply and the
 *   strip offers Close.
 * - `?openspecApply=<code>` makes every start of an apply run refuse with that
 *   code, for example `?openspecApply=change_not_ready: design,tasks`.
 * - `?openspecRework=<code>` does the same for a rework start, for example
 *   `?openspecRework=proposal_pending`.
 * - `?openspecProgress=blocked` makes the apply progress `blocked`, and
 *   `?openspecProgress=fail` makes its read fail.
 * - `?openspecRequest=<code>` makes every confirmation refuse with that code.
 * - `?openspecReworkBlocked=<code>` makes `g-52` rest in `blocked` at its
 *   finish with that blocker code, for example `proposal_pending` (GRU-FR-NGQE).
 * - `?openspecRequestState=failed` seeds the request as failed with the code
 *   `change_not_found`, and `declined` seeds it as declined (OCW-FR-CTWP).
 * - `?openspecPlan=update|findings|invalid` makes `g-51` rest on a plan rest
 *   (see mock-openspec-plan.ts).
 * - Each refusal flag also takes an OpenSpec failure, which rejects as an
 *   `OpenSpecFailure` object, and `?openspecBlocker=<failure>` makes `g-51`
 *   rest `blocked` on an OpenSpec blocker (see mock-openspec-failures.ts).
 */

import { NOT_HANDLED } from "./mock-github-polling";
import { planRestFields } from "./mock-openspec-plan";
import { failureFlag, failureOf, openspecBlocker } from "./mock-openspec-failures";

const STREAM = { id: "s-1", name: "editor-work" };
const CHANGE = "add-logout-3f2a";
const AGENT = { kind: "agent", agentId: "claude", handle: "claude", model: "opus-5", title: "" };
const ME = { kind: "human", login: "" };
const TARGET = { kind: "change", streamId: STREAM.id, change: CHANGE };
const SYNTHESIS = { kind: "agent", agentId: "synthesis", handle: "Synthesis", title: "" };

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

const NO_DRAFT = (prompt: string) => ({
  draftId: "",
  draftName: "",
  prompt,
  promptChecksum: "sha-ocr",
  capturedAt: "2026-09-08T09:00:00Z",
});

/** OCR-FR-VHQW: the over-fields of the working apply run `g-51`. */
export function applyRunFields(): Record<string, unknown> {
  const idle = query("openspecRuns") === "idle";
  return {
    id: "g-51",
    draftId: "",
    streamId: STREAM.id,
    streamName: STREAM.name,
    state: idle ? "completed" : "working",
    standingWork: "keep",
    workTurns: 1,
    reviewTurns: idle ? 1 : 0,
    commits: idle ? ["7a1b2c3d4e5f60718293a4b5c6d7e8f901234567"] : [],
    openspec: { phase: "apply", change: CHANGE },
    input: NO_DRAFT(`Apply the OpenSpec change "${CHANGE}": implement its tasks.`),
    // GRU-FR-HZLV: a check gate, so the detail slot says "correcting checks".
    checkpoint: {
      pass: 1,
      changedPaths: ["src/logout.ts"],
      hiddenPaths: [],
      hiddenPathsOmitted: 0,
      pendingEscalationAnswers: [],
      verdictRefusals: 0,
      blockAttempts: 0,
      ...(idle ? {} : { checkGate: { cause: "checks_failed", next: "correct" } }),
    },
    observability: {
      observabilityVersion: 1,
      currentStage: idle ? "done" : "working",
      stageCondition: idle ? "complete" : "active",
      stageHistory: [],
      passes: [
        {
          pass: 1,
          status: idle ? "passed" : "working",
          task: `Implement the tasks of ${CHANGE}.`,
          findings: [],
          startedAt: "2026-09-08T09:02:00Z",
        },
      ],
    },
    createdAt: "2026-09-08T09:00:00Z",
    updatedAt: "2026-09-08T09:10:00Z",
    // OPU-FR-UOYJ: `?openspecPlan=` makes the run rest on a plan rest.
    ...(planRestFields() ?? {}),
    // GRU-FR-LBPR / OFP-FR-TBHX: `?openspecBlocker=` rests the run on an
    // OpenSpec blocker, such as the image probe.
    ...openspecBlockedFields(),
  };
}

function openspecBlockedFields(): Record<string, unknown> {
  const blocker = openspecBlocker();
  if (!blocker) return {};
  return {
    state: "blocked",
    blocker,
    observability: {
      observabilityVersion: 1,
      currentStage: "working",
      stageCondition: "blocked",
      stageHistory: [],
      passes: [],
    },
  };
}

/** OCR-FR-TBGA / OCR-FR-GWGE: the over-fields of the completed rework run `g-52`. */
export function reworkRunFields(): Record<string, unknown> {
  const blocked = query("openspecReworkBlocked");
  return {
    id: "g-52",
    draftId: "",
    streamId: STREAM.id,
    streamName: STREAM.name,
    state: blocked ? "blocked" : "completed",
    ...(blocked
      ? { blocker: { code: blocked, message: `blocked: ${blocked}`, clearsBy: "Continue the run.", attempt: 1 } }
      : {}),
    standingWork: "keep",
    workTurns: 1,
    reviewTurns: 1,
    commits: [],
    openspec: {
      phase: "rework",
      change: CHANGE,
      instruction: "Name the window that closes, and add a notice when the session ends.",
      threadId: "ocd-1",
      requester: SYNTHESIS,
      proposalId: blocked ? null : "cep-1",
    },
    input: NO_DRAFT(`Rework the OpenSpec change "${CHANGE}" on the instruction of the author.`),
    observability: {
      observabilityVersion: 1,
      currentStage: blocked ? "review" : "done",
      stageCondition: blocked ? "blocked" : "complete",
      stageHistory: [],
      passes: [
        {
          pass: 1,
          status: "passed",
          task: `Rework ${CHANGE}.`,
          verdict: "ready",
          rationale: "The revision follows the instruction.",
          findings: [],
          startedAt: "2026-09-07T10:40:00Z",
          endedAt: "2026-09-07T10:58:00Z",
        },
      ],
    },
    createdAt: "2026-09-07T10:40:00Z",
    updatedAt: "2026-09-07T11:00:00Z",
  };
}

/** CAR-FR-TJAD: the pending request the card in `ocd-req` shows. */
const requests: any[] = [
  {
    id: "car-1",
    streamId: STREAM.id,
    change: CHANGE,
    action: "rework",
    instruction: "Split task 2 into the route and its test, and say which page the logout entry stands on.",
    rationale: "Task 2 mixes two pieces of work. A rework can split it before anything is built.",
    threadId: "ocd-req",
    commentId: "ocd-req-c2",
    agent: AGENT,
    state: query("openspecRequestState") ?? "pending",
    runId: null,
    lastRefusal: query("openspecRequestState") === "failed" ? "change_not_found" : null,
    createdAt: "2026-09-08T08:30:00Z",
    decidedAt: null,
  },
];

/** OCW-FR-QCPB: the discussion whose agent comment carries the request. */
export const REQUEST_DISCUSSION: any = {
  id: "ocd-req",
  target: TARGET,
  fragmentTarget: null,
  resolved: false,
  createdAt: "2026-09-08T08:20:00Z",
  updatedAt: "2026-09-08T08:30:00Z",
  comments: [
    {
      id: "ocd-req-c1",
      author: ME,
      body: "@arch are the tasks ready to build?",
      quotes: [],
      attachments: [],
      createdAt: "2026-09-08T08:20:00Z",
    },
    {
      id: "ocd-req-c2",
      author: AGENT,
      body: requests[0].rationale,
      quotes: [],
      attachments: [
        { kind: "changeActionRequest", requestId: "car-1", streamId: STREAM.id, change: CHANGE, action: "rework" },
      ],
      createdAt: "2026-09-08T08:30:00Z",
    },
  ],
};

/** What the stand-in needs of the core mock: the runs and the change discussions. */
export interface OpenspecRunsContext {
  runs: any[];
  makeRun: (over: Record<string, unknown>) => any;
  discussions: any[];
  /** OCR-FR-LNKQ / OCR-FR-TCFQ: what the change's edit proposal holds a start for. */
  proposalHold: () => "pending" | "commit_owed" | null;
}

let seq = 60;

function activeRunOf(ctx: OpenspecRunsContext) {
  return ctx.runs.find(
    (r) =>
      r.streamId === STREAM.id &&
      r.openspec?.change === CHANGE &&
      !["completed", "discarded", "failed"].includes(r.state),
  );
}

/** OCR-FR-IDFC: the checks a start takes here, in the order of OCR-FR-GTOM. */
function checkStart(
  ctx: OpenspecRunsContext,
  a: Record<string, any>,
  refusal: string | null,
  phase: "apply" | "rework",
) {
  // eslint-disable-next-line no-throw-literal
  if (a.streamId !== STREAM.id) throw "unknown_stream";
  // eslint-disable-next-line no-throw-literal
  if (a.change !== CHANGE) throw "change_not_found";
  const active = activeRunOf(ctx);
  // eslint-disable-next-line no-throw-literal
  if (active) throw `openspec_run_active: ${active.id}`;
  // OFP-FR-QLQJ: a refusal flag rejects as an `OpenSpecFailure` object.
  // eslint-disable-next-line no-throw-literal
  if (refusal) throw failureOf(refusal);
  // OCR-FR-LNKQ: a pending proposal, or a closed one whose commit is owed,
  // holds an apply start. OCR-FR-TCFQ: a pending one alone holds a rework.
  const hold = ctx.proposalHold();
  // eslint-disable-next-line no-throw-literal
  if (hold === "pending" || (hold === "commit_owed" && phase === "apply")) throw "proposal_pending";
}

function enqueue(ctx: OpenspecRunsContext, phase: "apply" | "rework", extra: Record<string, unknown>) {
  seq += 1;
  const now = new Date().toISOString();
  const run = ctx.makeRun({
    id: `g-${seq}`,
    draftId: "",
    state: "queued",
    standingWork: "keep",
    workTurns: 0,
    openspec: { phase, change: CHANGE, ...extra },
    input: NO_DRAFT(
      phase === "apply"
        ? `Apply the OpenSpec change "${CHANGE}": implement its tasks.`
        : `Rework the OpenSpec change "${CHANGE}" on the instruction of the author.`,
    ),
    observability: {
      observabilityVersion: 1,
      currentStage: "queued",
      stageCondition: "waiting",
      stageHistory: [],
      passes: [],
    },
    enqueuedAt: now,
    createdAt: now,
    updatedAt: now,
  });
  ctx.runs.push(run);
  fire("graduation-queue-changed", { projectKey: "/Users/demo/dev/acme", streamId: STREAM.id, worktreePath: "" });
  return run;
}

function decisionComment(ctx: OpenspecRunsContext, request: any, words: string): string {
  const thread = ctx.discussions.find((d) => d.id === request.threadId);
  const id = `${request.threadId}-c${(thread?.comments.length ?? 0) + 1}`;
  thread?.comments.push({ id, author: ME, body: words, quotes: [], attachments: [], createdAt: new Date().toISOString() });
  if (thread) fire("discussion-changed", thread);
  return id;
}

function decided(request: any, cause: string) {
  fire("change-action-requests-changed", {
    streamId: request.streamId,
    change: request.change,
    requestId: request.id,
    cause,
  });
}

/** Handle one OpenSpec change run or action request command, or answer NOT_HANDLED. */
export function openspecRunsInvoke(cmd: string, a: Record<string, any>, ctx: OpenspecRunsContext): unknown {
  switch (cmd) {
    case "read_apply_progress": {
      const mode = failureFlag("openspecProgress");
      // eslint-disable-next-line no-throw-literal
      if (mode === "fail") throw failureOf("cli_failed");
      // OCW-FR-BTSI / GRU-FR-PXCW: any other failure, for the short form.
      // eslint-disable-next-line no-throw-literal
      if (mode && mode !== "blocked") throw failureOf(mode);
      if (mode === "blocked") {
        return { state: "blocked", total: 4, complete: 0, nextTask: null, missingArtifacts: ["design", "tasks"] };
      }
      const idle = query("openspecRuns") === "idle";
      return {
        state: idle ? "all_done" : "ready",
        total: 4,
        complete: idle ? 4 : 2,
        nextTask: idle ? null : "Add the logout route to every page of the main window, with its keyboard shortcut",
        missingArtifacts: [],
      };
    }
    case "start_change_apply":
      checkStart(ctx, a, failureFlag("openspecApply"), "apply");
      return enqueue(ctx, "apply", {});
    case "start_change_rework": {
      // eslint-disable-next-line no-throw-literal
      if (String(a.instruction ?? "").trim() === "") throw "blank_instruction";
      checkStart(ctx, a, failureFlag("openspecRework"), "rework");
      const threadId = `ocd-rw-${seq + 1}`;
      const at = new Date().toISOString();
      const thread = {
        id: threadId,
        target: TARGET,
        fragmentTarget: null,
        resolved: false,
        createdAt: at,
        updatedAt: at,
        comments: [
          { id: `${threadId}-c1`, author: ME, body: String(a.instruction), quotes: [], attachments: [], createdAt: at },
        ],
      };
      ctx.discussions.push(thread);
      fire("discussion-changed", thread);
      const run = enqueue(ctx, "rework", {
        instruction: String(a.instruction),
        threadId,
        requester: SYNTHESIS,
      });
      return { run, threadId };
    }
    case "list_change_action_requests":
      if (a.streamId !== STREAM.id || a.change !== CHANGE) return [];
      return requests.map((r) => ({ ...r }));
    case "confirm_change_action_request": {
      const request = requests.find((r) => r.id === a.requestId);
      // eslint-disable-next-line no-throw-literal
      if (!request) throw "request_not_found";
      // eslint-disable-next-line no-throw-literal
      if (request.state !== "pending") throw "already_decided";
      try {
        checkStart(
          ctx,
          { streamId: request.streamId, change: request.change },
          failureFlag("openspecRequest"),
          request.action,
        );
      } catch (code) {
        // CAR-FR-ARMK: the request records the code, as a typed string.
        const failure = code as { code?: string; cause?: string; detail?: string };
        request.lastRefusal =
          typeof code === "object" && code && failure.code
            ? [failure.code, failure.cause ?? failure.detail].filter(Boolean).join(": ")
            : String(code);
        decided(request, "refused");
        throw code;
      }
      const run = enqueue(
        ctx,
        request.action,
        request.action === "rework"
          ? { instruction: request.instruction, threadId: request.threadId, requester: request.agent }
          : {},
      );
      request.state = "started";
      request.runId = run.id;
      request.decidedAt = new Date().toISOString();
      const commentId = decisionComment(ctx, request, `Confirmed: ${request.action}. The run “${request.action === "apply" ? "Apply" : "Rework"} ${CHANGE}” started.`);
      decided(request, "decided");
      return { request: { ...request }, run, commentId, originKind: "change_discussion" };
    }
    case "decline_change_action_request": {
      const request = requests.find((r) => r.id === a.requestId);
      // eslint-disable-next-line no-throw-literal
      if (!request) throw "request_not_found";
      // eslint-disable-next-line no-throw-literal
      if (request.state !== "pending") throw "already_decided";
      request.state = "declined";
      request.decidedAt = new Date().toISOString();
      const feedback = a.feedback ? ` ${a.feedback}` : "";
      const commentId = decisionComment(ctx, request, `Declined: ${request.action}.${feedback}`);
      decided(request, "decided");
      return { request: { ...request }, run: null, commentId, originKind: "change_discussion" };
    }
    default:
      return NOT_HANDLED;
  }
}
