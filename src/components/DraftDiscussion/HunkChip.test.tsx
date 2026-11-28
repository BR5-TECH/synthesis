/**
 * The action chip on the change under review (`DCR-draft-change-review.md`
 * DCR-FR-12, DCR-FR-28, DCR-FR-10).
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { useRef } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { HunkChip } from "./HunkChip";
import { HUNK_ATTR, hunkLabel, type DecoratedHunk } from "./hunkDecorations";

afterEach(cleanup);

function hunk(over: Partial<DecoratedHunk> = {}): DecoratedHunk {
  return {
    id: "h2",
    kind: "replace",
    state: "pending",
    from: 1,
    to: 5,
    after: "new",
    lost: false,
    position: 2,
    total: 3,
    agent: "@arch",
    editable: true,
    ...over,
  };
}

/**
 * The chip places itself against the change's own block in the scroller, so a
 * host holding that block is all it needs to render.
 */
function mount(h: DecoratedHunk) {
  // Built outside React. The test below builds the host inside React too.
  const scroller = document.createElement("div");
  const block = document.createElement("div");
  block.className = "hunk hunk--add";
  block.setAttribute(HUNK_ATTR, h.id);
  scroller.appendChild(block);
  document.body.appendChild(scroller);
  hosts.push(scroller);
  const handlers = {
    onAccept: vi.fn(),
    onReject: vi.fn(),
    onDiscuss: vi.fn(),
    onCompare: vi.fn(),
  };
  render(<HunkChip hunk={h} hostRef={{ current: scroller }} busy={false} {...handlers} />);
  return handlers;
}

const hosts: HTMLElement[] = [];
afterEach(() => {
  for (const host of hosts.splice(0)) host.remove();
});

describe("DCR-FR-12: what the chip holds", () => {
  it("DCR-FR-12: an undecided edit's chip holds Accept, Reject, and Discuss", () => {
    mount(hunk());
    expect(screen.getByRole("button", { name: /Accept/ })).toBeEnabled();
    expect(screen.getByRole("button", { name: /Reject/ })).toBeEnabled();
    expect(screen.getByRole("button", { name: /Discuss/ })).toBeEnabled();
    expect(screen.queryByTestId("hunk-chip-rejected")).toBeNull();
  });

  it("DCR-FR-12, DCR-FR-28: a rejected change's chip says Rejected in words and holds Accept and Discuss", () => {
    const handlers = mount(hunk({ state: "rejected" }));
    expect(screen.getByTestId("hunk-chip-rejected")).toHaveTextContent("Rejected");
    expect(screen.queryByRole("button", { name: /Reject/ })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /Accept/ }));
    expect(handlers.onAccept).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: /Discuss/ }));
    expect(handlers.onDiscuss).toHaveBeenCalledTimes(1);
    expect(handlers.onReject).not.toHaveBeenCalled();
  });
});

describe("DCR-FR-10: what a change is called", () => {
  it("DCR-FR-10: a rejected change's accessible name says that it is rejected", () => {
    expect(hunkLabel(hunk({ state: "rejected" }))).toBe(
      "Edit 2 of 3, replacement proposed by @arch, rejected",
    );
    expect(hunkLabel(hunk())).not.toContain("rejected");
  });
});

/** A host and its chip that mount in one commit, as a Source review does. */
function HostWithChip({ h }: { h: DecoratedHunk }) {
  const hostRef = useRef<HTMLDivElement>(null);
  return (
    <div ref={hostRef}>
      <div className="hunk hunk--add" {...{ [HUNK_ATTR]: h.id }}>
        new
      </div>
      <HunkChip hunk={h} hostRef={hostRef} busy={false} onAccept={vi.fn()} onReject={vi.fn()} onDiscuss={vi.fn()} />
    </div>
  );
}

describe("OCW-FR-COLR: a chip whose host mounts with it", () => {
  it("OCW-FR-COLR, DCR-FR-12: the chip shows when it mounts in the same commit as its host", () => {
    render(<HostWithChip h={hunk()} />);
    expect(screen.getByRole("button", { name: /Accept/ })).toBeInTheDocument();
  });
});

