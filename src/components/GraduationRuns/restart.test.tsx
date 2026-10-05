import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import { makeRun } from "../../test/graduationFixtures";
import type { GraduationRun, WorkStreamSummary } from "../../types";
import type { RunAct } from "./actionRow";
import { RestartRegionControl } from "./restart";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

const invoked = vi.mocked(invoke);

/** Two streams the run may be restarted on, one of them its own. */
const streams = (ids: string[]): WorkStreamSummary[] =>
  ids.map((id) => ({
    stream: {
      id,
      name: id,
      projectKey: "/Users/demo/dev/acme",
      branch: `synthesis/stream/${id}`,
      baseBranch: "main",
      baseRevision: "a91bc04",
      worktreePath: `/tmp/${id}`,
      createdAt: "2026-09-06T09:00:00Z",
      isMissing: false,
      busyRunId: occupied.includes(id) ? "g-9" : null,
    },
    // GSD-FR-WQPD: a stream nothing holds still makes a run wait where runs
    // are queued on it, so occupancy is reachable both ways here.
    queuedRunCount: queuedOn.includes(id) ? 2 : 0,
    aheadOfBase: 0,
    behindBase: 0,
    baseTipRevision: "a91bc04",
    missingCommits: [],
  }));

/** GSD-FR-WQPD: which of those streams a run holds. */
let occupied: string[];
/** GSD-FR-WQPD: which of them have runs queued on them instead. */
let queuedOn: string[];

let known: string[];
let refusals: Record<string, string>;

beforeEach(() => {
  known = ["s-1", "s-2"];
  occupied = [];
  queuedOn = [];
  refusals = {};
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    if (refusals[command]) throw refusals[command];
    if (command === "list_work_streams") return streams(known);
    if (command === "restart_graduation_run") return makeRun("r9", "queued");
    return undefined;
  });
});
afterEach(cleanup);

function draw(run: GraduationRun = makeRun("r1", "discarded")) {
  const onRestarted = vi.fn();
  const view = render(
    <RestartRegionControl run={run} busy={false} onRestarted={onRestarted} />,
  );
  return { onRestarted, view };
}

const confirm = () => screen.getByTestId("graduation-restart-confirm");

/**
 * Record what the confirmation brings into view.
 *
 * jsdom implements no scrolling, so what a surface asks for is the only
 * evidence that anything below the fold of the run region is reachable.
 */
function watchScrolling(): HTMLElement[] {
  const seen: HTMLElement[] = [];
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: function scrollIntoView(this: HTMLElement) {
      seen.push(this);
    },
  });
  return seen;
}

