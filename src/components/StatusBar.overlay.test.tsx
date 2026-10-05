import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { OVERLAY_VISIBLE_ROWS, StatusBar, type StatusBarProps } from "./StatusBar";
import type { Operation } from "../types";

// STB-status-bar.md — the in-flight operations overlay: ordering, the five-row
// viewport, actionable and plain rows, keyboard access, accessible names, and
// the overlay staying current (STB-FR-11, STB-FR-HJVM, STB-FR-RWPD,
// STB-FR-DNLC, STB-FR-YQFE, STB-FR-LKRN).

afterEach(cleanup);

function operation(sequence: number, overrides: Partial<Operation> = {}): Operation {
  return {
    id: `op-${sequence}`,
    kind: "scan",
    label: `Operation ${sequence}`,
    state: "running",
    sequence,
    ...overrides,
  };
}

/** Operations 1..n, newest first as PRG-FR-02 delivers them. */
function newestFirst(n: number): Operation[] {
  return Array.from({ length: n }, (_, i) => operation(n - i));
}

function props(overrides: Partial<StatusBarProps> = {}): StatusBarProps {
  return {
    onGlobalSettings: vi.fn(),
    onSettings: vi.fn(),
    operations: [],
    overlayOpen: true,
    onOpenOverlay: vi.fn(),
    onCloseOverlay: vi.fn(),
    onActivateOperation: vi.fn(),
    lineEndings: "lf",
    onSelectLineEndings: vi.fn(),
    indentation: null,
    onSelectIndentation: vi.fn(),
    diffTotals: null,
    ...overrides,
  };
}

function rowsOf(): HTMLElement[] {
  return within(screen.getByTestId("in-flight-rows")).getAllByRole("listitem");
}

describe("overlay ordering and the five-row viewport (STB-FR-11, STB-FR-HJVM)", () => {
  it("STB-FR-11: lists concurrent activities newest first, whatever their kind", () => {
    render(
      <StatusBar
        {...props({
          operations: [
            operation(4, { kind: "graduation", label: "Graduating Editor scroll fix…" }),
            operation(3, { kind: "agent", label: "@Helga is thinking about Push button…" }),
            operation(2, { kind: "git", label: "Pushing feature/x" }),
            operation(1, { kind: "something-new", label: "Unknown work" }),
          ],
        })}
      />,
    );
    expect(rowsOf().map((row) => row.textContent)).toEqual([
      "Graduating Editor scroll fix…",
      "@Helga is thinking about Push button…",
      "Pushing feature/x",
      "Unknown work",
    ]);
  });

  it("STB-FR-HJVM: the rows sit in one vertically scrollable list that is five rows tall", () => {
    render(<StatusBar {...props({ operations: newestFirst(8) })} />);
    const rows = screen.getByTestId("in-flight-rows");
    expect(OVERLAY_VISIBLE_ROWS).toBe(5);
    expect(rows).toHaveAttribute("data-visible-rows", "5");
    expect(rows).toHaveAttribute("data-overflowing", "true");
    // Every in-flight operation is a row, in the one list, not only five of them.
    expect(rowsOf()).toHaveLength(8);
    // The title is outside the scrolling list, so it stays put.
    expect(within(rows).queryByText("In flight")).not.toBeInTheDocument();
    // A region that scrolls takes a focus stop so the keyboard can scroll it.
    expect(rows).toHaveAttribute("tabindex", "0");
  });

  it("STB-FR-HJVM: five or fewer rows do not overflow and add no focus stop", () => {
    render(<StatusBar {...props({ operations: newestFirst(5) })} />);
    const rows = screen.getByTestId("in-flight-rows");
    expect(rowsOf()).toHaveLength(5);
    expect(rows).toHaveAttribute("data-overflowing", "false");
    expect(rows).not.toHaveAttribute("tabindex");
  });
});

