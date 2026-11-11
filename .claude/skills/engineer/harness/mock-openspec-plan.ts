/*
 * OpenSpec plan updates stand-in (OPU-openspec-plan-updates.md;
 * OCW-openspec-change-workspace.md OCW-FR-QSZP, OCW-FR-YRZK, OCW-FR-ZPDC,
 * OCW-FR-XJMR, OCW-FR-KZDG; GRU-graduation-runs.md GRU-FR-TCSM,
 * GRU-FR-TVDF, GRU-FR-PZJY, GRU-FR-BPNJ; DQA-discussion-question-answering.md
 * DQA-FR-XFEE), for UI auditing only.
 *
 * Flags (each makes the apply run `g-51` of `add-logout-3f2a` rest
 * `awaiting_author` on a plan rest):
 * - `?openspecPlan=update`: a `plan_update` rest. The plan thread `ocd-plan`
 *   holds the reason as a Synthesis comment, a Synthesis question set of two
 *   questions (the second has no option), and the plan proposal `cep-1`, whose
 *   comment moves from `ocd-1` to `ocd-plan`. While the set stands, every
 *   decision on `cep-1` is refused with `plan_questions_pending`, and Continue
 *   too. Submit the set, then decide the hunks, then Continue.
 * - `?openspecPlan=findings`: a `plan_findings` rest with two `plan` findings
 *   and one `work` finding. The plan thread holds the fixed text alone, and the
 *   rest holds no plan proposal. Continue is admitted at once.
 * - `?openspecPlan=invalid`: a `plan_update_invalid` rest with the cause
 *   `plan_deletes_file`, and no plan thread.
 * - `?openspecPlanContinue=<code>` makes every Continue of a plan rest refuse
 *   with that code once the rest admits it, for example
 *   `openspec_validation_unavailable: cli_failed`.
 */

const CHANGE = "add-logout-3f2a";
const STREAM_ID = "s-1";
const TARGET = { kind: "change", streamId: STREAM_ID, change: CHANGE };
const SYNTHESIS = { kind: "agent", agentId: "synthesis", handle: "Synthesis", title: "" };

export const PLAN_THREAD_ID = "ocd-plan";
export const PLAN_SET_ID = "qs-plan-1";
const PLAN_REASON =
  "Task 2 asks for a logout route on every page, but the design names one shared header. The plan should say which one the work builds.";

/** The rationale of the plan proposal comment: its own words, not the reason again. */
const PLAN_PROPOSAL_RATIONALE =
  "These edits make task 2 build one logout entry in the shared header, and drop the route of each page from the plan.";

export type PlanMode = "update" | "findings" | "invalid";

function query(name: string): string | null {
  if (typeof location === "undefined") return null;
  return new URLSearchParams(location.search).get(name);
}

/** The plan rest the harness seeds, or null. */
export function planMode(): PlanMode | null {
  const mode = query("openspecPlan");
  return mode === "update" || mode === "findings" || mode === "invalid" ? mode : null;
}

/** OPU-FR-VXVO: the questions, as the escalation gave them. */
const QUESTIONS = [
  {
    position: 1,
    question: "Where does the logout entry stand?",
    options: [
      {
        answer: "Build one logout entry in the shared header, and drop the per-page routes from task 2.",
        summary: "In the shared header",
        description: "One entry, one test. The pages keep no route of their own.",
      },
      {
        answer: "Keep a logout route on every page, as task 2 says now.",
        summary: "On every page",
        description: "Matches the tasks as written. More routes to test.",
      },
    ],
  },
  { position: 2, question: "What should the notice say when the session ends?", options: [] },
];

/** OPU-FR-RMZN: the state the plan rest moves through. */
const state = {
  setStands: planMode() === "update",
  answers: null as unknown[] | null,
};

/** OPU-FR-EVBZ: whether the question set of the plan update stands. */
export function planSetStands(): boolean {
  return state.setStands;
}

/** OPU-FR-VXVO: the set reserved in the plan thread, or null. */
export function planQuestionSet(): any | null {
  if (planMode() !== "update" || !state.setStands) return null;
  return {
    setId: PLAN_SET_ID,
    discussionId: PLAN_THREAD_ID,
    threadId: PLAN_THREAD_ID,
    askedBy: SYNTHESIS,
    askedAt: "2026-09-08T09:20:00Z",
    questions: QUESTIONS.map((q) => ({
      position: q.position,
      text: q.question,
      options: q.options.map((o, i) => ({ position: i + 1, value: o.summary, description: o.description })),
    })),
  };
}

