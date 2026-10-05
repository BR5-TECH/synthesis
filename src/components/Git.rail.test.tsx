import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Git } from "./Git";
import type { ReadyTasksBinding } from "./GitReadyTasks";
import { pollingView, readyTask } from "../test/githubPollingFixtures";

// The left rail's section controls (GIT-FR-02, GIT-FR-WQHD).

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
});
afterEach(cleanup);

const readyTasks: ReadyTasksBinding = {
  polling: {
    view: pollingView({ tasks: [readyTask(), readyTask(), readyTask()] }),
    readError: null,
    pollInFlight: false,
    refreshError: null,
    rowBusy: new Map(),
    rowErrors: new Map(),
    reload: vi.fn(async () => {}),
    refresh: vi.fn(async () => {}),
    claim: vi.fn(async () => {}),
    retry: vi.fn(async () => {}),
    graduateShadow: vi.fn(),
    openTaskIssue: vi.fn(),
    openShadowIssue: vi.fn(),
  },
  onOpenProjectSettings: () => {},
};

describe("the section controls (GIT-FR-02, GIT-FR-WQHD)", () => {
  it("GIT-FR-02, GIT-FR-WQHD: shows all five controls with their full labels and the count badge", () => {
    render(
      <Git
        onSwitchWorktree={vi.fn(async () => ({ ok: true as const }))}
        canCheckOutBranches
        readyTasks={readyTasks}
      />,
    );
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual([
      "Commits",
      "Branches",
      "PRs",
      "Ready tasks3",
      "Logs",
    ]);
    // Exactly one section shows at a time.
    expect(tabs.filter((t) => t.getAttribute("aria-selected") === "true")).toHaveLength(1);
  });

  it("GIT-FR-WQHD: the controls carry no scroll, clip or truncation of their own", () => {
    render(
      <Git
        onSwitchWorktree={vi.fn(async () => ({ ok: true as const }))}
        canCheckOutBranches
        readyTasks={readyTasks}
      />,
    );
    const list = screen.getByRole("tablist", { name: "Git sections" });
    // The wrapping is the stylesheet's (`style-invariants.git.test.ts`); the
    // markup must not undo it with an inline overflow.
    expect(list.className).toContain("git__tabs");
    for (const el of [list, ...screen.getAllByRole("tab")]) {
      expect(el.style.overflow).toBe("");
      expect(el.style.overflowX).toBe("");
      expect(el.style.textOverflow).toBe("");
    }
  });

  it("GIT-FR-WQHD, GIT-FR-RYPO: every control keeps its keyboard operation", async () => {
    render(
      <Git
        onSwitchWorktree={vi.fn(async () => ({ ok: true as const }))}
        canCheckOutBranches
        readyTasks={readyTasks}
      />,
    );
    for (const name of ["Branches", "PRs", "Logs", "Commits"]) {
      const tab = screen.getByRole("tab", { name });
      tab.focus();
      await userEvent.keyboard("{Enter}");
      expect(tab).toHaveAttribute("aria-selected", "true");
    }
    const ready = screen.getByRole("tab", { name: /Ready tasks/ });
    ready.focus();
    await userEvent.keyboard(" ");
    expect(ready).toHaveAttribute("aria-selected", "true");
  });
});
