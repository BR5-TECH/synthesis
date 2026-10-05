import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { ReadyTasks } from "./GitReadyTasks";
import { formatInstant } from "../state/githubPolling";
import { Git } from "./Git";
import type { GithubPollingController } from "../hooks/useGithubPolling";
import {
  pendingClaim,
  pollingView,
  readyTask,
  shadowRow,
} from "../test/githubPollingFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

function controller(
  over: Partial<GithubPollingController> = {},
): GithubPollingController {
  return {
    view: pollingView(),
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
    ...over,
  };
}

let onOpenProjectSettings: Mock<() => void>;
let onOpenDraft: Mock<(draftId: string) => void>;
const show = (polling: GithubPollingController) =>
  render(
    <ReadyTasks
      polling={polling}
      onOpenProjectSettings={onOpenProjectSettings}
      onOpenDraft={onOpenDraft}
    />,
  );

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
  onOpenProjectSettings = vi.fn();
  onOpenDraft = vi.fn();
});
afterEach(cleanup);

describe("the Ready tasks section (GIT-FR-OGHO, GIT-FR-02)", () => {
  it("GIT-FR-02 / GIT-FR-OGHO: Ready tasks is a section of the Git panel with its count", async () => {
    const polling = controller({
      view: pollingView({ tasks: [readyTask(), readyTask({ issueNumber: 43 })] }),
    });
    render(
      <Git
        onSwitchWorktree={vi.fn()}
        canCheckOutBranches
        readyTasks={{ polling, onOpenProjectSettings }}
        onOpenDraft={onOpenDraft}
      />,
    );
    const tab = screen.getByRole("tab", { name: /Ready tasks/ });
    expect(within(tab).getByText("2")).toBeInTheDocument();
    expect(screen.queryByTestId("ready-tasks")).toBeNull();
    // GIT-FR-RYPO: the section is reachable by keyboard.
    tab.focus();
    await userEvent.keyboard("{Enter}");
    expect(screen.getByTestId("ready-tasks")).toBeInTheDocument();
    expect(screen.getByTestId("ready-tasks-summary")).toHaveTextContent("Roadmap");
  });

  it("GIT-FR-OGHO: the view is read when the section mounts", () => {
    const polling = controller();
    show(polling);
    expect(polling.reload).toHaveBeenCalledTimes(1);
  });
});

