/**
 * The PR summary runs the Create a PR window follows, and the combine of run
 * summaries (`../../specifications/ui/CPR-create-pull-request.md`
 * CPR-FR-UNAN, CPR-FR-VOEB, CPR-FR-GYVD, CPR-FR-URIL, CPR-FR-WCBQ,
 * CPR-FR-PJTG, CPR-FR-ZZNM, CPR-FR-FFQD).
 *
 * A PR summary run runs in the background, so the author can close the window
 * while it works. The follower is module-level for that reason: it keeps its
 * subscription to `"graduation run changed"` while any saved state follows a
 * run, and it writes the result into the saved state of the head branch
 * whether the window is open or not. It stops with the last run it follows, and
 * when the project changes.
 */
import * as api from "../api";
import {
  summaryEndMessage,
  summaryStartMessage,
} from "../components/CreatePullRequest/messages";
import { parseRejection } from "../components/Git/errors";
import { onGraduationRunChanged } from "../events";
import { logDebug, logInfo, logWarn } from "../logging";
import type {
  BranchCompletedRun,
  GraduationRun,
  GraduationRunChangedPayload,
  PrSummaryMode,
  RunSummary,
} from "../types/graduation";
import { isTerminalRun } from "../types/graduation";
import { prSummaryResultOf } from "./graduation/prSummary";
import {
  allPullRequestSessions,
  getPullRequestSession,
  onPullRequestSessionsCleared,
  pullRequestSessionEpoch,
  updatePullRequestSession,
} from "./pullRequestSessions";

const DOMAINS = ["frontend", "ai"] as const;

/** The head branches whose start is in flight (CPR-FR-UNAN). */
const starting = new Set<string>();

let unlisten: Promise<(() => void) | null> | null = null;

/**
 * CPR-FR-ZZNM: one section for each run summary in landing order: a `##`
 * heading with its title, a blank line, and its description.
 */
export function combinedSections(summaries: RunSummary[]): string {
  return summaries.map((s) => `## ${s.title}\n\n${s.description}`).join("\n\n");
}

/** The run summaries of `runs`, in landing order, skipping a run with none. */
export function runSummariesOf(runs: BranchCompletedRun[]): RunSummary[] {
  return runs.flatMap((run) => (run.runSummary ? [run.runSummary] : []));
}

function isFollowing(head: string, runId: string): boolean {
  const summary = getPullRequestSession(head)?.summary;
  return summary?.runId === runId && summary.status === "running";
}

function anyRunning(): boolean {
  return allPullRequestSessions().some((s) => s.summary?.status === "running");
}

function stopListening(): void {
  const pending = unlisten;
  unlisten = null;
  void pending?.then((stop) => stop?.());
}

function ensureListening(): void {
  if (unlisten) return;
  unlisten = onGraduationRunChanged(onRunChanged).catch((error: unknown) => {
    logWarn([...DOMAINS], "pull request summary subscription failed", {
      error: parseRejection(error).code,
    });
    return null;
  });
}

onPullRequestSessionsCleared(() => {
  starting.clear();
  stopListening();
});

/**
 * CPR-FR-GYVD / CPR-FR-URIL: write what `run` says into the saved state of
 * `head`. A completed run replaces the fields with its result: the title and
 * the description for `diff`, the description alone for `combine`
 * (CPR-FR-PJTG). A failed or discarded one changes no field.
 */
function applyRun(head: string, run: GraduationRun): void {
  const session = getPullRequestSession(head);
  const summary = session?.summary;
  if (!session || !summary || summary.runId !== run.id) return;
  if (summary.status !== "running") return;
  if (!isTerminalRun(run.state)) {
    updatePullRequestSession(head, { summary: { ...summary, run } });
    return;
  }
  const result = prSummaryResultOf(run);
  if (run.state === "completed" && result) {
    const fields =
      summary.mode === "combine"
        ? { body: result.description }
        : { title: result.title, body: result.description };
    updatePullRequestSession(head, {
      ...fields,
      summary: { ...summary, run, status: "completed", message: null },
    });
  } else {
    const status = run.state === "discarded" ? "discarded" : "failed";
    updatePullRequestSession(head, {
      summary: {
        ...summary,
        run,
        status,
        message: summaryEndMessage(status, run.failure),
      },
    });
  }
  logInfo([...DOMAINS], "pull request summary run ended", {
    head,
    runId: run.id,
    mode: summary.mode,
    state: run.state,
    hasResult: Boolean(result),
  });
  if (!anyRunning()) stopListening();
}

