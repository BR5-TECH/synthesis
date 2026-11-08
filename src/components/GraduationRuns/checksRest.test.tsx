/**
 * GRU-FR-WVDV / GRU-FR-TOKG: a run at rest because its required checks did not
 * pass, in the Runs section. It says why, names each check with its exit
 * status or "not finished", says that nothing went to review and nothing was
 * committed, and offers Continue and Discard run.
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
import {
  checksCauseSentence,
  checksNotPassedRestOf,
  conditionSentence,
  raiseStatement,
  restCheckStatus,
  totalTimeLimitRestOf,
} from "../../state/graduation";
import { makeMergeRun, makeQueue, makeRun } from "../../test/graduationFixtures";
import type { GraduationQueue, GraduationRest, GraduationRun } from "../../types";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

let queue: GraduationQueue;

const RED: GraduationRest = {
  reason: "checks_not_passed",
  at: "2026-11-08T09:00:00Z",
  cause: "checks_red",
  checks: [
    { name: "tests", finished: true, exitStatus: 101 },
    { name: "build", finished: false, exitStatus: null },
  ],
};

/** A run at rest because its checks did not pass (GRD-FR-PYJN). */
function restingRun(over: Partial<GraduationRun> = {}): GraduationRun {
  return makeRun("r1", "awaiting_author", {
    // A budget window that a spent pass budget would report, so a test can
    // see that the rest does not say it.
    checkpoint: { pass: 2, passLimit: 2, passFloor: 1 },
    rest: RED,
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

async function open() {
  render(<GraduationRuns />);
  await screen.findByTestId("graduation-section");
  await waitFor(() => expect(commands()).toContain("list_graduation_queue"));
}

describe("Graduation runs — checks that did not pass", () => {
  it("GRU-FR-WVDV: the run says why its checks did not pass and names each check with its status", async () => {
    await open();
    const notice = await screen.findByTestId("graduation-checks-rest");
    expect(notice).toHaveTextContent("Stopped: the required checks did not pass.");
    expect(within(notice).getByTestId("graduation-checks-rest-cause")).toHaveTextContent(
      "A required check failed, and the work turn could not make it pass.",
    );
    const checks = within(notice).getAllByTestId("graduation-checks-rest-check");
    expect(checks.map((item) => item.textContent)).toEqual([
      "tests · exit status 101",
      "build · not finished",
    ]);
    expect(within(notice).getByTestId("graduation-checks-rest-detail")).toHaveTextContent(
      "Nothing went to review and nothing was committed.",
    );
    // GRD-FR-PYJN: a rest is not a spent pass budget, and holds no escalation.
    expect(screen.getByTestId("graduation-section")).not.toHaveTextContent(/passes spent|pass spent/);
    expect(screen.queryByTestId("graduation-escalation")).toBeNull();
    expect(screen.queryByTestId("graduation-rest")).toBeNull();
  });

  it("GRU-FR-WVDV: the run offers Continue and Discard run, and Continue continues it", async () => {
    await open();
    await screen.findByTestId("graduation-checks-rest");
    const region = screen.getByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Discard run" })).toBeInTheDocument();
    await userEvent.click(within(region).getByRole("button", { name: "Continue" }));
    await waitFor(() => expect(commands()).toContain("continue_graduation_run"));
    expect(invoked).toHaveBeenCalledWith("continue_graduation_run", { runId: "r1" });
  });

  it("GRU-FR-WVDV: a rest with no check record names no check", async () => {
    queue = makeQueue([
      restingRun({ rest: { reason: "checks_not_passed", at: RED.at, cause: "record_missing", checks: [] } }),
    ]);
    await open();
    const notice = await screen.findByTestId("graduation-checks-rest");
    expect(notice).toHaveTextContent("The work turn did not report which checks it ran.");
    expect(within(notice).queryByTestId("graduation-checks-rest-list")).toBeNull();
    expect(notice).toHaveTextContent("Nothing went to review and nothing was committed.");
  });

  it("GRU-FR-WVDV: a rest stored without a cause or checks reads as a missing record and names no check", async () => {
    queue = makeQueue([restingRun({ rest: { reason: "checks_not_passed", at: RED.at } })]);
    await open();
    const notice = await screen.findByTestId("graduation-checks-rest");
    expect(within(notice).getByTestId("graduation-checks-rest-cause")).toHaveTextContent(
      "The work turn did not report which checks it ran.",
    );
    expect(within(notice).queryByTestId("graduation-checks-rest-list")).toBeNull();
    const region = screen.getByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Continue" })).toBeInTheDocument();
  });

  it("GRU-FR-WKTL, GRU-FR-WVDV: a total time rest without its used time shows neither rest", async () => {
    queue = makeQueue([restingRun({ rest: { reason: "total_time_limit", at: RED.at, limitMs: 60_000 } })]);
    await open();
    await screen.findByRole("region", { name: "The selected run" });
    expect(screen.queryByTestId("graduation-rest")).toBeNull();
    expect(screen.queryByTestId("graduation-checks-rest")).toBeNull();
    expect(totalTimeLimitRestOf(queue.runs[0])).toBeNull();
    expect(checksNotPassedRestOf(queue.runs[0])).toBeNull();
  });

  it("GRU-FR-TOKG, GRU-FR-WVDV: a merge run at rest on its checks offers Continue and Discard run", async () => {
    queue = makeQueue([makeMergeRun("m1", "awaiting_author", { rest: RED })]);
    await open();
    await screen.findByTestId("graduation-checks-rest");
    const region = screen.getByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Continue" })).toBeInTheDocument();
    expect(within(region).getByRole("button", { name: "Discard run" })).toBeInTheDocument();
  });

  it("GRU-FR-WVDV: every cause has its own sentence, and a status reads as the check ended", () => {
    const causes = ["checks_red", "checks_unfinished", "record_missing", "work_failed"] as const;
    const sentences = causes.map(checksCauseSentence);
    expect(new Set(sentences).size).toBe(causes.length);
    expect(checksCauseSentence("work_failed")).toBe(
      "The work turn reported that it could not finish the work.",
    );
    expect(restCheckStatus({ name: "t", finished: true, exitStatus: 0 })).toBe("exit status 0");
    expect(restCheckStatus({ name: "t", finished: true, exitStatus: null })).toBe("not finished");
    expect(restCheckStatus({ name: "t", finished: false, exitStatus: 3 })).toBe("not finished");
  });

  it("GRU-FR-WVDV, GRU-FR-BLSS: the condition line and the raise name the checks, and no other rest reads as this one", () => {
    expect(conditionSentence(restingRun())).toBe("Stopped: the required checks did not pass");
    expect(raiseStatement(restingRun())).toBe("“Run r1” stopped: its required checks did not pass.");
    // A rest on checks is not a rest on time, and the other way round.
    expect(totalTimeLimitRestOf(restingRun())).toBeNull();
    const timed = restingRun({
      rest: { reason: "total_time_limit", at: RED.at, limitMs: 60_000, usedMs: 60_000 },
    });
    expect(checksNotPassedRestOf(timed)).toBeNull();
    // A run that is not at rest reads no rest at all.
    expect(checksNotPassedRestOf(makeRun("r3", "queued", { rest: RED }))).toBeNull();
  });
});
