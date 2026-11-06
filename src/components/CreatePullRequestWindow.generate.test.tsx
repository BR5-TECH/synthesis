/**
 * The Create a PR window writes its title and description from PR summary
 * runs, and keeps them per head branch
 * (`../../specifications/ui/CPR-create-pull-request.md` CPR-FR-QVYZ,
 * CPR-FR-ILFN, CPR-FR-UNAN, CPR-FR-VOEB, CPR-FR-GYVD, CPR-FR-URIL,
 * CPR-FR-IOGF, CPR-FR-WCBQ, CPR-FR-PJTG, CPR-FR-ZZNM, CPR-FR-SSQI).
 *
 * The backend is a mocked `invoke` that answers by command name and rejects
 * every command it does not expect. `"graduation run changed"` is a scripted
 * event bus. Nothing here depends on elapsed time: each wait is for a state.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { CreatePullRequestWindow } from "./CreatePullRequestWindow";
import type { PullRequestSource } from "./CreatePullRequest/types";
import { resetLogBufferForTest } from "../logging";
import { getPullRequestSession, updatePullRequestSession } from "../state/pullRequestSessions";
import {
  branchRun,
  pullRequestWindowRead,
  resetCreatePullRequestStores,
} from "../test/createPullRequestFixtures";
import { makePrSummaryRun } from "../test/graduationFixtures";
import type { BranchCompletedRun, GraduationRun } from "../types/graduation";
import type { GitBranch, PullRequestHeadState } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

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
  { name: "develop", kind: "local", isCurrent: false },
  { name: HEAD, kind: "local", isCurrent: true },
];

let runs: Record<string, GraduationRun>;
let completed: BranchCompletedRun[];
let vendor: string | null;
let start: (args: Record<string, unknown>) => Promise<GraduationRun>;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  resetCreatePullRequestStores();
  handlers.length = 0;
  runs = {};
  completed = [];
  vendor = "claude_code";
  start = async (args) => {
    const run = makePrSummaryRun("pr-1", "queued", {
      prSummary: { head: HEAD, base: String(args.base), mode: args.mode as "diff" },
    });
    runs[run.id] = run;
    return run;
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "list_branches":
        return BRANCHES;
      case "get_pull_request_head_state":
        return READY;
      case "start_pull_request_summary":
        return start(args);
      case "get_graduation_run":
        return runs[args.runId as string] ?? Promise.reject(new Error("unknown_run"));
      case "list_graduation_queue":
        return { projectKey: "/p", runs: Object.values(runs) };
      case "get_project_agentic_integration":
        return { vendor, resolution: "inherited", overrideVendor: null };
      case "list_branch_completed_runs":
        return completed;
      case "create_pull_request":
        return { number: 42, url: "https://github.com/acme/platform/pull/42" };
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
  const onClose = vi.fn();
  const view = render(
    <CreatePullRequestWindow
      source={SOURCE}
      onClose={onClose}
      onCreated={vi.fn()}
      onRequestToken={vi.fn()}
      onOpenGlobalSettings={vi.fn()}
      onSubmittingChange={vi.fn()}
    />,
  );
  return { ...view, onClose };
}

const generate = () => screen.getByTestId("create-pr-generate");
const description = () => screen.getByLabelText("Description") as HTMLTextAreaElement;
const ready = async () => {
  await waitFor(() => expect(screen.queryByTestId("create-pr-loading")).toBeNull());
  await waitFor(() => expect(generate()).toBeEnabled());
};

/** The run `pr-1` ended in `state`. */
function end(state: "completed" | "failed", result?: { title: string; description: string }) {
  runs["pr-1"] = {
    ...runs["pr-1"],
    state,
    failure:
      state === "failed" ? { code: "agent_failed", message: "The agent stopped.", retryable: false } : null,
    prSummary: { ...runs["pr-1"].prSummary!, result: result ?? null },
  };
  act(() => fire("graduation-run-changed", { runId: "pr-1", state }));
}

describe("the controls (CPR-FR-QVYZ)", () => {
  it("CPR-FR-QVYZ: holds the title, the description with Write and Preview, Generate, base, draft, features, head, tasks, Cancel and Submit", async () => {
    mount();
    await ready();
    const dialog = screen.getByRole("dialog", { name: "Create a PR" });
    expect(within(dialog).getByLabelText("Title")).toBeInTheDocument();
    expect(description()).toBeInTheDocument();
    expect(within(dialog).getByRole("tab", { name: "Write" })).toBeInTheDocument();
    expect(within(dialog).getByRole("tab", { name: "Preview" })).toBeInTheDocument();
    expect(generate()).toHaveTextContent("Generate");
    expect(within(dialog).getByLabelText("Base branch")).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Draft")).toBeInTheDocument();
    expect(within(dialog).getByTestId("create-pr-features-select")).toBeInTheDocument();
    expect(within(dialog).getByTestId("create-pr-head").tagName).toBe("SPAN");
    expect(within(dialog).getByTestId("create-pr-tasks")).toHaveTextContent("Tasks");
    const actions = [...dialog.querySelectorAll(".modal__actions button")].map((b) => b.textContent);
    expect(actions).toEqual(["Cancel", "Submit"]);
  });
});

