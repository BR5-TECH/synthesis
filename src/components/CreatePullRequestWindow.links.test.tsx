/**
 * The Create a PR window links the pull request to its tasks and features,
 * states when it is stacked, and keeps what the author did per head branch
 * (`../../specifications/ui/CPR-create-pull-request.md` CPR-FR-IDRP,
 * CPR-FR-GDDO, CPR-FR-PXQN, CPR-FR-BLEN, CPR-FR-XMRL, CPR-FR-IPKX,
 * CPR-FR-JBPQ, CPR-FR-SSQI, CPR-FR-ESXZ, CPR-FR-SQEP, CPR-FR-FFQD).
 *
 * The backend is a mocked `invoke` that answers by command name and rejects
 * every command it does not expect. Nothing here depends on elapsed time: each
 * wait is for a state.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { CreatePullRequestWindow } from "./CreatePullRequestWindow";
import type { PullRequestLinkResult } from "./CreatePullRequest/linking";
import type { PullRequestSource } from "./CreatePullRequest/types";
import { resetLogBufferForTest } from "../logging";
import { getPullRequestSession } from "../state/pullRequestSessions";
import {
  branchRun,
  linkedAll,
  PUBLICATION_AVAILABLE,
  pullRequestWindowRead,
  resetCreatePullRequestStores,
} from "../test/createPullRequestFixtures";
import type {
  BranchCompletedRun,
  CreatedPullRequest,
  GitBranch,
  PublicationMetadata,
  PullRequestHeadState,
  PullRequestIssueRef,
  PullRequestSummary,
  StackStep,
} from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const logged = vi.fn();
vi.mock("../logging", async (original) => {
  const real = await original<typeof import("../logging")>();
  return {
    ...real,
    logInfo: (...args: unknown[]) => logged("info", ...args),
    logWarn: (...args: unknown[]) => logged("warn", ...args),
  };
});

const { handlers } = vi.hoisted(() => ({
  handlers: [] as Array<{ name: string; fn: (e: { payload: unknown }) => void }>,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, fn: (e: { payload: unknown }) => void) => {
    const entry = { name, fn };
    handlers.push(entry);
    return () => {
      const at = handlers.indexOf(entry);
      if (at >= 0) handlers.splice(at, 1);
    };
  }),
}));
const fire = (name: string, payload: unknown) => {
  for (const h of handlers.filter((h) => h.name === name)) h.fn({ payload });
};

const HEAD = "feature/panel";
const SOURCE: PullRequestSource = { head: HEAD, base: "main", title: "panel" };

const READY: PullRequestHeadState = {
  head: HEAD,
  base: "main",
  hasRemote: true,
  remoteBranchExists: true,
  unpushed: 0,
  uncommittedPaths: [],
  aheadOfBase: 3,
};

const BRANCHES: GitBranch[] = [
  { name: "main", kind: "local", isCurrent: false },
  { name: "feature/pr-tools", kind: "local", isCurrent: false },
  { name: HEAD, kind: "local", isCurrent: true },
];

function metadata(parents: { number: number; title: string }[]): PublicationMetadata {
  const list = <T,>(items: T[]) => ({ state: "loaded" as const, items, errorCode: null, error: null });
  return {
    repositoryOwner: "acme",
    repositoryName: "platform",
    settings: {
      parentIssueTypes: ["Feature"],
      subIssueType: "Task",
      subIssueMilestonePolicy: "inherit_parent",
      rootIssueType: null,
      noteIssueType: "Feature",
    },
    parents: list(
      parents.map((p) => ({ ...p, issueType: "Feature", url: `https://github.com/acme/platform/issues/${p.number}`, milestone: null })),
    ),
    issueTypes: list([]),
    milestones: list([]),
    labels: list([]),
    subIssueType: { name: "Task", resolved: "Task" },
    rootIssueType: null,
    noteIssueType: null,
  };
}

function publication(owner: string, repository: string, issueNumber: number) {
  return {
    current: {
      provider: "github",
      repositoryOwner: owner,
      repositoryName: repository,
      issueNumber,
      issueUrl: `https://github.com/${owner}/${repository}/issues/${issueNumber}`,
      publishedAt: "2026-09-06T10:00:00Z",
      marker: "m",
    },
    history: [],
    attempt: null,
    eligibility: { publishable: true, reasonCode: null, reason: null, localAssets: [] },
  };
}

let completed: BranchCompletedRun[];
let publications: Record<string, unknown>;
let openPulls: () => Promise<PullRequestSummary[]>;
let availability: unknown;
let cached: PublicationMetadata | null;
let refreshed: PublicationMetadata | null;
let link: (args: Record<string, unknown>) => Promise<unknown>;
let create: () => Promise<unknown>;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  resetCreatePullRequestStores();
  handlers.length = 0;
  logged.mockReset();
  completed = [];
  publications = {};
  openPulls = async () => [];
  availability = PUBLICATION_AVAILABLE;
  cached = metadata([
    { number: 12, title: "Git flow" },
    { number: 14, title: "Voice" },
  ]);
  refreshed = null;
  link = async (args) => linkedAll(args.issues as PullRequestIssueRef[]);
  create = async () => ({ number: 42, url: "https://github.com/acme/platform/pull/42" });
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "list_branches":
        return BRANCHES;
      case "get_pull_request_head_state":
        return READY;
      case "list_branch_completed_runs":
        return completed;
      case "get_draft_publication":
        return publications[args.draftId as string] ?? Promise.reject("draft_not_found");
      case "list_pull_requests":
        return openPulls();
      case "get_note_publication_availability":
        return availability;
      case "get_publication_metadata_cache":
        return cached;
      case "refresh_publication_metadata":
        return {
          outcome: refreshed ? "changed" : "unchanged",
          errorCode: null,
          error: null,
          remoteName: "origin",
          metadata: refreshed,
        };
      case "link_pull_request_issues":
        return link(args);
      case "create_pull_request":
        return create();
      case "append_log_records":
        return undefined;
      default: {
        const read = pullRequestWindowRead(cmd, args);
        if (read) return read.answer;
        return Promise.reject(new Error(`unexpected command ${cmd}`));
      }
    }
  });
});

afterEach(cleanup);

function mount() {
  const onCreated = vi.fn<
    (c: CreatedPullRequest, l: PullRequestLinkResult | null, s?: StackStep | null) => void
  >();
  const onClose = vi.fn();
  const view = render(
    <CreatePullRequestWindow
      source={SOURCE}
      onClose={onClose}
      onCreated={onCreated}
      onRequestToken={vi.fn()}
      onOpenGlobalSettings={vi.fn()}
      onSubmittingChange={vi.fn()}
    />,
  );
  return { ...view, onCreated, onClose };
}

const submit = () => screen.getByTestId("create-pr-submit");
const tasks = () => screen.getByTestId("create-pr-tasks");
const ready = async () => {
  await waitFor(() => expect(screen.queryByTestId("create-pr-loading")).toBeNull());
  await waitFor(() => expect(tasks()).not.toHaveTextContent("Reading the tasks"));
  await waitFor(() => expect(submit()).toBeEnabled());
};

/** Pick the feature `#n` in the Features selector. */
async function pickFeature(name: RegExp) {
  await userEvent.click(await screen.findByTestId("create-pr-features-select"));
  await userEvent.click(await screen.findByRole("option", { name }));
}

