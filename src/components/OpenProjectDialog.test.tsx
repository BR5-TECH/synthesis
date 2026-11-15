import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";


import { OpenProjectDialog } from "./OpenProjectDialog";

afterEach(cleanup);

function renderDialog() {
  const onChoose = vi.fn();
  const onCancel = vi.fn();
  render(<OpenProjectDialog projectName="design-skills" onChoose={onChoose} onCancel={onCancel} />);
  return { onChoose, onCancel };
}

describe("OpenProjectDialog (PJT-FR-GASB)", () => {
  it("names the project in a modal dialog", () => {
    renderDialog();
    const dialog = screen.getByRole("dialog", { name: "Open design-skills" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
  });

  it("offers This tab, New tab and Cancel, and focuses New tab first", () => {
    renderDialog();
    expect(screen.getByRole("button", { name: "This tab" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New tab" })).toHaveFocus();
  });

  it("reports This tab", () => {
    const { onChoose } = renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "This tab" }));
    expect(onChoose).toHaveBeenCalledWith("this-tab");
  });

  it("reports New tab", () => {
    const { onChoose } = renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "New tab" }));
    expect(onChoose).toHaveBeenCalledWith("new-tab");
  });

  it("cancels with the button and with Escape, and chooses nothing", () => {
    const { onChoose, onCancel } = renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onChoose).not.toHaveBeenCalled();
  });
});

describe("OpenProjectDialog focus (PJT-FR-YCNT)", () => {
  it("keeps Tab and Shift+Tab inside the three controls", () => {
    renderDialog();
    const thisTab = screen.getByRole("button", { name: "This tab" });
    const cancel = screen.getByRole("button", { name: "Cancel" });
    cancel.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(thisTab).toHaveFocus();
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(cancel).toHaveFocus();
  });

  it("returns focus to the opener when it closes", () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const { unmount } = render(
      <OpenProjectDialog projectName="x" onChoose={() => {}} onCancel={() => {}} />,
    );
    expect(opener).not.toHaveFocus();
    unmount();
    expect(opener).toHaveFocus();
    opener.remove();
  });
});