describe("Write and Preview (CPR-FR-ILFN)", () => {
  it("CPR-FR-ILFN: Write is active at open and holds the text exactly as typed", async () => {
    mount();
    await ready();
    expect(screen.getByRole("tab", { name: "Write" })).toHaveAttribute("aria-selected", "true");
    await userEvent.click(description());
    await userEvent.paste("## Plan\n* one [link](https://x.test)");
    expect(description()).toHaveValue("## Plan\n* one [link](https://x.test)");
  });

  it("CPR-FR-ILFN: Preview renders the Markdown with inert links and edits nothing", async () => {
    mount();
    await ready();
    await userEvent.click(description());
    await userEvent.paste("**bold** [link](https://x.test)");
    await userEvent.click(screen.getByRole("tab", { name: "Preview" }));
    const preview = screen.getByTestId("create-pr-preview");
    expect(preview).toBeVisible();
    expect(within(preview).getByText("bold").tagName).toBe("STRONG");
    const link = within(preview).getByText("link");
    expect(link).not.toHaveAttribute("href");
    expect(link).toHaveAttribute("aria-disabled");
    expect(within(preview).queryByRole("textbox")).toBeNull();
  });

  it("CPR-FR-ILFN: switching tabs keeps the text and the caret of the Write tab", async () => {
    mount();
    await ready();
    await userEvent.type(description(), "abcdef");
    description().setSelectionRange(2, 4);
    await userEvent.click(screen.getByRole("tab", { name: "Preview" }));
    await userEvent.click(screen.getByRole("tab", { name: "Write" }));
    expect(description()).toHaveValue("abcdef");
    expect(description().selectionStart).toBe(2);
    expect(description().selectionEnd).toBe(4);
  });

  it("CPR-FR-ILFN, CPR-FR-NQPS: the arrow keys move between the tabs", async () => {
    mount();
    await ready();
    screen.getByRole("tab", { name: "Write" }).focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Preview" })).toHaveFocus();
    expect(screen.getByRole("tab", { name: "Preview" })).toHaveAttribute("aria-selected", "true");
  });
});

describe("Generate (CPR-FR-UNAN, CPR-FR-VOEB, CPR-FR-GYVD, CPR-FR-URIL, CPR-FR-IOGF)", () => {
  it("CPR-FR-UNAN, CPR-FR-VOEB: starts a diff run for the head and the chosen base, then says why Generate is unavailable and shows the run's progress", async () => {
    mount();
    await ready();
    await userEvent.selectOptions(screen.getByLabelText("Base branch"), "develop");
    await waitFor(() => expect(generate()).toBeEnabled());
    await userEvent.click(generate());

    expect(calls("start_pull_request_summary")[0][1]).toEqual({
      head: HEAD,
      base: "develop",
      mode: "diff",
    });
    await waitFor(() => expect(generate()).toBeDisabled());
    expect(generate()).toHaveAccessibleDescription(/PR summary run for this head branch is in progress/);
    const state = screen.getByTestId("create-pr-generation");
    expect(state).toHaveTextContent("Writing the title and the description from the diff…");
    expect(within(state).getByTestId("run-progress")).toBeInTheDocument();
    expect(within(state).getByText("Summarizing")).toBeInTheDocument();
  });

  it("CPR-FR-GYVD: a completed run replaces the title and the description in the open window", async () => {
    mount();
    await ready();
    await userEvent.type(description(), "draft text");
    await userEvent.click(generate());
    await waitFor(() => expect(generate()).toBeDisabled());
    end("completed", { title: "Add the panel", description: "## Summary" });
    await waitFor(() => expect(screen.getByLabelText("Title")).toHaveValue("Add the panel"));
    expect(description()).toHaveValue("## Summary");
    expect(calls("get_graduation_run").slice(-1)[0]?.[1]).toEqual({ runId: "pr-1" });
    await waitFor(() => expect(generate()).toBeEnabled());
  });

  it("CPR-FR-GYVD, CPR-FR-SSQI: a run that completes while the window is closed shows at the next opening", async () => {
    const first = mount();
    await ready();
    await userEvent.click(generate());
    await waitFor(() => expect(generate()).toBeDisabled());
    first.unmount();

    end("completed", { title: "Written while closed", description: "Closed body" });
    await waitFor(() => expect(getPullRequestSession(HEAD)?.title).toBe("Written while closed"));

    mount();
    expect(screen.getByLabelText("Title")).toHaveValue("Written while closed");
    expect(description()).toHaveValue("Closed body");
  });

  it("CPR-FR-URIL, CPR-FR-NQPS: a failed run changes no field, says the generation did not complete with its cause as an alert, and Generate is available again", async () => {
    mount();
    await ready();
    await userEvent.type(description(), "kept");
    await userEvent.click(generate());
    await waitFor(() => expect(generate()).toBeDisabled());
    end("failed");
    const alert = await screen.findByTestId("create-pr-generation-failed");
    expect(alert).toHaveTextContent("The generation did not complete: The agent stopped.");
    expect(alert).toHaveAttribute("role", "alert");
    expect(description()).toHaveValue("kept");
    expect(screen.getByLabelText("Title")).toHaveValue("panel");
    await waitFor(() => expect(generate()).toBeEnabled());
  });

  it("CPR-FR-UNAN: a refused start shows inline in words and changes no field", async () => {
    start = async () => Promise.reject("unknown branch");
    mount();
    await ready();
    await userEvent.click(generate());
    expect(await screen.findByTestId("create-pr-generation-failed")).toHaveTextContent(
      "This repository holds no such head branch or base branch.",
    );
    expect(screen.getByLabelText("Title")).toHaveValue("panel");
    expect(generate()).toBeEnabled();
  });

  it("CPR-FR-IOGF: a project that resolves no agentic integration makes Generate unavailable, and says why", async () => {
    vendor = null;
    mount();
    await waitFor(() => expect(screen.queryByTestId("create-pr-loading")).toBeNull());
    await waitFor(() =>
      expect(screen.getByTestId("create-pr-generate-block")).toHaveTextContent(
        "This project resolves no agentic integration, so Generate is unavailable.",
      ),
    );
    expect(generate()).toBeDisabled();
    expect(generate()).toHaveAccessibleDescription(/resolves no agentic integration/);
  });

  it("CPR-FR-VOEB: a run of the head branch in the queue at open makes Generate unavailable and shows its state", async () => {
    runs["pr-5"] = makePrSummaryRun("pr-5", "working", { prSummary: { head: HEAD, mode: "diff" } });
    mount();
    await waitFor(() => expect(screen.getByTestId("create-pr-generation")).toBeInTheDocument());
    expect(generate()).toBeDisabled();
    expect(calls("list_graduation_queue")).toHaveLength(1);
  });
});