describe("overlay rows that name a destination (STB-FR-RWPD, STB-FR-DNLC, STB-FR-YQFE)", () => {
  const withDestinations = [
    operation(5, {
      kind: "graduation",
      label: "Graduating Editor scroll fix…",
      activation: { type: "graduation_run", runId: "g-1" },
    }),
    operation(4, {
      kind: "agent",
      label: "@Helga is thinking about Push button…",
      activation: { type: "discussion", discussionId: "d-9" },
    }),
    operation(3, {
      kind: "git",
      label: "Pushing feature/x",
      activation: { type: "git_push", branch: "feature/x" },
    }),
    operation(2, { kind: "git", label: "Fetching remote branches" }),
  ];

  it("STB-FR-YQFE: an actionable row's accessible name states its label and its target", () => {
    render(<StatusBar {...props({ operations: withDestinations })} />);
    const names = within(screen.getByTestId("in-flight-rows"))
      .getAllByRole("button")
      .map((button) => button.getAttribute("aria-label"));
    expect(names).toEqual([
      "Graduating Editor scroll fix… — open graduation run",
      "@Helga is thinking about Push button… — open discussion",
      "Pushing feature/x — open branch feature/x in Git",
    ]);
  });

  it("STB-FR-YQFE: the label is shown as the producer supplied it, with no subject added", () => {
    render(<StatusBar {...props({ operations: withDestinations })} />);
    expect(
      screen.getByText("@Helga is thinking about Push button…"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/d-9/)).not.toBeInTheDocument();
  });

  it("STB-FR-YQFE: an actionable row keeps its progressbar semantics", () => {
    render(<StatusBar {...props({ operations: withDestinations.slice(0, 1) })} />);
    const overlay = screen.getByTestId("in-flight-overlay");
    expect(within(overlay).getByRole("progressbar")).toBeInTheDocument();
  });

  it("STB-FR-RWPD: activating an actionable row closes the overlay and hands its operation over", async () => {
    const p = props({ operations: withDestinations });
    const calls: string[] = [];
    (p.onCloseOverlay as ReturnType<typeof vi.fn>).mockImplementation(() =>
      calls.push("close"),
    );
    (p.onActivateOperation as ReturnType<typeof vi.fn>).mockImplementation(
      (op: Operation) => calls.push(`activate:${op.id}`),
    );
    render(<StatusBar {...p} />);
    await userEvent.click(
      screen.getByRole("button", { name: /Pushing feature\/x — open branch/ }),
    );
    expect(calls).toEqual(["close", "activate:op-3"]);
  });

  it("STB-FR-YQFE: an actionable row is reached by Tab and operated with Enter and Space", async () => {
    const p = props({ operations: withDestinations });
    render(<StatusBar {...p} />);
    const buttons = within(screen.getByTestId("in-flight-rows")).getAllByRole("button");
    buttons[0].focus();
    await userEvent.keyboard("{Enter}");
    expect(p.onActivateOperation).toHaveBeenLastCalledWith(withDestinations[0]);

    await userEvent.tab();
    expect(buttons[1]).toHaveFocus();
    await userEvent.keyboard(" ");
    expect(p.onActivateOperation).toHaveBeenLastCalledWith(withDestinations[1]);
    expect(p.onActivateOperation).toHaveBeenCalledTimes(2);
  });

  it("STB-FR-DNLC: a row with no destination is visible, plain text, and opens nothing", async () => {
    const p = props({ operations: withDestinations });
    render(<StatusBar {...p} />);
    const plain = rowsOf().find((row) => row.textContent === "Fetching remote branches")!;
    expect(plain).toBeDefined();
    expect(plain).toHaveAttribute("data-actionable", "false");
    // Not a button, not a disabled button, not a link, and no focus stop.
    expect(within(plain).queryByRole("button")).not.toBeInTheDocument();
    expect(within(plain).queryByRole("link")).not.toBeInTheDocument();
    expect(plain.querySelector("[disabled], [aria-disabled], [tabindex]")).toBeNull();
    await userEvent.click(plain);
    await userEvent.click(within(plain).getByText("Fetching remote branches"));
    expect(p.onActivateOperation).not.toHaveBeenCalled();
    expect(p.onCloseOverlay).not.toHaveBeenCalled();
  });

  it("STB-FR-DNLC: no destination is inferred from the kind or the label", () => {
    // A label and kind that read like a graduation run, a discussion and a push
    // make no row actionable when the producer supplied no destination.
    render(
      <StatusBar
        {...props({
          operations: [
            operation(3, { kind: "graduation", label: "Graduating run g-1" }),
            operation(2, { kind: "agent", label: "@Helga is thinking about Push button…" }),
            operation(1, { kind: "git", label: "Pushing feature/x" }),
          ],
        })}
      />,
    );
    expect(
      within(screen.getByTestId("in-flight-rows")).queryAllByRole("button"),
    ).toHaveLength(0);
    expect(rowsOf()).toHaveLength(3);
  });

  it("STB-FR-RWPD: a destination type this build does not know leaves its row plain and visible", async () => {
    const p = props({
      operations: [
        operation(1, {
          label: "From a newer producer",
          activation: { type: "elsewhere", target: "x" } as never,
        }),
      ],
    });
    render(<StatusBar {...p} />);
    expect(
      within(screen.getByTestId("in-flight-rows")).getByText("From a newer producer"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByTestId("in-flight-rows")).queryByRole("button"),
    ).not.toBeInTheDocument();
  });

  it("STB-FR-RWPD: a destination missing its target identity is not actionable", () => {
    render(
      <StatusBar
        {...props({
          operations: [
            operation(1, {
              label: "Half a destination",
              activation: { type: "graduation_run", runId: "" },
            }),
          ],
        })}
      />,
    );
    expect(
      within(screen.getByTestId("in-flight-rows")).queryByRole("button"),
    ).not.toBeInTheDocument();
  });
});

