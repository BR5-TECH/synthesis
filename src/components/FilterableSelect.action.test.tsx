import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { FilterableSelect, menuPlacement, type FilterableOption } from "./FilterableSelect";

afterEach(cleanup);

const OPTIONS: FilterableOption[] = [
  { id: "a", label: "Alpha", level: 1, title: "Alpha in full", ariaLabel: "Alpha, level 1" },
  { id: "b", label: "Beta", hint: "a hint" },
];

function renderAction(props: Partial<Parameters<typeof FilterableSelect>[0]> = {}) {
  const onChange = vi.fn();
  const onOpenChange = vi.fn();
  render(
    <FilterableSelect
      options={OPTIONS}
      value=""
      onChange={onChange}
      label="Do a thing"
      testId="act"
      alwaysNotify
      onOpenChange={onOpenChange}
      trigger={{ content: <svg data-testid="glyph" />, className: "btn btn--icon" }}
      {...props}
    />,
  );
  return { onChange, onOpenChange, trigger: screen.getByRole("button", { name: "Do a thing" }) };
}

describe("FilterableSelect as an action menu (EDT-FR-LGQA, EDT-FR-VUGQ)", () => {
  it("EDT-FR-LGQA: the trigger is a plain button that shows its content and takes the label as its name and tooltip", () => {
    const { trigger } = renderAction();
    expect(trigger).toHaveAttribute("title", "Do a thing");
    expect(trigger).toHaveClass("btn", "btn--icon");
    expect(trigger).not.toHaveAttribute("role");
    expect(trigger).toHaveAttribute("aria-haspopup", "listbox");
    expect(screen.getByTestId("glyph")).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Do a thing" })).toBeNull();
    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute("data-active", "true");
    expect(trigger).toHaveAttribute("aria-expanded", "true");
  });

  it("EDT-FR-KJLQ: the caller hears each open and close", () => {
    const { trigger, onOpenChange } = renderAction();
    fireEvent.click(trigger);
    expect(onOpenChange).toHaveBeenLastCalledWith(true);
    fireEvent.keyDown(screen.getByTestId("act-filter"), { key: "Escape" });
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    fireEvent.click(trigger);
    fireEvent.mouseDown(document.body);
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
    expect(onOpenChange).toHaveBeenCalledTimes(4);
  });

  it("EDT-FR-YSJX: with returnFocus false a pick leaves focus where the pick put it, and Escape still returns it", () => {
    const { trigger, onChange } = renderAction({ returnFocus: false });
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("option", { name: "Alpha, level 1" }));
    expect(onChange).toHaveBeenCalledWith("a");
    expect(trigger).not.toHaveFocus();

    fireEvent.click(trigger);
    fireEvent.keyDown(screen.getByTestId("act-filter"), { key: "Escape" });
    expect(trigger).toHaveFocus();
  });

  it("EDT-FR-PNGU: an option carries its level, its own tooltip, and its accessible name; the hint stays the fallback tooltip", () => {
    const { trigger } = renderAction();
    fireEvent.click(trigger);
    const alpha = screen.getByTestId("act-option-a");
    expect(alpha).toHaveAttribute("data-level", "1");
    expect(alpha).toHaveAttribute("title", "Alpha in full");
    expect(alpha).toHaveAccessibleName("Alpha, level 1");
    const beta = screen.getByTestId("act-option-b");
    expect(beta).not.toHaveAttribute("data-level");
    expect(beta).toHaveAttribute("title", "a hint");
  });

  it("EDT-FR-YSJX: the filter box takes focus without a scroll", () => {
    const focus = vi.spyOn(HTMLInputElement.prototype, "focus");
    try {
      const { trigger } = renderAction();
      fireEvent.click(trigger);
      expect(focus).toHaveBeenCalledWith({ preventScroll: true });
    } finally {
      focus.mockRestore();
    }
  });

  it("EDT-FR-VUGQ: a control that becomes disabled closes its open panel", () => {
    const onOpenChange = vi.fn();
    const props = {
      options: OPTIONS,
      value: "",
      onChange: vi.fn(),
      label: "Do a thing",
      testId: "act",
      onOpenChange,
      trigger: { content: "T", className: "btn" },
    };
    const view = render(<FilterableSelect {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "Do a thing" }));
    expect(screen.getByTestId("act-menu")).toBeInTheDocument();
    view.rerender(<FilterableSelect {...props} disabled />);
    expect(screen.queryByTestId("act-menu")).toBeNull();
    expect(screen.getByRole("button", { name: "Do a thing" })).toBeDisabled();
    expect(onOpenChange).toHaveBeenLastCalledWith(false);
  });

  it("the panel carries the caller's class, and the root carries its own", () => {
    const { trigger } = renderAction({ menuClassName: "my-panel", className: "my-root" });
    fireEvent.click(trigger);
    expect(screen.getByTestId("act-menu")).toHaveClass("fsel__menu", "menu", "my-panel");
    expect(trigger.parentElement).toHaveClass("fsel", "my-root");
  });
});

describe("menuPlacement with a least width (NAW-FR-XTGP)", () => {
  const viewport = { width: 1000, height: 800 };

  it("NAW-FR-XTGP: a narrow anchor gets a panel of the least width, kept inside the window at the right edge", () => {
    const anchor = { top: 10, bottom: 40, left: 960, width: 30 };
    const placement = menuPlacement(anchor, viewport, 300, 320);
    expect(placement.width).toBe(320);
    expect(placement.left + placement.width).toBeLessThanOrEqual(viewport.width - 8);
  });

  it("NAW-FR-XTGP: the least width never takes the panel out of a narrow window, and a wide anchor keeps its width", () => {
    expect(menuPlacement({ top: 10, bottom: 40, left: 0, width: 30 }, { width: 200, height: 800 }, 300, 320).width).toBe(184);
    expect(menuPlacement({ top: 10, bottom: 40, left: 0, width: 400 }, viewport, 300, 320).width).toBe(400);
    expect(menuPlacement({ top: 10, bottom: 40, left: 0, width: 30 }, viewport, 300).width).toBe(30);
  });
});
