import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { makeRun } from "../../test/graduationFixtures";
import {
  pickSelector,
  selectorNames,
  selectorValue,
  selectorValues,
} from "../../test/selectors";
import type { GraduationRun } from "../../types";
import { GraduationRail, type GraduationRailProps } from "./rail";

afterEach(cleanup);

/** The rail renders from props alone: it reads nothing and invokes nothing. */
function draw(over: Partial<GraduationRailProps> = {}) {
  const runs = over.runs ?? [makeRun("r1", "working")];
  const props: GraduationRailProps = {
    runs,
    listed: over.listed ?? runs,
    selected: over.selected ?? null,
    view: "in-flight",
    onView: vi.fn(),
    filter: "",
    onFilter: vi.fn(),
    busy: false,
    onSelect: vi.fn(),
    positionOf: () => null,
    onPause: vi.fn(),
    onContinue: vi.fn(),
    onAutoStart: vi.fn(),
    onArchive: vi.fn(),
    onRestart: vi.fn(),
    ...over,
  };
  render(<GraduationRail {...props} />);
  return props;
}

/** The row of one run, addressed by the draft it is about. */
function rowOf(name: string): HTMLElement {
  const row = screen
    .getAllByTestId("graduation-row")
    .find((button) => button.getAttribute("aria-label") === name);
  if (!row) throw new Error(`no row for “${name}”`);
  return row.closest(".graduation__row") as HTMLElement;
}

describe("the rail's rows", () => {
  const run = (over = {}) =>
    makeRun("r1", "working", {
      streamName: "editor-work",
      input: { ...makeRun("r1").input, draftName: "editor-scroll" },
      ...over,
    });

  it("GRT-FR-CTNO, GRT-FR-AMHS: a discarded row carries Restart, named for the run with the restart tooltip, and activating it asks for the confirmation", async () => {
    const props = draw({ runs: [run({ state: "discarded" })] });
    const restart = within(rowOf("editor-scroll")).getByRole("button", {
      name: "Restart “editor-scroll”",
    });
    expect(restart).toHaveAttribute(
      "title",
      expect.stringContaining("This run stays discarded and unchanged."),
    );
    await userEvent.click(restart);
    expect(props.onRestart).toHaveBeenCalledWith(expect.objectContaining({ id: "r1" }));
    expect(props.onSelect).not.toHaveBeenCalled();
  });

  it.each(["queued", "working", "interrupted", "completed", "failed"] as const)(
    "GRT-FR-CTNO: a %s row carries no Restart",
    (state) => {
      draw({ runs: [run({ state })] });
      expect(
        within(rowOf("editor-scroll")).queryByRole("button", { name: /^Restart/ }),
      ).toBeNull();
    },
  );

  it("GRT-FR-AMHS: the rail's Restart is disabled while a call runs", () => {
    draw({ runs: [run({ state: "discarded" })], busy: true });
    expect(
      within(rowOf("editor-scroll")).getByRole("button", { name: "Restart “editor-scroll”" }),
    ).toBeDisabled();
  });

  it("GRH-FR-FYTH: the row names the draft, the stream it runs in and its state", () => {
    draw({ runs: [run()] });
    const row = rowOf("editor-scroll");
    expect(within(row).getByText("editor-scroll")).toBeInTheDocument();
    expect(within(row).getByText("editor-work")).toBeInTheDocument();
    expect(within(row).getByText("Working")).toBeInTheDocument();
  });

  it("GRH-FR-FYTH: a run in a queue carries its place in that queue", () => {
    draw({ runs: [run({ state: "queued" })], positionOf: () => "2 ahead" });
    expect(within(rowOf("editor-scroll")).getByText("2 ahead")).toBeInTheDocument();
  });

  it("GRH-FR-ODLT: the row is named by its draft, and activating it selects that run", async () => {
    const props = draw({ runs: [run()] });
    // The state and the stream are about the run rather than what to call it,
    // so a name that swept them in would read back as "editor-scroll Working
    // in editor-work".
    const button = screen.getByRole("button", { name: "editor-scroll" });
    await userEvent.click(button);
    expect(props.onSelect).toHaveBeenCalledWith("r1");
  });

  it("GRH-FR-ODLT: exactly one row is marked as the one the region is rendering", () => {
    const runs = [run(), makeRun("r2", "queued")];
    draw({ runs, listed: runs, selected: "r2" });
    const current = document.querySelectorAll('[aria-current="true"]');
    expect(current).toHaveLength(1);
    expect(current[0].getAttribute("aria-label")).toBe("Run r2");
  });

  it("GRU-FR-NBRO: the stream-holding mark carries none of the meaning on its own", () => {
    draw({ runs: [run()] });
    const row = rowOf("editor-scroll");
    const marker = row.querySelector(".graduation__marker");
    expect(marker).not.toBeNull();
    expect(marker).toHaveAttribute("aria-hidden", "true");
    // The state is the word beside it, which is what actually says so.
    expect(within(row).getByText("Working")).toBeInTheDocument();
  });
});

