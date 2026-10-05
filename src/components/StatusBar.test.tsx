import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { StatusBar, type StatusBarProps } from "./StatusBar";
import type { Operation } from "../types";

// STB-status-bar.md — the strip's three regions in isolation. The App-level
// suite covers the wiring (which backend operations run, and when); this one
// covers what the regions render for a given set of inputs.

afterEach(cleanup);

function operation(overrides: Partial<Operation> = {}): Operation {
  return {
    id: "op-1",
    kind: "scan",
    label: "Indexing project…",
    state: "running",
    sequence: 1,
    ...overrides,
  };
}

function renderBar(overrides: Partial<StatusBarProps> = {}) {
  const props: StatusBarProps = {
    onGlobalSettings: vi.fn(),
    onSettings: vi.fn(),
    operations: [],
    overlayOpen: false,
    onOpenOverlay: vi.fn(),
    onCloseOverlay: vi.fn(),
    onActivateOperation: vi.fn(),
    lineEndings: "lf",
    onSelectLineEndings: vi.fn(),
    indentation: { kind: "spaces", width: 2 },
    onSelectIndentation: vi.fn(),
    diffTotals: { addedLines: 412, removedLines: 87, fileCount: 5 },
    ...overrides,
  };
  return { props, ...render(<StatusBar {...props} />) };
}

describe("leading region (STB-FR-04)", () => {
  it("hosts both settings buttons and activates the matching tab", async () => {
    const { props } = renderBar();
    const leading = screen.getByTestId("status-bar").firstElementChild!;

    await userEvent.click(
      within(leading as HTMLElement).getByRole("button", {
        name: "Global settings",
      }),
    );
    expect(props.onGlobalSettings).toHaveBeenCalledTimes(1);

    await userEvent.click(
      within(leading as HTMLElement).getByRole("button", {
        name: "Project settings",
      }),
    );
    expect(props.onSettings).toHaveBeenCalledTimes(1);
  });
});

describe("centre region — progress (STB-FR-05–STB-FR-09)", () => {
  it("displays the most recently started of the operations in flight", () => {
    // STB-FR-05, STB-FR-06: a scan is running and an install starts; the install shows.
    // The list arrives most-recently-started first (PRG-FR-02).
    renderBar({
      operations: [
        operation({ id: "op-2", kind: "install", label: "Installing plugin…", sequence: 2 }),
        operation({ id: "op-1", label: "Indexing project…", sequence: 1 }),
      ],
    });
    expect(screen.getByText("Installing plugin…")).toBeInTheDocument();
    expect(screen.queryByText("Indexing project…")).not.toBeInTheDocument();
  });

  it("falls back to the next in-flight operation when the displayed one ends", () => {
    // STB-FR-05, STB-FR-06, second half: the install terminates while the scan runs on.
    const { rerender } = renderBar({
      operations: [
        operation({ id: "op-2", label: "Installing plugin…", sequence: 2 }),
        operation({ id: "op-1", label: "Indexing project…", sequence: 1 }),
      ],
    });
    rerender(
      <StatusBar
        {...{
          onGlobalSettings: vi.fn(),
          onSettings: vi.fn(),
          operations: [operation({ id: "op-1", label: "Indexing project…", sequence: 1 })],
          overlayOpen: false,
          onOpenOverlay: vi.fn(),
          onCloseOverlay: vi.fn(),
          onActivateOperation: vi.fn(),
          lineEndings: "lf" as const,
          onSelectLineEndings: vi.fn(),
          indentation: null,
          onSelectIndentation: vi.fn(),
          diffTotals: null,
        }}
      />,
    );
    expect(screen.getByText("Indexing project…")).toBeInTheDocument();
  });

  it("renders nothing at all and is not a click target while idle", async () => {
    // STB-FR-10 / STB-FR-07: no bar, no track, no label, no placeholder — and
    // clicking where the bar was opens nothing.
    const { props } = renderBar({ operations: [] });
    expect(screen.queryByTestId("status-bar-progress")).not.toBeInTheDocument();
    expect(screen.queryByTestId("progress-track")).not.toBeInTheDocument();

    const centre = screen.getByTestId("status-bar-center");
    expect(centre).toBeEmptyDOMElement();
    await userEvent.click(centre);
    expect(props.onOpenOverlay).not.toHaveBeenCalled();
  });

  it("renders a determinate bar for an operation reporting a total", () => {
    // STB-FR-08, STB-FR-09: 340 of 500 -> a bar filled to that proportion, label beside it.
    renderBar({
      operations: [operation({ completed: 340, total: 500, label: "Pushing…" })],
    });
    const track = screen.getByTestId("progress-track");
    expect(track).toHaveAttribute("data-determinate", "true");
    expect(track).toHaveAttribute("aria-valuenow", "340");
    expect(track).toHaveAttribute("aria-valuemax", "500");
    expect(track.firstElementChild).toHaveStyle({ width: "68%" });
    expect(screen.getByText("Pushing…")).toBeInTheDocument();
  });

  it("renders an indeterminate busy state for an operation with no total", () => {
    renderBar({ operations: [operation({ label: "Installing…" })] });
    const track = screen.getByTestId("progress-track");
    expect(track).toHaveAttribute("data-determinate", "false");
    expect(track).not.toHaveAttribute("aria-valuenow");
    expect(screen.getByText("Installing…")).toBeInTheDocument();
  });

  it("switches an indeterminate bar to determinate in place", async () => {
    // STB-FR-08: the SAME bar switches rendering rather than being replaced by
    // a new one. Node identity is what "in place" means here.
    const { rerender, props } = renderBar({
      operations: [operation({ id: "op-9", label: "Indexing…" })],
    });
    const before = screen.getByTestId("progress-track");
    expect(before).toHaveAttribute("data-determinate", "false");
    // Node identity alone is a weak signal — React reconciles by position, and
    // the track sits at the same position either way. Stamping the live node is
    // what makes a remount detectable: a replaced element loses the marker,
    // which is exactly what restarting the animation would look like.
    (before as HTMLElement & { __marker?: symbol }).__marker = Symbol("live");
    const marker = (before as HTMLElement & { __marker?: symbol }).__marker;

    rerender(
      <StatusBar
        {...props}
        operations={[
          operation({ id: "op-9", label: "Indexing…", completed: 0, total: 900 }),
        ]}
      />,
    );
    const after = screen.getByTestId("progress-track");
    expect((after as HTMLElement & { __marker?: symbol }).__marker).toBe(marker);
    expect(after).toHaveAttribute("data-determinate", "true");
  });

  it("renders an unrecognised kind with the same generic treatment", () => {
    // STB-FR-14, STB-FR-07 / PRG-FR-12: label and progress alone, no branch on `kind`.
    renderBar({
      operations: [
        operation({ kind: "something-nobody-has-seen", label: "Doing a thing…" }),
      ],
    });
    expect(screen.getByText("Doing a thing…")).toBeInTheDocument();
    expect(screen.getByTestId("progress-track")).toBeInTheDocument();
  });
});

