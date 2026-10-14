/**
 * GRU-FR-WKTL / GRU-FR-TTLR / GRU-FR-QLRQ / GRU-FR-TOKG: a run that stopped on
 * a time limit, in the Runs section. A run at rest on its total time limit
 * states the limit and the time it used, offers Continue, and routes to the
 * Total time per run slider. A run interrupted at the stage time limit routes
 * to the Time per turn slider.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { resetLogBufferForTest } from "../../logging";
import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import { conditionSentence, raiseStatement } from "../../state/graduation";
import {
  makeMergeRun,
  makeObservability,
  makePass,
  makeQueue,
  makeRun,
} from "../../test/graduationFixtures";
import type { GraduationQueue, GraduationRun } from "../../types";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

let queue: GraduationQueue;

const FIVE_HOURS = 18_000_000;

/** A run at rest on its total time limit (GRD-FR-SKLO). */
function restingRun(over: Partial<GraduationRun> = {}): GraduationRun {
  return makeRun("r1", "awaiting_author", {
    // A budget window that a spent pass budget would report, so a test can
    // see that the rest does not say it.
    checkpoint: { pass: 2, passLimit: 2, passFloor: 1 },
    turnTimeUsedMs: FIVE_HOURS,
    rest: {
      reason: "total_time_limit",
      at: "2026-10-03T22:02:36Z",
      limitMs: FIVE_HOURS,
      usedMs: FIVE_HOURS,
    },
    ...over,
  });
}

beforeEach(() => {
  queue = makeQueue([restingRun()]);
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    switch (command) {
      case "list_graduation_queue":
        return queue;
      case "load_app_preferences":
        return { graduationRailWidthFraction: 0.2 };
      case "list_work_streams":
        return [];
      case "continue_graduation_run":
        return queue.runs[0];
      case "append_log_records":
        return undefined;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
  listened.mockReset();
  listened.mockImplementation(async () => () => {});
  resetAppPreferencesCache();
  forgetEverything();
  forgetEveryDraft();
  resetLogBufferForTest();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function commands(): string[] {
  return invoked.mock.calls.map(([command]) => command as string);
}

async function open(props: Parameters<typeof GraduationRuns>[0] = {}) {
  render(<GraduationRuns {...props} />);
  await screen.findByTestId("graduation-section");
  await waitFor(() => expect(commands()).toContain("list_graduation_queue"));
}

describe("Graduation runs — time limits", () => {
  it("GRU-FR-WKTL: a run at rest on the total time limit states the reason, the limit and the used time", async () => {
    await open();
    const notice = await screen.findByTestId("graduation-rest");
    expect(notice).toHaveTextContent(
      "Stopped at the total time limit · 5 h of 5 h used",
    );
    expect(screen.getByTestId("graduation-rest-detail")).toHaveTextContent(
      "The run used 5 h of working and reviewing time. Its total time limit is 5 h.",
    );
    // GRD-FR-SKLO: a rest is not a spent pass budget, so no surface says one.
    expect(screen.getByTestId("graduation-section")).not.toHaveTextContent(/passes spent|pass spent/);
    expect(screen.queryByTestId("graduation-blocker")).toBeNull();
    expect(screen.queryByTestId("graduation-escalation")).toBeNull();
  });

  it("GRU-FR-WKTL: the run offers Continue, and Continue continues it", async () => {
    await open();
    await screen.findByTestId("graduation-rest");
    const region = screen.getByRole("region", { name: "The selected run" });
    await userEvent.click(within(region).getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(commands()).toContain("continue_graduation_run"));
  });

  it("GRU-FR-TTLR: Change the time limit routes to the Total time per run slider and does not continue the run", async () => {
    const onOpenSettings = vi.fn();
    await open({ onOpenSettings });
    const notice = await screen.findByTestId("graduation-rest");
    await userEvent.click(within(notice).getByRole("button", { name: "Change the time limit" }));
    expect(onOpenSettings).toHaveBeenCalledWith("project/graduation:total-time-limit");
    expect(commands()).not.toContain("continue_graduation_run");
  });

  it("GRU-FR-WKTL: a used time that is not whole hours reads in hours and minutes", async () => {
    queue = makeQueue([
      restingRun({
        rest: {
          reason: "total_time_limit",
          at: "2026-10-03T22:02:36Z",
          limitMs: 12_600_000,
          usedMs: 12_600_000 + 90_000,
        },
      }),
    ]);
    await open();
    expect(await screen.findByTestId("graduation-rest")).toHaveTextContent(
      "Stopped at the total time limit · 3 h 32 min of 3 h 30 min used",
    );
  });

  it("GRU-FR-QLRQ: a run interrupted at the stage time limit routes to the Time per turn slider", async () => {
    queue = makeQueue([
      makeRun("r1", "interrupted", {
        observability: makeObservability({ stageCondition: "stopped", passes: [makePass()] }),
        interruption: {
          reason: "execution_timeout",
          detail: "The agent turn ran for 3 h and reached the stage time limit of 3 h.",
          at: "2026-10-03T22:02:36Z",
        },
      }),
    ]);
    const onOpenSettings = vi.fn();
    await open({ onOpenSettings });
    const notice = await screen.findByTestId("graduation-interruption");
    await userEvent.click(within(notice).getByRole("button", { name: "Change the time limit" }));
    expect(onOpenSettings).toHaveBeenCalledWith("project/graduation:stage-time-limit");
    expect(screen.queryByTestId("graduation-rest")).toBeNull();
  });

  it("GRU-FR-TOKG, GRU-FR-WKTL: a merge run at rest on the total time limit offers Continue and the route", async () => {
    queue = makeQueue([
      makeMergeRun("m1", "awaiting_author", {
        rest: {
          reason: "total_time_limit",
          at: "2026-10-03T22:02:36Z",
          limitMs: FIVE_HOURS,
          usedMs: FIVE_HOURS,
        },
      }),
    ]);
    const onOpenSettings = vi.fn();
    await open({ onOpenSettings });
    const notice = await screen.findByTestId("graduation-rest");
    expect(notice).toHaveTextContent("Stopped at the total time limit · 5 h of 5 h used");
    const region = screen.getByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Continue" })).toBeInTheDocument();
    await userEvent.click(within(notice).getByRole("button", { name: "Change the time limit" }));
    expect(onOpenSettings).toHaveBeenCalledWith("project/graduation:total-time-limit");
  });

  it("GRU-FR-FZCN, GRU-FR-WKTL: a spent pass budget still says the passes it spent, and a rest does not", () => {
    const spent = makeRun("r2", "awaiting_author", {
      checkpoint: { pass: 2, passLimit: 2, passFloor: 1 },
    });
    expect(conditionSentence(spent)).toBe("Waiting for your decision · 2 passes spent");
    expect(conditionSentence(restingRun())).toBe(
      "Stopped at the total time limit · 5 h of 5 h used",
    );
  });

  it("GRU-FR-BLSS, GRU-FR-WKTL: the raise of a run at rest on the total time limit names that limit", () => {
    expect(raiseStatement(restingRun())).toBe("“Run r1” stopped at its total time limit.");
    expect(raiseStatement(makeRun("r2", "awaiting_author"))).toBe(
      "“Run r2” is waiting for your decision.",
    );
  });
});
