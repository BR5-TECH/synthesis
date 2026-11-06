/**
 * The PR summary runs the Create a PR window follows, and the combine of run
 * summaries (`../../specifications/ui/CPR-create-pull-request.md`).
 *
 * The follower is module-level, so these tests drive it with no window
 * mounted: that is the case CPR-FR-GYVD is about. The backend is a mocked
 * `invoke` routed by command name, and `"graduation run changed"` is a scripted
 * event bus. Nothing here waits for time; each wait is for a state.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import { resetLogBufferForTest } from "../logging";
import { branchRun, resetCreatePullRequestStores } from "../test/createPullRequestFixtures";
import { makePrSummaryRun } from "../test/graduationFixtures";
import type { GraduationRun } from "../types/graduation";
import {
  clearAllPullRequestSessions,
  clearPullRequestSession,
  getPullRequestSession,
  openPullRequestSession,
  updatePullRequestSession,
} from "./pullRequestSessions";
import {
  combinedSections,
  combinePullRequestRunSummaries,
  discoverPullRequestSummaryRun,
  generatePullRequestSummary,
} from "./pullRequestSummaries";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const logged = vi.fn();
vi.mock("../logging", async (original) => {
  const real = await original<typeof import("../logging")>();
  const spy =
    (level: string) =>
    (...args: unknown[]) =>
      logged(level, ...args);
  return {
    ...real,
    logInfo: spy("info"),
    logWarn: spy("warn"),
    logDebug: spy("debug"),
  };
});

const { handlers } = vi.hoisted(() => ({
  handlers: [] as Array<(e: { payload: unknown }) => void>,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_name: string, handler: (e: { payload: unknown }) => void) => {
    handlers.push(handler);
    return () => {
      const at = handlers.indexOf(handler);
      if (at >= 0) handlers.splice(at, 1);
    };
  }),
}));
const fireRunChanged = (runId: string, state: string) => {
  for (const h of [...handlers]) h({ payload: { runId, state } });
};

const HEAD = "feature/panel";

let runs: Record<string, GraduationRun>;
let start: (args: Record<string, unknown>) => Promise<GraduationRun>;
let vendor: string | null;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  resetCreatePullRequestStores();
  handlers.length = 0;
  logged.mockReset();
  runs = {};
  vendor = "claude_code";
  start = async (args) => {
    const run = makePrSummaryRun("pr-1", "queued", {
      prSummary: { head: HEAD, base: String(args.base), mode: args.mode as "diff" },
    });
    runs[run.id] = run;
    return run;
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "start_pull_request_summary":
        return start(args);
      case "get_graduation_run":
        return runs[args.runId as string] ?? Promise.reject(new Error("unknown_run"));
      case "list_graduation_queue":
        return { projectKey: "/p", runs: Object.values(runs) };
      case "get_project_agentic_integration":
        return { vendor, resolution: "inherited", overrideVendor: null };
      case "append_log_records":
        return undefined;
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
  openPullRequestSession({ head: HEAD, base: "main", title: "panel" });
  updatePullRequestSession(HEAD, { body: "typed by the author" });
});

const session = () => getPullRequestSession(HEAD)!;
const until = (check: () => void) => vi.waitFor(check);

/** The run `pr-1` ended in `state`, with `result` when it completed. */
function end(state: "completed" | "failed" | "discarded", result?: { title: string; description: string }) {
  const run = runs["pr-1"];
  runs["pr-1"] = {
    ...run,
    state,
    failure: state === "failed" ? { code: "agent_failed", message: "The agent stopped.", retryable: false } : null,
    prSummary: { ...run.prSummary!, result: result ?? null },
  };
  fireRunChanged("pr-1", state);
}

describe("Generate (CPR-FR-UNAN, CPR-FR-VOEB)", () => {
  it("CPR-FR-UNAN: starts a diff run for the head branch and the chosen base", async () => {
    updatePullRequestSession(HEAD, { base: "develop" });
    await generatePullRequestSummary(HEAD);
    expect(calls("start_pull_request_summary")).toHaveLength(1);
    expect(calls("start_pull_request_summary")[0][1]).toEqual({
      head: HEAD,
      base: "develop",
      mode: "diff",
    });
    expect(session().summary).toMatchObject({ runId: "pr-1", mode: "diff", status: "running" });
  });

  it("CPR-FR-UNAN: a second activation while one starts or runs starts nothing", async () => {
    const first = generatePullRequestSummary(HEAD);
    const second = generatePullRequestSummary(HEAD);
    await Promise.all([first, second]);
    await generatePullRequestSummary(HEAD);
    expect(calls("start_pull_request_summary")).toHaveLength(1);
  });

  it("CPR-FR-UNAN: a refused start is stated in words and changes no field", async () => {
    start = async () => Promise.reject("pr_summary_active: pr-9");
    await generatePullRequestSummary(HEAD);
    expect(session().summary).toMatchObject({
      status: "failed",
      message: "A PR summary run for this head branch is already running.",
    });
    expect(session().title).toBe("panel");
    expect(session().body).toBe("typed by the author");
    // Generate is available again.
    await generatePullRequestSummary(HEAD);
    expect(calls("start_pull_request_summary")).toHaveLength(2);
  });

  it("CPR-FR-VOEB: follows the run through the event, with its newest record", async () => {
    await generatePullRequestSummary(HEAD);
    runs["pr-1"] = { ...runs["pr-1"], state: "working" };
    fireRunChanged("pr-1", "working");
    await until(() => expect(session().summary?.run?.state).toBe("working"));
    expect(session().summary?.status).toBe("running");
  });

  it("CPR-FR-VOEB: an event of another run reads nothing", async () => {
    await generatePullRequestSummary(HEAD);
    fireRunChanged("other-run", "completed");
    expect(calls("get_graduation_run")).toHaveLength(0);
  });
});

