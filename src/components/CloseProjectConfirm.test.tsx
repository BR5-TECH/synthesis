import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { CloseProjectConfirm } from "./CloseProjectConfirm";

afterEach(cleanup);

describe("CloseProjectConfirm (PJT-FR-OJZG)", () => {
  it("names the project and focuses the safe choice", () => {
    render(<CloseProjectConfirm projectName="acme" onConfirm={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByRole("alertdialog", { name: "Close acme" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Keep open" })).toHaveFocus();
  });

  it("stops and closes on confirm", () => {
    const onConfirm = vi.fn();
    render(<CloseProjectConfirm projectName="acme" onConfirm={onConfirm} onCancel={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Stop and close" }));
    expect(onConfirm).toHaveBeenCalled();
  });

  it("keeps the tab open on Cancel and on Escape", () => {
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    render(<CloseProjectConfirm projectName="acme" onConfirm={onConfirm} onCancel={onCancel} />);
    fireEvent.click(screen.getByRole("button", { name: "Keep open" }));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(2);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
