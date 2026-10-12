/**
 * The Create a PR window pushes the head branch before it creates the pull
 * request (`../../specifications/ui/CPR-create-pull-request.md` CPR-FR-PLWK,
 * CPR-FR-ODTB).
 *
 * The backend is a mocked `invoke` that answers by command name and rejects
 * every command it does not expect. Nothing here depends on elapsed time: each
 * wait is for a state.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { CreatePullRequestWindow } from "./CreatePullRequestWindow";
import type {
  PullRequestInput,
  PullRequestResume,
  PullRequestSource,
} from "./CreatePullRequest/types";
import type { CreatedPullRequest, GitBranch, PullRequestHeadState } from "../types";
import { resetLogBufferForTest } from "../logging";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const SOURCE: PullRequestSource = {
  head: "synthesis/stream/docker-in-docker",
  base: "main",
  title: "Docker-in-Docker",
};

const PUSHED: PullRequestHeadState = {
  head: SOURCE.head,
  base: "main",
  hasRemote: true,
  remoteBranchExists: true,
  unpushed: 0,
  uncommittedPaths: [],
  aheadOfBase: 2,
};

const NOT_PUSHED: PullRequestHeadState = {
  ...PUSHED,
  remoteBranchExists: false,
  unpushed: null,
};

const BRANCHES: GitBranch[] = [
  { name: "main", kind: "local", isCurrent: true },
  { name: SOURCE.head, kind: "local", isCurrent: false },
];

let headState: () => Promise<PullRequestHeadState>;
let push: () => Promise<unknown>;
let create: () => Promise<unknown>;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
/** The commands the window sent, without the reads and the log flushes. */
const writes = () =>
  invokeMock.mock.calls
    .map((c) => c[0] as string)
    .filter((cmd) => cmd === "push_branch" || cmd === "create_pull_request");

beforeEach(() => {
  resetLogBufferForTest();
  headState = async () => NOT_PUSHED;
  push = async () => {
    // The remote holds the branch from now on.
    headState = async () => PUSHED;
  };
  create = async () => ({ number: 7, url: "https://github.com/acme/platform/pull/7" });
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "list_branches":
        return BRANCHES;
      case "get_pull_request_head_state":
        return headState();
      case "push_branch":
        return push();
      case "create_pull_request":
        return create();
      case "append_log_records":
        return undefined;
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
});

afterEach(cleanup);

interface Handlers {
  onClose: Mock<() => void>;
  onCreated: Mock<(created: CreatedPullRequest) => void>;
  onRequestToken: Mock<(input: PullRequestInput) => void>;
  onOpenGlobalSettings: Mock<() => void>;
  onSubmittingChange: Mock<(submitting: boolean) => void>;
}

function mount(resume: PullRequestResume | null = null): Handlers {
  const handlers: Handlers = {
    onClose: vi.fn<() => void>(),
    onCreated: vi.fn<(created: CreatedPullRequest) => void>(),
    onRequestToken: vi.fn<(input: PullRequestInput) => void>(),
    onOpenGlobalSettings: vi.fn<() => void>(),
    onSubmittingChange: vi.fn<(submitting: boolean) => void>(),
  };
  render(<CreatePullRequestWindow source={SOURCE} resume={resume} {...handlers} />);
  return handlers;
}

const submit = () => screen.getByTestId("create-pr-submit");
const ready = async () => {
  await waitFor(() => expect(screen.queryByTestId("create-pr-loading")).toBeNull());
  await screen.findByRole("option", { name: "main" });
};

