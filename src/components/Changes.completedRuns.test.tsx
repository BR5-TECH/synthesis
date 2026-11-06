/**
 * The read-only Completed runs list below the Changes footer
 * (`../../specifications/ui/CHG-changes.md` CHG-FR-QLZO, CHG-FR-SUTJ,
 * CHG-FR-GXNP, CHG-FR-KIPM).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  screen,
  waitFor,
  waitForElementToBeRemoved,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetPanelReveals } from "../state/panelReveal";
import { readStylesheet } from "../test/readStylesheet";
import { pickSelector } from "../test/selectors";
import {
  completedRun,
  cssRule,
  makeBackend,
  renderPanel,
  resetHandlers,
  type Backend,
} from "../test/changesFixtures";
import { ERR_NOT_A_REPOSITORY } from "../comparison";
import type { BranchCompletedRun } from "../types";

const invokeMock = vi.fn();
let listeners: Record<string, ((event: { payload: unknown }) => void)[]> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: unknown }) => void) => {
    (listeners[event] ??= []).push(cb);
    return () => {
      listeners[event] = (listeners[event] ?? []).filter((fn) => fn !== cb);
    };
  }),
}));

const backend = makeBackend(invokeMock);
const reads = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "list_branch_completed_runs");

beforeEach(() => {
  resetLogBufferForTest();
  resetPanelReveals();
  invokeMock.mockReset();
  resetAppPreferencesCache();
  resetHandlers();
  listeners = {};
});
afterEach(cleanup);

const list = () => screen.queryByRole("region", { name: "Completed runs" });
const rowTexts = () =>
  within(list()!)
    .getAllByRole("listitem")
    .map((li) => li.textContent);

/** Fire one backend event at every listener the panel holds for it. */
function emit(event: string, payload: unknown = {}) {
  act(() => {
    for (const cb of listeners[event] ?? []) cb({ payload });
  });
}

/** Wait until the list subscribed to `event`, then fire it. */
async function fire(event: string, payload: unknown = {}) {
  await waitFor(() => expect(listeners[event]?.length ?? 0).toBeGreaterThan(0));
  emit(event, payload);
}