/** Read `runId` and apply it, unless the project changed meanwhile. */
async function refresh(head: string, runId: string, terminal: boolean): Promise<void> {
  const at = pullRequestSessionEpoch();
  try {
    const run = await api.getGraduationRun(runId);
    if (at !== pullRequestSessionEpoch()) return;
    applyRun(head, run);
  } catch (error) {
    logWarn([...DOMAINS], "pull request summary run read failed", {
      head,
      runId,
      error: parseRejection(error).code,
    });
    if (at === pullRequestSessionEpoch() && terminal) {
      lose(head, runId, "The run could not be read.");
    }
  }
}

/**
 * The run the window follows can no longer be read. The follow ends as a
 * failure, so Generate is available again (CPR-FR-URIL).
 */
function lose(head: string, runId: string, message: string): void {
  const summary = getPullRequestSession(head)?.summary;
  if (!summary || summary.runId !== runId || summary.status !== "running") return;
  updatePullRequestSession(head, { summary: { ...summary, status: "failed", message } });
  logWarn([...DOMAINS], "pull request summary run lost", { head, runId });
  if (!anyRunning()) stopListening();
}

function onRunChanged(payload: GraduationRunChangedPayload): void {
  for (const session of allPullRequestSessions()) {
    const summary = session.summary;
    if (!summary || !isFollowing(session.head, payload.runId)) continue;
    // A change that keeps the state changes nothing this window shows.
    if (!isTerminalRun(payload.state) && summary.run?.state === payload.state) continue;
    void refresh(session.head, payload.runId, isTerminalRun(payload.state));
  }
  // A saved state that a created pull request cleared follows nothing more.
  if (!anyRunning()) stopListening();
}

/** CPR-FR-VOEB: follow `run` for `head` until it is terminal. */
function follow(head: string, run: GraduationRun, mode: PrSummaryMode): void {
  // A created pull request cleared the saved state: nothing follows the run.
  if (!getPullRequestSession(head)) return;
  updatePullRequestSession(head, {
    summary: { runId: run.id, mode, status: "running", run, message: null },
  });
  ensureListening();
  if (isTerminalRun(run.state)) applyRun(head, run);
}

async function start(head: string, mode: PrSummaryMode): Promise<void> {
  const session = getPullRequestSession(head);
  if (!session || starting.has(head)) return;
  starting.add(head);
  const at = pullRequestSessionEpoch();
  const base = session.base;
  updatePullRequestSession(head, {
    summary: { runId: null, mode, status: "starting", run: null, message: null },
  });
  logInfo([...DOMAINS], "pull request summary run start requested", { head, base, mode });
  try {
    const run = await api.startPullRequestSummary(head, base, mode);
    if (at !== pullRequestSessionEpoch()) return;
    logInfo([...DOMAINS], "pull request summary run started", { head, runId: run.id, mode });
    follow(head, run, mode);
  } catch (error) {
    if (at !== pullRequestSessionEpoch()) return;
    logWarn([...DOMAINS], "pull request summary run start refused", {
      head,
      mode,
      error: parseRejection(error).code,
    });
    updatePullRequestSession(head, {
      summary: {
        runId: null,
        mode,
        status: "failed",
        run: null,
        message: summaryStartMessage(error),
      },
    });
  } finally {
    // A start of an older project must not clear the marker of a newer one.
    if (at === pullRequestSessionEpoch()) starting.delete(head);
  }
}

/**
 * CPR-FR-UNAN: **Generate** starts a `diff` run for the head branch and the
 * chosen base. A second activation while one starts or runs is refused here.
 */
export function generatePullRequestSummary(head: string): Promise<void> {
  const summary = getPullRequestSession(head)?.summary;
  if (summary && (summary.status === "starting" || summary.status === "running")) {
    logDebug([...DOMAINS], "pull request generation refused, one runs", { head });
    return Promise.resolve();
  }
  return start(head, "diff");
}