describe("in-flight operations overlay (STB-FR-10–STB-FR-14)", () => {
  const three = [
    operation({ id: "op-3", label: "Installing plugin…", sequence: 3 }),
    operation({ id: "op-2", label: "Pushing feature/x…", sequence: 2, completed: 5, total: 10 }),
    operation({ id: "op-1", label: "Indexing project…", sequence: 1 }),
  ];

  it("opens on a click and lists every in-flight operation, displayed one first", async () => {
    // STB-FR-10, STB-FR-11.
    const { props, rerender } = renderBar({ operations: three });
    await userEvent.click(screen.getByTestId("status-bar-progress"));
    expect(props.onOpenOverlay).toHaveBeenCalledTimes(1);

    rerender(<StatusBar {...props} operations={three} overlayOpen />);
    const overlay = screen.getByTestId("in-flight-overlay");
    const rows = within(overlay).getAllByTestId("progress-track");
    expect(rows).toHaveLength(3);
    // The first row is the operation the centre region displays.
    expect(within(overlay).getAllByTitle(/…/)[0]).toHaveTextContent(
      "Installing plugin…",
    );
  });

  it("is read-only — no cancel, stop, retry or dismiss control", async () => {
    // STB-FR-13: cancelling belongs to the surface that owns the
    // operation (RUN-FR-05), not here.
    renderBar({ operations: three, overlayOpen: true });
    const overlay = screen.getByTestId("in-flight-overlay");
    // Guard against a vacuous pass: an empty overlay has no controls either.
    expect(within(overlay).getAllByTestId("progress-track")).toHaveLength(3);
    expect(within(overlay).queryAllByRole("button")).toHaveLength(0);
    for (const forbidden of [/cancel/i, /stop/i, /retry/i, /dismiss/i]) {
      expect(within(overlay).queryByText(forbidden)).not.toBeInTheDocument();
    }
  });

  it("closes on Escape, on an outside pointer-down, and on a second click", async () => {
    // STB-FR-07 / STB-FR-14.
    const { props } = renderBar({ operations: three, overlayOpen: true });

    await userEvent.keyboard("{Escape}");
    expect(props.onCloseOverlay).toHaveBeenCalled();

    (props.onCloseOverlay as ReturnType<typeof vi.fn>).mockClear();
    await userEvent.click(document.body);
    expect(props.onCloseOverlay).toHaveBeenCalled();

    (props.onCloseOverlay as ReturnType<typeof vi.fn>).mockClear();
    await userEvent.click(screen.getByTestId("status-bar-progress"));
    expect(props.onCloseOverlay).toHaveBeenCalled();
    expect(props.onOpenOverlay).not.toHaveBeenCalled();
  });

  it("closes on its own when the last in-flight operation terminates", () => {
    // STB-FR-14, STB-FR-07, third clause: the region it is anchored to stops rendering
    // (STB-FR-07), so the overlay cannot outlive it.
    const { props } = renderBar({ operations: [], overlayOpen: true });
    expect(screen.queryByTestId("in-flight-overlay")).not.toBeInTheDocument();
    expect(props.onCloseOverlay).toHaveBeenCalled();
  });
});

