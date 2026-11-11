/**
 * An apply run at rest on a plan rest, in the Runs section
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-FZCN,
 * GRU-FR-TCSM, GRU-FR-TVDF, GRU-FR-PZJY, GRU-FR-BPNJ). It says what the rest
 * asks, renders no question form, opens the change with the plan thread
 * selected, and states a refused Continue beside the action row.
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
  configureOpenSpecChangeRoutes,
  resetChangeDiscussionRequests,
} from "../../state/openspec/changeRoutes";
import { makeQueue, makeRun } from "../../test/graduationFixtures";
import type { GraduationQueue, GraduationRest, GraduationRun } from "../../types";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);
const openChange = vi.fn();

const AT = "2026-11-08T09:00:00Z";
const CHANGE = "add-logout";

const WRITE_FAILED_WORDS =
  "The run could not be made ready to continue. Nothing changed; try Continue again.";

let queue: GraduationQueue;
/** What `continue_graduation_run` rejects with, or null where it is accepted. */
let continueRefusal: string | null;

const UPDATE: GraduationRest = {
  reason: "plan_update",
  at: AT,
  origin: "work",
  threadId: "pt-1",
  proposalId: "p-1",
  setId: "qs-1",
  questions: [{ position: 1, question: "Which header?", options: [] }],
  planReason: "The plan is wrong.",
};

/** An apply run of the change, at rest on `rest`. */
function planRun(rest: GraduationRest): GraduationRun {
  return makeRun("r1", "awaiting_author", {
    checkpoint: { pass: 2, passLimit: 2, passFloor: 1 },
    openspec: { phase: "apply", change: CHANGE },
    input: { draftId: "", draftName: "", prompt: "p", promptChecksum: "c", capturedAt: AT },
    rest,
  });
}

beforeEach(() => {
  queue = makeQueue([planRun(UPDATE)]);
  continueRefusal = null;
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    switch (command) {
      case "list_graduation_queue":
        return queue;
      case "load_app_preferences":
        return { graduationRailWidthFraction: 0.2 };
      case "list_work_streams":
        return [];
      case "read_apply_progress":
        return { state: "ready", total: 4, complete: 2, nextTask: "Add the route", missingArtifacts: [] };
      case "continue_graduation_run":
        if (continueRefusal) throw continueRefusal;
        return queue.runs[0];
      case "append_log_records":
        return undefined;
      default:
        throw new Error(`unexpected command ${command}`);
    }
  });
  listened.mockReset();
  listened.mockImplementation(async () => () => {});
  openChange.mockReset();
  configureOpenSpecChangeRoutes({ open: openChange });
  resetChangeDiscussionRequests();
  resetAppPreferencesCache();
  forgetEverything();
  forgetEveryDraft();
  resetLogBufferForTest();
});
afterEach(() => {
  configureOpenSpecChangeRoutes(null);
  cleanup();
  vi.restoreAllMocks();
});

async function selectedRegion() {
  render(<GraduationRuns />);
  await screen.findByTestId("graduation-plan-rest");
  return screen.getByRole("region", { name: "The selected run" });
}