describe("the rail's two controls", () => {
  it("GRH-FR-WIMY: the view selector offers the four positions, in order, and reports a change", async () => {
    const props = draw();
    expect(selectorValues("Run view")).toEqual([
      "in-flight",
      "completed",
      "archived",
      "all",
    ]);
    // The four words the author reads, in the same order.
    expect(selectorNames("Run view")).toEqual([
      "Runs that have not ended",
      "Runs that completed",
      "Runs the author filed away",
      "Every run this project has made",
    ]);
    expect(selectorValue("Run view")).toBe("in-flight");
    await pickSelector("Run view", "completed");
    expect(props.onView).toHaveBeenCalledWith("completed");
  });

  it("GRH-FR-BDMB: the filter is a named field, and what is typed is reported", async () => {
    const props = draw();
    // The field is controlled by the section, so what a keystroke reports is
    // the text the section is to hold rather than the field's own history.
    const field = screen.getByRole("searchbox", { name: "Filter runs" });
    await userEvent.type(field, "e");
    expect(props.onFilter).toHaveBeenCalledWith("e");
  });

  it("SNV-FR-60: a project that has graduated nothing renders the empty state", () => {
    draw({ runs: [], listed: [] });
    expect(screen.getByTestId("graduation-empty")).toHaveTextContent(
      /Nothing graduating/,
    );
  });

  it("SNV-FR-61: a filter that admits nothing is not the empty state", () => {
    draw({ runs: [makeRun("r1")], listed: [], filter: "zzz" });
    expect(screen.queryByTestId("graduation-empty")).toBeNull();
    expect(screen.getByText("No run matches this filter.")).toBeInTheDocument();
    // Both controls are still there, still holding what the author set: their
    // next move is to change them rather than to start a run.
    expect(screen.getByRole("searchbox", { name: "Filter runs" })).toHaveValue(
      "zzz",
    );
    expect(selectorValues("Run view")).toHaveLength(4);
  });
});

describe("the controls a row carries", () => {
  const named = (id: string, state: GraduationRun["state"], over = {}) =>
    makeRun(id, state, {
      input: { ...makeRun(id).input, draftName: `run ${id}` },
      ...over,
    });

  it("GRU-FR-XQVG: a working run offers Pause and no other arrangement control", () => {
    draw({ runs: [named("r1", "working")] });
    expect(
      screen.getByRole("button", { name: "Pause “run r1”" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Auto-start/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Resume|Continue/ })).toBeNull();
  });

  it("GRU-FR-XQVG: a queued run offers auto-start and no pause", () => {
    draw({ runs: [named("r1", "queued")] });
    expect(screen.queryByRole("button", { name: /^Pause/ })).toBeNull();
    expect(
      screen.getByRole("button", { name: /Auto-start is on for “run r1”/ }),
    ).toBeInTheDocument();
  });

  it("GRU-FR-XQVG: a run the author paused offers Resume, and any other stop offers Continue", () => {
    draw({
      runs: [
        named("r1", "interrupted", {
          interruption: {
            reason: "author_pause",
            detail: "You paused it.",
            streamReleased: true,
            at: "2026-09-06T09:20:00Z",
          },
        }),
        named("r2", "interrupted", {
          interruption: {
            reason: "application_shutdown",
            detail: "The application closed.",
            streamReleased: true,
            at: "2026-09-06T09:20:00Z",
          },
        }),
      ],
    });
    expect(
      screen.getByRole("button", { name: "Resume “run r1”" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Continue “run r2”" }),
    ).toBeInTheDocument();
  });

  it("GRU-FR-NBRO: auto-start states the value it holds in words", async () => {
    const props = draw({ runs: [named("r1", "queued", { autoStart: false })] });
    const control = screen.getByRole("button", {
      name: /Auto-start is off for “run r1”\. Turn it on/,
    });
    await userEvent.click(control);
    expect(props.onAutoStart).toHaveBeenCalledWith(
      expect.objectContaining({ id: "r1" }),
      true,
    );
  });

  it("GRH-FR-DYNU: archiving is a filing act, and the control says so where it is asked", async () => {
    const props = draw({ runs: [named("r1", "completed")] });
    const archive = screen.getByRole("button", { name: "Archive “run r1”" });
    expect(archive.getAttribute("title")).toMatch(/does not stop it/);
    await userEvent.click(archive);
    expect(props.onArchive).toHaveBeenCalledWith(
      expect.objectContaining({ id: "r1" }),
      true,
    );
  });

  it("GRH-FR-DYNU: a filed-away run offers Restore, which files it back", async () => {
    const props = draw({
      runs: [named("r1", "completed", { archived: true })],
      listed: [named("r1", "completed", { archived: true })],
    });
    await userEvent.click(screen.getByRole("button", { name: "Restore “run r1”" }));
    expect(props.onArchive).toHaveBeenCalledWith(
      expect.objectContaining({ id: "r1" }),
      false,
    );
  });

  it("GRU-FR-ZVTC: no control acts again while a call this section started is in flight", () => {
    draw({ runs: [named("r1", "working")], busy: true });
    // Every control of the row except the one that opens the run: what it
    // reports is a selection rather than an act on the backend.
    const controls = within(rowOf("run r1"))
      .getAllByRole("button")
      .filter((button) => button.getAttribute("aria-label") !== "run r1");
    expect(controls.length).toBeGreaterThan(0);
    for (const control of controls) expect(control).toBeDisabled();
  });

  it("GRU-FR-NBRO: each row's controls name the run they act on", () => {
    draw({ runs: [named("r1", "working"), named("r2", "working")] });
    const names = screen
      .getAllByRole("button", { name: /^Pause/ })
      .map((button) => button.getAttribute("aria-label"));
    expect(new Set(names).size).toBe(2);
  });
});