describe("overlay stays current (STB-FR-LKRN)", () => {
  it("STB-FR-LKRN: rows appear, update in place and leave as operations start, advance and end", () => {
    const first = [operation(1, { label: "Indexing project…" })];
    const { rerender } = render(<StatusBar {...props({ operations: first })} />);
    expect(rowsOf()).toHaveLength(1);

    // A newer operation starts: it becomes the first row; the older one stays below.
    const two = [operation(2, { label: "Installing plugin…" }), ...first];
    rerender(<StatusBar {...props({ operations: two })} />);
    expect(rowsOf().map((row) => row.textContent)).toEqual([
      "Installing plugin…",
      "Indexing project…",
    ]);

    // An update changes its row in place and does not move it.
    const updated = [
      operation(2, { label: "Installing plugin…", completed: 3, total: 4 }),
      operation(1, { label: "Indexing project…" }),
    ];
    rerender(<StatusBar {...props({ operations: updated })} />);
    const tracks = within(screen.getByTestId("in-flight-rows")).getAllByTestId("progress-track");
    expect(tracks[0]).toHaveAttribute("aria-valuenow", "3");
    expect(tracks[1]).toHaveAttribute("data-determinate", "false");
    expect(rowsOf().map((row) => row.textContent)).toEqual([
      "Installing plugin…",
      "Indexing project…",
    ]);

    // An operation ends: nothing completed stays in the list.
    rerender(<StatusBar {...props({ operations: [updated[1]] })} />);
    expect(rowsOf().map((row) => row.textContent)).toEqual(["Indexing project…"]);
    expect(screen.queryByText("Installing plugin…")).not.toBeInTheDocument();
  });

  it("STB-FR-LKRN: the list keeps its scroll position while a row above it updates", () => {
    const many = newestFirst(8);
    const { rerender } = render(<StatusBar {...props({ operations: many })} />);
    const list = screen.getByTestId("in-flight-rows");
    list.scrollTop = 40;
    rerender(
      <StatusBar
        {...props({
          operations: [{ ...many[0], completed: 1, total: 2 }, ...many.slice(1)],
        })}
      />,
    );
    // The same element, not a replacement, so the browser keeps its position.
    expect(screen.getByTestId("in-flight-rows")).toBe(list);
    expect(list.scrollTop).toBe(40);
  });
});
