/**
 * The selected run's action row, driven through the whole section
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-DVWY,
 * GRU-FR-KQPE, GRU-FR-ZMHB, GRU-FR-PVXD, GRU-FR-CLFA, GRU-FR-QYEE).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import { makeQueue, makeRun } from "../../test/graduationFixtures";
import type { GraduationQueue } from "../../types";
import { pickSelector } from "../../test/selectors";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

/** What the backend answers with, per command. */
let queue: GraduationQueue;
let refusals: Record<string, string>;

beforeEach(() => {
  queue = makeQueue([makeRun("r1", "working")]);
  refusals = {};
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    if (refusals[command]) throw refusals[command];
    switch (command) {
      case "list_graduation_queue":
        return queue;
      case "load_app_preferences":
        return { graduationRailWidthFraction: 0.2 };
      case "list_work_streams":
        return [];
      default:
        return undefined;
    }
  });
  listened.mockReset();
  listened.mockImplementation(async () => () => {});
  resetAppPreferencesCache();
  forgetEverything();
  forgetEveryDraft();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/** Render the section and wait for its first read of the queue. */
async function open() {
  render(<GraduationRuns />);
  await screen.findByTestId("graduation-section");
  await waitFor(() => expect(commands()).toContain("list_graduation_queue"));
}

/** Open the section and put the rail in **All**, which lists runs that ended. */
async function openAll() {
  await open();
  await pickSelector("Run view", "all");
}

/** The command names the section has invoked, in order. */
function commands(): string[] {
  return invoked.mock.calls.map(([command]) => command as string);
}

/**
 * Hold one command unanswered until the test settles it, so a test can act on
 * the section while the command is still running.
 */
function hold(command: string) {
  const settle = {
    resolve: (_value?: unknown) => {},
    reject: (_reason: unknown) => {},
  };
  const answer = new Promise((resolve, reject) => {
    settle.resolve = resolve;
    settle.reject = reject;
  });
  const base = invoked.getMockImplementation();
  invoked.mockImplementation(((name: string, args?: unknown) =>
    name === command ? answer : base?.(name, args as never)) as NonNullable<typeof base>);
  return settle;
}

/** A browser drops focus from a control that becomes disabled; jsdom does not. */
function dropFocusAsABrowserDoes() {
  // jsdom neither drops focus from a disabled control nor blurs one, so focus
  // passes through a control that can hold it and is let go from there.
  const filter = screen.getByRole("searchbox", { name: "Filter runs" });
  filter.focus();
  filter.blur();
}

