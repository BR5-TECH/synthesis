import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

import { TocControl } from "./TocControl";
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

describe("TocControl", () => {
  it("EDT-FR-KJLQ: a closed panel reads nothing, and closing it ends the subscription", () => {
    const { outline, listeners } = fakeOutline([entry(1, "One", 0)]);
    render(<TocControl outline={outline} />);
    expect(outline.getSnapshot).not.toHaveBeenCalled();
    expect(outline.subscribe).not.toHaveBeenCalled();

    const button = screen.getByRole("button", { name: "Table of contents" });
    fireEvent.click(button);
    expect(outline.getSnapshot).toHaveBeenCalled();
    expect(listeners.size).toBe(1);

    fireEvent.click(button);
    expect(listeners.size).toBe(0);
  });

  it("EDT-FR-KJLQ, EDT-FR-PNGU: the open list follows the outline, with the full text in each tooltip", () => {
    const { outline, set } = fakeOutline([entry(1, "One", 0)]);
    render(<TocControl outline={outline} />);
    fireEvent.click(screen.getByRole("button", { name: "Table of contents" }));
    expect(screen.getAllByRole("treeitem").map((i) => i.textContent)).toEqual(["One"]);

    const long = "A heading that is far too long to show in full in the panel";
    set([entry(1, "One", 0), entry(3, long, 10)]);
    const items = screen.getAllByRole("treeitem");
    expect(items).toHaveLength(2);
    expect(items[1]).toHaveAttribute("title", long);
    expect(items[1]).toHaveAttribute("aria-level", "3");
  });

  it("EDT-FR-PNGU: a heading with no text shows a stand-in rather than a blank row", () => {
    const { outline } = fakeOutline([entry(2, "", 0)]);
    render(<TocControl outline={outline} />);
    fireEvent.click(screen.getByRole("button", { name: "Table of contents" }));
    expect(screen.getByRole("treeitem")).toHaveTextContent("Empty heading");
  });

  it("EDT-FR-YSJX: selecting an entry closes the panel and reveals that entry", () => {
    const first = entry(1, "One", 0);
    const second = entry(2, "Two", 5);
    const { outline } = fakeOutline([first, second]);
    render(<TocControl outline={outline} />);
    fireEvent.click(screen.getByRole("button", { name: "Table of contents" }));
    fireEvent.click(screen.getByRole("treeitem", { name: "Two" }));
    expect(screen.queryByRole("tree")).toBeNull();
    expect(outline.reveal).toHaveBeenCalledWith(second);
  });

  it("EDT-FR-KOUB: Tab from the control closes a panel that has no entry to focus", () => {
    const { outline } = fakeOutline([]);
    render(<TocControl outline={outline} />);
    const button = screen.getByRole("button", { name: "Table of contents" });
    fireEvent.click(button);
    expect(screen.getByText("No headings")).toBeInTheDocument();
    fireEvent.keyDown(button, { key: "Tab" });
    expect(screen.queryByText("No headings")).toBeNull();
  });

  it("EDT-FR-AQCR: ArrowDown on the control opens the panel with focus on the first entry", () => {
    const { outline } = fakeOutline([entry(1, "One", 0)]);
    render(<TocControl outline={outline} />);
    fireEvent.keyDown(screen.getByRole("button", { name: "Table of contents" }), {
      key: "ArrowDown",
    });
    expect(screen.getByRole("treeitem", { name: "One" })).toHaveFocus();
  });
});