describe("Tasks (CPR-FR-IDRP)", () => {
  it("CPR-FR-IDRP: names the issue of each published draft of the branch's completed runs, each draft read once", async () => {
    completed = [
      branchRun("r1", { draftId: "d1", draftName: "PR summary run" }),
      branchRun("r2", { draftId: "d1", draftName: "PR summary run" }),
      branchRun("r3", { draftId: "d2", draftName: "Issue links" }),
      branchRun("r4", { draftId: "d3", draftName: "Not published" }),
    ];
    publications = {
      d1: publication("acme", "platform", 31),
      d2: publication("acme", "platform", 32),
      d3: { ...publication("acme", "platform", 1), current: null },
    };
    mount();
    await ready();
    expect(calls("list_branch_completed_runs")[0][1]).toEqual({ branch: HEAD });
    expect(calls("get_draft_publication").map((c) => c[1])).toEqual([
      { draftId: "d1" },
      { draftId: "d2" },
      { draftId: "d3" },
    ]);
    const items = within(tasks()).getAllByRole("listitem").map((li) => li.textContent);
    expect(items).toEqual(["#31 PR summary run", "#32 Issue links"]);
  });

  it("CPR-FR-IDRP: a branch with no published draft says that no task is linked", async () => {
    completed = [branchRun("r1", { draftId: "d9" })];
    mount();
    await ready();
    expect(screen.getByTestId("create-pr-tasks-empty")).toHaveTextContent("No task is linked.");
  });
});

