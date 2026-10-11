/**
 * The destination toggle of the graduation start dialog
 * (`../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-BZHW): one
 * segmented toggle of four, a radio group with a roving tab stop.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import {
  DestinationChoice,
  type Destination,
  type DestinationInput,
} from "./GraduationStart/destination";

function Harness({
  initial = "stream",
  noStream = false,
  disabled = false,
  onChange = () => {},
}: {
  initial?: Destination;
  noStream?: boolean;
  disabled?: boolean;
  onChange?: (value: Destination, via: DestinationInput) => void;
}) {
  const [value, setValue] = useState<Destination>(initial);
  return (
    <DestinationChoice
      value={value}
      disabled={disabled}
      noStream={noStream}
      onChange={(next, via) => {
        onChange(next, via);
        setValue(next);
      }}
    />
  );
}

afterEach(cleanup);

const segment = (name: string) => screen.getByRole("radio", { name });
const chosen = () =>
  screen.getAllByRole("radio").find((radio) => radio.getAttribute("aria-checked") === "true");

describe("the destination toggle", () => {
  it("GSD-FR-BZHW: the four choices are one radio group, in order, with one chosen", () => {
    render(<Harness />);
    const group = screen.getByRole("radiogroup", { name: "Where the run works" });
    expect(within(group).getAllByRole("radio").map((radio) => radio.textContent)).toEqual([
      "Existing stream",
      "New stream",
      "Temporary stream",
      "Work directly",
    ]);
    expect(segment("Existing stream")).toBeChecked();
    expect(segment("New stream")).not.toBeChecked();
    expect(segment("Temporary stream")).not.toBeChecked();
    expect(segment("Work directly")).not.toBeChecked();
  });

  it("GSD-FR-BZHW: a click chooses a segment, and a click on the chosen one changes nothing", async () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    await userEvent.click(segment("Temporary stream"));
    expect(segment("Temporary stream")).toBeChecked();
    expect(segment("Existing stream")).not.toBeChecked();
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange).toHaveBeenLastCalledWith("temporary", "click");
    await userEvent.click(segment("Temporary stream"));
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("GSD-FR-BZHW: the Tab key reaches the chosen segment only", async () => {
    render(
      <>
        <button type="button">before</button>
        <Harness initial="new" />
        <button type="button">after</button>
      </>,
    );
    expect(screen.getAllByRole("radio").map((radio) => radio.tabIndex)).toEqual([-1, 0, -1, -1]);
    screen.getByRole("button", { name: "before" }).focus();
    await userEvent.tab();
    expect(segment("New stream")).toHaveFocus();
    await userEvent.tab();
    expect(screen.getByRole("button", { name: "after" })).toHaveFocus();
  });

  it("GSD-FR-BZHW: the arrow keys choose and focus the next segment, and wrap at the ends", async () => {
    const onChange = vi.fn();
    render(<Harness onChange={onChange} />);
    segment("Existing stream").focus();
    await userEvent.keyboard("{ArrowRight}");
    // GSD-FR-WMTD: the dialog keeps focus on the toggle for an arrow choice.
    expect(onChange).toHaveBeenLastCalledWith("new", "arrows");
    expect(segment("New stream")).toBeChecked();
    expect(segment("New stream")).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(segment("Temporary stream")).toBeChecked();
    await userEvent.keyboard("{ArrowRight}{ArrowRight}");
    expect(segment("Existing stream")).toBeChecked();
    expect(segment("Existing stream")).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(segment("Work directly")).toBeChecked();
    expect(segment("Work directly")).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(segment("Temporary stream")).toBeChecked();
  });

  it("GSD-FR-BZHW: Home and End choose the first and the last segment", async () => {
    render(<Harness initial="new" />);
    segment("New stream").focus();
    await userEvent.keyboard("{End}");
    expect(segment("Work directly")).toBeChecked();
    expect(segment("Work directly")).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(segment("Existing stream")).toBeChecked();
    expect(segment("Existing stream")).toHaveFocus();
  });

  it("GSD-FR-BZHW, GSD-FR-LDGM: with no stream, Existing stream is a disabled segment the keys pass over", async () => {
    render(<Harness initial="new" noStream />);
    expect(segment("Existing stream")).toBeDisabled();
    expect(segment("New stream")).toBeEnabled();
    expect(segment("Work directly")).toBeEnabled();
    segment("New stream").focus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(segment("Work directly")).toBeChecked();
    await userEvent.keyboard("{ArrowRight}");
    expect(segment("New stream")).toBeChecked();
    await userEvent.keyboard("{Home}");
    expect(segment("New stream")).toBeChecked();
    expect(segment("New stream")).toHaveFocus();
    await userEvent.click(segment("Existing stream"));
    expect(segment("Existing stream")).not.toBeChecked();
  });

  it("GSD-FR-LDGM: where the chosen segment is disabled, the Tab key reaches the first enabled one", async () => {
    render(
      <>
        <button type="button">before</button>
        <Harness initial="stream" noStream />
      </>,
    );
    expect(screen.getAllByRole("radio").map((radio) => radio.tabIndex)).toEqual([-1, 0, -1, -1]);
    screen.getByRole("button", { name: "before" }).focus();
    await userEvent.tab();
    expect(segment("New stream")).toHaveFocus();
  });

  it("GSD-FR-BZHW: a value the dialog sets moves the tab stop with it", () => {
    const props = { disabled: false, noStream: false, onChange: () => {} };
    const { rerender } = render(<DestinationChoice value="new" {...props} />);
    expect(segment("New stream").tabIndex).toBe(0);
    rerender(<DestinationChoice value="stream" {...props} />);
    expect(segment("Existing stream")).toBeChecked();
    expect(screen.getAllByRole("radio").map((radio) => radio.tabIndex)).toEqual([0, -1, -1, -1]);
  });

  it("GSD-FR-WMTD: the arrow keys, Home and End do not scroll the dialog", () => {
    render(<Harness />);
    for (const key of ["ArrowRight", "ArrowLeft", "ArrowUp", "ArrowDown", "Home", "End"]) {
      // `fireEvent` returns false when the handler prevents the default.
      expect(fireEvent.keyDown(segment("Existing stream"), { key })).toBe(false);
    }
  });

  it("GSD-FR-BFOU: while a start is in flight every segment is inert", async () => {
    const onChange = vi.fn();
    render(<Harness initial="new" disabled onChange={onChange} />);
    for (const radio of screen.getAllByRole("radio")) expect(radio).toBeDisabled();
    await userEvent.click(segment("Work directly"));
    // React does not send a click to a disabled button, but it does send a key.
    fireEvent.keyDown(segment("New stream"), { key: "ArrowRight" });
    fireEvent.keyDown(segment("New stream"), { key: "End" });
    expect(onChange).not.toHaveBeenCalled();
    expect(chosen()).toBe(segment("New stream"));
    expect(screen.getAllByRole("radio").every((radio) => radio.tabIndex === -1)).toBe(true);
  });
});
