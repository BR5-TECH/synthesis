/**
 * The action chip on the change under review (`DCR-draft-change-review.md`
 * DCR-FR-12, DCR-FR-28, DCR-FR-10).
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

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
  // Built outside React, so the host is there before the chip's layout effect
  // asks for it — a ref on a parent is attached only after its children.
  const scroller = document.createElement("div");
  const block = document.createElement("div");
  block.className = "hunk hunk--add";
  block.setAttribute(HUNK_ATTR, h.id);
  scroller.appendChild(block);
  document.body.appendChild(scroller);
  hosts.push(scroller);
  const handlers = { onAccept: vi.fn(), onReject: vi.fn(), onDiscuss: vi.fn() };
  render(<HunkChip hunk={h} hostRef={{ current: scroller }} busy={false} {...handlers} />);
  return handlers;
}

const hosts: HTMLElement[] = [];
afterEach(() => {
  for (const host of hosts.splice(0)) host.remove();
});

describe("DCR-FR-12: what the chip holds", () => {
  it("DCR-FR-12: an undecided change's chip holds Accept, Reject, and Discuss", () => {
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
      "Change 2 of 3, replacement proposed by @arch, rejected",
    );
    expect(hunkLabel(hunk())).not.toContain("rejected");
  });
});
