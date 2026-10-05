import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { logDebug } from "../../logging";
import { makePass } from "../../test/graduationFixtures";
import type { PassRecord } from "../../types/graduationObservability";
import { PassHistory } from "./passes";

vi.mock("../../logging", async (original) => ({
  ...(await original<typeof import("../../logging")>()),
  logDebug: vi.fn(),
}));

// jsdom has no layout and no `scrollIntoView`. The mock records which row the
// history asked the region to bring into view, and with which alignment.
const scrollIntoView = vi.fn();
beforeEach(() => {
  scrollIntoView.mockReset();
  vi.mocked(logDebug).mockClear();
  Element.prototype.scrollIntoView = scrollIntoView;
});
afterEach(() => {
  cleanup();
  delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
});

const revised = (): PassRecord =>
  makePass({
    pass: 1,
    status: "failed",
    task: "Stop the crash when the last tab is closed.",
    verdict: "revise",
    rationale: "The fix leaves the pinned tab on the same path.",
    findings: [
      {
        severity: "major",
        description: "A pinned tab still closes through the removed branch.",
        affectedFiles: ["src/components/TabStrip.tsx"],
        correction: "Route the pinned tab through the same guard.",
      },
    ],
    nextInstruction: "Cover the pinned tab and add a test for it.",
  });

const ready = (): PassRecord =>
  makePass({
    pass: 2,
    status: "passed",
    task: "Cover the pinned tab and add a test for it.",
    verdict: "ready",
    rationale: "Both paths are covered.",
    findings: [],
  });

function draw(passes: PassRecord[], current = passes.length) {
  render(<PassHistory passes={passes} current={current} />);
}

/** The row of one pass, by the heading it carries. */
function rowOf(heading: string): HTMLElement {
  const row = screen
    .getAllByTestId("graduation-pass")
    .find((pass) => pass.textContent?.includes(heading));
  if (!row) throw new Error(`no pass row for “${heading}”`);
  return row;
}

/** The control that opens and closes one pass's account. */
function toggleOf(heading: string): HTMLElement {
  return within(rowOf(heading)).getByRole("button");
}