describe("the first opening of a head branch (CPR-FR-WCBQ, CPR-FR-PJTG, CPR-FR-ZZNM)", () => {
  it("CPR-FR-WCBQ, CPR-FR-PJTG: combines the run summaries into the title and description, then follows a combine run", async () => {
    completed = [
      branchRun("r1", { runSummary: { title: "First run", description: "Did one." } }),
      branchRun("r2", { runSummary: { title: "Second run", description: "Did two." } }),
    ];
    mount();
    await waitFor(() => expect(screen.getByLabelText("Title")).toHaveValue("First run"));
    expect(description()).toHaveValue("## First run\n\nDid one.\n\n## Second run\n\nDid two.");
    await waitFor(() => expect(calls("start_pull_request_summary")).toHaveLength(1));
    expect(calls("start_pull_request_summary")[0][1]).toEqual({ head: HEAD, base: "main", mode: "combine" });
    expect(await screen.findByTestId("create-pr-generation")).toHaveTextContent(
      "Writing the description from the run summaries…",
    );
    end("completed", { title: "Agent title", description: "Agent body" });
    await waitFor(() => expect(description()).toHaveValue("Agent body"));
    expect(screen.getByLabelText("Title")).toHaveValue("First run");
  });

  it("CPR-FR-ZZNM: a second opening while the saved state exists does not combine again", async () => {
    completed = [branchRun("r1", { runSummary: { title: "First run", description: "Did one." } })];
    const first = mount();
    await waitFor(() => expect(calls("start_pull_request_summary")).toHaveLength(1));
    first.unmount();
    act(() => updatePullRequestSession(HEAD, { title: "Edited by the author" }));
    mount();
    await waitFor(() => expect(calls("list_branch_completed_runs")).toHaveLength(2));
    await waitFor(() => expect(screen.getByTestId("create-pr-tasks")).not.toHaveTextContent("Reading"));
    // One macrotask runs after every continuation the read queued: this waits
    // for an order, not for a time.
    await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
    expect(calls("start_pull_request_summary")).toHaveLength(1);
    expect(screen.getByLabelText("Title")).toHaveValue("Edited by the author");
  });

  it("CPR-FR-WCBQ: a field the author typed in before the runs were read keeps their text", async () => {
    let release: (runs: BranchCompletedRun[]) => void = () => {};
    const pending = new Promise<BranchCompletedRun[]>((resolve) => (release = resolve));
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) =>
      cmd === "list_branch_completed_runs" ? pending : base(cmd, args),
    );
    mount();
    await waitFor(() => expect(screen.queryByTestId("create-pr-loading")).toBeNull());
    await userEvent.type(description(), "my own words");
    release([branchRun("r1", { runSummary: { title: "First run", description: "Did one." } })]);
    await waitFor(() => expect(screen.getByLabelText("Title")).toHaveValue("First run"));
    expect(description()).toHaveValue("my own words");
  });

  it("CPR-FR-WCBQ: no run summary keeps the source and starts no run", async () => {
    completed = [branchRun("r1")];
    mount();
    await ready();
    await waitFor(() => expect(screen.getByTestId("create-pr-tasks")).not.toHaveTextContent("Reading"));
    expect(screen.getByLabelText("Title")).toHaveValue("panel");
    expect(description()).toHaveValue("");
    expect(calls("start_pull_request_summary")).toHaveLength(0);
  });
});
