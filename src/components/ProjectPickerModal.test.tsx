import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const api = vi.hoisted(() => ({
  listRecentProjects: vi.fn(),
  resolveProjectAtPath: vi.fn(),
  openProjectAtPath: vi.fn(),
  centerPicker: vi.fn(),
}));
vi.mock("../api", () => api);

const window_ = vi.hoisted(() => ({ setSize: vi.fn(), setResizable: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => window_,
  LogicalSize: class {},
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

import { ProjectPickerModal } from "./ProjectPickerModal";

const handle = {
  name: "design",
  path: "/work/design",
  activeWorktreePath: "/work/design",
  rememberedWorktreeUnavailable: false,
};

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset();
  api.listRecentProjects.mockResolvedValue([
    { name: "design", path: "/work/design", pinned: false, missing: false, lastOpened: "2026-01-01T00:00:00Z" },
  ]);
  api.resolveProjectAtPath.mockResolvedValue(handle);
  api.centerPicker.mockResolvedValue(undefined);
  window_.setSize.mockReset();
  window_.setResizable.mockReset();
});

afterEach(cleanup);

describe("the Project picker modal (PPK-FR-GMYH)", () => {
  it("is a dialog named Project picker over the window", async () => {
    render(<ProjectPickerModal onChoose={vi.fn()} onDismiss={vi.fn()} />);
    expect(screen.getByRole("dialog", { name: "Project picker" })).toBeInTheDocument();
    await screen.findByText("design");
  });

  it("leaves the size of the window alone", async () => {
    render(<ProjectPickerModal onChoose={vi.fn()} onDismiss={vi.fn()} />);
    await screen.findByText("design");
    expect(window_.setSize).not.toHaveBeenCalled();
    expect(api.centerPicker).not.toHaveBeenCalled();
  });

  it("resolves a chosen project and mounts nothing", async () => {
    const onChoose = vi.fn();
    render(<ProjectPickerModal onChoose={onChoose} onDismiss={vi.fn()} />);
    fireEvent.click(await screen.findByText("design"));
    await waitFor(() => expect(onChoose).toHaveBeenCalledWith(handle));
    expect(api.resolveProjectAtPath).toHaveBeenCalledWith("/work/design");
    expect(api.openProjectAtPath).not.toHaveBeenCalled();
  });

  it("is dismissed by the close control and by Escape, and chooses nothing", async () => {
    const onChoose = vi.fn();
    const onDismiss = vi.fn();
    render(<ProjectPickerModal onChoose={onChoose} onDismiss={onDismiss} />);
    await screen.findByText("design");
    fireEvent.click(screen.getByRole("button", { name: "Close project picker" }));
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onDismiss).toHaveBeenCalledTimes(2);
    expect(onChoose).not.toHaveBeenCalled();
  });

  it("shows the error of a failed choice inline", async () => {
    render(<ProjectPickerModal onChoose={vi.fn()} onDismiss={vi.fn()} error="not a project" />);
    expect(screen.getByRole("alert")).toHaveTextContent("not a project");
    await screen.findByText("design");
  });
});