describe("the pass history", () => {
  it("GRU-FR-YYXN: one row per pass, each with its own description and status", () => {
    draw([revised(), ready()]);
    const rows = screen.getAllByTestId("graduation-pass");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("First pass");
    expect(rows[0]).toHaveTextContent("The review asked for a revision");
    expect(rows[1]).toHaveTextContent("Pass 2");
    expect(rows[1]).toHaveTextContent("The review found the work ready");
  });

  it("GRU-FR-YYXN: the newest pass is the one open, and only one is", () => {
    draw([revised(), ready()]);
    const open = screen
      .getAllByRole("button", { expanded: true })
      .map((button) => button.textContent);
    expect(open).toHaveLength(1);
    expect(open[0]).toMatch(/Pass 2/);
  });

  it("GRU-FR-YYXN: opening a row closes the one that was open, and it closes again", async () => {
    draw([revised(), ready()]);
    await userEvent.click(toggleOf("First pass"));
    expect(toggleOf("First pass")).toHaveAttribute("aria-expanded", "true");
    expect(toggleOf("Pass 2")).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(toggleOf("First pass"));
    expect(toggleOf("First pass")).toHaveAttribute("aria-expanded", "false");
    expect(rowOf("First pass").querySelector(".graduation__account")).toBeNull();
  });

  it("GRU-FR-VDSK: the open pass's row is the direct child its sticky rule selects", async () => {
    draw([revised(), ready()]);
    // The stylesheet pins `.graduation__pass[data-open="true"] >
    // .graduation__pass-row`, so the DOM must keep that exact shape.
    const open = toggleOf("Pass 2");
    expect(open).toHaveClass("graduation__pass-row");
    expect(open.parentElement).toHaveClass("graduation__pass");
    expect(open.parentElement).toHaveAttribute("data-open", "true");
    expect(toggleOf("First pass").parentElement).not.toHaveAttribute("data-open");
    // The row stays the control that closes it.
    await userEvent.click(open);
    expect(open).toHaveAttribute("aria-expanded", "false");
    expect(open.parentElement).not.toHaveAttribute("data-open");
  });

  it("GRU-FR-YYXN: an open row shows the task, the verdict, the rationale, the findings and the next instruction", () => {
    draw([revised()], 1);
    expect(screen.getByText("What this pass was asked to do")).toBeInTheDocument();
    expect(
      screen.getByText("Stop the crash when the last tab is closed."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Why the review asked for a revision"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("The fix leaves the pinned tab on the same path."),
    ).toBeInTheDocument();
    const finding = screen.getByTestId("graduation-finding");
    expect(finding).toHaveTextContent("Major");
    expect(finding).toHaveTextContent(
      "A pinned tab still closes through the removed branch.",
    );
    expect(finding).toHaveTextContent("src/components/TabStrip.tsx");
    expect(finding).toHaveTextContent("Route the pinned tab through the same guard.");
    expect(screen.getByText("What the next turn was told")).toBeInTheDocument();
    expect(
      screen.getByText("Cover the pinned tab and add a test for it."),
    ).toBeInTheDocument();
  });

  it("GRU-FR-YYXN: a review that gave no rationale says so rather than nothing", () => {
    draw([makePass({ status: "passed", verdict: "ready", rationale: null })], 1);
    expect(screen.getByText("The review gave no rationale.")).toBeInTheDocument();
  });

  it("GRU-FR-YYXN: a pass with nothing recorded for its task says so", () => {
    draw([makePass({ task: "" })], 1);
    expect(screen.getByText("Nothing was recorded.")).toBeInTheDocument();
  });

  it("GRU-FR-YYXN: a history of two passes says so, and a row says how many findings it raised", () => {
    draw([revised(), ready()]);
    expect(screen.getByText("2 passes")).toBeInTheDocument();
    expect(within(rowOf("First pass")).getByText("1 finding")).toBeInTheDocument();
  });

  it("GRU-FR-YYXN: a history of one pass says so", () => {
    draw([makePass()]);
    expect(screen.getByText("1 pass")).toBeInTheDocument();
  });

  it("GRU-FR-YYXN: a run that has recorded no pass renders no history", () => {
    draw([]);
    expect(screen.queryByTestId("graduation-passes")).toBeNull();
  });

  it("GRU-FR-YYXN: the pass the run stands at is marked, and on one row alone", () => {
    draw([revised(), ready()], 1);
    const marked = document.querySelectorAll('[data-current="true"]');
    expect(marked).toHaveLength(1);
    expect(marked[0]).toHaveTextContent("First pass");
  });

  it("GRU-FR-ZYDL: the first render scrolls nothing", () => {
    draw([revised(), ready()]);
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it("GRU-FR-ZYDL: closing the open pass brings its row into view", async () => {
    draw([revised(), ready()]);
    await userEvent.click(toggleOf("Pass 2"));
    expect(toggleOf("Pass 2")).toHaveAttribute("aria-expanded", "false");
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    expect(scrollIntoView.mock.contexts[0]).toBe(toggleOf("Pass 2"));
    // "nearest" moves nothing while the row is already in view.
    expect(scrollIntoView).toHaveBeenCalledWith({
      block: "nearest",
      inline: "nearest",
    });
  });

  it("GRU-FR-ZYDL: opening a pass brings the row the author pressed into view", async () => {
    draw([revised(), ready()]);
    await userEvent.click(toggleOf("First pass"));
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    expect(scrollIntoView.mock.contexts[0]).toBe(toggleOf("First pass"));
  });

  it("GRU-FR-ZYDL: every press on the same row brings it into view again", async () => {
    draw([revised(), ready()]);
    for (let press = 0; press < 3; press++) {
      await userEvent.click(toggleOf("First pass"));
    }
    expect(scrollIntoView).toHaveBeenCalledTimes(3);
    for (const context of scrollIntoView.mock.contexts) {
      expect(context).toBe(toggleOf("First pass"));
    }
  });

  it("GRU-FR-ZLWI: opening and closing passes in any order keeps every row rendered", async () => {
    draw([revised(), ready()]);
    const presses = [
      "First pass",
      "Pass 2",
      "First pass",
      "First pass",
      "Pass 2",
      "Pass 2",
    ];
    for (const [index, heading] of presses.entries()) {
      await userEvent.click(toggleOf(heading));
      const rows = screen.getAllByTestId("graduation-pass");
      expect(rows).toHaveLength(2);
      expect(rows[0]).toHaveTextContent("First pass");
      expect(rows[1]).toHaveTextContent("Pass 2");
      for (const row of rows) {
        expect(within(row).getByRole("button")).toBeInTheDocument();
      }
      expect(
        screen.queryAllByRole("button", { expanded: true }).length,
      ).toBeLessThanOrEqual(1);
      expect(scrollIntoView.mock.contexts[index]).toBe(toggleOf(heading));
    }
    // The last press closed the newest pass: no account stays open.
    expect(screen.queryAllByRole("button", { expanded: true })).toHaveLength(0);
  });

  it("GRU-FR-ZLWI: a reload with a new pass list keeps the rows and the open pass, and scrolls nothing", async () => {
    const { rerender } = render(
      <PassHistory passes={[revised(), ready()]} current={2} />,
    );
    await userEvent.click(toggleOf("First pass"));
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    const third = makePass({ pass: 3, status: "working" });
    rerender(<PassHistory passes={[revised(), ready(), third]} current={3} />);
    expect(screen.getAllByTestId("graduation-pass")).toHaveLength(3);
    expect(toggleOf("First pass")).toHaveAttribute("aria-expanded", "true");
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
  });

  it("GRU-FR-ZLWI: a reload that drops the pressed pass keeps the other rows and scrolls nothing", async () => {
    const { rerender } = render(
      <PassHistory passes={[revised(), ready()]} current={2} />,
    );
    await userEvent.click(toggleOf("First pass"));
    rerender(<PassHistory passes={[ready()]} current={2} />);
    expect(screen.getAllByTestId("graduation-pass")).toHaveLength(1);
    expect(toggleOf("Pass 2")).toBeInTheDocument();
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
  });

  it("an open and a close are logged by pass number alone", async () => {
    draw([revised(), ready()]);
    await userEvent.click(toggleOf("First pass"));
    await userEvent.click(toggleOf("First pass"));
    const logged = vi.mocked(logDebug);
    expect(logged).toHaveBeenNthCalledWith(
      1,
      ["frontend"],
      "a pass account was opened",
      { pass: 1 },
    );
    expect(logged).toHaveBeenNthCalledWith(
      2,
      ["frontend"],
      "a pass account was closed",
      { pass: 1 },
    );
    // Agent text never reaches the log.
    const record = JSON.stringify(logged.mock.calls);
    for (const text of [
      "Stop the crash when the last tab is closed.",
      "The fix leaves the pinned tab on the same path.",
      "A pinned tab still closes through the removed branch.",
      "Cover the pinned tab and add a test for it.",
    ]) {
      expect(record).not.toContain(text);
    }
  });

  it("GRU-FR-MRPE: every text a pass carries renders as escaped plain text", () => {
    draw(
      [
        makePass({
          task: "<b>task</b>",
          status: "failed",
          verdict: "revise",
          rationale: "<b>why</b>",
          nextInstruction: "<b>next</b>",
          findings: [
            {
              severity: "minor",
              description: "<b>what</b>",
              affectedFiles: ["<b>path</b>"],
              correction: "<b>fix</b>",
            },
          ],
        }),
      ],
      1,
    );
    const history = screen.getByTestId("graduation-passes");
    expect(history.querySelector("b")).toBeNull();
    expect(history.textContent).toContain("<b>task</b>");
    expect(history.textContent).toContain("<b>fix</b>");
  });
});

describe("the pass of an interrupted run", () => {
  const working = (): PassRecord =>
    makePass({ pass: 2, status: "working", task: "Cover the pinned tab." });

  it("GRU-FR-JAEY: a pass still working reads as stopped, with the cause", () => {
    render(
      <PassHistory
        passes={[revised(), working()]}
        current={2}
        stoppedCause="the turn reached its time limit"
      />,
    );
    expect(rowOf("Pass 2")).toHaveTextContent("Stopped — the turn reached its time limit");
    expect(rowOf("Pass 2")).not.toHaveTextContent("Working");
    // A settled pass keeps the verdict its review gave.
    expect(rowOf("First pass")).toHaveTextContent("The review asked for a revision");
  });

  it("GRU-FR-JAEY: while the run works, the same pass reads as working", () => {
    render(<PassHistory passes={[revised(), working()]} current={2} />);
    expect(rowOf("Pass 2")).toHaveTextContent("Working");
    expect(rowOf("Pass 2")).not.toHaveTextContent("Stopped");
  });
});