describe("the push notice (CPR-FR-PLWK)", () => {
  it("CPR-FR-PLWK, CPR-FR-HGXL, CPR-FR-NQPS: a branch the remote lacks is a notice that Submit pushes first, and Submit stays available", async () => {
    mount();
    expect(await screen.findByTestId("create-pr-notice-not-pushed")).toHaveTextContent(
      "The branch is not on the remote yet. Submit pushes it first.",
    );
    expect(screen.queryByTestId(/create-pr-block-/)).toBeNull();
    await waitFor(() => expect(submit()).toBeEnabled());
    // The notice is in the live state region, so assistive technology reads it.
    expect(screen.getByTestId("create-pr-state")).toContainElement(
      screen.getByTestId("create-pr-notice-not-pushed"),
    );
    expect(calls("push_branch")).toHaveLength(0);
  });

  it("CPR-FR-PLWK: missing commits are counted in the notice", async () => {
    headState = async () => ({ ...PUSHED, unpushed: 2 });
    mount();
    expect(await screen.findByTestId("create-pr-notice-unpushed")).toHaveTextContent(
      "2 commits are not on the remote yet. Submit pushes them first.",
    );
    await waitFor(() => expect(submit()).toBeEnabled());
  });

  it("CPR-FR-PLWK, CPR-FR-VYPG: a repository with no remote shows its block and no push notice", async () => {
    headState = async () => ({ ...NOT_PUSHED, hasRemote: false });
    mount();
    expect(await screen.findByTestId("create-pr-block-no-remote")).toBeInTheDocument();
    expect(screen.queryByTestId(/create-pr-notice-/)).toBeNull();
    expect(submit()).toBeDisabled();
  });
});

describe("Submit pushes, then creates (CPR-FR-ODTB, CPR-FR-KMHY, CPR-FR-XMRL)", () => {
  it("CPR-FR-ODTB, CPR-FR-KMHY: pushes the head branch, creates the pull request, then reads the head state anew", async () => {
    const h = mount();
    await ready();
    await waitFor(() => expect(submit()).toBeEnabled());
    const readsBefore = calls("get_pull_request_head_state").length;

    await userEvent.click(submit());

    await waitFor(() => expect(h.onCreated).toHaveBeenCalledWith({
      number: 7,
      url: "https://github.com/acme/platform/pull/7",
    }));
    expect(writes()).toEqual(["push_branch", "create_pull_request"]);
    expect(calls("push_branch")[0][1]).toEqual({ branch: SOURCE.head });
    expect(calls("create_pull_request")[0][1]).toMatchObject({ head: SOURCE.head, base: "main" });
    expect(calls("get_pull_request_head_state").length).toBe(readsBefore + 1);
    // The new read comes after the push.
    const order = invokeMock.mock.calls.map((c) => c[0] as string);
    expect(order.lastIndexOf("get_pull_request_head_state")).toBeGreaterThan(
      order.indexOf("push_branch"),
    );
  });

  it("CPR-FR-ODTB: missing commits are pushed the same way", async () => {
    headState = async () => ({ ...PUSHED, unpushed: 1 });
    const h = mount();
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());
    await waitFor(() => expect(h.onCreated).toHaveBeenCalled());
    expect(writes()).toEqual(["push_branch", "create_pull_request"]);
  });

  it("CPR-FR-ODTB: a remote that holds the whole branch gets no push", async () => {
    headState = async () => PUSHED;
    const h = mount();
    await ready();
    expect(screen.queryByTestId(/create-pr-notice-/)).toBeNull();
    await userEvent.click(submit());
    await waitFor(() => expect(h.onCreated).toHaveBeenCalled());
    expect(writes()).toEqual(["create_pull_request"]);
  });

  it("CPR-FR-KMHY, CPR-FR-ODTB: two activations in the same moment push once", async () => {
    let releasePush: () => void = () => {};
    push = () => new Promise<void>((resolve) => (releasePush = resolve));
    const h = mount();
    await waitFor(() => expect(submit()).toBeEnabled());
    const button = submit();
    act(() => {
      button.click();
      button.click();
    });
    await waitFor(() => expect(calls("push_branch")).toHaveLength(1));
    releasePush();
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(submit()).toHaveTextContent("Submit"));
    expect(writes()).toEqual(["push_branch", "create_pull_request"]);
  });

  it("CPR-FR-XMRL, CPR-FR-KMHY: Submit says what runs, and a second activation during the push sends nothing", async () => {
    let releasePush: () => void = () => {};
    push = () => new Promise<void>((resolve) => (releasePush = resolve));
    let releaseCreate: (v: unknown) => void = () => {};
    create = () => new Promise((resolve) => (releaseCreate = resolve));
    const h = mount();
    await waitFor(() => expect(submit()).toBeEnabled());

    await userEvent.click(submit());
    await waitFor(() => expect(submit()).toHaveTextContent("Pushing the branch…"));
    expect(submit()).toBeDisabled();
    expect(h.onSubmittingChange).toHaveBeenLastCalledWith(true);
    // Escape does not close the window while the push runs.
    await userEvent.keyboard("{Escape}");
    expect(h.onClose).not.toHaveBeenCalled();
    submit().click();
    expect(calls("push_branch")).toHaveLength(1);

    releasePush();
    await waitFor(() => expect(submit()).toHaveTextContent("Creating the pull request…"));
    expect(calls("create_pull_request")).toHaveLength(1);

    releaseCreate({ number: 7, url: "https://github.com/acme/platform/pull/7" });
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
    expect(calls("push_branch")).toHaveLength(1);
  });
});