describe("Completed runs", () => {
  it("CHG-FR-QLZO, GRD-FR-BHFP: below the footer, rows name the title, the kind and when the work landed, in landing order, and the heading states the count", async () => {
    backend({
      completedRuns: [
        completedRun("g1", { title: "Voice playback", hoursAgo: 2 }),
        completedRun("g2", { title: "Audio devices", kind: "direct", hoursAgo: 1 }),
      ],
    });
    const { container } = renderPanel();
    const region = await screen.findByRole("region", { name: "Completed runs" });

    expect(region).toHaveTextContent(/^Completed runs · 2/);
    expect(rowTexts()).toEqual([
      "Voice playbackdraft · landed 2h ago",
      "Audio devicesdirect · landed 1h ago",
    ]);
    // Below the footer: the list follows the footer row in the panel.
    const footer = container.querySelector(".changes-actions")!;
    expect(footer.compareDocumentPosition(region) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(reads()[0]?.[1]).toEqual({ branch: "feature/x" });
  });

  it("CHG-FR-SUTJ: renders in both modes", async () => {
    backend({ completedRuns: [completedRun("g1")] });
    renderPanel();
    await screen.findByRole("region", { name: "Completed runs" });
    await pickSelector("Comparison mode", "branch");
    await screen.findByText("CHG-changes.md");
    expect(list()).toBeInTheDocument();
  });

  it("CHG-FR-SUTJ: absent on a branch that holds no completed run", async () => {
    let runs: BranchCompletedRun[] = [completedRun("g1")];
    backend({ completedRuns: () => runs });
    renderPanel();
    await screen.findByText("Run g1");
    runs = [];
    await fire("changes-updated");
    await waitForElementToBeRemoved(() => list());
    expect(screen.queryByText("Completed runs")).toBeNull();
  });

  it("CHG-FR-SUTJ: absent on a detached HEAD, and no list is read", async () => {
    let branch: string | null = "feature/x";
    backend({ completedRuns: [completedRun("g1")] });
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_worktrees_and_branches") {
        const context = await base(cmd, args);
        const active = context.worktrees[0];
        if (branch === null) {
          delete active.branch;
          active.isDetached = true;
        }
        return context;
      }
      return base(cmd, args);
    });
    renderPanel();
    await screen.findByText("Run g1");
    const readsBefore = reads().length;
    branch = null;
    await fire("worktree-context-changed", { activeWorktreePath: "/repo", isDetached: true });
    await waitForElementToBeRemoved(() => list());
    expect(reads()).toHaveLength(readsBefore);
  });

  it("CHG-FR-SUTJ: a branch that cannot be read says the list could not be read", async () => {
    let failing = false;
    backend({ completedRuns: [completedRun("g1")] });
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_worktrees_and_branches" && failing) throw new Error("no worktree");
      return base(cmd, args);
    });
    renderPanel();
    await screen.findByText("Run g1");
    failing = true;
    await fire("graduation-queue-changed");
    await screen.findByText("The completed runs could not be read.");
    expect(screen.queryByText("Run g1")).toBeNull();
  });

  it("CHG-FR-SUTJ: absent in the not-a-repository state", async () => {
    backend({
      fail: { list_uncommitted_changes: ERR_NOT_A_REPOSITORY },
      completedRuns: [completedRun("g1")],
    });
    renderPanel();
    await screen.findByText("Not a Git repository.");
    expect(list()).toBeNull();
  });

  it("CHG-FR-SUTJ: a read that fails says so and shows no rows", async () => {
    backend({ fail: { list_branch_completed_runs: "unknown branch" } });
    renderPanel();
    const region = await screen.findByRole("region", { name: "Completed runs" });
    expect(within(region).getByRole("status")).toHaveTextContent(
      "The completed runs could not be read.",
    );
    expect(within(region).queryAllByRole("listitem")).toHaveLength(0);
  });

  it("CHG-FR-SUTJ: a failed read after rows were shown takes the rows away", async () => {
    let fail = false;
    backend({
      completedRuns: () => {
        if (fail) throw new Error("unknown branch");
        return [completedRun("g1")];
      },
    });
    renderPanel();
    await screen.findByText("Run g1");
    fail = true;
    await fire("graduation-run-changed", { runId: "g1", state: "completed" });
    await screen.findByText("The completed runs could not be read.");
    expect(screen.queryByText("Run g1")).toBeNull();
  });

  it.each([
    ["graduation-run-changed", { runId: "g2", state: "completed" }],
    ["graduation-queue-changed", { projectKey: "p", streamId: "", worktreePath: "" }],
    ["changes-updated", {}],
  ])("CHG-FR-GXNP: reads again on %s", async (event, payload) => {
    let runs: BranchCompletedRun[] = [completedRun("g1")];
    backend({ completedRuns: () => runs });
    renderPanel();
    await screen.findByText("Run g1");
    const before = reads().length;
    runs = [completedRun("g1"), completedRun("g2")];
    await fire(event, payload);
    await screen.findByText("Run g2");
    expect(reads().length).toBeGreaterThan(before);
    expect(list()).toHaveTextContent(/^Completed runs · 2/);
  });

  it("CHG-FR-GXNP: reads again for the new branch when the current branch changes", async () => {
    let branch = "feature/x";
    const config: Backend = {
      completedRuns: (asked) =>
        asked === "feature/x" ? [completedRun("old")] : [completedRun("new")],
    };
    backend(config);
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_worktrees_and_branches") {
        const context = await base(cmd, args);
        context.worktrees[0].branch = branch;
        return context;
      }
      return base(cmd, args);
    });
    renderPanel();
    await screen.findByText("Run old");
    branch = "feature/y";
    await fire("worktree-context-changed", {
      activeWorktreePath: "/repo",
      branch: "feature/y",
      isDetached: false,
    });
    await screen.findByText("Run new");
    expect(screen.queryByText("Run old")).toBeNull();
    expect(reads()[reads().length - 1]?.[1]).toEqual({ branch: "feature/y" });
  });

  it("CHG-FR-GXNP: a response for a branch that is no longer current is discarded", async () => {
    let branch = "feature/x";
    let releaseOld: (runs: BranchCompletedRun[]) => void = () => {};
    backend({
      completedRuns: (asked) =>
        asked === "feature/x"
          ? new Promise<BranchCompletedRun[]>((resolve) => (releaseOld = resolve))
          : [completedRun("new")],
    });
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "list_worktrees_and_branches") {
        const context = await base(cmd, args);
        context.worktrees[0].branch = branch;
        return context;
      }
      return base(cmd, args);
    });
    renderPanel();
    await waitFor(() => expect(reads()).toHaveLength(1));

    branch = "feature/y";
    await fire("worktree-context-changed", {
      activeWorktreePath: "/repo",
      branch: "feature/y",
      isDetached: false,
    });
    await screen.findByText("Run new");
    // The read for the old branch answers last, and changes nothing.
    await act(async () => {
      releaseOld([completedRun("old")]);
      // One macrotask, so every promise of the old read has settled.
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(screen.queryByText("Run old")).toBeNull();
    expect(screen.getByText("Run new")).toBeInTheDocument();
  });

  it("CHG-FR-KIPM: offers no control, and a row is not actionable", async () => {
    backend({ completedRuns: [completedRun("g1"), completedRun("g2")] });
    renderPanel();
    const region = await screen.findByRole("region", { name: "Completed runs" });
    expect(within(region).queryAllByRole("button")).toHaveLength(0);
    expect(within(region).queryAllByRole("link")).toHaveLength(0);
    for (const row of within(region).getAllByRole("listitem")) {
      expect(row).not.toHaveAttribute("tabindex");
      expect(row).not.toHaveAttribute("role", "button");
    }
    const callsBefore = invokeMock.mock.calls.length;
    await userEvent.click(screen.getByText("Run g1"));
    expect(invokeMock.mock.calls.slice(callsBefore).map((c) => c[0])).not.toContain(
      "get_graduation_run",
    );
  });

  it("CHG-FR-KIPM: shows at most five rows at a time and scrolls itself, so the tree keeps its height", async () => {
    backend({
      completedRuns: Array.from({ length: 8 }, (_, i) => completedRun(`g${i}`)),
    });
    renderPanel();
    await screen.findByRole("region", { name: "Completed runs" });
    expect(within(list()!).getAllByRole("listitem")).toHaveLength(8);

    const css = readStylesheet("components.css");
    const section = cssRule(css, ".changes-completed");
    expect(section).toMatch(/flex\s*:\s*none/);
    expect(section).toMatch(/--changes-completed-row-h\s*:\s*\d+px/);
    const rows = cssRule(css, ".changes-completed__list");
    expect(rows).toMatch(/max-height\s*:\s*calc\(5 \* var\(--changes-completed-row-h\)\)/);
    expect(rows).toMatch(/overflow-y\s*:\s*auto/);
    expect(cssRule(css, ".changes-completed__row")).toMatch(
      /(^|[\s;])height\s*:\s*var\(--changes-completed-row-h\)/,
    );
  });
});