/**
 * CPR-FR-VOEB: learn the PR summary run of `head` from the queue. A followed
 * run is read anew, because an event may have gone while nothing listened. A
 * run that is not terminal and that nothing follows yet is followed.
 */
export async function discoverPullRequestSummaryRun(head: string): Promise<void> {
  const at = pullRequestSessionEpoch();
  let runs: GraduationRun[];
  try {
    runs = (await api.listGraduationQueue())?.runs ?? [];
  } catch (error) {
    logWarn([...DOMAINS], "pull request summary queue read failed", {
      head,
      error: parseRejection(error).code,
    });
    return;
  }
  if (at !== pullRequestSessionEpoch()) return;
  const session = getPullRequestSession(head);
  if (!session) return;
  const mine = runs.filter((run) => run.prSummary?.head === head);
  const followed = session.summary;
  if (followed?.status === "running" && followed.runId) {
    const run = mine.find((r) => r.id === followed.runId);
    if (run) {
      ensureListening();
      applyRun(head, run);
    } else {
      lose(head, followed.runId, "The run is no longer in the queue.");
    }
    return;
  }
  if (followed?.status === "starting") return;
  const active = [...mine].reverse().find((run) => !isTerminalRun(run.state));
  if (!active) return;
  logInfo([...DOMAINS], "pull request summary run found in the queue", {
    head,
    runId: active.id,
  });
  follow(head, active, active.prSummary?.mode ?? "diff");
}

/**
 * CPR-FR-WCBQ / CPR-FR-PJTG / CPR-FR-ZZNM: combine the run summaries of
 * `runs` into the saved state of `head`. The title is the first summary's
 * title. The description holds one section for each summary, and a `combine`
 * run replaces it when the project resolves an agentic integration. A combine
 * runs once for a head branch.
 */
export async function combinePullRequestRunSummaries(
  head: string,
  runs: BranchCompletedRun[],
  before?: { title: string; body: string },
): Promise<void> {
  const session = getPullRequestSession(head);
  if (!session || session.combined) return;
  const summaries = runSummariesOf(runs);
  if (summaries.length === 0) {
    logDebug([...DOMAINS], "pull request combine skipped, no run summary", { head });
    return;
  }
  const at = pullRequestSessionEpoch();
  // A field the author changed while the runs were read keeps their text.
  const keepTitle = before !== undefined && session.title !== before.title;
  const keepBody = before !== undefined && session.body !== before.body;
  updatePullRequestSession(head, {
    ...(keepTitle ? {} : { title: summaries[0].title }),
    ...(keepBody ? {} : { body: combinedSections(summaries) }),
    combined: true,
  });
  let resolves = false;
  try {
    resolves = ((await api.getProjectAgenticIntegration())?.vendor ?? null) !== null;
  } catch (error) {
    logWarn([...DOMAINS], "pull request combine integration read failed", {
      head,
      error: parseRejection(error).code,
    });
  }
  if (at !== pullRequestSessionEpoch() || !getPullRequestSession(head)) return;
  logInfo([...DOMAINS], "pull request run summaries combined", {
    head,
    summaryCount: summaries.length,
    withRun: resolves,
  });
  if (resolves) await start(head, "combine");
}

/**
 * CPR-FR-WCBQ / CPR-FR-IDRP: the completed runs of `head`. When `combine` is
 * true, the combine follows the answer whether the window is still open or
 * not.
 */
export async function readBranchCompletedRuns(
  head: string,
  combine: boolean,
): Promise<BranchCompletedRun[]> {
  const at = pullRequestSessionEpoch();
  const opened = getPullRequestSession(head);
  const before = opened ? { title: opened.title, body: opened.body } : undefined;
  const answer = await api.listBranchCompletedRuns(head);
  const runs = Array.isArray(answer) ? answer : [];
  if (combine && at === pullRequestSessionEpoch()) {
    void combinePullRequestRunSummaries(head, runs, before);
  }
  return runs;
}

/** Tests: forget the starts in flight and the subscription. */
export function resetPullRequestSummariesForTest(): void {
  starting.clear();
  unlisten = null;
}