describe("Features (CPR-FR-GDDO, CPR-FR-PXQN)", () => {
  it("CPR-FR-GDDO, CPR-FR-PXQN: offers the parent issues of the cache of the named remote, refreshes once, and holds several selections", async () => {
    mount();
    await ready();
    expect(calls("get_publication_metadata_cache")[0][1]).toEqual({ remoteName: "origin" });
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    await pickFeature(/#12 Git flow/);
    await pickFeature(/#14 Voice/);
    const chips = within(screen.getByTestId("create-pr-features")).getAllByRole("listitem");
    expect(chips.map((c) => c.textContent)).toEqual(["#12 Git flow", "#14 Voice"]);
    expect(getPullRequestSession(HEAD)?.features.map((f) => f.number)).toEqual([12, 14]);
    await userEvent.click(screen.getByRole("button", { name: "Remove feature #12" }));
    expect(getPullRequestSession(HEAD)?.features.map((f) => f.number)).toEqual([14]);
  });

  it("CPR-FR-GDDO: filters by number and by title", async () => {
    mount();
    await ready();
    await userEvent.click(await screen.findByTestId("create-pr-features-select"));
    await userEvent.keyboard("voi");
    expect(screen.queryByRole("option", { name: /#12 Git flow/ })).toBeNull();
    expect(screen.getByRole("option", { name: /#14 Voice/ })).toBeInTheDocument();
    await userEvent.clear(document.activeElement as HTMLInputElement);
    await userEvent.keyboard("#12");
    expect(screen.getByRole("option", { name: /#12 Git flow/ })).toBeInTheDocument();
    expect(screen.queryByRole("option", { name: /#14 Voice/ })).toBeNull();
  });

  it("CPR-FR-PXQN: the refresh and the change event replace the offered issues", async () => {
    cached = null;
    refreshed = metadata([{ number: 20, title: "Refreshed" }]);
    const h = mount();
    await ready();
    await userEvent.click(await screen.findByTestId("create-pr-features-select"));
    expect(await screen.findByRole("option", { name: /#20 Refreshed/ })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    // The Escape of the open list closed the list alone, not the window.
    expect(h.onClose).not.toHaveBeenCalled();
    cached = metadata([{ number: 21, title: "From the cache" }]);
    act(() => fire("publication-metadata-changed", { remoteName: "origin", metadata: null }));
    await waitFor(() => expect(calls("get_publication_metadata_cache")).toHaveLength(2));
    expect(calls("get_publication_metadata_cache")[1][1]).toEqual({ remoteName: "origin" });
    await userEvent.click(screen.getByTestId("create-pr-features-select"));
    expect(await screen.findByRole("option", { name: /#21 From the cache/ })).toBeInTheDocument();
    // An event of another remote reads nothing.
    act(() => fire("publication-metadata-changed", { remoteName: "upstream", metadata: null }));
    expect(calls("get_publication_metadata_cache")).toHaveLength(2);
  });

  it("CPR-FR-PXQN: when publication is not available the selector is unavailable and states the reason", async () => {
    availability = { ...PUBLICATION_AVAILABLE, available: false, remoteName: null, reason: "The project has no GitHub remote." };
    mount();
    await ready();
    expect(screen.getByTestId("create-pr-features-select")).toBeDisabled();
    expect(screen.getByTestId("create-pr-features-unavailable")).toHaveTextContent(
      "Features are unavailable: The project has no GitHub remote.",
    );
    expect(calls("get_publication_metadata_cache")).toHaveLength(0);
  });
});

describe("linking on submit (CPR-FR-BLEN, CPR-FR-XMRL, CPR-FR-ESXZ, CPR-FR-FFQD)", () => {
  beforeEach(() => {
    completed = [branchRun("r1", { draftId: "d1", draftName: "Task one" })];
    publications = { d1: publication("acme", "platform", 31) };
  });

  it("CPR-FR-BLEN, CPR-FR-ESXZ: after the creation links the tasks, then the features, each once, then clears the saved state", async () => {
    const h = mount();
    await ready();
    await pickFeature(/#12 Git flow/);
    await userEvent.click(submit());
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
    expect(calls("link_pull_request_issues")[0][1]).toEqual({
      number: 42,
      issues: [
        { owner: "acme", repository: "platform", number: 31 },
        { owner: "acme", repository: "platform", number: 12 },
      ],
    });
    expect(h.onCreated.mock.calls[0][1]).toEqual({
      linked: [
        { owner: "acme", repository: "platform", number: 31 },
        { owner: "acme", repository: "platform", number: 12 },
      ],
      failed: [],
      closesOnMerge: true,
    });
    expect(getPullRequestSession(HEAD)).toBeNull();
  });

  it("CPR-FR-XMRL: Submit says Linking the issues… while the link runs, and nothing can be used", async () => {
    let release: (v: unknown) => void = () => {};
    link = () => new Promise((resolve) => (release = resolve));
    const h = mount();
    await ready();
    await userEvent.click(submit());
    await waitFor(() => expect(submit()).toHaveTextContent("Linking the issues…"));
    expect(submit()).toBeDisabled();
    expect(screen.getByTestId("create-pr-generate")).toBeDisabled();
    expect(screen.getByLabelText("Title")).toBeDisabled();
    expect(screen.getByTestId("create-pr-features-select")).toBeDisabled();
    await userEvent.keyboard("{Escape}");
    expect(h.onClose).not.toHaveBeenCalled();
    release(linkedAll([{ owner: "acme", repository: "platform", number: 31 }]));
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
  });

  it("CPR-FR-BLEN: a link that fails does not fail the submission, and every issue is reported as not linked", async () => {
    link = async () => Promise.reject("github_unreachable");
    const h = mount();
    await ready();
    await userEvent.click(submit());
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
    expect(h.onCreated.mock.calls[0][1]).toEqual({
      linked: [],
      failed: [{ owner: "acme", repository: "platform", number: 31 }],
      closesOnMerge: null,
    });
    expect(screen.queryByTestId("create-pr-failure")).toBeNull();
  });

  it("CPR-FR-BLEN, CPR-FR-HGXL: Submit waits for the tasks, so a quick Submit still links them", async () => {
    let release: (v: unknown) => void = () => {};
    publications = {};
    const pending = new Promise((resolve) => (release = resolve));
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === "get_draft_publication") {
        await pending;
        return publication("acme", "platform", 31);
      }
      if (cmd === "list_branches") return BRANCHES;
      if (cmd === "get_pull_request_head_state") return READY;
      if (cmd === "list_branch_completed_runs") return completed;
      if (cmd === "create_pull_request") return create();
      if (cmd === "link_pull_request_issues") return link(args);
      if (cmd === "append_log_records") return undefined;
      const read = pullRequestWindowRead(cmd, args);
      return read ? read.answer : Promise.reject(new Error(`unexpected command ${cmd}`));
    });
    const h = mount();
    await waitFor(() => expect(screen.queryByTestId("create-pr-loading")).toBeNull());
    expect(submit()).toBeDisabled();
    expect(submit()).toHaveAccessibleDescription(/Reading the tasks…/);
    release(undefined);
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
    expect(calls("link_pull_request_issues")[0][1]).toEqual({
      number: 42,
      issues: [{ owner: "acme", repository: "platform", number: 31 }],
    });
  });

  it("CPR-FR-IDRP, CPR-FR-BLEN: a tasks read that fails leaves Submit available and links the features alone", async () => {
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) =>
      cmd === "list_branch_completed_runs" ? Promise.reject("unknown branch") : base(cmd, args),
    );
    const h = mount();
    await waitFor(() => expect(tasks()).toHaveTextContent("The tasks could not be read"));
    await pickFeature(/#12 Git flow/);
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
    expect(calls("link_pull_request_issues")[0][1]).toEqual({
      number: 42,
      issues: [{ owner: "acme", repository: "platform", number: 12 }],
    });
    expect(logged.mock.calls.map((c) => c[2])).toContain("pull request tasks not linked, read failed");
  });

  it("CPR-FR-BLEN: with no task and no feature no link is asked for", async () => {
    completed = [];
    const h = mount();
    await ready();
    await userEvent.click(submit());
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledWith(
      { number: 42, url: "https://github.com/acme/platform/pull/42" },
      null,
      null,
    ));
    expect(calls("link_pull_request_issues")).toHaveLength(0);
  });

  it("CPR-FR-FFQD: the link outcome is logged with the head, the number and the issue count, and no title or description text", async () => {
    const h = mount();
    await ready();
    await userEvent.clear(screen.getByLabelText("Title"));
    await userEvent.type(screen.getByLabelText("Title"), "Private title");
    await userEvent.type(screen.getByLabelText("Description"), "Private body");
    await userEvent.click(submit());
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
    const record = logged.mock.calls.find((c) => c[2] === "pull request issues linked");
    expect(record?.[3]).toMatchObject({ head: HEAD, number: 42, issueCount: 1, linkedCount: 1 });
    const text = JSON.stringify(logged.mock.calls);
    expect(text).not.toContain("Private title");
    expect(text).not.toContain("Private body");
  });

  it("CPR-FR-SQEP, CPR-FR-ESXZ: a failed creation keeps the title, description, base, toggle and features, and the saved state", async () => {
    create = async () => Promise.reject("github_unreachable");
    mount();
    await ready();
    await userEvent.type(screen.getByLabelText("Description"), "Body");
    await userEvent.click(screen.getByLabelText("Draft"));
    await pickFeature(/#14 Voice/);
    await userEvent.click(submit());
    expect(await screen.findByTestId("create-pr-failure")).toBeInTheDocument();
    expect(screen.getByLabelText("Description")).toHaveValue("Body");
    expect(screen.getByLabelText("Draft")).toBeChecked();
    expect(within(screen.getByTestId("create-pr-features")).getByText("#14 Voice")).toBeInTheDocument();
    expect(getPullRequestSession(HEAD)).toMatchObject({ body: "Body", draft: true });
    expect(calls("link_pull_request_issues")).toHaveLength(0);
  });
});

describe("the saved state (CPR-FR-SSQI)", () => {
  it("CPR-FR-SSQI: Cancel closes the window and keeps the title, description, base, draft and features for the next opening", async () => {
    const first = mount();
    await ready();
    await userEvent.clear(screen.getByLabelText("Title"));
    await userEvent.type(screen.getByLabelText("Title"), "Kept title");
    await userEvent.type(screen.getByLabelText("Description"), "Kept body");
    await userEvent.selectOptions(screen.getByLabelText("Base branch"), "feature/pr-tools");
    await userEvent.click(screen.getByLabelText("Draft"));
    await pickFeature(/#12 Git flow/);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(first.onClose).toHaveBeenCalledTimes(1);
    expect(calls("create_pull_request")).toHaveLength(0);
    first.unmount();

    mount();
    expect(screen.getByLabelText("Title")).toHaveValue("Kept title");
    expect(screen.getByLabelText("Description")).toHaveValue("Kept body");
    expect(screen.getByLabelText("Base branch")).toHaveValue("feature/pr-tools");
    expect(screen.getByLabelText("Draft")).toBeChecked();
    expect(within(screen.getByTestId("create-pr-features")).getByText("#12 Git flow")).toBeInTheDocument();
  });
});

describe("stacked pull requests (CPR-FR-IPKX, CPR-FR-JBPQ)", () => {
  const STACK: PullRequestSummary = {
    number: 41,
    title: "PR tools",
    state: "open",
    isDraft: false,
    author: "alice",
    headBranch: "feature/pr-tools",
    baseBranch: "main",
    createdAt: "2026-09-01T00:00:00Z",
    updatedAt: "2026-09-02T00:00:00Z",
  };

  it("CPR-FR-IPKX: reads the open pull requests once and states that the pull request goes into the base", async () => {
    openPulls = async () => [STACK];
    mount();
    await ready();
    expect(calls("list_pull_requests")).toEqual([["list_pull_requests", { state: "open" }]]);
    expect(screen.getByTestId("create-pr-target")).toHaveTextContent("The pull request goes into main.");
  });

  it("CPR-FR-IPKX, CPR-FR-JBPQ: a base that is the head of an open pull request is stacked on it, and its keywords close nothing", async () => {
    openPulls = async () => [STACK];
    mount();
    await ready();
    await userEvent.selectOptions(screen.getByLabelText("Base branch"), "feature/pr-tools");
    const target = screen.getByTestId("create-pr-target");
    expect(target).toHaveTextContent("Stacked on #41 “PR tools”.");
    expect(target).toHaveTextContent(
      "Its closing keywords close no issue when it merges, because GitHub closes an issue only when a pull request merges into the default branch.",
    );
    expect(calls("list_pull_requests")).toHaveLength(1);
  });

  it("CPR-FR-IPKX: a failed read states no kind", async () => {
    openPulls = async () => Promise.reject("github_unreachable");
    mount();
    await ready();
    expect(screen.queryByTestId("create-pr-target")).toBeNull();
  });
});
