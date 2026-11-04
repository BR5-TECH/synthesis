import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";

import { TocControl, matchesHeading } from "./TocControl";
import type { OutlineEntry } from "./outline";
import type { DocumentOutline } from "./useDocumentOutline";

afterEach(cleanup);

/** An outline whose list a test sets, and whose reads the test counts. */
function fakeOutline(initial: OutlineEntry[]) {
  let entries = initial;
  const listeners = new Set<() => void>();
  const outline = {
    subscribe: vi.fn((listener: () => void) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    }),
    getSnapshot: vi.fn(() => entries),
    reveal: vi.fn(),
  } satisfies DocumentOutline;
  const set = (next: OutlineEntry[]) => {
    entries = next;
    act(() => listeners.forEach((l) => l()));
  };
  return { outline, set, listeners };
}

const entry = (level: OutlineEntry["level"], text: string, pos: number): OutlineEntry => ({
  level,
  text,
  target: { kind: "rich", pos },
});

const trigger = () => screen.getByRole("button", { name: "Table of contents" });
const list = () => screen.getByRole("listbox", { name: "Table of contents" });
const options = () => within(list()).queryAllByRole("option");
const filter = () => screen.getByTestId("toc-filter");

describe("TocControl", () => {
  it("EDT-FR-KJLQ: a closed panel reads nothing, and closing it ends the subscription", () => {
    const { outline, listeners } = fakeOutline([entry(1, "One", 0)]);
    render(<TocControl outline={outline} />);
    expect(outline.getSnapshot).not.toHaveBeenCalled();
    expect(outline.subscribe).not.toHaveBeenCalled();

    fireEvent.click(trigger());
    expect(outline.getSnapshot).toHaveBeenCalled();
    expect(listeners.size).toBe(1);

    fireEvent.click(trigger());
    expect(listeners.size).toBe(0);
  });

  it("EDT-FR-KJLQ, EDT-FR-PNGU: the open list follows the outline, with the full text in each tooltip", () => {
    const { outline, set } = fakeOutline([entry(1, "One", 0)]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    expect(options().map((o) => o.textContent)).toEqual(["One"]);

    const long = "A heading that is far too long to show in full in the panel";
    set([entry(1, "One", 0), entry(3, long, 10)]);
    expect(options()).toHaveLength(2);
    expect(options()[1]).toHaveAttribute("title", long);
  });

  it("EDT-FR-AQCR, EDT-FR-EKCY: each entry carries its level, and its accessible name gives the level", () => {
    const { outline } = fakeOutline([entry(1, "One", 0), entry(4, "Four", 5), entry(6, "Six", 9)]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    expect(options().map((o) => o.getAttribute("data-level"))).toEqual(["1", "4", "6"]);
    expect(screen.getByRole("option", { name: "Four, heading level 4" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Six, heading level 6" })).toBeInTheDocument();
  });

  it("EDT-FR-QWZN, EDT-FR-YSJX: a heading with no text is not listed, and the entries after it still reveal their own heading", () => {
    const after = entry(3, "After", 9);
    const { outline } = fakeOutline([entry(1, "Before", 0), entry(2, "", 5), after]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    expect(options().map((o) => o.textContent)).toEqual(["Before", "After"]);
    fireEvent.click(screen.getByRole("option", { name: "After, heading level 3" }));
    expect(outline.reveal).toHaveBeenCalledWith(after);
  });

  it("EDT-FR-QWZN, EDT-FR-KJLQ, EDT-FR-EBNU: the open list follows a heading that gains or loses its text", () => {
    const { outline, set } = fakeOutline([entry(2, "", 0), entry(2, " ", 4)]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    expect(options()).toHaveLength(0);
    expect(screen.getByTestId("toc-menu")).toHaveClass("toc__panel--empty");

    set([entry(2, "", 0), entry(2, "X", 4)]);
    expect(options().map((o) => o.textContent)).toEqual(["X"]);
    expect(screen.queryByText("No headings")).toBeNull();
    expect(screen.getByTestId("toc-menu")).not.toHaveClass("toc__panel--empty");

    set([entry(2, "", 0), entry(2, "", 4)]);
    expect(screen.getByText("No headings")).toBeInTheDocument();
    expect(screen.getByTestId("toc-menu")).toHaveClass("toc__panel--empty");
  });

  it("EDT-FR-QWZN, EDT-FR-EBNU: a document whose headings all have no text shows \"No headings\"", () => {
    const { outline } = fakeOutline([entry(2, "", 0), entry(3, "", 4)]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    expect(screen.queryAllByRole("option")).toHaveLength(0);
    expect(screen.getByText("No headings")).toBeInTheDocument();
    expect(screen.getByTestId("toc-menu")).toHaveClass("toc__panel--empty");
  });

  it("EDT-FR-YSJX: one click on an entry closes the panel and reveals that entry", () => {
    const first = entry(1, "One", 0);
    const second = entry(2, "Two", 5);
    const { outline } = fakeOutline([first, second]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    fireEvent.click(screen.getByRole("option", { name: /^Two/ }));
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(outline.reveal).toHaveBeenCalledTimes(1);
    expect(outline.reveal).toHaveBeenCalledWith(second);
  });

  it("EDT-FR-YSJX: a press on an entry keeps focus in the filter box, so nothing moves under the pointer before the click", () => {
    const { outline } = fakeOutline([entry(1, "One", 0), entry(2, "Two", 5)]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    expect(filter()).toHaveFocus();
    const pressed = fireEvent.mouseDown(screen.getByRole("option", { name: /^Two/ }));
    expect(pressed).toBe(false);
    expect(filter()).toHaveFocus();
  });

  it("EDT-FR-YSJX: a selection does not give focus back to the control, which the editing surface takes", () => {
    const { outline } = fakeOutline([entry(1, "One", 0)]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    fireEvent.click(screen.getByRole("option", { name: /^One/ }));
    expect(trigger()).not.toHaveFocus();
  });

  it("EDT-FR-IZHD: the filter keeps the headings whose text holds it, with no regard to case", () => {
    const { outline } = fakeOutline([
      entry(1, "Library panel", 0),
      entry(2, "Filtering", 5),
      entry(3, "Persistence of filters", 9),
    ]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    fireEvent.change(filter(), { target: { value: "FILTER" } });
    expect(options().map((o) => o.textContent)).toEqual(["Filtering", "Persistence of filters"]);
    expect(screen.getByTestId("toc-count")).toHaveTextContent("2 of 3 shown");

    // The id of an entry is its index, which the filter never matches.
    fireEvent.change(filter(), { target: { value: "2" } });
    expect(options()).toHaveLength(0);

    fireEvent.change(filter(), { target: { value: "" } });
    expect(options()).toHaveLength(3);
  });

  it("EDT-FR-IZHD, EDT-FR-AQCR: Enter selects the top entry of the filtered list", () => {
    const target = entry(2, "Filtering", 5);
    const { outline } = fakeOutline([entry(1, "Library panel", 0), target]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    fireEvent.change(filter(), { target: { value: "filt" } });
    fireEvent.keyDown(filter(), { key: "Enter" });
    expect(outline.reveal).toHaveBeenCalledWith(target);
  });

  it("EDT-FR-AQCR: the arrow keys move the highlight, and Enter selects the highlighted entry", () => {
    const third = entry(3, "Three", 9);
    const { outline } = fakeOutline([entry(1, "One", 0), entry(2, "Two", 5), third]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    fireEvent.keyDown(filter(), { key: "ArrowDown" });
    fireEvent.keyDown(filter(), { key: "ArrowDown" });
    expect(options()[2]).toHaveAttribute("data-highlighted", "true");
    expect(filter()).toHaveAttribute("aria-activedescendant", options()[2].id);
    fireEvent.keyDown(filter(), { key: "Enter" });
    expect(outline.reveal).toHaveBeenCalledWith(third);
  });

  it("EDT-FR-IZHD: the filter is discarded when the panel closes", () => {
    const { outline } = fakeOutline([entry(1, "One", 0), entry(2, "Two", 5)]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    fireEvent.change(filter(), { target: { value: "two" } });
    expect(options()).toHaveLength(1);
    fireEvent.keyDown(filter(), { key: "Escape" });
    fireEvent.click(trigger());
    expect(filter()).toHaveValue("");
    expect(options()).toHaveLength(2);
  });

  it("EDT-FR-EBNU: a filter with no match shows \"No heading matches\", and Enter selects nothing", () => {
    const { outline } = fakeOutline([entry(1, "One", 0)]);
    render(<TocControl outline={outline} />);
    fireEvent.click(trigger());
    fireEvent.change(filter(), { target: { value: "zzz" } });
    expect(screen.getByText("No heading matches")).toBeInTheDocument();
    expect(screen.queryByText("No headings")).toBeNull();
    fireEvent.keyDown(filter(), { key: "Enter" });
    expect(outline.reveal).not.toHaveBeenCalled();
    expect(list()).toBeInTheDocument();
  });

  it("EDT-FR-EBNU: a document with no heading shows \"No headings\", in a panel marked empty", () => {
    const { outline } = fakeOutline([]);
    render(<TocControl outline={outline} />);
    expect(trigger()).toBeEnabled();
    fireEvent.click(trigger());
    expect(screen.getByText("No headings")).toBeInTheDocument();
    expect(screen.getByTestId("toc-menu")).toHaveClass("toc__panel--empty");
  });

  it("EDT-FR-PNGU: a panel under the control at the right edge of the window has its least width and stays inside the window", () => {
    const { outline } = fakeOutline([entry(1, "One", 0)]);
    render(<TocControl outline={outline} />);
    const right = window.innerWidth - 10;
    vi.spyOn(trigger(), "getBoundingClientRect").mockReturnValue({
      left: right - 30,
      right,
      top: 10,
      bottom: 40,
      width: 30,
      height: 30,
      x: right - 30,
      y: 10,
      toJSON: () => ({}),
    });
    fireEvent.click(trigger());
    const panel = screen.getByTestId("toc-menu");
    expect(panel.style.width).toBe("320px");
    expect(parseFloat(panel.style.left) + 320).toBeLessThanOrEqual(window.innerWidth - 8);
  });

  it("matchesHeading: an empty filter keeps all entries, and the label alone never matches", () => {
    expect(matchesHeading({ id: "0", label: "One", title: "One" }, "  ")).toBe(true);
    expect(matchesHeading({ id: "0", label: "", title: "" }, "empty")).toBe(false);
  });
});
