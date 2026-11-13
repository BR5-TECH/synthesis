/**
 * The Drafts panel row of a propose run's draft (`DRP-drafts-panel.md`
 * DRP-FR-YGOU).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, within } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));

import { flat, openDraftMenu, renderPanel } from "../test/draftsPanelFixtures";
import { resetLogBufferForTest } from "../logging";
import { resetPanelReveals } from "../state/panelReveal";
import { configureOpenSpecChangeRoutes } from "../state/openspec/changeRoutes";
import type { DraftGraduation, DraftSummary, GraduationRunState } from "../types";

const open = vi.fn();

const row = (
  name: string,
  state: GraduationRunState,
  change: DraftGraduation["openspecChange"],
): DraftSummary => ({
  id: `d-${name}`,
  name,
  status: state === "completed" ? "graduated" : "active",
  folder: "",
  updatedAt: "2026-07-31T08:00:00Z",
  graduation: {
    runId: `r-${name}`,
    state,
    locked: state !== "completed",
    graduated: state === "completed",
    openspecChange: change,
  },
});

const CHANGE = { name: "add-logout-3f2a", streamId: "s-7" };

let rows: DraftSummary[];

beforeEach(() => {
  resetLogBufferForTest();
  resetPanelReveals();
  open.mockReset();
  configureOpenSpecChangeRoutes({ open });
  rows = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_drafts") return flat(rows);
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});
afterEach(() => {
  configureOpenSpecChangeRoutes(null);
  cleanup();
});

const menuLabels = () =>
  within(screen.getByRole("menu"))
    .getAllByRole("menuitem")
    .map((n) => n.textContent);

describe("the row of a propose run's draft", () => {
  it("DRP-FR-YGOU: a completed propose run adds Open change below the divider, which opens the change on its stream", async () => {
    rows = [row("proposed", "completed", CHANGE)];
    renderPanel();
    await screen.findByText("proposed");
    await act(async () => void openDraftMenu("proposed"));
    const labels = menuLabels();
    expect(labels.slice(-2)).toEqual(["Go to run", "Open change"]);
    const sep = screen.getByRole("menu").querySelector(".menu-sep");
    expect(sep?.nextElementSibling).toHaveTextContent("Go to run");
    await act(async () => {
      within(screen.getByRole("menu")).getByText("Open change").click();
    });
    expect(open).toHaveBeenCalledWith({ kind: "stream", streamId: "s-7" }, "add-logout-3f2a", {});
  });

  it("DRP-FR-YGOU: a propose run that is not completed adds no Open change", async () => {
    rows = [row("working", "working", CHANGE)];
    renderPanel();
    await screen.findByText("working");
    await act(async () => void openDraftMenu("working"));
    expect(menuLabels()).not.toContain("Open change");
  });

  it("DRP-FR-YGOU: a completed run that is no propose run adds no Open change", async () => {
    rows = [row("plain", "completed", null)];
    renderPanel();
    await screen.findByText("plain");
    await act(async () => void openDraftMenu("plain"));
    expect(menuLabels()).not.toContain("Open change");
    expect(menuLabels()).toContain("Go to run");
  });
});