describe("the selected run's action row", () => {
  it("GRU-FR-DVWY: an interrupted run that made commits offers Revert, and pressing it reverts", async () => {
    queue = makeQueue([makeRun("r1", "interrupted", { commits: ["c0ffee1"] })]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Continue" })).toBeInTheDocument();
    await userEvent.click(within(region).getByRole("button", { name: "Revert" }));
    await waitFor(() => expect(commands()).toContain("revert_graduation_run"));
  });

  it("GRU-FR-DVWY: a blocked, an awaiting-author, a discarded and a failed run that made commits offer Revert", async () => {
    queue = makeQueue([makeRun("r0", "blocked", { commits: ["c0ffee1"] })]);
    await open();
    let region = await screen.findByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Revert" })).toBeInTheDocument();
    cleanup();

    queue = makeQueue([makeRun("r1", "awaiting_author", { commits: ["c0ffee1"] })]);
    await open();
    region = await screen.findByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Revert" })).toBeInTheDocument();
    cleanup();

    queue = makeQueue([makeRun("r2", "discarded", { commits: ["c0ffee1"] })]);
    await openAll();
    region = await screen.findByRole("region", { name: "The selected run" });
    const revert = within(region).getByRole("button", { name: "Revert" });
    const restart = within(region).getByRole("button", { name: /Restart/ });
    // The actions render in one order whichever of them a run offers.
    expect(revert.compareDocumentPosition(restart) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    cleanup();

    queue = makeQueue([makeRun("r3", "failed", { commits: ["c0ffee1"] })]);
    await openAll();
    region = await screen.findByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Discard run" })).toBeInTheDocument();
    expect(within(region).getByRole("button", { name: "Revert" })).toBeInTheDocument();
  });

  it("GRU-FR-DVWY: a queued run, a run doing agent work and a run that made no commit offer no Revert", async () => {
    queue = makeQueue([makeRun("r1", "queued", { commits: ["c0ffee1"] })]);
    await open();
    let region = await screen.findByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Discard run" })).toBeInTheDocument();
    expect(within(region).queryByRole("button", { name: "Revert" })).toBeNull();
    cleanup();

    queue = makeQueue([makeRun("r2", "working", { commits: ["c0ffee1"] })]);
    await open();
    region = await screen.findByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Pause" })).toBeInTheDocument();
    expect(within(region).queryByRole("button", { name: "Revert" })).toBeNull();
    cleanup();

    queue = makeQueue([makeRun("r2b", "reviewing", { commits: ["c0ffee1"] })]);
    await open();
    region = await screen.findByRole("region", { name: "The selected run" });
    expect(within(region).queryByRole("button", { name: "Revert" })).toBeNull();
    cleanup();

    queue = makeQueue([makeRun("r3", "interrupted", { commits: [] })]);
    await open();
    region = await screen.findByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Continue" })).toBeInTheDocument();
    expect(within(region).queryByRole("button", { name: "Revert" })).toBeNull();
  });

  it("GRD-FR-MDQZ, GRD-FR-BNTC: the Pause tooltip says the run keeps its stream and its slot, and the runs behind it wait", async () => {
    queue = makeQueue([makeRun("r1", "working")]);
    await open();
    const button = await screen.findByRole("button", { name: "Pause “Run r1”" });
    const title = button.getAttribute("title") ?? "";
    expect(title).toMatch(/keeps its work stream and its project slot/);
    expect(title).toMatch(/runs behind it wait/);
    expect(title).not.toMatch(/gives its work stream back/);
  });

  it("GRD-FR-CYIB, GRD-FR-BNTC: the Continue tooltip says the run continues at once, because it still holds its queue", async () => {
    queue = makeQueue([makeRun("r1", "interrupted")]);
    await open();
    const button = await screen.findByRole("button", { name: /^(Continue|Resume) “Run r1”$/ });
    const title = button.getAttribute("title") ?? "";
    expect(title).toMatch(/Continues this run at once/);
    expect(title).toMatch(/still holds its work stream and its project slot/);
    expect(title).not.toMatch(/when that stream is free/);
  });

  it("GRU-FR-CLFA: the discard of a run with runs queued behind it names them and waits for a confirmation", async () => {
    queue = makeQueue([
      makeRun("r0", "completed"),
      makeRun("r1", "awaiting_author"),
      makeRun("r2", "queued"),
      makeRun("r3", "queued", { autoStart: false }),
    ]);
    await open();
    await userEvent.click(await screen.findByRole("button", { name: "Run r1" }));
    const region = await screen.findByRole("region", { name: "The selected run" });
    await userEvent.click(within(region).getByRole("button", { name: "Discard run" }));

    const confirm = await within(region).findByTestId("graduation-removal-confirm");
    expect(confirm).toHaveTextContent("Run r2");
    expect(confirm).toHaveTextContent("Run r3");
    expect(confirm).not.toHaveTextContent("Run r0");
    expect(confirm).toHaveTextContent("Their drafts return to the active drafts, not graduated.");
    expect(commands()).not.toContain("discard_graduation_run");

    await userEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));
    expect(within(region).queryByTestId("graduation-removal-confirm")).toBeNull();
    expect(commands()).not.toContain("discard_graduation_run");

    await userEvent.click(within(region).getByRole("button", { name: "Discard run" }));
    const again = await within(region).findByTestId("graduation-removal-confirm");
    await userEvent.click(within(again).getByRole("button", { name: "Discard run" }));
    await waitFor(() => expect(commands()).toContain("discard_graduation_run"));
  });

  it("GRU-FR-CLFA: a discard with no run queued behind it is invoked at once", async () => {
    queue = makeQueue([makeRun("r1", "awaiting_author"), makeRun("r0", "completed")]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    await userEvent.click(within(region).getByRole("button", { name: "Discard run" }));
    await waitFor(() => expect(commands()).toContain("discard_graduation_run"));
    expect(within(region).queryByTestId("graduation-removal-confirm")).toBeNull();
  });

  it("GRU-FR-CLFA: the revert of an interrupted run with runs queued behind it confirms, and a revert of a completed run does not", async () => {
    queue = makeQueue([
      makeRun("r1", "interrupted", { commits: ["c0ffee1"] }),
      makeRun("r2", "queued"),
    ]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    await userEvent.click(within(region).getByRole("button", { name: "Revert" }));
    const confirm = await within(region).findByTestId("graduation-removal-confirm");
    expect(confirm).toHaveTextContent("Run r2");
    expect(commands()).not.toContain("revert_graduation_run");
    await userEvent.click(within(confirm).getByRole("button", { name: "Revert run" }));
    await waitFor(() => expect(commands()).toContain("revert_graduation_run"));
    cleanup();

    invoked.mockClear();
    queue = makeQueue([
      makeRun("c1", "completed", { commits: ["c0ffee1"] }),
      makeRun("c2", "queued"),
    ]);
    await openAll();
    await userEvent.click(await screen.findByRole("button", { name: "Run c1" }));
    const completed = await screen.findByRole("region", { name: "The selected run" });
    await userEvent.click(within(completed).getByRole("button", { name: "Revert" }));
    await waitFor(() => expect(commands()).toContain("revert_graduation_run"));
    expect(within(completed).queryByTestId("graduation-removal-confirm")).toBeNull();
  });

  it("GRU-FR-CLFA: a run that never held its queue removes nothing, so its discard is not confirmed", async () => {
    queue = makeQueue([makeRun("r1", "queued"), makeRun("r2", "queued")]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    await userEvent.click(within(region).getByRole("button", { name: "Discard run" }));
    await waitFor(() => expect(commands()).toContain("discard_graduation_run"));
    expect(within(region).queryByTestId("graduation-removal-confirm")).toBeNull();
  });

  it("GRU-FR-QYEE: a failed run says that the runs queued behind it wait for its discard", async () => {
    queue = makeQueue([makeRun("r1", "failed"), makeRun("r2", "queued")]);
    await openAll();
    await userEvent.click(await screen.findByRole("button", { name: "Run r1" }));
    const region = await screen.findByRole("region", { name: "The selected run" });
    expect(within(region).getByTestId("graduation-failed-hold")).toHaveTextContent(
      "stay queued until you discard it",
    );
    expect(within(region).queryByRole("button", { name: /Restart/ })).toBeNull();
    cleanup();

    queue = makeQueue([makeRun("r3", "failed")]);
    await openAll();
    const alone = await screen.findByRole("region", { name: "The selected run" });
    expect(within(alone).queryByTestId("graduation-failed-hold")).toBeNull();
  });

  it("GRU-FR-KQPE, GRU-FR-OZAR: a refused action renders beside the action row that issued it, brought into view", async () => {
    const original = Element.prototype.scrollIntoView;
    const scrolled = vi.fn();
    Element.prototype.scrollIntoView = scrolled;
    try {
      queue = makeQueue([makeRun("r1", "interrupted", { commits: ["c0ffee1"] })]);
      refusals = { revert_graduation_run: "run_not_revertable" };
      await open();
      const region = await screen.findByRole("region", { name: "The selected run" });
      await userEvent.click(within(region).getByRole("button", { name: "Revert" }));
      const refusal = await within(region).findByTestId("graduation-action-refusal");
      expect(refusal).toHaveAttribute("role", "alert");
      expect(refusal).toHaveTextContent("nothing to revert");
      // Directly after the row, rather than at the top of the region.
      expect(refusal.previousElementSibling).toHaveClass("graduation__actions");
      // GRU-FR-OZAR: in the pinned foot bar, which is what keeps it in the
      // visible part of the region with no scroll. One alert at most.
      expect(refusal.closest(".graduation__foot")).not.toBeNull();
      expect(within(region).getAllByRole("alert")).toHaveLength(1);
      expect(screen.queryByTestId("graduation-error")).toBeNull();
      expect(scrolled).toHaveBeenCalledWith({ block: "nearest" });
      expect(scrolled.mock.contexts).toContain(refusal);
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it.each([
    ["Pause", "pause_graduation_run", () => makeRun("r1", "working")],
    [/^(Continue|Resume)$/, "continue_graduation_run", () => makeRun("r1", "interrupted")],
    ["Discard run", "discard_graduation_run", () => makeRun("r1", "queued")],
    [
      "Revert",
      "revert_graduation_run",
      () => makeRun("r1", "interrupted", { commits: ["c0ffee1"] }),
    ],
  ])("GRU-FR-KQPE: a refused %s renders beside the action row, as the one alert", async (name, command, run) => {
    queue = makeQueue([run()]);
    refusals = { [command]: "run_state_not_permitted" };
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    await userEvent.click(within(region).getByRole("button", { name }));
    const refusal = await within(region).findByTestId("graduation-action-refusal");
    expect(refusal.previousElementSibling).toHaveClass("graduation__actions");
    expect(screen.queryByTestId("graduation-error")).toBeNull();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
  });

  it("GRU-FR-KQPE / GRU-FR-XQVG: a list-row refusal replaces the row's refusal, so one alert shows", async () => {
    queue = makeQueue([makeRun("r1", "working")]);
    refusals = { pause_graduation_run: "run_state_not_permitted" };
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    await userEvent.click(within(region).getByRole("button", { name: "Pause" }));
    await within(region).findByTestId("graduation-action-refusal");
    await userEvent.click(screen.getByRole("button", { name: "Pause “Run r1”" }));
    expect(await within(region).findByTestId("graduation-error")).toBeInTheDocument();
    expect(within(region).queryByTestId("graduation-action-refusal")).toBeNull();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
  });

  it("GRU-FR-ZMHB / GRU-FR-PVXD: while an act runs only the run's controls are disabled, and lost focus returns to the pressed control", async () => {
    queue = makeQueue([makeRun("r1", "interrupted", { commits: ["c0ffee1"] })]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    const settle = hold("revert_graduation_run");
    await userEvent.click(within(region).getByRole("button", { name: "Revert" }));
    await waitFor(() =>
      expect(within(region).getByRole("button", { name: "Revert" })).toBeDisabled(),
    );
    expect(within(region).getByRole("button", { name: /^(Continue|Resume)$/ })).toBeDisabled();
    expect(within(region).getByRole("button", { name: "Discard run" })).toBeDisabled();
    expect(within(region).getByRole("button", { name: "Open draft" })).toBeEnabled();
    dropFocusAsABrowserDoes();
    expect(document.activeElement).toBe(document.body);
    await act(async () => {
      settle.reject("run_not_revertable");
    });
    await within(region).findByTestId("graduation-action-refusal");
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(region).getByRole("button", { name: "Revert" }),
      ),
    );
  });

  it("GRU-FR-ZMHB: focus the author moved elsewhere while an act runs stays there", async () => {
    queue = makeQueue([makeRun("r1", "interrupted", { commits: ["c0ffee1"] })]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    const settle = hold("revert_graduation_run");
    await userEvent.click(within(region).getByRole("button", { name: "Revert" }));
    const filter = screen.getByRole("searchbox", { name: "Filter runs" });
    filter.focus();
    await act(async () => {
      settle.reject("run_not_revertable");
    });
    await within(region).findByTestId("graduation-action-refusal");
    expect(document.activeElement).toBe(filter);
  });

  it("GRU-FR-PVXD: after a pause settles, focus goes to Resume in Pause's place", async () => {
    queue = makeQueue([makeRun("r1", "working")]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    const settle = hold("pause_graduation_run");
    await userEvent.click(within(region).getByRole("button", { name: "Pause" }));
    dropFocusAsABrowserDoes();
    queue = makeQueue([
      makeRun("r1", "interrupted", {
        interruption: {
          reason: "author_pause",
          detail: "You paused it.",
          at: "2026-09-06T09:20:00Z",
        },
      }),
    ]);
    await act(async () => {
      settle.resolve(undefined);
    });
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(region).getByRole("button", { name: "Resume" }),
      ),
    );
  });

  it("GRU-FR-KQPE / GRU-FR-ZMHB: a refusal that settles after another run is selected neither renders nor moves focus", async () => {
    queue = makeQueue([
      makeRun("r1", "interrupted", { commits: ["c0ffee1"] }),
      makeRun("r2", "interrupted"),
    ]);
    await open();
    await userEvent.click(await screen.findByRole("button", { name: "Run r1" }));
    const region = screen.getByRole("region", { name: "The selected run" });
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
    const settle = hold("revert_graduation_run");
    await userEvent.click(within(region).getByRole("button", { name: "Revert" }));
    const other = screen.getByRole("button", { name: "Run r2" });
    await userEvent.click(other);
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r2"));
    await act(async () => {
      settle.reject("run_not_revertable");
    });
    expect(within(region).queryByTestId("graduation-action-refusal")).toBeNull();
    expect(document.activeElement).toBe(other);
    // Nor does it wait for the author to come back to the run it was about.
    await userEvent.click(screen.getByRole("button", { name: "Run r1" }));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
    expect(within(region).queryByTestId("graduation-action-refusal")).toBeNull();
  });

  it("GRU-FR-ZMHB: where the pressed control is no longer offered, focus goes to the row's first control", async () => {
    queue = makeQueue([makeRun("r1", "interrupted", { commits: [] })]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    // The continue resolves, and the listing read back after it holds a queued
    // run, which offers no Continue.
    queue = makeQueue([makeRun("r1", "queued")]);
    await userEvent.click(within(region).getByRole("button", { name: "Continue" }));
    await waitFor(() =>
      expect(within(region).queryByRole("button", { name: "Continue" })).toBeNull(),
    );
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(region).getByRole("button", { name: "Open draft" }),
      ),
    );
  });
});