describe("rows (GIT-FR-LUSE, GIT-FR-OZYT, GIT-FR-OLNA)", () => {
  it("GIT-FR-LUSE: an unclaimed row shows title, owner/name, number, link, and Status", async () => {
    const polling = controller();
    show(polling);
    const row = screen.getByTestId("ready-task-42");
    expect(row).toHaveTextContent("Add retry to the sync job");
    expect(row).toHaveTextContent("acme/platform");
    expect(row).toHaveTextContent("#42");
    expect(row).toHaveTextContent("Ready");
    await userEvent.click(
      within(row).getByRole("button", { name: /Open issue #42/ }),
    );
    expect(polling.openTaskIssue).toHaveBeenCalledWith(42);
  });

  it("GIT-FR-OZYT: a shadow row shows its status in words and links to its issue and draft", async () => {
    const polling = controller({
      view: pollingView({ shadows: [shadowRow()] }),
    });
    show(polling);
    const row = screen.getByTestId("shadow-draft-gh-1");
    expect(row).toHaveTextContent("Cache invalidation");
    expect(row).toHaveTextContent("GitHub task, ready to graduate");
    expect(row).toHaveTextContent("acme/platform · #9");
    // Apart from the unclaimed rows.
    expect(screen.getByRole("list", { name: "GitHub drafts" })).toContainElement(row);
    await userEvent.click(within(row).getByRole("button", { name: /Open issue #9/ }));
    expect(polling.openShadowIssue).toHaveBeenCalledWith(
      "gh-1",
      "https://github.com/acme/platform/issues/9",
      9,
    );
    await userEvent.click(
      within(row).getByRole("button", { name: "Open draft Cache invalidation" }),
    );
    expect(onOpenDraft).toHaveBeenCalledWith("gh-1");
  });

  it("GIT-FR-OLNA: Graduate is offered on a github_shadow row no run holds, and nowhere else", async () => {
    const polling = controller({
      view: pollingView({
        shadows: [
          shadowRow(),
          shadowRow({ draftId: "gh-2", name: "Held", issueNumber: 10, locked: true }),
          shadowRow({ draftId: "gh-3", name: "Done", issueNumber: 11, status: "graduated" }),
        ],
      }),
    });
    show(polling);
    await userEvent.click(
      screen.getByRole("button", { name: "Graduate Cache invalidation (#9)" }),
    );
    expect(polling.graduateShadow).toHaveBeenCalledWith("gh-1", "Cache invalidation");
    expect(
      within(screen.getByTestId("shadow-draft-gh-2")).queryByRole("button", {
        name: /^Graduate/,
      }),
    ).toBeNull();
    expect(screen.getByTestId("shadow-draft-gh-2")).toHaveTextContent("Graduating");
    expect(screen.getByTestId("shadow-draft-gh-3")).toHaveTextContent("Graduated");
    expect(
      within(screen.getByTestId("shadow-draft-gh-3")).queryByRole("button", {
        name: /^Graduate/,
      }),
    ).toBeNull();
  });

  it("GIT-FR-OLNA / GIT-FR-FTHT: a pending claim renders its own row with Retry", async () => {
    const polling = controller({
      view: pollingView({ pendingClaims: [pendingClaim()] }),
    });
    show(polling);
    const row = screen.getByTestId("pending-claim-17");
    expect(row).toHaveTextContent("issue #17");
    await userEvent.click(
      within(row).getByRole("button", { name: "Retry claim of issue #17" }),
    );
    expect(polling.retry).toHaveBeenCalledWith(17);
  });

  it("GIT-FR-NQTZ: Claim and graduate claims that row's issue, and a refusal renders on that row", async () => {
    const polling = controller({
      rowErrors: new Map([[42, "This issue is no longer a ready Task on GitHub, so it was not claimed."]]),
    });
    show(polling);
    const row = screen.getByTestId("ready-task-42");
    expect(within(row).getByRole("alert")).toHaveTextContent(/no longer a ready Task/);
    await userEvent.click(
      within(row).getByRole("button", {
        name: "Claim and graduate #42 Add retry to the sync job",
      }),
    );
    expect(polling.claim).toHaveBeenCalledWith(42);
  });
});

describe("states (GIT-FR-HNGQ, GIT-FR-RUAH, GIT-FR-EZFL)", () => {
  it("GIT-FR-HNGQ: not configured routes to Project settings", async () => {
    show(
      controller({
        view: pollingView({
          settings: { projectNodeId: null, intervalMinutes: null },
          configuration: { state: "unset", errorCode: null, error: null, projectTitle: null },
          tasks: [],
        }),
      }),
    );
    const state = screen.getByTestId("ready-tasks-not-configured");
    expect(state).toHaveTextContent(/Not configured/);
    await userEvent.click(within(state).getByRole("button", { name: "Open Project settings" }));
    expect(onOpenProjectSettings).toHaveBeenCalled();
    expect(screen.queryByTestId("ready-tasks-empty")).toBeNull();
  });

  it("GIT-FR-HNGQ: a configuration error shows its text and routes to Project settings", async () => {
    show(
      controller({
        view: pollingView({
          configuration: {
            state: "invalid",
            errorCode: "status_field_missing",
            error: "The Project has no Status field.",
            projectTitle: "Roadmap",
          },
        }),
      }),
    );
    const state = screen.getByTestId("ready-tasks-config-error");
    expect(state).toHaveTextContent("The Project has no Status field.");
    expect(state).toHaveTextContent(/Polling is disabled/);
    expect(screen.queryByTestId("ready-task-42")).toBeNull();
    await userEvent.click(within(state).getByRole("button", { name: "Open Project settings" }));
    expect(onOpenProjectSettings).toHaveBeenCalled();
    // GIT-FR-EZFL: Refresh is unavailable while the configuration is invalid.
    expect(screen.getByRole("button", { name: /Refresh ready tasks/ })).toBeDisabled();
  });

  it("GIT-FR-HNGQ: loading while a poll is in flight and no rows exist", () => {
    show(controller({ view: pollingView({ tasks: [], lastSuccessAt: null, polling: true }) }));
    expect(screen.getByText("Loading ready tasks…")).toBeInTheDocument();
    expect(screen.queryByTestId("ready-tasks-empty")).toBeNull();
  });

  it("GIT-FR-HNGQ: empty when the last successful poll found no unclaimed task", () => {
    show(controller({ view: pollingView({ tasks: [] }) }));
    expect(screen.getByTestId("ready-tasks-empty")).toHaveTextContent(/No ready tasks/);
  });

  it("GIT-FR-RUAH: after a failed poll the rows stay, marked stale with the instant, under the error", () => {
    show(
      controller({
        view: pollingView({
          stale: true,
          lastErrorCode: "github_unreachable",
          lastError: "GitHub could not be reached.",
        }),
      }),
    );
    const note = screen.getByTestId("ready-tasks-stale-note");
    expect(note).toHaveTextContent("GitHub could not be reached.");
    expect(note).toHaveTextContent(
      `Stale: these rows are from the last successful poll, at ${formatInstant("2026-10-01T09:00:00Z")}.`,
    );
    expect(screen.getByTestId("ready-tasks-stale")).toBeInTheDocument();
    expect(screen.getByTestId("ready-task-42")).toBeInTheDocument();
    // The error renders above the rows.
    expect(
      note.compareDocumentPosition(screen.getByTestId("ready-task-42")) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("GIT-FR-RUAH: stale text never reads as an empty result", () => {
    show(
      controller({
        view: pollingView({
          tasks: [],
          stale: true,
          lastErrorCode: "github_unreachable",
          lastError: "GitHub could not be reached.",
        }),
      }),
    );
    expect(screen.queryByTestId("ready-tasks-empty")).toBeNull();
    expect(screen.getByTestId("ready-tasks-stale-note")).toHaveTextContent(/Stale/);
  });

  it("GIT-FR-EZFL: Refresh polls at once, with or without an interval", async () => {
    const polling = controller({
      view: pollingView({ settings: { projectNodeId: "P1", intervalMinutes: null } }),
    });
    show(polling);
    const refresh = screen.getByRole("button", { name: "Refresh ready tasks" });
    expect(refresh).toBeEnabled();
    await userEvent.click(refresh);
    expect(polling.refresh).toHaveBeenCalled();
  });

  it("GIT-FR-EZFL / GIT-FR-RYPO: Refresh is disabled and busy while a poll is in flight, in words and in state", () => {
    show(controller({ pollInFlight: true }));
    const refresh = screen.getByRole("button", { name: "Refreshing ready tasks" });
    expect(refresh).toBeDisabled();
    expect(refresh).toHaveAttribute("aria-busy", "true");
    expect(refresh).toHaveTextContent("Refreshing…");
  });

  it("GIT-FR-EZFL: Refresh is unavailable with no Project, and says why", () => {
    show(
      controller({
        view: pollingView({ settings: { projectNodeId: null, intervalMinutes: 5 } }),
      }),
    );
    const refresh = screen.getByRole("button", { name: /Refresh ready tasks \(unavailable: No GitHub Project is selected\)/ });
    expect(refresh).toBeDisabled();
  });
});

describe("accessibility and concurrency (GIT-FR-RYPO, GIT-FR-TZUI)", () => {
  it("GIT-FR-RYPO / GIT-FR-TZUI: a claim in flight states it in words and disables its own row alone", () => {
    const polling = controller({
      view: pollingView({
        tasks: [readyTask(), readyTask({ issueNumber: 43, title: "Second" })],
        pendingClaims: [pendingClaim()],
      }),
      rowBusy: new Map([[42, "claim" as const]]),
    });
    show(polling);
    const busy = screen.getByRole("button", {
      name: "Claiming #42 Add retry to the sync job",
    });
    expect(busy).toBeDisabled();
    expect(busy).toHaveAttribute("aria-busy", "true");
    expect(busy).toHaveTextContent("Claiming…");
    // The other actions of its own row are disabled too.
    expect(
      within(screen.getByTestId("ready-task-42")).getByRole("button", { name: /Open issue #42/ }),
    ).toBeDisabled();
    // Every other control stays usable.
    expect(screen.getByRole("button", { name: "Claim and graduate #43 Second" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Retry claim of issue #17" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Refresh ready tasks" })).toBeEnabled();
  });

  it("GIT-FR-RYPO: a retry in flight states it in words and in its accessible state", () => {
    show(
      controller({
        view: pollingView({ pendingClaims: [pendingClaim()] }),
        rowBusy: new Map([[17, "retry" as const]]),
      }),
    );
    const busy = screen.getByRole("button", { name: "Retrying claim of issue #17" });
    expect(busy).toBeDisabled();
    expect(busy).toHaveAttribute("aria-busy", "true");
  });

  it("GIT-FR-RYPO: every action is a keyboard-operable button", async () => {
    const polling = controller({ view: pollingView({ shadows: [shadowRow()] }) });
    show(polling);
    const claim = screen.getByRole("button", {
      name: "Claim and graduate #42 Add retry to the sync job",
    });
    claim.focus();
    await userEvent.keyboard("{Enter}");
    expect(polling.claim).toHaveBeenCalledWith(42);
    screen.getByRole("button", { name: "Graduate Cache invalidation (#9)" }).focus();
    await userEvent.keyboard(" ");
    expect(polling.graduateShadow).toHaveBeenCalled();
  });
});

describe("polls and actions together (GIT-FR-TZUI, GIT-FR-RYPO)", () => {
  it("GIT-FR-TZUI: while a poll is in flight, Claim, Retry and Graduate stay enabled and only Refresh is disabled", () => {
    show(
      controller({
        pollInFlight: true,
        view: pollingView({
          polling: true,
          pendingClaims: [pendingClaim()],
          shadows: [shadowRow()],
        }),
      }),
    );
    expect(
      screen.getByRole("button", { name: "Claim and graduate #42 Add retry to the sync job" }),
    ).toBeEnabled();
    expect(screen.getByRole("button", { name: "Retry claim of issue #17" })).toBeEnabled();
    expect(
      screen.getByRole("button", { name: "Graduate Cache invalidation (#9)" }),
    ).toBeEnabled();
    expect(screen.getByRole("button", { name: /Open issue #42/ })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Refreshing ready tasks" })).toBeDisabled();
  });

  it("GIT-FR-RYPO: Retry and Refresh are operable by keyboard", async () => {
    const polling = controller({ view: pollingView({ pendingClaims: [pendingClaim()] }) });
    show(polling);
    screen.getByRole("button", { name: "Retry claim of issue #17" }).focus();
    await userEvent.keyboard("{Enter}");
    expect(polling.retry).toHaveBeenCalledWith(17);
    screen.getByRole("button", { name: "Refresh ready tasks" }).focus();
    await userEvent.keyboard(" ");
    expect(polling.refresh).toHaveBeenCalledTimes(1);
    await userEvent.tab();
    expect(document.activeElement).not.toBe(document.body);
  });
});
