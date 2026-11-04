/**
 * HTML comments in a Diff tab's Rich rendering (DFV-FR-SZZX, DFV-FR-NJIS): a
 * break and a comment are blocks of their own, an alignment marker belongs to
 * the block it aligns, and both sides show them as the Editor does.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, within } from "@testing-library/react";

import { resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetLogBufferForTest } from "../logging";
import { makeWireBackend, renderDiff, target } from "../test/diffViewFixtures";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const wireBackend = makeWireBackend(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
  resetDiffModes();
  resetAppPreferencesCache();
  resetLogBufferForTest();
});

afterEach(cleanup);

async function rich(oldText: string, newText: string): Promise<HTMLElement> {
  wireBackend({
    prefs: { diffRenderingMode: "rich" },
    revisions: { old: oldText, new: newText, isBinary: false },
  });
  renderDiff(target("notes.md"));
  return screen.findByTestId("diff-rich-unified");
}

/** Every marked unit of kind `kind`, static or editable, with its mark. */
function units(view: HTMLElement, kind: string): Array<{ el: HTMLElement; mark: string | null }> {
  return Array.from(view.querySelectorAll<HTMLElement>(`[data-kind="${kind}"]`)).map((el) => ({
    el,
    mark: el.getAttribute("data-mark"),
  }));
}

describe("Rich marking of markup (DFV-FR-SZZX)", () => {
  it("DFV-FR-SZZX: a changed break line marks its spacer, and the text under it stays unmarked", async () => {
    const view = await rich("<!-- %% -->\nSame text.\n", "<!-- %%% -->\nSame text.\n");
    await screen.findAllByRole("separator", { name: /Vertical break/ });
    const breaks = units(view, "break");
    expect(breaks.map((u) => u.mark).sort()).toEqual(["added", "removed"]);
    expect(units(view, "paragraph").map((u) => u.mark)).toEqual(["none"]);
    expect(screen.getByRole("separator", { name: "Vertical break V3" })).toBeInTheDocument();
    expect(screen.getByRole("separator", { name: "Vertical break V2" })).toBeInTheDocument();
  });

  it("DFV-FR-SZZX: a changed alignment marker marks the block it aligns", async () => {
    const view = await rich("<!-- center -->\nSame text.\n", "<!-- right -->\nSame text.\n");
    await screen.findAllByText("Same text.");
    const paragraphs = units(view, "paragraph");
    expect(paragraphs.map((u) => u.mark).sort()).toEqual(["added", "removed"]);
    const aligns = Array.from(view.querySelectorAll("p[data-align]")).map((p) =>
      p.getAttribute("data-align"),
    );
    expect(aligns.sort()).toEqual(["center", "right"]);
  });

  it("DFV-FR-SZZX: an unchanged break and comment stay unmarked beside a changed paragraph", async () => {
    const view = await rich(
      "<!-- % -->\n<!-- note -->\n\nOld words.\n",
      "<!-- % -->\n<!-- note -->\n\nNew words.\n",
    );
    await screen.findByText(/New/);
    expect(units(view, "break").map((u) => u.mark)).toEqual(["none"]);
    expect(units(view, "comment").map((u) => u.mark)).toEqual(["none"]);
  });

  it("DFV-FR-SZZX: a comment inside a list item keeps the marking of the blocks after it", async () => {
    const view = await rich(
      "- a\n  <!-- c -->\n- b\n\n<!-- % -->\nOld words.\n",
      "- a\n  <!-- c -->\n- b\n\n<!-- % -->\nNew words.\n",
    );
    await screen.findByText(/New/);
    expect(units(view, "list-item").map((u) => u.mark)).toEqual(["none", "none"]);
    expect(units(view, "break").map((u) => u.mark)).toEqual(["none"]);
    expect(units(view, "paragraph").map((u) => u.mark).sort()).toEqual(["added", "removed"]);
  });
});

describe("both sides show the markup (DFV-FR-NJIS)", () => {
  it("DFV-FR-SZZX: a changed comment marks its chip", async () => {
    const view = await rich("<!-- first -->\n\nText.\n", "<!-- second -->\n\nText.\n");
    await screen.findAllByRole("img", { name: "HTML comment" });
    expect(units(view, "comment").map((u) => u.mark).sort()).toEqual(["added", "removed"]);
  });

  it("DFV-FR-NJIS, EDT-FR-KMAR: comment text never becomes an element on either side", async () => {
    const view = await rich(
      '<!-- <img src="x" onerror="alert(1)"> -->\n\nOld <!-- <b>b</b> -->.\n',
      "New <!-- <script>bad()</script> -->.\n",
    );
    await screen.findByText(/New/);
    expect(view.querySelector("img")).toBeNull();
    expect(view.querySelector("b")).toBeNull();
    expect(view.querySelector("script")).toBeNull();
  });

  it("DFV-FR-NJIS: a removed comment is an inert chip that keeps its tooltip", async () => {
    const view = await rich("<!-- gone soon -->\n\nText.\n", "Text.\n");
    const removed = units(view, "comment").find((u) => u.mark === "removed");
    expect(removed).toBeDefined();
    const chip = within(removed!.el).getByRole("img", { name: "HTML comment" });
    expect(chip.closest("[contenteditable='true']")).toBeNull();
    act(() => chip.focus());
    expect(within(removed!.el).getByRole("tooltip")).toHaveTextContent("gone soon");
    fireEvent.keyDown(chip, { key: "Escape" });
    expect(within(removed!.el).queryByRole("tooltip")).toBeNull();
  });

  it("DFV-FR-NJIS: the editable target shows a spacer, an aligned block, and an inline chip", async () => {
    const view = await rich("Old.\n", "<!-- %% -->\n<!-- center -->\nNew <!-- inline --> words.\n");
    await screen.findByRole("separator", { name: "Vertical break V2" });
    const target = view.querySelector(".diff-run") as HTMLElement;
    expect(target).not.toBeNull();
    expect(target.querySelector(".md-break")).not.toBeNull();
    expect(target.querySelector("p[data-align='center']")).toHaveTextContent("New");
    expect(within(target).getByRole("img", { name: "HTML comment" })).toBeInTheDocument();
  });
});
