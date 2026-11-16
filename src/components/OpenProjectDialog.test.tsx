import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { OpenProjectDialog } from "./OpenProjectDialog";

afterEach(cleanup);

function renderDialog() {
  const onChoose = vi.fn();
  const onCancel = vi.fn();
  render(
    <OpenProjectDialog
      projectName="design-skills"
      currentName="acme"
      onChoose={onChoose}
      onCancel={onCancel}
    />,
  );
  return { onChoose, onCancel };
}

describe("OpenProjectDialog (PJT-FR-GASB, PJT-FR-DLGT)", () => {
  it("PJT-FR-GASB, PJT-FR-YCNT: names the project in a modal dialog", () => {
    renderDialog();
    const dialog = screen.getByRole("dialog", { name: "Open design-skills" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
  });

  it("PJT-FR-DLGT: says which project is open here and asks where the new one opens", () => {
    renderDialog();
    expect(
      screen.getByText("acme is open in this window. Where do you want to open design-skills?"),
    ).toBeInTheDocument();
    expect(screen.getByRole("dialog")).toHaveAccessibleDescription(
      "acme is open in this window. Where do you want to open design-skills?",
    );
  });

  it("PJT-FR-DLGT: each choice is a card that describes what it does", () => {
    renderDialog();
    expect(screen.getByRole("button", { name: "This tab" })).toHaveAccessibleDescription(
      "acme closes in this window. Its changes are saved first.",
    );
    expect(screen.getByRole("button", { name: "New tab" })).toHaveAccessibleDescription(
      "design-skills opens in a new tab. acme stays open with its files and runs.",
    );
  });

  it("PJT-FR-GASB: offers This tab, New tab and Cancel, and focuses New tab first", () => {
    renderDialog();
    expect(screen.getByRole("button", { name: "This tab" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "New tab" })).toHaveFocus();
  });

  it("PJT-FR-GASB: reports This tab, not remembered", () => {
    const { onChoose } = renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "This tab" }));
    expect(onChoose).toHaveBeenCalledWith("this-tab", false);
  });

  it("PJT-FR-GASB: reports New tab, not remembered", () => {
    const { onChoose } = renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "New tab" }));
    expect(onChoose).toHaveBeenCalledWith("new-tab", false);
  });

  it("PJT-FR-GASB: cancels with the button and with Escape, and chooses nothing", () => {
    const { onChoose, onCancel } = renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onChoose).not.toHaveBeenCalled();
  });
});

describe("OpenProjectDialog Remember my choice (PJT-FR-RMCB)", () => {
  it("PJT-FR-RMCB: shows the checkbox off, with the hint that names Settings", () => {
    renderDialog();
    const box = screen.getByRole("checkbox", { name: "Remember my choice" });
    expect(box).not.toBeChecked();
    expect(box).toHaveAccessibleDescription(
      "From now on, projects open this way with no question. You can change this in Settings → Navigation.",
    );
  });

  it("PJT-FR-RMCB, PJT-FR-RMBR: reports the choice with the flag when the box is on", () => {
    const { onChoose } = renderDialog();
    fireEvent.click(screen.getByRole("checkbox", { name: "Remember my choice" }));
    fireEvent.click(screen.getByRole("button", { name: "This tab" }));
    expect(onChoose).toHaveBeenCalledWith("this-tab", true);
  });

  it("PJT-FR-RMCB: the box is off again at the next show", () => {
    renderDialog();
    fireEvent.click(screen.getByRole("checkbox", { name: "Remember my choice" }));
    cleanup();
    renderDialog();
    expect(screen.getByRole("checkbox", { name: "Remember my choice" })).not.toBeChecked();
  });
});

describe("OpenProjectDialog focus (PJT-FR-YCNT)", () => {
  it("PJT-FR-YCNT: moves through the cards, the checkbox and Cancel, and keeps Tab inside", () => {
    renderDialog();
    const thisTab = screen.getByRole("button", { name: "This tab" });
    const box = screen.getByRole("checkbox", { name: "Remember my choice" });
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const newTab = screen.getByRole("button", { name: "New tab" });
    const order = Array.from(
      screen.getByRole("dialog").querySelectorAll<HTMLElement>("button, input"),
    );
    expect(order).toEqual([thisTab, newTab, box, cancel]);
    cancel.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(thisTab).toHaveFocus();
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(cancel).toHaveFocus();
  });

  it("PJT-FR-YCNT: Space toggles the checkbox", () => {
    renderDialog();
    const box = screen.getByRole("checkbox", { name: "Remember my choice" });
    box.focus();
    fireEvent.click(box);
    expect(box).toBeChecked();
  });

  it("PJT-FR-YCNT: returns focus to the opener when it closes", () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const { unmount } = render(
      <OpenProjectDialog projectName="x" currentName="y" onChoose={() => {}} onCancel={() => {}} />,
    );
    expect(opener).not.toHaveFocus();
    unmount();
    expect(opener).toHaveFocus();
    opener.remove();
  });
});