describe("a push that fails (CPR-FR-ODTB, CPR-FR-SQEP, CPR-FR-VZUZ, CPR-FR-FGGU, CPR-FR-VZNE)", () => {
  it("CPR-FR-ODTB, CPR-FR-SQEP, CPR-FR-VZNE: creates no pull request, shows the refusal inline and keeps the input", async () => {
    push = async () => Promise.reject("cannot push non-fastforwardable reference");
    const h = mount();
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.type(screen.getByLabelText("Description"), "Why");

    await userEvent.click(submit());

    expect(await screen.findByTestId("create-pr-failure")).toHaveTextContent(
      `The remote refused the push of ${SOURCE.head}: cannot push non-fastforwardable reference`,
    );
    expect(calls("create_pull_request")).toHaveLength(0);
    expect(h.onCreated).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Description")).toHaveValue("Why");
    expect(h.onSubmittingChange).toHaveBeenLastCalledWith(false);
    // The author can try again.
    expect(submit()).toBeEnabled();
    expect(submit()).toHaveTextContent("Submit");
  });

  it("CPR-FR-FGGU: a missing token from the push offers the route to Global settings", async () => {
    push = async () => Promise.reject("github_token_missing");
    const h = mount();
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());
    expect(await screen.findByTestId("create-pr-failure")).toHaveTextContent(
      "No GitHub token is stored. Add one in Global settings → GitHub.",
    );
    await userEvent.click(screen.getByTestId("create-pr-global-settings"));
    expect(h.onOpenGlobalSettings).toHaveBeenCalledTimes(1);
    expect(h.onClose).not.toHaveBeenCalled();
    expect(calls("create_pull_request")).toHaveLength(0);
  });

  it("CPR-FR-VZUZ: a selection-required push hands back what was typed and creates nothing", async () => {
    push = async () => Promise.reject("github_token_selection_required");
    const h = mount();
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(screen.getByLabelText("Draft"));
    await userEvent.click(submit());
    await waitFor(() =>
      expect(h.onRequestToken).toHaveBeenCalledWith({
        title: SOURCE.title,
        body: "",
        base: "main",
        draft: true,
      }),
    );
    expect(screen.queryByTestId("create-pr-failure")).toBeNull();
    expect(calls("create_pull_request")).toHaveLength(0);
  });

  it("CPR-FR-VZUZ, CPR-FR-ODTB: coming back after a confirmed token pushes and creates once with what was held", async () => {
    const h = mount({
      input: { title: "Held", body: "Kept", base: "main", draft: false },
      submit: true,
      notice: null,
    });
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
    // The window is idle again, so no second run can still be on its way.
    await waitFor(() => expect(submit()).toHaveTextContent("Submit"));
    await waitFor(() => expect(screen.queryByTestId("create-pr-loading")).toBeNull());
    expect(writes()).toEqual(["push_branch", "create_pull_request"]);
    expect(calls("create_pull_request")[0][1]).toMatchObject({ title: "Held", body: "Kept" });
  });

  it("CPR-FR-VZUZ: a push that still asks for a token after the picker is an inline failure, not another picker", async () => {
    push = async () => Promise.reject("github_token_selection_required");
    const h = mount({
      input: { title: "Held", body: "", base: "main", draft: false },
      submit: true,
      notice: null,
    });
    expect(await screen.findByTestId("create-pr-failure")).toHaveTextContent(
      "Choose which GitHub token this project should use.",
    );
    expect(h.onRequestToken).not.toHaveBeenCalled();
    expect(calls("create_pull_request")).toHaveLength(0);
  });
});