describe("DCR-FR-HBTM: Compare on the chip of a diagram hunk", () => {
  const diagram = { oldSource: "flowchart TD\n  A --> B", newSource: "flowchart TD\n  A --> C" };

  it("DCR-FR-HBTM: a text hunk's chip has no Compare button", () => {
    mount(hunk());
    expect(screen.queryByRole("button", { name: "Compare diagrams" })).toBeNull();
  });

  it("DCR-FR-HBTM: an undecided diagram hunk's chip holds Accept, Reject, Discuss, and Compare, in that order", () => {
    mount(hunk({ diagram, editable: false }));
    const names = screen
      .getAllByRole("button")
      .map((b) => b.getAttribute("aria-label") ?? b.textContent?.trim());
    expect(names).toEqual(["Accept", "Reject", "Discuss →", "Compare diagrams"]);
  });

  it("DCR-FR-HBTM: Compare is an icon with a text label, named and titled Compare diagrams", () => {
    mount(hunk({ diagram, editable: false }));
    const compare = screen.getByRole("button", { name: "Compare diagrams" });
    expect(compare).toHaveAttribute("title", "Compare diagrams");
    expect(compare).toHaveTextContent("Compare");
    expect(compare.querySelector("svg")).not.toBeNull();
  });

  it("DCR-FR-HBTM: a rejected diagram hunk's chip says Rejected and holds Accept, Discuss, and Compare", () => {
    mount(hunk({ diagram, editable: false, state: "rejected" }));
    expect(screen.getByTestId("hunk-chip-rejected")).toHaveTextContent("Rejected");
    expect(screen.queryByRole("button", { name: /^Reject/ })).toBeNull();
    expect(screen.getByRole("button", { name: /Accept/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Discuss/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Compare diagrams" })).toBeInTheDocument();
  });

  it("DCR-FR-HBTM, DCR-FR-UKMP: Compare decides nothing, gives its button to the review, and works by keyboard", async () => {
    const handlers = mount(hunk({ diagram, editable: false }));
    const compare = screen.getByRole("button", { name: "Compare diagrams" });
    compare.focus();
    await userEvent.keyboard("{Enter}");
    expect(handlers.onCompare).toHaveBeenCalledTimes(1);
    expect(handlers.onCompare).toHaveBeenCalledWith(compare);
    expect(handlers.onAccept).not.toHaveBeenCalled();
    expect(handlers.onReject).not.toHaveBeenCalled();
  });

  it("DCR-FR-HBTM: Compare stays on while a decision is in flight, because it decides nothing", () => {
    const scroller = document.createElement("div");
    const block = document.createElement("div");
    block.className = "hunk hunk--add";
    block.setAttribute(HUNK_ATTR, "h2");
    scroller.appendChild(block);
    document.body.appendChild(scroller);
    hosts.push(scroller);
    render(
      <HunkChip
        hunk={hunk({ diagram, editable: false })}
        hostRef={{ current: scroller }}
        busy
        onAccept={vi.fn()}
        onReject={vi.fn()}
        onDiscuss={vi.fn()}
        onCompare={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Compare diagrams" })).toBeEnabled();
    expect(screen.getByRole("button", { name: /Accept/ })).toBeDisabled();
  });

  it("DCR-FR-UKMP: when the chip goes away with the focus on Compare, the focus goes to the document column", () => {
    const scroller = document.createElement("div");
    const block = document.createElement("div");
    block.className = "hunk hunk--add";
    block.setAttribute(HUNK_ATTR, "h2");
    scroller.appendChild(block);
    document.body.appendChild(scroller);
    hosts.push(scroller);
    const view = render(
      <HunkChip
        hunk={hunk({ diagram, editable: false })}
        hostRef={{ current: scroller }}
        busy={false}
        onAccept={vi.fn()}
        onReject={vi.fn()}
        onDiscuss={vi.fn()}
        onCompare={vi.fn()}
      />,
    );
    screen.getByRole("button", { name: "Compare diagrams" }).focus();
    view.unmount();
    expect(scroller).toHaveFocus();
  });

  it("DCR-FR-BYSX, PVC-FR-VBPY: a diagram hunk's accessible name gives its kind as a diagram edit, addition, or removal, and counts it as an edit", () => {
    expect(hunkLabel(hunk({ diagram }))).toBe(
      "Edit 2 of 3, diagram edit proposed by @arch",
    );
    expect(hunkLabel(hunk({ diagram: { ...diagram, oldSource: null }, kind: "add" }))).toBe(
      "Edit 2 of 3, diagram addition proposed by @arch",
    );
    expect(hunkLabel(hunk({ diagram: { ...diagram, newSource: null }, kind: "del" }))).toBe(
      "Edit 2 of 3, diagram removal proposed by @arch",
    );
    expect(hunkLabel(hunk({ diagram }))).not.toMatch(/change/i);
  });
});