describe("restarting a discarded run", () => {
  it("GRU-FR-WDWB: it confirms before it invokes anything", async () => {
    draw();
    expect(screen.queryByTestId("graduation-restart-confirm")).toBeNull();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    expect(await screen.findByTestId("graduation-restart-confirm")).toBeInTheDocument();
    expect(
      invoked.mock.calls.map(([command]) => command),
    ).not.toContain("restart_graduation_run");
  });

  it("GRT-FR-IMRI: the confirmation opens on the stream the discarded run ran on", async () => {
    draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveValue("s-1"),
    );
  });

  it("GRT-FR-IMRI: a run whose own stream is gone still opens on a live one", async () => {
    known = ["s-2", "s-3"];
    draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveValue("s-2"),
    );
    expect(within(confirm()).getAllByRole("option")).toHaveLength(2);
    expect(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    ).toBeEnabled();
  });

  it("GRT-FR-VWHM: a project with no live stream chooses none, and the act is unavailable", async () => {
    known = [];
    draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("option", { name: "Choose a work stream" }),
      ).toBeInTheDocument(),
    );
    expect(
      within(confirm()).getByRole("combobox", { name: "Work stream" }),
    ).toHaveValue("");
    expect(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    ).toBeDisabled();
  });

  it("GRU-FR-WDWB: restarting sends the run and the chosen stream once", async () => {
    const { onRestarted } = draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveValue("s-1"),
    );
    await userEvent.selectOptions(
      within(confirm()).getByRole("combobox", { name: "Work stream" }),
      "s-2",
    );
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    );
    await waitFor(() => expect(onRestarted).toHaveBeenCalled());
    const calls = invoked.mock.calls.filter(
      ([command]) => command === "restart_graduation_run",
    );
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toEqual({
      runId: "r1",
      streamId: "s-2",
      standingWork: "commit",
      standingWorkMessage: null,
    });
    expect(screen.queryByTestId("graduation-restart-confirm")).toBeNull();
  });

  it("GRT-FR-SQNB, GSD-FR-WQPD: a restart onto a free stream is asked nothing about standing work", async () => {
    draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveValue("s-1"),
    );
    // The new run starts as soon as it is made, so what stands in the stream
    // now is what it will find.
    expect(within(confirm()).queryByTestId("standing-work-choice")).toBeNull();
  });

  it("GRT-FR-SQNB, GSU-FR-IRAC: the confirmation asks what the new run does with standing work, and sends its own answer", async () => {
    occupied = ["s-1"];
    const { onRestarted } = draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const choice = () => within(confirm()).getByTestId("standing-work-choice");
    await waitFor(() => expect(choice()).toBeInTheDocument());
    // The same three positions the start dialog asks, resting on the same one.
    expect(
      within(choice())
        .getAllByRole("radio")
        .map((radio) => (radio as HTMLInputElement).value),
    ).toEqual(["keep", "commit", "commit_and_push"]);
    expect(
      within(choice()).getByRole("radio", { name: /^Commit it firstIt is/ }),
    ).toBeChecked();
    // It renders no path set, and reads no working copy to build one from:
    // what stands in the stream when the new run starts is not what stands
    // there now.
    expect(
      invoked.mock.calls.map(([command]) => command),
    ).toEqual(["list_work_streams"]);

    await userEvent.click(
      within(choice()).getByRole("radio", { name: /Leave it uncommitted/ }),
    );
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    );
    await waitFor(() => expect(onRestarted).toHaveBeenCalled());
    expect(
      invoked.mock.calls.filter(
        ([command]) => command === "restart_graduation_run",
      )[0][1],
    ).toEqual({
      runId: "r1",
      streamId: "s-1",
      standingWork: "keep",
      standingWorkMessage: null,
    });
  });

  it("GRT-FR-SQNB, GRT-FR-KSBC: a refused restart leaves both answers standing", async () => {
    occupied = ["s-1"];
    refusals = { restart_graduation_run: "docker_backend_unverified" };
    draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const choice = () => within(confirm()).getByTestId("standing-work-choice");
    await waitFor(() => expect(choice()).toBeInTheDocument());
    await userEvent.click(
      within(choice()).getByRole("radio", { name: /Leave it uncommitted/ }),
    );
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    );

    await waitFor(() =>
      expect(within(confirm()).getByRole("alert")).toBeInTheDocument(),
    );
    // The confirmation is still open on what the author answered, so the
    // retry is theirs to make rather than theirs to re-enter.
    expect(
      within(choice()).getByRole("radio", { name: /Leave it uncommitted/ }),
    ).toBeChecked();
  });

  it("GRT-FR-VWHM: the act is brought into view when the confirmation opens, and again when it grows", async () => {
    occupied = ["s-1"];
    const scrolled = watchScrolling();
    draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    // The confirmation is taller than the run region is at rest, so what has
    // to be reachable is the row the act stands in.
    const actions = () =>
      within(confirm()).getByRole("button", { name: "Restart run" })
        .parentElement as HTMLElement;
    await waitFor(() => expect(scrolled).toContain(actions()));

    // The form grows when the standing-work question changes shape, and the
    // act moves down with it.
    const before = scrolled.length;
    await userEvent.click(
      within(confirm()).getByRole("radio", { name: /Leave it uncommitted/ }),
    );
    expect(scrolled.length).toBeGreaterThan(before);
    expect(scrolled[scrolled.length - 1]).toBe(actions());
  });

  it("GRT-FR-VWHM: the confirmation takes the keyboard when it opens", async () => {
    draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    // The confirmation is taller than the run region is at rest and opens at
    // its foot, so a confirmation nothing brought into view reads as a control
    // that did nothing.
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveFocus(),
    );
  });

  it("GRT-FR-SQNB, GSD-FR-MZTB: the message travels with the restart, and an empty one is none", async () => {
    queuedOn = ["s-1"];
    const { onRestarted } = draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const message = () =>
      within(confirm()).getByRole("textbox", { name: /Commit message/ });
    await waitFor(() => expect(message()).toBeInTheDocument());
    // What an empty message commits under is the run's own name.
    expect(message()).toHaveAttribute("placeholder", "Run r1");

    await userEvent.type(message(), "  Notes from the meeting  ");
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    );
    await waitFor(() => expect(onRestarted).toHaveBeenCalled());
    expect(
      invoked.mock.calls.filter(
        ([command]) => command === "restart_graduation_run",
      )[0][1],
    ).toEqual({
      runId: "r1",
      streamId: "s-1",
      standingWork: "commit",
      standingWorkMessage: "Notes from the meeting",
    });
  });

  it("GRT-FR-SQNB, GSD-FR-MZTB: a message written before a choice that commits nothing is not sent", async () => {
    occupied = ["s-1"];
    const { onRestarted } = draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const choice = () => within(confirm()).getByTestId("standing-work-choice");
    await waitFor(() => expect(choice()).toBeInTheDocument());
    await userEvent.type(
      within(confirm()).getByRole("textbox", { name: /Commit message/ }),
      "Notes from the meeting",
    );
    await userEvent.click(
      within(choice()).getByRole("radio", { name: /Leave it uncommitted/ }),
    );
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    );
    await waitFor(() => expect(onRestarted).toHaveBeenCalled());
    expect(
      invoked.mock.calls.filter(
        ([command]) => command === "restart_graduation_run",
      )[0][1],
    ).toEqual({
      runId: "r1",
      streamId: "s-1",
      standingWork: "keep",
      standingWorkMessage: null,
    });
  });

  it("GRT-FR-SQNB, GSD-FR-WQPD: an answer given for one stream is not sent for a free one", async () => {
    occupied = ["s-1"];
    const { onRestarted } = draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const choice = () => within(confirm()).getByTestId("standing-work-choice");
    await waitFor(() => expect(choice()).toBeInTheDocument());
    await userEvent.type(
      within(confirm()).getByRole("textbox", { name: /Commit message/ }),
      "Notes from the meeting",
    );
    await userEvent.click(
      within(choice()).getByRole("radio", { name: /Leave it uncommitted/ }),
    );

    // The new run now starts at once, so nothing it was told about waiting
    // travels with it.
    await userEvent.selectOptions(
      within(confirm()).getByRole("combobox", { name: "Work stream" }),
      "s-2",
    );
    expect(within(confirm()).queryByTestId("standing-work-choice")).toBeNull();
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    );
    await waitFor(() => expect(onRestarted).toHaveBeenCalled());
    expect(
      invoked.mock.calls.filter(
        ([command]) => command === "restart_graduation_run",
      )[0][1],
    ).toEqual({
      runId: "r1",
      streamId: "s-2",
      standingWork: "commit",
      standingWorkMessage: null,
    });
  });

  it("GRT-FR-SQNB: a confirmation opened again rests at the same position", async () => {
    occupied = ["s-1"];
    draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const choice = () => within(confirm()).getByTestId("standing-work-choice");
    await waitFor(() => expect(choice()).toBeInTheDocument());
    await userEvent.click(
      within(choice()).getByRole("radio", { name: /Leave it uncommitted/ }),
    );
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Cancel" }),
    );
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() => expect(choice()).toBeInTheDocument());
    expect(
      within(choice()).getByRole("radio", { name: /^Commit it firstIt is/ }),
    ).toBeChecked();
  });

  it("GRT-FR-IMRI: a stream the author chose is not put back by a re-read of the run", async () => {
    const run = makeRun("r1", "discarded");
    const { view } = draw(run);
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const picker = () =>
      within(confirm()).getByRole("combobox", { name: "Work stream" });
    await waitFor(() => expect(picker()).toHaveValue("s-1"));
    await userEvent.selectOptions(picker(), "s-2");
    // Every read of the queue hands this control a new object for the same run.
    view.rerender(
      <RestartRegionControl
        run={{ ...run }}
        busy={false}
        onRestarted={vi.fn()}
      />,
    );
    await waitFor(() => expect(picker()).toHaveValue("s-2"));
  });

  it("GRU-FR-ZMHB / GRT-FR-KSBC: inside the section a restart runs as the row's act, and its refusal stays in the confirmation", async () => {
    const onRestarted = vi.fn();
    const rowAct = vi.fn(
      async (
        _what: string,
        action: () => Promise<unknown>,
        _pressed?: unknown,
        onRefused?: (message: string) => void,
      ) => {
        try {
          await action();
          return true;
        } catch {
          onRefused?.("The restart was refused.");
          return false;
        }
      },
    );
    refusals = { restart_graduation_run: "stream_not_found" };
    render(
      <RestartRegionControl
        run={makeRun("r1", "discarded")}
        busy={false}
        onRestarted={onRestarted}
        act={rowAct as RunAct}
      />,
    );
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveValue("s-1"),
    );
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    );
    expect(await within(confirm()).findByRole("alert")).toHaveTextContent(
      "The restart was refused.",
    );
    expect(rowAct).toHaveBeenCalledWith(
      "Restarted the run.",
      expect.any(Function),
      { runId: "r1", action: "restart" },
      expect.any(Function),
    );
    expect(onRestarted).not.toHaveBeenCalled();

    refusals = {};
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    );
    await waitFor(() => expect(onRestarted).toHaveBeenCalled());
  });

  it("GRU-FR-KQPE: another act of the row starting drops the confirmation's refusal, so one alert shows", async () => {
    const run = makeRun("r1", "discarded");
    const rowAct = vi.fn(
      async (
        _what: string,
        action: () => Promise<unknown>,
        _pressed?: unknown,
        onRefused?: (message: string) => void,
      ) => {
        try {
          await action();
          return true;
        } catch {
          onRefused?.("The restart was refused.");
          return false;
        }
      },
    );
    refusals = { restart_graduation_run: "stream_not_found" };
    const view = render(
      <RestartRegionControl run={run} busy={false} onRestarted={vi.fn()} act={rowAct as RunAct} />,
    );
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveValue("s-1"),
    );
    await userEvent.click(within(confirm()).getByRole("button", { name: "Restart run" }));
    expect(await within(confirm()).findByRole("alert")).toBeInTheDocument();

    // Another control of the row, Revert say, starts an act of its own.
    view.rerender(
      <RestartRegionControl run={run} busy={true} onRestarted={vi.fn()} act={rowAct as RunAct} />,
    );
    await waitFor(() => expect(within(confirm()).queryByRole("alert")).toBeNull());
  });

  it("GRU-FR-ZMHB: an accepted restart closes its confirmation before the row's act settles", async () => {
    const onRestarted = vi.fn();
    const rowAct = vi.fn(async (_what: string, action: () => Promise<unknown>) => {
      await action();
      // The act has not settled yet, so the row is still disabled here: the
      // Restart control must already stand again for focus to return to it.
      await waitFor(() =>
        expect(screen.queryByTestId("graduation-restart-confirm")).toBeNull(),
      );
      return true;
    });
    render(
      <RestartRegionControl
        run={makeRun("r1", "discarded")}
        busy={false}
        onRestarted={onRestarted}
        act={rowAct as RunAct}
      />,
    );
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveValue("s-1"),
    );
    await userEvent.click(within(confirm()).getByRole("button", { name: "Restart run" }));
    await waitFor(() => expect(onRestarted).toHaveBeenCalled());
    expect(screen.getByTestId("graduation-restart")).toHaveAttribute("data-action", "restart");
  });

  it("GRU-FR-ZMHB: while an act of the row runs, the confirmation's own acts are disabled", async () => {
    const run = makeRun("r1", "discarded");
    const view = render(
      <RestartRegionControl run={run} busy={false} onRestarted={vi.fn()} />,
    );
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveValue("s-1"),
    );
    view.rerender(
      <RestartRegionControl run={run} busy={true} onRestarted={vi.fn()} />,
    );
    expect(within(confirm()).getByRole("button", { name: "Restart run" })).toBeDisabled();
    expect(within(confirm()).getByRole("button", { name: "Cancel" })).toBeDisabled();
  });

  it("GRT-FR-KSBC: a refusal renders against the run, which is unchanged", async () => {
    refusals = { restart_graduation_run: "stream_not_found" };
    const { onRestarted } = draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveValue("s-1"),
    );
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    );
    expect(await within(confirm()).findByRole("alert")).toBeInTheDocument();
    expect(onRestarted).not.toHaveBeenCalled();
  });

  it("GRT-FR-KSBC: a refusal belongs to the attempt it was raised for", async () => {
    refusals = { restart_graduation_run: "stream_not_found" };
    draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(
        within(confirm()).getByRole("combobox", { name: "Work stream" }),
      ).toHaveValue("s-1"),
    );
    await userEvent.click(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    );
    await within(confirm()).findByRole("alert");
    await userEvent.click(within(confirm()).getByRole("button", { name: "Cancel" }));
    await userEvent.click(screen.getByTestId("graduation-restart"));
    expect(within(confirm()).queryByRole("alert")).toBeNull();
  });

  it("GRT-FR-VWHM: an unreadable stream list renders inline and leaves the act unavailable", async () => {
    refusals = { list_work_streams: "project_not_open" };
    draw();
    await userEvent.click(screen.getByTestId("graduation-restart"));
    expect(await within(confirm()).findByRole("alert")).toBeInTheDocument();
    expect(
      within(confirm()).getByRole("button", { name: "Restart run" }),
    ).toBeDisabled();
  });
});
