/**
 * The raise of an interrupted graduation run at the App seam
 * (`GRU-graduation-runs.md` GRU-FR-BLSS, GRU-FR-FJZD, GRU-FR-KWQX, and
 * `NTF-notifications.md` NTF-FR-24): a stop on a failure calls the author
 * back, and a stop the author or the application made does not. A run raises
 * once per entry into a state, not on each change while it stays there.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";

import App from "./App";
import { flushLogs, resetLogBufferForTest } from "./logging";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { mintAddress } from "./state/notificationAddress";
import { resetNotifications } from "./state/notifications";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetPanelReveals } from "./state/panelReveal";
import {
  activeWorktreePath,
  defaultInvoke,
  enterIde,
  resetAppFixture,
} from "./test/appFixtures";
import { makeRun } from "./test/graduationFixtures";
import type { GraduationInterruptionReason } from "./types/graduation";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<string, Array<(e: { payload?: unknown }) => void>>,
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
  listen: vi.fn(async (name: string, handler: (e: { payload?: unknown }) => void) => {
    (eventHandlers[name] ??= []).push(handler);
    return () => {
      eventHandlers[name] = (eventHandlers[name] ?? []).filter((h) => h !== handler);
    };
  }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => {
  const win = {
    setResizable: async () => {},
    setMaximizable: async () => {},
    setSize: async () => {},
    isMaximized: async () => false,
    unmaximize: async () => {},
    maximize: async () => {},
    outerSize: async () => ({ width: 1440, height: 900 }),
    onResized: async () => () => {},
    scaleFactor: async () => 1,
  };
  return {
    LogicalSize: class {
      constructor(
        public width: number,
        public height: number,
      ) {}
    },
    getCurrentWindow: () => win,
    availableMonitors: async () => [],
  };
});

function fireBusEvent(name: string, payload?: unknown) {
  for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
}
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const posts = () =>
  calls("post_notification").map(
    (c) => (c[1] as { request: Record<string, unknown> }).request,
  );

let run: ReturnType<typeof makeRun>;

function interrupted(reason: GraduationInterruptionReason) {
  return makeRun("r1", "interrupted", {
    interruption: {
      reason,
      detail: "What the backend said.",
      at: "2026-10-03T22:02:36Z",
    },
  });
}

beforeEach(() => {
  resetLogBufferForTest();
  resetPanelReveals();
  resetAppFixture();
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  resetNotifications();
  for (const k in eventHandlers) delete eventHandlers[k];
  run = interrupted("execution_timeout");
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "get_graduation_run":
        return run;
      default:
        return defaultInvoke(cmd, args);
    }
  });
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
  vi.spyOn(document, "hasFocus").mockReturnValue(false);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("the raise of an interrupted graduation run", () => {
  it("GRU-FR-BLSS, GRU-FR-FJZD, NTF-FR-24: a run stopped at its time limit raises once, naming the run and the cause", async () => {
    render(<App />);
    await enterIde();
    act(() => fireBusEvent("graduation-run-changed", { runId: "r1" }));
    await waitFor(() => expect(posts()).toHaveLength(1));
    const posted = posts()[0];
    expect(posted.key).toBe("r1:interrupted");
    expect(posted.body).toBe(
      `“${run.input.draftName}” stopped: the turn reached its time limit.`,
    );
    expect(posted.payload).toBe(
      mintAddress("~/dev/acme", activeWorktreePath, { kind: "run", runId: "r1" }),
    );
  });

  it.each(["author_pause", "application_shutdown", "project_changed"] as const)(
    "GRU-FR-BLSS, NTF-FR-24: a run stopped on %s raises nothing",
    async (reason) => {
      run = interrupted(reason);
      render(<App />);
      await enterIde();
      act(() => fireBusEvent("graduation-run-changed", { runId: "r1" }));
      await waitFor(() => expect(calls("get_graduation_run").length).toBeGreaterThan(0));
      // The read answered, and nothing was raised on it.
      await new Promise((resolve) => setTimeout(resolve, 20));
      expect(posts()).toHaveLength(0);
    },
  );
});

describe("the raise of a run that stays in its rest", () => {
  const AT = "2026-11-08T09:00:00Z";
  function planRest(over: Partial<NonNullable<ReturnType<typeof makeRun>["rest"]>> = {}) {
    return makeRun("r1", "awaiting_author", {
      openspec: { phase: "apply", change: "add-logout" },
      rest: { reason: "plan_update", at: AT, threadId: "pt-1", setId: "qs-1", ...over },
    });
  }

  /** The records the app logged so far, flushed at once. */
  function logged(): string[] {
    flushLogs();
    return calls("append_log_records").flatMap((c) =>
      (c[1] as { records: { message: string }[] }).records.map((r) => r.message),
    );
  }

  /** Fire one change and wait until the app has read the run for it. */
  async function change(next: ReturnType<typeof makeRun>) {
    run = next;
    const reads = calls("get_graduation_run").length;
    act(() => fireBusEvent("graduation-run-changed", { runId: "r1" }));
    await waitFor(() => expect(calls("get_graduation_run").length).toBeGreaterThan(reads));
  }

  it("GRU-FR-BLSS, GRU-FR-KWQX: a second change of the same plan rest raises nothing, and a new rest raises again", async () => {
    render(<App />);
    await enterIde();
    await change(planRest());
    await waitFor(() => expect(posts()).toHaveLength(1));
    expect(posts()[0].key).toBe("r1:awaiting_author");
    // The submission of the answers saves the run in the same rest.
    await change(planRest({ answers: [{ questionPosition: 1, ownAnswer: "The header." }] }));
    await waitFor(() => expect(logged()).toContain("a run changed in the state it raised for"));
    expect(posts()).toHaveLength(1);
    // A new rest is a new entry, so it raises.
    await change(planRest({ reason: "plan_findings", at: "2026-11-08T10:00:00Z", setId: null }));
    await waitFor(() => expect(posts()).toHaveLength(2));
  });

  it("GRU-FR-BLSS, GRU-FR-KWQX: a run that already rests when the project opens raises nothing when the answers are handed over", async () => {
    const resting = planRest();
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      switch (cmd) {
        case "get_graduation_run":
          return run;
        case "list_graduation_queue": {
          const queue = (await defaultInvoke(cmd, args)) as Record<string, unknown>;
          return { ...queue, runs: [resting] };
        }
        default:
          return defaultInvoke(cmd, args);
      }
    });
    render(<App />);
    await enterIde();
    await waitFor(() =>
      expect(logged()).toContain("the resting runs of the project count as raised"),
    );
    await change(planRest({ answers: [{ questionPosition: 1, ownAnswer: "The header." }] }));
    await waitFor(() => expect(logged()).toContain("a run changed in the state it raised for"));
    expect(posts()).toHaveLength(0);
    // A new rest of that run is news, so it raises.
    await change(planRest({ reason: "plan_findings", at: "2026-11-08T10:00:00Z", setId: null }));
    await waitFor(() => expect(posts()).toHaveLength(1));
  });

  it("GRU-FR-BLSS: a run that leaves its rest and enters the same rest again raises again", async () => {
    render(<App />);
    await enterIde();
    await change(planRest());
    await waitFor(() => expect(posts()).toHaveLength(1));
    await change(makeRun("r1", "working", { openspec: { phase: "apply", change: "add-logout" } }));
    await waitFor(() => expect(logged()).toContain("a run left the state it raised for"));
    await change(planRest());
    await waitFor(() => expect(posts()).toHaveLength(2));
  });
});