describe("Graduation runs — plan rests", () => {
  it("GRU-FR-TCSM, GRU-FR-FZCN: a plan_update rest says what it asks and where, and renders no question form", async () => {
    const region = await selectedRegion();
    const notice = within(region).getByTestId("graduation-plan-rest");
    expect(notice).toHaveTextContent("The run asks to change the plan of its change.");
    expect(notice).toHaveTextContent("Answer its questions and decide its edits in the change discussion");
    // OPU-FR-QUST: the rest holds no escalation, so no question block renders.
    expect(screen.queryByTestId("graduation-escalation")).toBeNull();
    expect(within(region).queryByRole("button", { name: "Submit" })).toBeNull();
    expect(region).not.toHaveTextContent(/passes spent|pass spent/);
    expect(within(region).getByRole("button", { name: "Continue" })).toBeInTheDocument();
    expect(within(region).getByRole("button", { name: "Discard run" })).toBeInTheDocument();
  });

  it("GRU-FR-TCSM, OCW-FR-QSZP: Open change opens the change tab with the plan thread selected", async () => {
    const region = await selectedRegion();
    await userEvent.click(within(region).getByRole("button", { name: "Open change" }));
    expect(openChange).toHaveBeenCalledTimes(1);
    const [location, name, options] = openChange.mock.calls[0];
    expect(location).toEqual({ kind: "stream", streamId: "s-1" });
    expect(name).toBe(CHANGE);
    expect(options).toMatchObject({ thread: "pt-1" });
  });

  it("GRU-FR-TVDF, OCW-FR-QSZP: a plan_findings rest counts its plan findings, and Open change selects the plan thread", async () => {
    const finding = (kind: "work" | "plan") => ({
      severity: "major" as const,
      description: "d",
      affectedFiles: [],
      correction: "c",
      kind,
    });
    queue = makeQueue([
      planRun({
        reason: "plan_findings",
        at: AT,
        origin: "review",
        threadId: "pt-2",
        findings: [finding("plan"), finding("work"), finding("plan")],
      }),
    ]);
    const region = await selectedRegion();
    expect(within(region).getByTestId("graduation-plan-rest")).toHaveTextContent(
      "The review found that the plan of the change is wrong: 2 plan findings. Settle them in the change discussion",
    );
    expect(within(region).getByRole("button", { name: "Continue" })).toBeInTheDocument();
    expect(within(region).getByRole("button", { name: "Discard run" })).toBeInTheDocument();
    await userEvent.click(within(region).getByRole("button", { name: "Open change" }));
    expect(openChange.mock.calls[0][2]).toMatchObject({ thread: "pt-2" });
  });

  it("GRU-FR-PZJY: a plan_update_invalid rest says why, that nothing was committed, and offers Continue and Discard run", async () => {
    queue = makeQueue([planRun({ reason: "plan_update_invalid", at: AT, cause: "plan_file_not_text" })]);
    const region = await selectedRegion();
    const notice = within(region).getByTestId("graduation-plan-rest");
    expect(notice).toHaveTextContent(
      "The run gave a plan update that the application refused. It proposed a file that is not text.",
    );
    expect(within(notice).getByTestId("graduation-plan-rest-detail")).toHaveTextContent(
      "Nothing was committed.",
    );
    expect(within(region).getByRole("button", { name: "Continue" })).toBeInTheDocument();
    expect(within(region).getByRole("button", { name: "Discard run" })).toBeInTheDocument();
    // A refused plan update has no plan thread to select.
    await userEvent.click(within(region).getByRole("button", { name: "Open change" }));
    expect(openChange.mock.calls[0][2]).not.toHaveProperty("thread");
  });

  it("GRU-FR-PZJY: Continue of a plan_update_invalid rest invokes continue_graduation_run with the run", async () => {
    queue = makeQueue([planRun({ reason: "plan_update_invalid", at: AT, cause: "plan_update_empty" })]);
    const region = await selectedRegion();
    await userEvent.click(within(region).getByRole("button", { name: "Continue" }));
    await waitFor(() =>
      expect(invoked).toHaveBeenCalledWith("continue_graduation_run", { runId: "r1" }),
    );
    // The act has settled only when it announces that it was accepted.
    await waitFor(() =>
      expect(screen.getByTestId("graduation-row-announcement").textContent).toBe(
        "Continued the run.",
      ),
    );
    expect(screen.queryByTestId("graduation-action-refusal")).toBeNull();
    expect(screen.queryByTestId("graduation-error")).toBeNull();
  });

  it.each([
    ["plan_questions_pending", "Answer the questions in the change discussion first, then continue the run."],
    [
      "proposal_pending",
      "Decide the plan edits in the change discussion first, or their commit is still owed. Then continue the run.",
    ],
    [
      "openspec_validation_unavailable: cli_failed",
      "OpenSpec could not be read (cli_failed), so the run did not continue.",
    ],
    ["write_failed", WRITE_FAILED_WORDS],
  ])("GRU-FR-BPNJ: a Continue refused with %s is stated in words beside the action row", async (code, words) => {
    continueRefusal = code;
    const region = await selectedRegion();
    await userEvent.click(within(region).getByRole("button", { name: "Continue" }));
    const alert = await screen.findByTestId("graduation-action-refusal");
    expect(alert).toHaveAttribute("role", "alert");
    // The exact text, so that a prefix such as "Error: " shows as a failure.
    expect(alert.textContent).toBe(words);
    // A refused Continue changes nothing: the rest still stands.
    expect(within(region).getByTestId("graduation-plan-rest")).toBeInTheDocument();
  });

  it("GRU-FR-BPNJ, GRU-FR-XQVG: a refused Continue from the rail of a run at a plan rest states the plan words, with no prefix", async () => {
    continueRefusal = "write_failed";
    await selectedRegion();
    await userEvent.click(screen.getByRole("button", { name: "Continue “Apply add-logout”" }));
    const alert = await screen.findByTestId("graduation-error");
    expect(alert.textContent).toBe(WRITE_FAILED_WORDS);
  });

  it("GRU-FR-BPNJ, GRU-FR-XQVG: a refused Continue of a run at another rest keeps the words of every other run", async () => {
    queue = makeQueue([planRun({ reason: "checks_not_passed", at: AT })]);
    continueRefusal = "proposal_pending";
    render(<GraduationRuns />);
    const region = await screen.findByRole("region", { name: "The selected run" });
    await userEvent.click(await within(region).findByRole("button", { name: "Continue" }));
    const alert = await screen.findByTestId("graduation-action-refusal");
    // The general words do not know the plan codes, so the code renders as itself.
    expect(alert.textContent).toBe("proposal_pending");
    await userEvent.click(screen.getByRole("button", { name: "Continue “Apply add-logout”" }));
    const rail = await screen.findByTestId("graduation-error");
    expect(rail.textContent).toBe("proposal_pending");
  });
});