/** OPU-FR-QXUE / OPU-FR-MCSG: the plan thread, or null. */
export function planDiscussion(): any | null {
  const mode = planMode();
  if (mode === null || mode === "invalid") return null;
  const at = "2026-09-08T09:20:00Z";
  const comments: any[] =
    mode === "update"
      ? [
          { id: `${PLAN_THREAD_ID}-c1`, author: SYNTHESIS, body: PLAN_REASON, quotes: [], attachments: [], createdAt: at },
          {
            id: `${PLAN_THREAD_ID}-c2`,
            author: SYNTHESIS,
            body: PLAN_PROPOSAL_RATIONALE,
            quotes: [],
            attachments: [{ kind: "changeProposal", proposalId: "cep-1", streamId: STREAM_ID, change: CHANGE }],
            createdAt: at,
          },
        ]
      : [
          {
            id: `${PLAN_THREAD_ID}-c1`,
            author: SYNTHESIS,
            body:
              `The review of the apply run found that the plan of "${CHANGE}" is wrong. Ask an agent here to propose the edits, then use Continue.\n\n` +
              FINDINGS.filter((f) => f.kind === "plan")
                .map((f) => `- ${f.description} ${f.correction}`)
                .join("\n"),
            quotes: [],
            attachments: [],
            createdAt: at,
          },
        ];
  return { id: PLAN_THREAD_ID, target: TARGET, fragmentTarget: null, resolved: false, createdAt: at, updatedAt: at, comments };
}

const FINDINGS = [
  {
    severity: "major",
    description: "Task 2 asks for a route on every page, but the design has one shared header.",
    affectedFiles: ["openspec/changes/add-logout-3f2a/tasks.md"],
    correction: "Make task 2 build the entry in the shared header.",
    kind: "plan",
  },
  {
    severity: "minor",
    description: "The spec delta names no notice text for the end of a session.",
    affectedFiles: ["openspec/changes/add-logout-3f2a/specs/auth/spec.md"],
    correction: "Add the notice text to the Logout requirement.",
    kind: "plan",
  },
  {
    severity: "minor",
    description: "The logout handler logs the session id.",
    affectedFiles: ["src/logout.ts"],
    correction: "Log the session kind instead.",
    kind: "work",
  },
];

/** OPU-FR-UOYJ: the over-fields of `g-51` at rest on the plan rest, or null. */
export function planRestFields(): Record<string, unknown> | null {
  const mode = planMode();
  if (mode === null) return null;
  const at = "2026-09-08T09:20:00Z";
  const rest =
    mode === "update"
      ? {
          reason: "plan_update",
          at,
          origin: "work",
          threadId: PLAN_THREAD_ID,
          proposalId: "cep-1",
          setId: PLAN_SET_ID,
          questions: QUESTIONS,
          answers: state.answers,
          planReason: PLAN_REASON,
        }
      : mode === "findings"
        ? { reason: "plan_findings", at, origin: "review", threadId: PLAN_THREAD_ID, findings: FINDINGS }
        : { reason: "plan_update_invalid", at, cause: "plan_deletes_file" };
  return {
    state: "awaiting_author",
    rest,
    observability: {
      observabilityVersion: 1,
      currentStage: mode === "findings" ? "review" : "working",
      stageCondition: "waiting",
      stageHistory: [],
      passes: [
        {
          pass: 1,
          status: "working",
          task: `Implement the tasks of ${CHANGE}.`,
          findings: [],
          startedAt: "2026-09-08T09:02:00Z",
        },
      ],
    },
  };
}

/** OPU-FR-TPYT: a submission of the plan set writes its answers on the rest. */
export function notePlanSubmission(threadId: string, answers: unknown[], runs: any[]): void {
  if (threadId !== PLAN_THREAD_ID || planMode() !== "update") return;
  state.setStands = false;
  state.answers = answers;
  const run = runs.find((r) => r.id === "g-51");
  if (run?.rest?.reason === "plan_update") run.rest = { ...run.rest, answers };
}

/**
 * OPU-FR-THOZ / OPU-FR-PXDY: Continue of a plan rest. Returns false where the
 * run is not at rest on a plan rest. A refusal is thrown, and changes nothing.
 */
export function planContinue(run: any, proposalHold: () => "pending" | "commit_owed" | null): boolean {
  const reason = run?.rest?.reason;
  if (run?.state !== "awaiting_author") return false;
  if (reason !== "plan_update" && reason !== "plan_findings" && reason !== "plan_update_invalid") return false;
  if (reason === "plan_update" && run.rest.setId && !run.rest.answers) {
    // eslint-disable-next-line no-throw-literal
    throw "plan_questions_pending";
  }
  if (reason !== "plan_update_invalid" && run.rest.proposalId && proposalHold() !== null) {
    // eslint-disable-next-line no-throw-literal
    throw "proposal_pending";
  }
  const refusal = query("openspecPlanContinue");
  // eslint-disable-next-line no-throw-literal
  if (refusal) throw refusal;
  run.rest = null;
  run.state = "working";
  return true;
}