describe("the end of a followed run (CPR-FR-GYVD, CPR-FR-URIL)", () => {
  it("CPR-FR-GYVD: a completed diff run replaces the title and the description, with no window open", async () => {
    await generatePullRequestSummary(HEAD);
    end("completed", { title: "Add the panel", description: "## Summary\n* The panel" });
    await until(() => expect(session().summary?.status).toBe("completed"));
    expect(session().title).toBe("Add the panel");
    expect(session().body).toBe("## Summary\n* The panel");
    expect(calls("get_graduation_run").slice(-1)[0]?.[1]).toEqual({ runId: "pr-1" });
  });

  it("CPR-FR-URIL: a failed run changes no field and keeps the typed cause", async () => {
    await generatePullRequestSummary(HEAD);
    end("failed");
    await until(() => expect(session().summary?.status).toBe("failed"));
    expect(session().summary?.message).toBe("The agent stopped.");
    expect(session().title).toBe("panel");
    expect(session().body).toBe("typed by the author");
  });

  it("CPR-FR-URIL: a discarded run changes no field and says so", async () => {
    await generatePullRequestSummary(HEAD);
    end("discarded");
    await until(() => expect(session().summary?.status).toBe("discarded"));
    expect(session().summary?.message).toBe("The run was discarded.");
    expect(session().body).toBe("typed by the author");
  });

  it("CPR-FR-URIL, CPR-FR-UNAN: after the end, Generate starts again", async () => {
    await generatePullRequestSummary(HEAD);
    end("failed");
    await until(() => expect(session().summary?.status).toBe("failed"));
    await generatePullRequestSummary(HEAD);
    expect(calls("start_pull_request_summary")).toHaveLength(2);
  });

  it("CPR-FR-FFQD: the start and the end are logged with the head and the run id, and no title or description text", async () => {
    await generatePullRequestSummary(HEAD);
    end("completed", { title: "Secret title", description: "Secret description" });
    await until(() => expect(session().summary?.status).toBe("completed"));
    const messages = logged.mock.calls.map((c) => c[2]);
    expect(messages).toContain("pull request summary run started");
    expect(messages).toContain("pull request summary run ended");
    const ended = logged.mock.calls.find((c) => c[2] === "pull request summary run ended")!;
    expect(ended[3]).toMatchObject({ head: HEAD, runId: "pr-1", state: "completed" });
    const text = JSON.stringify(logged.mock.calls);
    expect(text).not.toContain("Secret title");
    expect(text).not.toContain("Secret description");
    expect(text).not.toContain("typed by the author");
  });

  it("CPR-FR-ESXZ: an answer that arrives after a change of the project changes nothing", async () => {
    let release: (run: GraduationRun) => void = () => {};
    start = () => new Promise((resolve) => (release = resolve));
    const pending = generatePullRequestSummary(HEAD);
    clearAllPullRequestSessions();
    openPullRequestSession({ head: HEAD, base: "main", title: "other project" });
    release(makePrSummaryRun("pr-1", "queued", { prSummary: { head: HEAD } }));
    await pending;
    expect(session().summary).toBeNull();
    expect(handlers).toHaveLength(0);
  });
});

describe("a run that can no longer be read (CPR-FR-URIL)", () => {
  it("CPR-FR-URIL: a terminal run that cannot be read ends the follow, so Generate is available again", async () => {
    await generatePullRequestSummary(HEAD);
    delete runs["pr-1"];
    fireRunChanged("pr-1", "completed");
    await until(() => expect(session().summary?.status).toBe("failed"));
    expect(session().summary?.message).toBe("The run could not be read.");
    expect(session().body).toBe("typed by the author");
    expect(handlers).toHaveLength(0);
  });

  it("CPR-FR-URIL: a followed run that is gone from the queue ends the follow", async () => {
    await generatePullRequestSummary(HEAD);
    delete runs["pr-1"];
    await discoverPullRequestSummaryRun(HEAD);
    expect(session().summary).toMatchObject({
      status: "failed",
      message: "The run is no longer in the queue.",
    });
  });

  it("CPR-FR-URIL: a completed run with no result changes no field", async () => {
    await generatePullRequestSummary(HEAD);
    end("completed");
    await until(() => expect(session().summary?.status).toBe("failed"));
    expect(session().title).toBe("panel");
  });

  it("CPR-FR-ESXZ: a run that ends after its saved state was cleared writes nothing and stops the listener", async () => {
    await generatePullRequestSummary(HEAD);
    clearPullRequestSession(HEAD);
    end("completed", { title: "T", description: "D" });
    await until(() => expect(handlers).toHaveLength(0));
    expect(getPullRequestSession(HEAD)).toBeNull();
  });
});

