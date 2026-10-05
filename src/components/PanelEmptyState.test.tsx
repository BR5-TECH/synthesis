import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  EMPTY_BODY_CLASS,
  FILTERED_BODY_CLASS,
  PanelEmptyState,
  PanelFilteredState,
} from "./PanelEmptyState";

// Vitest runs without `globals`, so testing-library registers no auto-cleanup
// and a mounted tree would otherwise leak into the next test's queries.
afterEach(cleanup);

/**
 * SNV-FR-60's composition contract, asserted once on the shared component
 * rather than re-derived in each panel's suite.
 *
 * The panels' own tests say *that* they render this block; this says *what* the
 * block is. The geometry that centres it is CSS, guarded in
 * `../test/style-invariants.test.ts` — with `css: false` no rendering test can
 * see it.
 */
describe("PanelEmptyState (SNV-FR-60)", () => {
  it("composes the line, the sentence, and the action in that fixed order", () => {
    const { container } = render(
      <PanelEmptyState
        line="No drafts yet."
        action={{ label: "New draft", onClick: vi.fn() }}
      >
        A draft is where a new artifact is developed.
      </PanelEmptyState>,
    );

    const block = container.querySelector(".panel-empty");
    expect(block).not.toBeNull();
    // The order is the requirement, not an accident of how it reads: a block
    // that put its button first, or its sentence above its line, would satisfy
    // every presence assertion in every panel suite.
    const parts = [...block!.children];
    expect(parts).toHaveLength(3);
    expect(parts[0]).toHaveTextContent("No drafts yet.");
    expect(parts[0].tagName).toBe("P");
    expect(parts[1]).toHaveTextContent(
      "A draft is where a new artifact is developed.",
    );
    expect(parts[1].tagName).toBe("P");
    expect(parts[2].tagName).toBe("BUTTON");
  });

  it("renders the action as a button that invokes its handler", async () => {
    const onClick = vi.fn();
    render(
      <PanelEmptyState line="No drafts yet." action={{ label: "New draft", onClick }}>
        A sentence.
      </PanelEmptyState>,
    );

    const button = screen.getByRole("button", { name: "New draft" });
    // Not a submit: this is the shared block for every vertical panel, and one
    // of them enclosing it in a form would otherwise submit it.
    expect(button).toHaveAttribute("type", "button");
    await userEvent.click(button);
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("carries an icon beside the action's label when one is given", () => {
    render(
      <PanelEmptyState
        line="No drafts yet."
        action={{
          label: "New draft",
          icon: <svg data-testid="plus" />,
          onClick: vi.fn(),
        }}
      >
        A sentence.
      </PanelEmptyState>,
    );

    const button = screen.getByRole("button", { name: "New draft" });
    expect(button.querySelector("[data-testid=plus]")).not.toBeNull();
  });

  it("renders no action at all — not a disabled one — for a surface with none", () => {
    const { container } = render(
      <PanelEmptyState line="No comments yet.">
        A thread is opened from the comment rail.
      </PanelEmptyState>,
    );

    const block = container.querySelector(".panel-empty");
    expect(block!.children).toHaveLength(2);
    // SNV-FR-60 asks for the absence of a control, which a disabled `button`,
    // `select`, or `[aria-disabled]` would not satisfy.
    expect(
      block!.querySelectorAll("button, :disabled, [aria-disabled]"),
    ).toHaveLength(0);
  });

  it("names the body class a panel puts on the region holding it", () => {
    // The centring lives on that container (style-invariants), so a panel that
    // forgets this class gets the flush-top-left defect back.
    expect(EMPTY_BODY_CLASS.split(/\s+/)).toContain("vpanel__body");
    expect(EMPTY_BODY_CLASS.split(/\s+/)).toContain("vpanel__body--empty");
  });
});

/**
 * SNV-FR-61's counterpart. What is asserted here is that it is *not* the block
 * above — a narrowed list says the controls in the author's hands admit
 * nothing, and leaves them there to be changed.
 */
describe("PanelFilteredState (SNV-FR-61)", () => {
  it("renders the message alone, with no line, sentence, or action", () => {
    const { container } = render(
      <PanelFilteredState>No draft matches this filter.</PanelFilteredState>,
    );

    expect(screen.getByText("No draft matches this filter.")).toBeInTheDocument();
    expect(container.querySelector(".panel-filtered")).not.toBeNull();
    // Reaching for the empty-state block here is the mistake SNV-FR-61 names.
    expect(container.querySelector(".panel-empty")).toBeNull();
    expect(container.querySelector("button")).toBeNull();
  });

  it("carries a body class distinct from the empty state's", () => {
    // The two states are told apart by what the body does with them, so a panel
    // cannot land in one while styling itself as the other.
    expect(FILTERED_BODY_CLASS).not.toBe(EMPTY_BODY_CLASS);
    expect(FILTERED_BODY_CLASS.split(" ")).toContain("vpanel__body");
  });
});
