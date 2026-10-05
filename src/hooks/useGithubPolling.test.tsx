import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";

import { useGithubPolling, type GithubPollingOptions } from "./useGithubPolling";
import { pollingView } from "../test/githubPollingFixtures";
import type { GithubPollingView } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type Handler = (ev: { payload: unknown }) => void;
let handlers: [string, Handler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: Handler) => {
    handlers.push([name, handler]);
    return Promise.resolve(() => {
      handlers = handlers.filter(([, h]) => h !== handler);
    });
  },
}));
const firePollingChanged = (newIssues: unknown[] = []) =>
  handlers
    .filter(([n]) => n === "github-polling-changed")
    .forEach(([, h]) => h({ payload: { newIssues } }));

/** What `get_github_polling_state` answers; a test may change it. */
let state: GithubPollingView;
/** What `poll_github_ready_tasks` does; a test may replace it. */
let pollImpl: () => Promise<GithubPollingView>;
let claimImpl: (n: number) => Promise<unknown>;
let retryImpl: (n: number) => Promise<unknown>;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
/** The order in which the claim flow's commands ran. */
let order: string[];

beforeEach(() => {
  vi.useFakeTimers();
  handlers = [];
  order = [];
  state = pollingView();
  pollImpl = async () => state;
  claimImpl = async () => ({ draftId: "gh-42", draftName: "Add retry" });
  retryImpl = async () => ({ draftId: "gh-17", draftName: "Old claim" });
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: { issueNumber?: number }) => {
    order.push(cmd);
    switch (cmd) {
      case "get_github_polling_state":
        return state;
      case "poll_github_ready_tasks":
        return pollImpl();
      case "claim_github_task":
        return claimImpl(args!.issueNumber!);
      case "retry_github_claim":
        return retryImpl(args!.issueNumber!);
      default:
        return undefined;
    }
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const flush = () => act(async () => {
  await vi.advanceTimersByTimeAsync(0);
});
const advance = (ms: number) => act(async () => {
  await vi.advanceTimersByTimeAsync(ms);
});

function mount(over: Partial<GithubPollingOptions> = {}) {
  const openGraduationStart = vi.fn(async () => {
    order.push("open_dialog");
    return true;
  });
  const props: GithubPollingOptions = {
    active: true,
    projectKey: "~/dev/acme",
    contentRootEpoch: 0,
    draftsRevision: 0,
    openGraduationStart,
    ...over,
  };
  const hook = renderHook((p: GithubPollingOptions) => useGithubPolling(p), {
    initialProps: props,
  });
  return { ...hook, props, openGraduationStart };
}

describe("the polling schedule (GIT-FR-CVSB, GIT-FR-SLRD)", () => {
  it("GIT-FR-CVSB: polls once when the project opens, then once per interval", async () => {
    const { result } = mount();
    await flush();
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
    expect(result.current.view?.tasks).toHaveLength(1);

    await advance(5 * 60_000 - 1);
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
    await advance(1);
    expect(calls("poll_github_ready_tasks")).toHaveLength(2);
    await advance(5 * 60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(3);
  });

  it("GIT-FR-CVSB / SET-FR-NLIX: no interval starts no launch poll and no timed poll", async () => {
    state = pollingView({ settings: { projectNodeId: "P1", intervalMinutes: null } });
    mount();
    await flush();
    await advance(60 * 60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(0);
    expect(calls("get_github_polling_state").length).toBeGreaterThan(0);
  });

  it("GIT-FR-CVSB: no selected Project starts no poll", async () => {
    state = pollingView({ settings: { projectNodeId: null, intervalMinutes: 5 } });
    mount();
    await flush();
    await advance(10 * 60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(0);
  });

  it("GIT-FR-CVSB: an invalid configuration starts no poll, and one found by a poll stops the timer", async () => {
    state = pollingView({
      configuration: {
        state: "invalid",
        errorCode: "status_field_missing",
        error: "No Status field.",
        projectTitle: "Roadmap",
      },
    });
    mount();
    await flush();
    await advance(10 * 60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(0);
  });

  it("GIT-FR-CVSB: a configuration error found by the launch poll stops the timed polls", async () => {
    pollImpl = async () =>
      pollingView({
        configuration: {
          state: "invalid",
          errorCode: "ready_option_missing",
          error: "No Ready option.",
          projectTitle: "Roadmap",
        },
      });
    mount();
    await flush();
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
    await advance(20 * 60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
  });

  it("GIT-FR-SLRD: a settings change reported by the event reschedules from that moment", async () => {
    mount();
    await flush();
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
    await advance(3 * 60_000);

    state = pollingView({ settings: { projectNodeId: "P1", intervalMinutes: 1 } });
    firePollingChanged();
    await flush();
    await advance(60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(2);
    await advance(60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(3);
  });

  it("GIT-FR-SLRD: setting the interval Off through Project settings stops the timer", async () => {
    mount();
    await flush();
    state = pollingView({ settings: { projectNodeId: "P1", intervalMinutes: null } });
    firePollingChanged();
    await flush();
    await advance(30 * 60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
  });

  it("GIT-FR-SLRD: closing the project stops its schedule", async () => {
    const { rerender, props } = mount();
    await flush();
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
    rerender({ ...props, active: false, projectKey: null });
    await flush();
    await advance(30 * 60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
  });

  it("GIT-FR-SLRD: switching the project starts a fresh session and drops the old one's result", async () => {
    let release: (v: GithubPollingView) => void = () => {};
    pollImpl = () => new Promise((resolve) => (release = resolve));
    const { result, rerender, props } = mount();
    await flush();
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);

    rerender({ ...props, projectKey: "~/dev/other" });
    // The previous project's poll settles late; its rows must not appear.
    const late = pollingView({ tasks: [] });
    release(late);
    await flush();
    expect(result.current.view?.tasks).toHaveLength(1);
    // The new project ran its own launch poll.
    expect(calls("poll_github_ready_tasks").length).toBe(2);
  });

  it("GIT-FR-CVSB: a timer tick while a poll is in flight starts no second poll", async () => {
    let release: (v: GithubPollingView) => void = () => {};
    pollImpl = () => new Promise((resolve) => (release = resolve));
    const { result } = mount();
    await flush();
    expect(result.current.pollInFlight).toBe(true);
    await advance(5 * 60_000);
    await advance(5 * 60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
    await act(async () => {
      await result.current.refresh();
    });
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
    release(state);
    await flush();
    expect(result.current.pollInFlight).toBe(false);
  });

  it("GIT-FR-OGHO: every github-polling-changed re-reads the view", async () => {
    const { result } = mount();
    await flush();
    const before = calls("get_github_polling_state").length;
    state = pollingView({ tasks: [] });
    firePollingChanged();
    await flush();
    expect(calls("get_github_polling_state").length).toBe(before + 1);
    expect(result.current.view?.tasks).toHaveLength(0);
  });

  it("GIT-FR-EZFL: Refresh polls at once even with the interval Off, and a refusal is reported", async () => {
    state = pollingView({ settings: { projectNodeId: "P1", intervalMinutes: null } });
    const { result } = mount();
    await flush();
    await act(async () => {
      await result.current.refresh();
    });
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);

    pollImpl = async () => Promise.reject("polling_configuration_invalid");
    await act(async () => {
      await result.current.refresh();
    });
    expect(result.current.refreshError).toMatch(/configuration has an error/);
  });
});

describe("the claim flow (GIT-FR-NQTZ, GIT-FR-FTHT, GIT-FR-TZUI)", () => {
  it("GIT-FR-NQTZ: claim opens the start dialog then acknowledges", async () => {
    const { result, openGraduationStart } = mount();
    await flush();
    order = [];
    await act(async () => {
      await result.current.claim(42);
    });
    expect(openGraduationStart).toHaveBeenCalledWith("gh-42", "Add retry");
    expect(order).toEqual([
      "claim_github_task",
      "open_dialog",
      "acknowledge_github_claim",
    ]);
    expect(calls("acknowledge_github_claim")[0][1]).toEqual({ issueNumber: 42 });
    expect(result.current.rowBusy.size).toBe(0);
  });

  it("GIT-FR-NQTZ: nothing is acknowledged until the dialog is open", async () => {
    let opened: (v: boolean) => void = () => {};
    const openGraduationStart = vi.fn(
      () => new Promise<boolean>((resolve) => (opened = resolve)),
    );
    const { result } = mount({ openGraduationStart });
    await flush();
    let done: Promise<void> = Promise.resolve();
    act(() => {
      done = result.current.claim(42);
    });
    await flush();
    expect(openGraduationStart).toHaveBeenCalled();
    expect(calls("acknowledge_github_claim")).toHaveLength(0);
    expect(result.current.rowBusy.get(42)).toBe("claim");
    await act(async () => {
      opened(true);
      await done;
    });
    expect(calls("acknowledge_github_claim")).toHaveLength(1);
  });

  it("GIT-FR-NQTZ: a dialog that never opened leaves the claim pending and unacknowledged", async () => {
    const openGraduationStart = vi.fn(async () => false);
    const { result } = mount({ openGraduationStart });
    await flush();
    await act(async () => {
      await result.current.claim(42);
    });
    expect(calls("acknowledge_github_claim")).toHaveLength(0);
  });

  it("GIT-FR-NQTZ: a refusal renders on that row and opens no dialog", async () => {
    claimImpl = () => Promise.reject("task_not_ready");
    const { result, openGraduationStart } = mount();
    await flush();
    await act(async () => {
      await result.current.claim(42);
    });
    expect(openGraduationStart).not.toHaveBeenCalled();
    expect(calls("acknowledge_github_claim")).toHaveLength(0);
    expect(result.current.rowErrors.get(42)).toMatch(/no longer a ready Task/);
  });

  it("GIT-FR-FTHT: Retry invokes retry_github_claim and continues as a claim does", async () => {
    const { result, openGraduationStart } = mount();
    await flush();
    order = [];
    await act(async () => {
      await result.current.retry(17);
    });
    expect(openGraduationStart).toHaveBeenCalledWith("gh-17", "Old claim");
    expect(order).toEqual([
      "retry_github_claim",
      "open_dialog",
      "acknowledge_github_claim",
    ]);
    expect(calls("claim_github_task")).toHaveLength(0);
  });

  it("GIT-FR-FTHT: a refused retry renders on its row", async () => {
    retryImpl = () => Promise.reject("github_unreachable");
    const { result, openGraduationStart } = mount();
    await flush();
    await act(async () => {
      await result.current.retry(17);
    });
    expect(openGraduationStart).not.toHaveBeenCalled();
    expect(result.current.rowErrors.get(17)).toMatch(/could not be reached/);
  });

  it("GIT-FR-TZUI: a claim in flight holds its own row alone", async () => {
    let release: (v: unknown) => void = () => {};
    claimImpl = (n) =>
      n === 42
        ? new Promise((resolve) => (release = resolve))
        : Promise.resolve({ draftId: "gh-7", draftName: "Other" });
    const { result } = mount();
    await flush();
    let first: Promise<void> = Promise.resolve();
    act(() => {
      first = result.current.claim(42);
    });
    await flush();
    expect(result.current.rowBusy.get(42)).toBe("claim");
    expect(result.current.rowBusy.has(7)).toBe(false);
    // A second claim of the same row is not sent; another row's is.
    await act(async () => {
      await result.current.claim(42);
      await result.current.claim(7);
    });
    expect(calls("claim_github_task").map((c) => c[1])).toEqual([
      { issueNumber: 42 },
      { issueNumber: 7 },
    ]);
    // Refresh stays usable while the claim runs.
    await act(async () => {
      await result.current.refresh();
    });
    expect(calls("poll_github_ready_tasks").length).toBe(2);
    await act(async () => {
      release({ draftId: "gh-42", draftName: "Add retry" });
      await first;
    });
    expect(result.current.rowBusy.size).toBe(0);
  });

  it("GIT-FR-OLNA: Graduate on a shadow row opens the dialog and claims nothing", async () => {
    const { result, openGraduationStart } = mount();
    await flush();
    act(() => result.current.graduateShadow("gh-1", "Cache invalidation"));
    expect(openGraduationStart).toHaveBeenCalledWith("gh-1", "Cache invalidation");
    expect(calls("claim_github_task")).toHaveLength(0);
    expect(calls("acknowledge_github_claim")).toHaveLength(0);
  });

  it("GIT-FR-LUSE / GIT-FR-OZYT: the issue links invoke their operations", async () => {
    const { result } = mount();
    await flush();
    act(() => result.current.openTaskIssue(42));
    act(() =>
      result.current.openShadowIssue(
        "gh-1",
        "https://github.com/acme/platform/issues/9",
        9,
      ),
    );
    await flush();
    expect(calls("open_github_task_issue")[0][1]).toEqual({ issueNumber: 42 });
    expect(calls("open_publication_issue")[0][1]).toEqual({
      draftId: "gh-1",
      url: "https://github.com/acme/platform/issues/9",
    });
  });

  it("GPP-FR-UBDE: a drafts revision re-reads the shadow rows", async () => {
    const { rerender, props } = mount();
    await flush();
    const before = calls("get_github_polling_state").length;
    rerender({ ...props, draftsRevision: 1 });
    await flush();
    expect(calls("get_github_polling_state").length).toBe(before + 1);
  });
});

describe("the polling session (GIT-FR-SLRD, GPP-FR-DATH)", () => {
  it("GIT-FR-SLRD / GPP-FR-DATH: a worktree switch restarts the schedule and drops a late result", async () => {
    let release: (v: GithubPollingView) => void = () => {};
    pollImpl = () => new Promise((resolve) => (release = resolve));
    const { result, rerender, props } = mount();
    await flush();
    expect(calls("poll_github_ready_tasks")).toHaveLength(1);
    const lateRelease = release;

    pollImpl = async () => state;
    rerender({ ...props, contentRootEpoch: 1 });
    await flush();
    // The new session ran its own launch poll at once, not blocked by the old one.
    expect(calls("poll_github_ready_tasks")).toHaveLength(2);
    expect(result.current.view?.tasks).toHaveLength(1);

    // The previous worktree's poll settles late and changes nothing.
    lateRelease(pollingView({ tasks: [] }));
    await flush();
    expect(result.current.view?.tasks).toHaveLength(1);

    // The schedule runs from the switch.
    await advance(5 * 60_000);
    expect(calls("poll_github_ready_tasks")).toHaveLength(3);
  });

  it("GIT-FR-NQTZ / GPP-FR-ELWS: a switch while the claim runs opens no dialog and acknowledges nothing", async () => {
    let release: (v: unknown) => void = () => {};
    claimImpl = () => new Promise((resolve) => (release = resolve));
    const { result, rerender, props, openGraduationStart } = mount();
    await flush();
    let done: Promise<void> = Promise.resolve();
    act(() => {
      done = result.current.claim(42);
    });
    await flush();
    rerender({ ...props, contentRootEpoch: 1 });
    await act(async () => {
      release({ draftId: "gh-42", draftName: "Add retry" });
      await done;
    });
    expect(openGraduationStart).not.toHaveBeenCalled();
    expect(calls("acknowledge_github_claim")).toHaveLength(0);
  });

  it("GIT-FR-NQTZ / GPP-FR-ELWS: a switch while the dialog opens acknowledges nothing", async () => {
    let opened: (v: boolean) => void = () => {};
    const openGraduationStart = vi.fn(
      () => new Promise<boolean>((resolve) => (opened = resolve)),
    );
    const { result, rerender, props } = mount({ openGraduationStart });
    await flush();
    let done: Promise<void> = Promise.resolve();
    act(() => {
      done = result.current.claim(42);
    });
    await flush();
    expect(openGraduationStart).toHaveBeenCalled();
    rerender({ ...props, openGraduationStart, projectKey: "~/dev/other" });
    await act(async () => {
      opened(true);
      await done;
    });
    expect(calls("acknowledge_github_claim")).toHaveLength(0);
  });
});

