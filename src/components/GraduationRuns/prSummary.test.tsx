/**
 * A PR summary run in the graduation section
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-WEBM,
 * GRU-FR-AQKZ, GRU-FR-DQSA, GRU-FR-LJUY).
 *
 * A PR summary run is a graduation run that carries `prSummary` data and has no
 * draft and no stream. These tests render the whole section against a mocked
 * backend, so what is asserted is what an author reads and can press.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { resetLogBufferForTest } from "../../logging";
import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import { makePrSummaryRun, makeQueue } from "../../test/graduationFixtures";
import { pickSelector } from "../../test/selectors";
import type { GraduationQueue, GraduationRun } from "../../types";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

let queue: GraduationQueue;

beforeEach(() => {
  queue = makeQueue([makePrSummaryRun("p1", "working")]);
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    switch (command) {
      case "list_graduation_queue":
        return queue;
      case "load_app_preferences":
        return { graduationRailWidthFraction: 0.2 };
      case "list_work_streams":
        return [];
      case "get_graduation_capacity":
        return { limit: 2, inUse: 1, waitingForSlot: [] };
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

/** The region of the selected run. */
const region = () => screen.getByLabelText("The selected run");

/** Serve one run and open the section on it, in **All** where it has ended. */
async function serve(run: GraduationRun, all = false) {
  queue = makeQueue([run]);
  render(<GraduationRuns />);
  await screen.findByTestId("graduation-section");
  if (all) await pickSelector("Run view", "all");
  await screen.findByTestId("graduation-provenance");
}

describe("a PR summary run in the rail and the region (GRU-FR-WEBM, GRU-FR-AQKZ)", () => {
  it("GRU-FR-WEBM: the rail lists it under `Summarize <head>`, and its provenance names the head, the base and the mode", async () => {
    await serve(makePrSummaryRun("p1", "working"));

    const row = screen.getByTestId("graduation-row");
    expect(row).toHaveAttribute("aria-label", "Summarize feature/panel");
    expect(within(row).getByText("feature/panel")).toBeInTheDocument();
    expect(within(region()).getByText("Summarize feature/panel")).toBeInTheDocument();
    expect(screen.getByTestId("graduation-provenance")).toHaveTextContent(
      "Summarizes the pull request from “feature/panel” into “main”, from the diff.",
    );
  });

  it("GRU-FR-WEBM: a combine run says it is made from run summaries", async () => {
    await serve(makePrSummaryRun("p1", "queued", { prSummary: { mode: "combine" } }));

    expect(screen.getByTestId("graduation-provenance")).toHaveTextContent(
      "Summarizes the pull request from “feature/panel” into “main”, from run summaries.",
    );
  });

  it("GRU-FR-AQKZ: the stage row holds Queued, Summarizing and Done, and shows no pass and no change set", async () => {
    await serve(
      makePrSummaryRun("p1", "working", {
        checkpoint: { pass: 1, passLimit: 1, changedPaths: ["src/a.ts"] },
      }),
    );

    const progress = screen.getByTestId("run-progress");
    const stages = [...progress.querySelectorAll("[data-stage]")].map((el) =>
      el.getAttribute("data-stage"),
    );
    expect(stages).toEqual(["queued", "working", "done"]);
    const text = (id: string) =>
      progress.querySelector(`[data-stage="${id}"]`)?.textContent ?? "";
    expect(text("queued")).toContain("Queued");
    expect(text("working")).toContain("Summarizing");
    expect(text("done")).toContain("Done");
    expect(region().textContent).not.toMatch(/Review/);
    expect(region().textContent).not.toMatch(/pass \d/);
    expect(region().textContent).not.toMatch(/src\/a\.ts/);
  });
});

describe("what a PR summary run renders as its end (GRU-FR-DQSA)", () => {
  it("GRU-FR-DQSA: a completed run renders its title on one line and its description as Markdown with inert links", async () => {
    await serve(
      makePrSummaryRun("p1", "completed", {
        prSummary: {
          result: {
            title: "Show an empty state in the queue panel",
            description:
              "## Summary\n\nThe panel says **nothing is queued**. See [the issue](https://example.com/1).",
          },
        },
      }),
      true,
    );

    const result = await screen.findByTestId("graduation-pr-summary");
    expect(within(result).getByTestId("graduation-pr-summary-title")).toHaveTextContent(
      "Show an empty state in the queue panel",
    );
    const description = within(result).getByTestId("graduation-pr-summary-description");
    expect(within(description).getByText("nothing is queued").tagName).toBe("STRONG");
    const link = within(description).getByText("the issue");
    expect(link.tagName).toBe("A");
    expect(link).not.toHaveAttribute("href");
    expect(link).toHaveAttribute("aria-disabled");
  });

  it("GRU-FR-DQSA: a failed run states the typed failure and renders no result", async () => {
    await serve(
      makePrSummaryRun("p1", "failed", {
        failure: {
          code: "unreadable_answer",
          message: "The agent's answer is not a title and a description this application can read.",
          retryable: false,
        },
        prSummary: { result: { title: "stale", description: "stale" } },
      }),
      true,
    );

    expect(screen.getByTestId("graduation-failure")).toHaveTextContent(
      "The agent's answer is not a title and a description this application can read.",
    );
    expect(screen.queryByTestId("graduation-pr-summary")).toBeNull();
    // GRD-FR-CCHK: a failed PR summary run holds no queue, so it names no run
    // a discard would remove.
    expect(screen.queryByTestId("graduation-failed-hold")).toBeNull();
  });

  it("GRU-FR-DQSA: a run that is not completed renders no result", async () => {
    await serve(
      makePrSummaryRun("p1", "interrupted", {
        interruption: { reason: "author_pause", at: "2026-09-06T09:20:00Z", detail: "" },
        prSummary: { result: { title: "stale", description: "stale" } },
      }),
    );

    expect(screen.queryByTestId("graduation-pr-summary")).toBeNull();
  });
});

describe("what a PR summary run offers (GRU-FR-LJUY)", () => {
  it("GRU-FR-LJUY: a working run offers Pause and Discard run, and no Open draft", async () => {
    await serve(makePrSummaryRun("p1", "working"));

    expect(within(region()).getByRole("button", { name: /Pause/ })).toBeInTheDocument();
    expect(within(region()).getByRole("button", { name: /Discard run/ })).toBeInTheDocument();
    expect(within(region()).queryByRole("button", { name: /Open draft/ })).toBeNull();
  });

  it("GRU-FR-LJUY: an interrupted run offers Continue", async () => {
    await serve(
      makePrSummaryRun("p1", "interrupted", {
        interruption: { reason: "author_pause", at: "2026-09-06T09:20:00Z", detail: "" },
      }),
    );

    expect(
      within(region()).getByRole("button", { name: /Continue|Resume/ }),
    ).toBeInTheDocument();
  });

  it("GRU-FR-LJUY: a discarded run offers no Restart and no Revert, and nothing applies a result to a pull request", async () => {
    // A record that names a commit still offers no Revert.
    await serve(makePrSummaryRun("p1", "discarded", { commits: ["abc1234"] }), true);

    await waitFor(() => expect(screen.getAllByTestId("graduation-row")).toHaveLength(1));
    expect(screen.queryByRole("button", { name: /Restart/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Revert/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /pull request/i })).toBeNull();
  });
});