describe("trailing region — editing conventions (STB-FR-16–STB-FR-24)", () => {
  it("shows the project's line-ending convention and reports a selection", async () => {
    // STB-FR-16, STB-FR-17.
    const { props } = renderBar({ lineEndings: "lf" });
    const select = screen.getByTestId("line-ending-select");
    expect(select).toHaveValue("lf");

    await userEvent.selectOptions(select, "crlf");
    expect(props.onSelectLineEndings).toHaveBeenCalledWith("crlf");
  });

  it("keeps the line-ending control enabled whatever the active tab is", () => {
    // STB-FR-22 / STB-FR-20: the convention is a property of the project, not
    // of a tab, so a Dashboard tab (no artifact -> no indentation) leaves it
    // alone.
    renderBar({ indentation: null });
    expect(screen.getByTestId("line-ending-select")).toBeEnabled();
    // …while the indentation control is in its neutral disabled state naming no
    // convention (STB-FR-22).
    const inert = screen.getByTestId("indentation-inert");
    expect(inert).toHaveAttribute("aria-disabled", "true");
    expect(inert).not.toHaveTextContent(/tabs|spaces/i);
    expect(screen.queryByTestId("indentation-select")).not.toBeInTheDocument();
  });

  it("describes the active artifact's indentation and reports an override", async () => {
    // STB-FR-21, STB-FR-22, STB-FR-23, DFV-FR-42, DFV-FR-54, first half.
    const { props } = renderBar({ indentation: { kind: "spaces", width: 2 } });
    const select = screen.getByTestId("indentation-select");
    expect(select).toHaveValue("spaces-2");
    expect(screen.getByRole("option", { name: "Spaces: 2" })).toBeInTheDocument();

    await userEvent.selectOptions(select, "tabs");
    expect(props.onSelectIndentation).toHaveBeenCalledWith({ kind: "tabs" });
  });

  it("describes a detected width that is not one of the offered choices", () => {
    // STB-FR-21 / EDT-FR-37: detection reports the artifact's real unit, which
    // need not be 2, 4, or 8 — a CommonMark ordered list whose continuation
    // lines align under "1. " is indented three spaces. A controlled <select>
    // whose value matches no option renders blank, so the control would stop
    // describing the artifact at all.
    renderBar({ indentation: { kind: "spaces", width: 3 } });
    const select = screen.getByTestId("indentation-select") as HTMLSelectElement;
    expect(select).toHaveValue("spaces-3");
    expect(screen.getByRole("option", { name: "Spaces: 3" })).toBeInTheDocument();
    expect(select.selectedIndex).toBeGreaterThanOrEqual(0);
    // …and it is offered in width order alongside the standard choices.
    expect(
      Array.from(select.options).map((o) => o.textContent),
    ).toEqual(["Tabs", "Spaces: 2", "Spaces: 3", "Spaces: 4", "Spaces: 8"]);
  });

  it("offers only the standard widths when the detected one is among them", () => {
    renderBar({ indentation: { kind: "spaces", width: 4 } });
    const select = screen.getByTestId("indentation-select") as HTMLSelectElement;
    expect(
      Array.from(select.options).map((o) => o.textContent),
    ).toEqual(["Tabs", "Spaces: 2", "Spaces: 4", "Spaces: 8"]);
  });

  it("orders the trailing controls line endings, indentation, then diffstat", () => {
    // STB-FR-03 / wireframe layout note: the diffstat sits closest to the
    // trailing edge.
    renderBar();
    const region = screen.getByTestId("status-bar").lastElementChild!;
    const order = Array.from(region.children).map((el) =>
      el.getAttribute("data-testid"),
    );
    expect(order).toEqual([
      "line-ending-select",
      "indentation-select",
      "status-bar-diffstat",
    ]);
  });
});

describe("trailing region — diff summary (STB-FR-25–STB-FR-29)", () => {
  it("renders the added and removed totals", () => {
    // STB-FR-25.
    renderBar({ diffTotals: { addedLines: 412, removedLines: 87, fileCount: 5 } });
    const diffstat = screen.getByTestId("status-bar-diffstat");
    expect(diffstat).toHaveTextContent("+412");
    expect(diffstat).toHaveTextContent("−87");
  });

  it("renders zeros as a real answer", () => {
    // STB-FR-30's end state: a clean worktree reports zero added and zero
    // removed rather than disappearing.
    renderBar({ diffTotals: { addedLines: 0, removedLines: 0, fileCount: 0 } });
    const diffstat = screen.getByTestId("status-bar-diffstat");
    expect(diffstat).toHaveTextContent("+0");
    expect(diffstat).toHaveTextContent("−0");
  });

  it("renders nothing in its place outside a Git repository", () => {
    // STB-FR-29: no zeros, no error message, and the rest of the
    // strip is unaffected.
    renderBar({ diffTotals: null });
    expect(screen.queryByTestId("status-bar-diffstat")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Global settings" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Project settings" })).toBeInTheDocument();
    expect(screen.getByTestId("line-ending-select")).toBeInTheDocument();
    expect(screen.getByTestId("indentation-select")).toBeInTheDocument();
  });
});