describe("the queue (CPR-FR-VOEB)", () => {
  it("CPR-FR-VOEB: a run of the head branch that is not terminal is learned from the queue", async () => {
    runs["pr-7"] = makePrSummaryRun("pr-7", "working", { prSummary: { head: HEAD, mode: "diff" } });
    runs["pr-0"] = makePrSummaryRun("pr-0", "completed", { prSummary: { head: HEAD } });
    runs["pr-x"] = makePrSummaryRun("pr-x", "working", { prSummary: { head: "another" } });
    await discoverPullRequestSummaryRun(HEAD);
    expect(session().summary).toMatchObject({ runId: "pr-7", status: "running" });
  });

  it("CPR-FR-GYVD: a followed run that ended while no event arrived is applied from the queue", async () => {
    await generatePullRequestSummary(HEAD);
    runs["pr-1"] = {
      ...runs["pr-1"],
      state: "completed",
      prSummary: { ...runs["pr-1"].prSummary!, result: { title: "T2", description: "D2" } },
    };
    await discoverPullRequestSummaryRun(HEAD);
    expect(session().title).toBe("T2");
    expect(session().body).toBe("D2");
  });

  it("CPR-FR-VOEB: a queue that holds only terminal runs follows nothing", async () => {
    runs["pr-0"] = makePrSummaryRun("pr-0", "failed", { prSummary: { head: HEAD } });
    await discoverPullRequestSummaryRun(HEAD);
    expect(session().summary).toBeNull();
  });
});

describe("the combine (CPR-FR-WCBQ, CPR-FR-PJTG, CPR-FR-ZZNM)", () => {
  const SUMMARIES = [
    branchRun("r1", { runSummary: { title: "First run", description: "Did one." } }),
    branchRun("r2"),
    branchRun("r3", { runSummary: { title: "Third run", description: "Did three." } }),
  ];

  it("CPR-FR-ZZNM: the sections are a heading, a blank line and the description, in landing order", () => {
    expect(
      combinedSections([
        { title: "A", description: "a" },
        { title: "B", description: "b" },
      ]),
    ).toBe("## A\n\na\n\n## B\n\nb");
  });

  it("CPR-FR-PJTG, CPR-FR-ZZNM: sets the first title, writes the sections, and starts a combine run", async () => {
    await combinePullRequestRunSummaries(HEAD, SUMMARIES);
    expect(session().title).toBe("First run");
    expect(session().body).toBe("## First run\n\nDid one.\n\n## Third run\n\nDid three.");
    expect(calls("start_pull_request_summary")[0][1]).toEqual({
      head: HEAD,
      base: "main",
      mode: "combine",
    });
    expect(session().summary).toMatchObject({ mode: "combine", status: "running" });
    expect(logged.mock.calls.map((c) => c[2])).toContain("pull request run summaries combined");
  });

  it("CPR-FR-PJTG: a completed combine run fills only the description", async () => {
    await combinePullRequestRunSummaries(HEAD, SUMMARIES);
    end("completed", { title: "Agent title", description: "Agent description" });
    await until(() => expect(session().summary?.status).toBe("completed"));
    expect(session().title).toBe("First run");
    expect(session().body).toBe("Agent description");
  });

  it("CPR-FR-ZZNM: a combine run that fails keeps the sections", async () => {
    await combinePullRequestRunSummaries(HEAD, SUMMARIES);
    end("failed");
    await until(() => expect(session().summary?.status).toBe("failed"));
    expect(session().body).toBe("## First run\n\nDid one.\n\n## Third run\n\nDid three.");
  });

  it("CPR-FR-ZZNM: with no agentic integration the sections stay and no run starts", async () => {
    vendor = null;
    await combinePullRequestRunSummaries(HEAD, SUMMARIES);
    expect(session().body).toBe("## First run\n\nDid one.\n\n## Third run\n\nDid three.");
    expect(calls("start_pull_request_summary")).toHaveLength(0);
    expect(session().summary).toBeNull();
  });

  it("CPR-FR-WCBQ: no run summary starts no run and keeps the source", async () => {
    await combinePullRequestRunSummaries(HEAD, [branchRun("r1"), branchRun("r2")]);
    expect(session().title).toBe("panel");
    expect(calls("start_pull_request_summary")).toHaveLength(0);
    expect(calls("get_project_agentic_integration")).toHaveLength(0);
  });

  it("CPR-FR-ZZNM: a combine runs once for a head branch", async () => {
    await combinePullRequestRunSummaries(HEAD, SUMMARIES);
    updatePullRequestSession(HEAD, { title: "edited" });
    await combinePullRequestRunSummaries(HEAD, SUMMARIES);
    expect(calls("start_pull_request_summary")).toHaveLength(1);
    expect(session().title).toBe("edited");
  });
});
