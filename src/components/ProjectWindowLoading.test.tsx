import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { ProjectWindowLoading } from "./ProjectWindowLoading";

afterEach(cleanup);

describe("ProjectWindowLoading (PWN-FR-KQZB)", () => {
  it("PWN-FR-KQZB: names the project while it opens, with no close control", () => {
    render(<ProjectWindowLoading name="design" error="" onClose={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("Opening design…");
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("PWN-FR-KQZB: shows the failure and closes the tab on request", () => {
    const onClose = vi.fn();
    render(<ProjectWindowLoading name="design" error="not a project" onClose={onClose} />);
    expect(screen.getByRole("status")).toHaveTextContent("Could not open design");
    expect(screen.getByRole("alert")).toHaveTextContent("not a project");
    fireEvent.click(screen.getByRole("button", { name: "Close this tab" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
