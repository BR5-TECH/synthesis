import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";


import { ProjectTabStrip } from "./ProjectTabStrip";
import type { ProjectWindow } from "../types";

afterEach(cleanup);

function win(id: string, over: Partial<ProjectWindow> = {}): ProjectWindow {
  return {
    projectId: `/work/${id}`,
    name: `Project ${id}`,
    windowLabel: `project-${id}`,
    needsAttention: false,
    ...over,
  };
}

function renderStrip(
  windows: ProjectWindow[],
  own = "/work/a",
  handlers: { onActivate?: (id: string) => void; onClose?: (id: string) => void } = {},
) {
  const onActivate = handlers.onActivate ?? vi.fn();
  const onClose = handlers.onClose ?? vi.fn();
  render(
    <ProjectTabStrip
      windows={windows}
      ownProjectId={own}
      onActivate={onActivate}
      onClose={onClose}
    />,
  );
  return { onActivate, onClose };
}

describe("ProjectTabStrip (PJT-FR-UHOC)", () => {
  it("renders nothing with no project window", () => {
    renderStrip([]);
    expect(screen.queryByTestId("project-tab-strip")).not.toBeInTheDocument();
  });

  it("renders nothing while one project is open", () => {
    renderStrip([win("a")]);
    expect(screen.queryByTestId("project-tab-strip")).not.toBeInTheDocument();
  });

  it("renders a tablist from two open projects", () => {
    renderStrip([win("a"), win("b")]);
    expect(screen.getByRole("tablist", { name: "Open projects" })).toBeInTheDocument();
    expect(screen.getAllByRole("tab")).toHaveLength(2);
  });

  it("renders three or more tabs in the order of the list", () => {
    renderStrip([win("a"), win("b"), win("c")]);
    expect(screen.getAllByRole("tab").map((t) => t.getAttribute("aria-label"))).toEqual([
      "Project a",
      "Project b",
      "Project c",
    ]);
  });
});

describe("ProjectTabStrip tabs (PJT-FR-XWPP)", () => {
  it("shows the full name, never shortened, as text and as accessible name", () => {
    const name = "A very long project name that no tab strip would ever shorten";
    renderStrip([win("a", { name }), win("b")]);
    const tab = screen.getByRole("tab", { name });
    expect(tab).toHaveTextContent(name);
  });

  it("marks only the tab of this window as selected", () => {
    renderStrip([win("a"), win("b")], "/work/b");
    expect(screen.getByRole("tab", { name: "Project a" })).toHaveAttribute(
      "aria-selected",
      "false",
    );
    expect(screen.getByRole("tab", { name: "Project b" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("shows the project id as tooltip", () => {
    renderStrip([win("a"), win("b")]);
    expect(screen.getByRole("tab", { name: "Project b" })).toHaveAttribute(
      "title",
      "/work/b",
    );
  });
});

describe("ProjectTabStrip activation and close (PJT-FR-JEGM)", () => {
  it("activates the project of a clicked tab", () => {
    const { onActivate } = renderStrip([win("a"), win("b")]);
    fireEvent.click(screen.getByRole("tab", { name: "Project b" }));
    expect(onActivate).toHaveBeenCalledWith("/work/b");
  });

  it("closes only the project of the clicked x", () => {
    const { onClose, onActivate } = renderStrip([win("a"), win("b")]);
    fireEvent.click(screen.getByRole("button", { name: "Close Project b" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onClose).toHaveBeenCalledWith("/work/b");
    expect(onActivate).not.toHaveBeenCalled();
  });
});

describe("ProjectTabStrip needs attention (PJT-FR-UXCT)", () => {
  it("shows a dot before the name of a flagged tab only", () => {
    renderStrip([win("a"), win("b", { needsAttention: true })]);
    const dots = screen.getAllByTestId("project-tab-dot");
    expect(dots).toHaveLength(1);
    const tab = screen.getByRole("tab", { name: "Project b" });
    expect(tab).toContainElement(dots[0]);
    expect(dots[0]).toHaveAccessibleName("needs attention");
    expect(tab.firstElementChild).toBe(dots[0]);
  });

  it("shows no dot when no project needs attention", () => {
    renderStrip([win("a"), win("b")]);
    expect(screen.queryByTestId("project-tab-dot")).not.toBeInTheDocument();
  });
});

describe("ProjectTabStrip keyboard (PJT-FR-LGBV)", () => {
  it("gives roving focus to the tab of this window only", () => {
    renderStrip([win("a"), win("b")], "/work/b");
    expect(screen.getByRole("tab", { name: "Project a" })).toHaveAttribute("tabindex", "-1");
    expect(screen.getByRole("tab", { name: "Project b" })).toHaveAttribute("tabindex", "0");
  });

  it("moves focus with the arrows and does not wrap", () => {
    renderStrip([win("a"), win("b"), win("c")]);
    const [a, b, c] = screen.getAllByRole("tab");
    a.focus();
    fireEvent.keyDown(a, { key: "ArrowLeft" });
    expect(a).toHaveFocus();
    fireEvent.keyDown(a, { key: "ArrowRight" });
    expect(b).toHaveFocus();
    fireEvent.keyDown(b, { key: "ArrowRight" });
    expect(c).toHaveFocus();
    fireEvent.keyDown(c, { key: "ArrowRight" });
    expect(c).toHaveFocus();
  });

  it("moves focus to the first and last tab with Home and End", () => {
    renderStrip([win("a"), win("b"), win("c")]);
    const [a, b, c] = screen.getAllByRole("tab");
    b.focus();
    fireEvent.keyDown(b, { key: "End" });
    expect(c).toHaveFocus();
    fireEvent.keyDown(c, { key: "Home" });
    expect(a).toHaveFocus();
  });

  it("activates the focused tab with Enter and with Space", () => {
    const { onActivate } = renderStrip([win("a"), win("b")]);
    const b = screen.getByRole("tab", { name: "Project b" });
    fireEvent.keyDown(b, { key: "Enter" });
    fireEvent.keyDown(b, { key: " " });
    expect(onActivate).toHaveBeenCalledTimes(2);
    expect(onActivate).toHaveBeenCalledWith("/work/b");
  });

  it("closes the focused tab with Delete", () => {
    const { onClose } = renderStrip([win("a"), win("b")]);
    fireEvent.keyDown(screen.getByRole("tab", { name: "Project a" }), { key: "Delete" });
    expect(onClose).toHaveBeenCalledWith("/work/a");
  });
});

describe("ProjectTabStrip scrolling (PJT-FR-VRRI)", () => {
  it("scrolls the tab of this window into view", () => {
    const scroll = vi.fn();
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = scroll;
    try {
      renderStrip([win("a"), win("b"), win("c")], "/work/c");
      expect(scroll).toHaveBeenCalled();
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });
});
