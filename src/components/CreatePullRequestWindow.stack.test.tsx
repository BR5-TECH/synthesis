/**
 * The Create a PR window on a stack
 * (`../../specifications/ui/CPR-create-pull-request.md` CPR-FR-QXTB,
 * CPR-FR-HZRN, CPR-FR-YDLV).
 *
 * The backend is a mocked `invoke` that answers by command name and rejects
 * every command it does not expect. Each wait is for a state.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { CreatePullRequestWindow } from "./CreatePullRequestWindow";
import type { PullRequestLinkResult } from "./CreatePullRequest/linking";
import type { PullRequestSource } from "./CreatePullRequest/types";
import type {
  CreatedPullRequest,
  GitBranch,
  PullRequestHeadState,
  StackedPullRequest,
  StackStep,
} from "../types";
import { resetLogBufferForTest } from "../logging";
import {
  pullRequestWindowRead,
  resetCreatePullRequestStores,
} from "../test/createPullRequestFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const SOURCE: PullRequestSource = {
  head: "feature/editor-scroll",
  base: "feature/pr-tools",
  title: "editor-scroll",
  stackParent: "feature/pr-tools",
};

const BRANCHES: GitBranch[] = [
  { name: "main", kind: "local", isCurrent: false },
  { name: "feature/pr-tools", kind: "local", isCurrent: false },
  { name: SOURCE.head, kind: "local", isCurrent: true },
];

const headState = (base: string | null): PullRequestHeadState => ({
  head: SOURCE.head,
  base: base ?? SOURCE.base,
  hasRemote: true,
  remoteBranchExists: true,
  unpushed: 0,
  uncommittedPaths: [],
  aheadOfBase: 2,
});

let submitToStack: () => Promise<StackedPullRequest>;
let unpushed: number;
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  resetCreatePullRequestStores();
  unpushed = 0;
  submitToStack = async () => ({
    number: 51,
    url: "https://github.com/acme/platform/pull/51",
    stack: { state: "added", stackNumber: 3 },
  });
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "list_branches":
        return BRANCHES;
      case "get_pull_request_head_state":
        return { ...headState((args.base as string | null) ?? null), unpushed };
      case "push_branch":
        unpushed = 0;
        return undefined;
      case "submit_pull_request_to_stack":
        return submitToStack();
      case "create_pull_request":
        return { number: 52, url: "https://github.com/acme/platform/pull/52" };
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

type OnCreated = (
  created: CreatedPullRequest,
  link: PullRequestLinkResult | null,
  stack?: StackStep | null,
) => void;

function mount(source = SOURCE): Mock<OnCreated> {
  const onCreated = vi.fn<OnCreated>();
  render(
    <CreatePullRequestWindow
      source={source}
      resume={null}
      onClose={vi.fn()}
      onCreated={onCreated}
      onRequestToken={vi.fn()}
      onOpenGlobalSettings={vi.fn()}
      onSubmittingChange={vi.fn()}
    />,
  );
  return onCreated;
}

const submit = () => screen.getByTestId("create-pr-submit");
const ready = async () => {
  await waitFor(() => expect(screen.queryByTestId("create-pr-loading")).toBeNull());
  await screen.findByRole("option", { name: "main" });
  await waitFor(() => expect(submit()).toBeEnabled());
};

describe("submitting to a stack (CPR-FR-QXTB, CPR-FR-HZRN, CPR-FR-YDLV)", () => {
  it("CPR-FR-HZRN: with the stack parent as the base, the window says Submit adds the pull request to the stack", async () => {
    mount();
    await ready();
    expect(
      within(screen.getByRole("dialog")).getByText(
        "Submit adds the pull request to the stack of feature/pr-tools.",
      ),
    ).toBeVisible();
  });

  it("CPR-FR-QXTB: the push of this window runs before the submission to the stack", async () => {
    unpushed = 2;
    const onCreated = mount();
    await ready();
    await userEvent.click(submit());
    await waitFor(() => expect(onCreated).toHaveBeenCalledTimes(1));
    const writes = invokeMock.mock.calls
      .map((c) => c[0] as string)
      .filter((cmd) => cmd === "push_branch" || cmd === "submit_pull_request_to_stack");
    expect(writes).toEqual(["push_branch", "submit_pull_request_to_stack"]);
    expect(calls("push_branch")[0][1]).toEqual({ branch: SOURCE.head });
  });

  it("CPR-FR-QXTB, CPR-FR-YDLV: Submit sends one submission to the stack, and reports the pull request with its stack step", async () => {
    const onCreated = mount();
    await ready();
    await userEvent.click(submit());

    await waitFor(() =>
      expect(onCreated).toHaveBeenCalledWith(
        { number: 51, url: "https://github.com/acme/platform/pull/51" },
        null,
        { state: "added", stackNumber: 3 },
      ),
    );
    expect(calls("submit_pull_request_to_stack")).toHaveLength(1);
    expect(calls("submit_pull_request_to_stack")[0][1]).toEqual({
      title: "editor-scroll",
      body: "",
      base: "feature/pr-tools",
      head: SOURCE.head,
      draft: false,
    });
    expect(calls("create_pull_request")).toHaveLength(0);
  });

  it("CPR-FR-YDLV: a pull request not added to its stack is still a success", async () => {
    submitToStack = async () => ({
      number: 51,
      url: "https://github.com/acme/platform/pull/51",
      stack: { state: "not_added", error: "stack_busy" },
    });
    const onCreated = mount();
    await ready();
    await userEvent.click(submit());
    await waitFor(() =>
      expect(onCreated).toHaveBeenCalledWith(
        expect.objectContaining({ number: 51 }),
        null,
        { state: "not_added", error: "stack_busy" },
      ),
    );
    expect(screen.queryByTestId("create-pr-failure")).toBeNull();
  });

  it("CPR-FR-YDLV: a failed creation is a failure of the window, which keeps its state", async () => {
    submitToStack = () => Promise.reject("pull_request_exists");
    const onCreated = mount();
    await ready();
    await userEvent.click(submit());
    await waitFor(() => expect(calls("submit_pull_request_to_stack")).toHaveLength(1));
    await waitFor(() => expect(submit()).toBeEnabled());
    // CPR-FR-SQEP: the failure shows inline.
    expect(await screen.findByTestId("create-pr-failure")).toBeInTheDocument();
    expect(onCreated).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Title")).toHaveValue("editor-scroll");
  });

  it("CPR-FR-QXTB, CPR-FR-HZRN: another base makes a plain creation, and the statement goes away", async () => {
    const onCreated = mount();
    await ready();
    await userEvent.selectOptions(screen.getByTestId("create-pr-base"), "main");
    await waitFor(() => expect(screen.queryByTestId("create-pr-stack")).toBeNull());
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());
    await waitFor(() =>
      expect(onCreated).toHaveBeenCalledWith(
        { number: 52, url: "https://github.com/acme/platform/pull/52" },
        null,
        null,
      ),
    );
    expect(calls("create_pull_request")[0][1]).toMatchObject({ base: "main" });
    expect(calls("submit_pull_request_to_stack")).toHaveLength(0);
  });

  it("CPR-FR-QXTB: a source with no stack parent never submits to a stack", async () => {
    const onCreated = mount({ head: SOURCE.head, base: "feature/pr-tools", title: "x" });
    await ready();
    expect(screen.queryByTestId("create-pr-stack")).toBeNull();
    await userEvent.click(submit());
    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    expect(calls("submit_pull_request_to_stack")).toHaveLength(0);
    expect(calls("create_pull_request")).toHaveLength(1);
  });
});