describe("a window that is gone or blocked (CPR-FR-XMRL, CPR-FR-HGXL, CPR-FR-VZUZ)", () => {
  it("CPR-FR-XMRL, CPR-FR-ODTB: a push that finishes after the window is gone creates nothing", async () => {
    let releasePush: () => void = () => {};
    push = () => new Promise<void>((resolve) => (releasePush = resolve));
    const h = mount();
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());
    await waitFor(() => expect(calls("push_branch")).toHaveLength(1));

    cleanup();
    h.onSubmittingChange.mockClear();
    await act(async () => {
      releasePush();
      // One macrotask runs after every continuation the push queued, so the
      // window has done all it will do. This waits for an order, not a time.
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(calls("create_pull_request")).toHaveLength(0);
    expect(h.onCreated).not.toHaveBeenCalled();
    // A window that is gone does not clear the guard of one that replaced it.
    expect(h.onSubmittingChange).not.toHaveBeenCalled();
  });

  it("CPR-FR-VZUZ, CPR-FR-ODTB: a resume waits for the head state before it decides to push", async () => {
    let releaseRead: (s: PullRequestHeadState) => void = () => {};
    headState = () => new Promise((resolve) => (releaseRead = resolve));
    const h = mount({
      input: { title: "Held", body: "", base: "main", draft: false },
      submit: true,
      notice: null,
    });
    await waitFor(() => expect(calls("get_pull_request_head_state")).toHaveLength(1));
    expect(writes()).toEqual([]);

    releaseRead(NOT_PUSHED);

    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
    expect(writes()).toEqual(["push_branch", "create_pull_request"]);
  });

  it("CPR-FR-VZUZ, CPR-FR-HGXL: a resume does not submit while a block stands", async () => {
    headState = async () => ({ ...NOT_PUSHED, uncommittedPaths: ["a.md"] });
    mount({
      input: { title: "Held", body: "", base: "main", draft: false },
      submit: true,
      notice: null,
    });
    expect(await screen.findByTestId("create-pr-block-uncommitted")).toBeInTheDocument();
    expect(submit()).toBeDisabled();

    // The block clears. A resume that waited instead of holding back would
    // submit now, so no submission here proves it was held back while blocked.
    headState = async () => NOT_PUSHED;
    await userEvent.click(screen.getByTestId("create-pr-recheck"));
    await waitFor(() => expect(submit()).toBeEnabled());
    expect(writes()).toEqual([]);
  });

  it("CPR-FR-XMRL: after a failed push, a submission with no push due says only that it creates", async () => {
    push = async () => Promise.reject("github_unreachable");
    let releaseCreate: (v: unknown) => void = () => {};
    create = () => new Promise((resolve) => (releaseCreate = resolve));
    const h = mount();
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());
    expect(await screen.findByTestId("create-pr-failure")).toHaveTextContent(/Could not reach GitHub/);

    // Pushed elsewhere: the next read finds the branch on the remote.
    headState = async () => PUSHED;
    await userEvent.click(screen.getByTestId("create-pr-recheck"));
    await waitFor(() => expect(screen.queryByTestId(/create-pr-notice-/)).toBeNull());
    await userEvent.click(submit());
    await waitFor(() => expect(submit()).toHaveTextContent("Creating the pull request…"));
    expect(calls("push_branch")).toHaveLength(1);

    releaseCreate({ number: 7, url: "https://github.com/acme/platform/pull/7" });
    await waitFor(() => expect(h.onCreated).toHaveBeenCalledTimes(1));
  });
});
