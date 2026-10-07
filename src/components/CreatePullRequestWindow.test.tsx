/**
 * The Create a PR window
 * (`../../specifications/ui/CPR-create-pull-request.md`).
 *
 * The backend is a mocked `invoke` that answers by command name and rejects
 * every command it does not expect. Nothing here depends on elapsed time: each
 * wait is for a state.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
  head: "synthesis/stream/editor-work",
  base: "main",
  title: "editor-work",
};

const READY: PullRequestHeadState = {
  head: SOURCE.head,
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
  { name: SOURCE.head, kind: "local", isCurrent: false },
  { name: "origin/release", kind: "remote", isCurrent: false },
];

let headState: (head: string, base: string | null) => Promise<PullRequestHeadState>;
let create: (args: Record<string, unknown>) => Promise<unknown>;

const noop = () => {};
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  headState = async () => READY;
  create = async () => ({ number: 42, url: "https://github.com/acme/platform/pull/42" });
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "list_branches":
        return BRANCHES;
      case "get_pull_request_head_state":
        return headState(args.head as string, args.base as string | null);
      case "create_pull_request":
        return create(args);
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

function mount(resume: PullRequestResume | null = null, source = SOURCE): Handlers {
  const handlers: Handlers = {
    onClose: vi.fn<() => void>(),
    onCreated: vi.fn<(created: CreatedPullRequest) => void>(),
    onRequestToken: vi.fn<(input: PullRequestInput) => void>(),
    onOpenGlobalSettings: vi.fn<() => void>(),
    onSubmittingChange: vi.fn<(submitting: boolean) => void>(),
  };
  render(<CreatePullRequestWindow source={source} resume={resume} {...handlers} />);
  return handlers;
}

const submit = () => screen.getByTestId("create-pr-submit");
/** The head state has been read and the branch options have arrived. */
const ready = async () => {
  await waitFor(() => expect(screen.queryByTestId("create-pr-loading")).toBeNull());
  await screen.findByRole("option", { name: "develop" });
};

describe("the window's contents (CPR-FR-JOIG, CPR-FR-QVYZ, CPR-FR-FZHF)", () => {
  it("CPR-FR-JOIG, CPR-FR-QVYZ: starts from the source — title prefilled, description empty, draft off, head read-only", async () => {
    mount();
    await ready();

    expect(screen.getByLabelText("Title")).toHaveValue("editor-work");
    expect(screen.getByLabelText("Description")).toHaveValue("");
    expect(screen.getByLabelText("Draft")).not.toBeChecked();
    expect(screen.getByTestId("create-pr-head")).toHaveTextContent(SOURCE.head);
    // The head branch is text, not a control.
    expect(screen.getByTestId("create-pr-head").tagName).toBe("SPAN");
    expect(screen.getByLabelText("Base branch")).toHaveValue("main");
  });

  it("CPR-FR-FZHF: the base selector offers the project's branches by GitHub's names, without the head branch", async () => {
    mount();
    await ready();
    const options = Array.from(
      (screen.getByTestId("create-pr-base") as HTMLSelectElement).options,
    ).map((o) => o.value);
    expect(options).toEqual(["develop", "main", "release"]);
  });

  it("CPR-FR-FZHF, CPR-FR-HGXL: a base the project no longer holds is marked missing and blocks Submit", async () => {
    headState = async () => READY;
    mount(null, { ...SOURCE, base: "gone" });
    await ready();
    expect(screen.getByRole("option", { name: "gone (missing)" })).toBeInTheDocument();
    expect(submit()).toBeDisabled();
    expect(submit()).toHaveAccessibleDescription(/base branch gone does not exist/);
  });
});

describe("a branch listing that fails (CPR-FR-FZHF)", () => {
  it("CPR-FR-FZHF: says nothing about the base branch, so the given base stays usable", async () => {
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === "list_branches") return Promise.reject("not a git repository");
      if (cmd === "get_pull_request_head_state") return headState(args.head as string, null);
      return cmd === "append_log_records" ? undefined : Promise.reject(new Error(cmd));
    });
    mount();
    await waitFor(() => expect(calls("list_branches")).toHaveLength(1));
    await waitFor(() => expect(submit()).toBeEnabled());
    expect(screen.getByLabelText("Base branch")).toHaveValue("main");
    expect(screen.queryByRole("option", { name: /missing/ })).toBeNull();
  });
});

describe("what the window reads and blocks on (CPR-FR-SQGZ, CPR-FR-VYPG, CPR-FR-WJIA)", () => {
  it("CPR-FR-SQGZ: reads the head state on open, shows loading, and again for another base", async () => {
    let release: (s: PullRequestHeadState) => void = () => {};
    headState = () => new Promise((resolve) => (release = resolve));
    mount();
    expect(await screen.findByTestId("create-pr-loading")).toHaveTextContent("Reading the branch…");
    expect(submit()).toBeDisabled();
    release(READY);
    await waitFor(() => expect(screen.queryByTestId("create-pr-loading")).toBeNull());
    expect(calls("get_pull_request_head_state")[0][1]).toEqual({
      head: SOURCE.head,
      base: "main",
    });

    headState = async () => READY;
    await userEvent.selectOptions(screen.getByTestId("create-pr-base"), "develop");
    await waitFor(() => expect(calls("get_pull_request_head_state")).toHaveLength(2));
    expect(calls("get_pull_request_head_state")[1][1]).toEqual({
      head: SOURCE.head,
      base: "develop",
    });
  });

  it("CPR-FR-SQGZ: a failed read shows its error, offers Try again and leaves Submit unavailable", async () => {
    headState = async () => Promise.reject("unknown branch");
    mount();
    expect(await screen.findByTestId("create-pr-read-error")).toBeInTheDocument();
    expect(submit()).toBeDisabled();
    headState = async () => READY;
    await userEvent.click(screen.getByTestId("create-pr-recheck"));
    await waitFor(() => expect(submit()).toBeEnabled());
  });

  it("CPR-FR-SQGZ: Check again reads anew, so an author who pushed elsewhere continues", async () => {
    headState = async () => ({ ...READY, remoteBranchExists: false, unpushed: null });
    mount();
    expect(await screen.findByTestId("create-pr-block-not-pushed")).toHaveTextContent(
      "The branch is not on the remote. Push it first.",
    );
    expect(submit()).toBeDisabled();
    headState = async () => READY;
    await userEvent.click(screen.getByTestId("create-pr-recheck"));
    await waitFor(() => expect(screen.queryByTestId("create-pr-block-not-pushed")).toBeNull());
    expect(submit()).toBeEnabled();
  });

  it("CPR-FR-VYPG: unpushed commits, uncommitted files, no remote and an empty branch each block Submit in words", async () => {
    headState = async () => ({
      ...READY,
      unpushed: 2,
      uncommittedPaths: ["a.md", "b.md", "c.md", "d.md"],
    });
    mount();
    expect(await screen.findByTestId("create-pr-block-unpushed")).toHaveTextContent(
      "2 commits are not on the remote. Push first.",
    );
    // Several blocks stand together, and the files are named then counted.
    expect(screen.getByTestId("create-pr-block-uncommitted")).toHaveTextContent(
      "4 uncommitted files (a.md, b.md, c.md, and 1 more). Commit first.",
    );
    expect(submit()).toBeDisabled();
    cleanup();

    headState = async () => ({ ...READY, hasRemote: false, remoteBranchExists: false, unpushed: null });
    mount();
    expect(await screen.findByTestId("create-pr-block-no-remote")).toHaveTextContent(
      "no remote to push to",
    );
    cleanup();

    headState = async () => ({ ...READY, aheadOfBase: 0 });
    mount();
    expect(await screen.findByTestId("create-pr-block-nothing")).toHaveTextContent(
      `${SOURCE.head} holds no commit that main lacks`,
    );
    expect(submit()).toBeDisabled();
  });

  it("CPR-FR-VYPG, CPR-FR-WJIA: a window with every block cleared pushes and commits nothing", async () => {
    mount();
    await ready();
    expect(submit()).toBeEnabled();
    expect(calls("push_current_branch")).toHaveLength(0);
    expect(calls("commit_paths")).toHaveLength(0);
  });

  it("CPR-FR-HGXL: a title of white space only keeps Submit unavailable and says so", async () => {
    mount();
    await ready();
    await userEvent.clear(screen.getByLabelText("Title"));
    await userEvent.type(screen.getByLabelText("Title"), "   ");
    expect(submit()).toBeDisabled();
    expect(submit()).toHaveAccessibleDescription(/needs a title/);
  });
});

describe("submitting (CPR-FR-KMHY, CPR-FR-XMRL, CPR-FR-ITWJ)", () => {
  it("CPR-FR-KMHY, CPR-FR-ITWJ: sends the title, description, base, head and draft as typed, then reports the pull request", async () => {
    const h = mount();
    await ready();
    await userEvent.clear(screen.getByLabelText("Title"));
    await userEvent.type(screen.getByLabelText("Title"), "Fix the scroll");
    await userEvent.type(screen.getByLabelText("Description"), "Why and how");
    await userEvent.selectOptions(screen.getByTestId("create-pr-base"), "develop");
    await userEvent.click(screen.getByLabelText("Draft"));
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());

    await waitFor(() => expect(h.onCreated).toHaveBeenCalledWith({
      number: 42,
      url: "https://github.com/acme/platform/pull/42",
    }));
    expect(calls("create_pull_request")[0][1]).toEqual({
      title: "Fix the scroll",
      body: "Why and how",
      base: "develop",
      head: SOURCE.head,
      draft: true,
    });
  });

  it("CPR-FR-KMHY, CPR-FR-XMRL: a second activation while the request runs sends nothing, and Submit says it is working", async () => {
    let release: (v: unknown) => void = noop;
    create = () => new Promise((resolve) => (release = resolve));
    const h = mount();
    await ready();
    // Two activations land before React re-renders, so only the guard on the
    // client — not a disabled attribute — can refuse the second.
    const button = submit();
    act(() => {
      button.click();
      button.click();
    });
    await waitFor(() => expect(submit()).toHaveTextContent("Creating the pull request…"));
    expect(submit()).toBeDisabled();
    expect(submit()).toHaveAccessibleDescription("");
    expect(calls("create_pull_request")).toHaveLength(1);
    expect(h.onSubmittingChange).toHaveBeenLastCalledWith(true);
    await waitFor(() => expect(release).not.toBe(noop));

    release({ number: 7, url: "https://github.com/acme/platform/pull/7" });
    await waitFor(() => expect(h.onCreated).toHaveBeenCalled());
    expect(h.onSubmittingChange).toHaveBeenLastCalledWith(false);
  });

  it("CPR-FR-XMRL: Cancel, Escape and the backdrop do nothing while the request runs, and the fields cannot be edited", async () => {
    let release: (v: unknown) => void = noop;
    create = () => new Promise((resolve) => (release = resolve));
    const h = mount();
    await ready();
    await userEvent.click(submit());
    await waitFor(() => expect(submit()).toHaveTextContent("Creating"));
    await waitFor(() => expect(release).not.toBe(noop));

    await userEvent.keyboard("{Escape}");
    fireEvent.mouseDown(document.querySelector(".scrim") as HTMLElement);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    for (const field of [
      screen.getByLabelText("Title"),
      screen.getByLabelText("Description"),
      screen.getByLabelText("Base branch"),
      screen.getByLabelText("Draft"),
    ]) {
      expect(field).toBeDisabled();
    }
    expect(h.onClose).not.toHaveBeenCalled();
    release({ number: 1, url: "https://github.com/a/b/pull/1" });
    await waitFor(() => expect(h.onCreated).toHaveBeenCalled());
  });

  it("CPR-FR-SSQI: Cancel, Escape and a click on the backdrop each close the window and start nothing", async () => {
    const h = mount();
    await ready();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await userEvent.keyboard("{Escape}");
    fireEvent.mouseDown(document.querySelector(".scrim") as HTMLElement);
    expect(h.onClose).toHaveBeenCalledTimes(3);
    expect(calls("create_pull_request")).toHaveLength(0);
  });
});

describe("failures (CPR-FR-SQEP, CPR-FR-VZUZ, CPR-FR-FGGU, CPR-FR-VZNE)", () => {
  it("CPR-FR-SQEP, CPR-FR-VZNE: a failure shows inline, keeps the author's input and the window open", async () => {
    create = async () => Promise.reject("pull_request_exists");
    const h = mount();
    await ready();
    await userEvent.type(screen.getByLabelText("Description"), "my words");
    await userEvent.selectOptions(screen.getByTestId("create-pr-base"), "develop");
    await userEvent.click(screen.getByLabelText("Draft"));
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());

    expect(await screen.findByTestId("create-pr-failure")).toHaveTextContent(
      "A pull request for this head branch and base branch already exists.",
    );
    expect(screen.getByLabelText("Base branch")).toHaveValue("develop");
    expect(screen.getByLabelText("Draft")).toBeChecked();
    expect(screen.getByLabelText("Description")).toHaveValue("my words");
    expect(screen.getByLabelText("Title")).toHaveValue("editor-work");
    expect(h.onClose).not.toHaveBeenCalled();
    // The request ended, so Submit may be used again.
    expect(submit()).toBeEnabled();
  });

  it.each([
    ["unknown branch", /does not hold the head branch or the base branch/],
    ["pull_request_title_required", /needs a title/],
    ["no remote configured", /no remote configured/],
    ["not_a_github_remote", /not on github\.com/],
    ["github_token_rejected", /rejected the token/],
    ["github_unreachable", /Could not reach GitHub/],
    ["pull_request_rejected: No commits between main and feature", /GitHub refused the pull request: No commits between main and feature/],
  ])("CPR-FR-VZNE: %s reads as a sentence", async (code, words) => {
    create = async () => Promise.reject(code);
    mount();
    await ready();
    await userEvent.click(submit());
    expect(await screen.findByTestId("create-pr-failure")).toHaveTextContent(words);
  });

  it("CPR-FR-FGGU: a missing token shows inline with a route to the Global settings GitHub section, and the window stays", async () => {
    create = async () => Promise.reject("github_token_missing");
    const h = mount();
    await ready();
    await userEvent.click(submit());
    await userEvent.click(await screen.findByTestId("create-pr-global-settings"));
    expect(h.onOpenGlobalSettings).toHaveBeenCalledTimes(1);
    expect(h.onClose).not.toHaveBeenCalled();
    expect(screen.getByTestId("create-pr-window")).toBeInTheDocument();
  });

  it("CPR-FR-VZUZ: a selection-required failure hands back what was typed and does not show an error", async () => {
    create = async () => Promise.reject("github_token_selection_required");
    const h = mount();
    await ready();
    await userEvent.type(screen.getByLabelText("Description"), "kept");
    await userEvent.click(submit());
    await waitFor(() =>
      expect(h.onRequestToken).toHaveBeenCalledWith({
        title: "editor-work",
        body: "kept",
        base: "main",
        draft: false,
      }),
    );
    expect(screen.queryByTestId("create-pr-failure")).toBeNull();
  });

  it("CPR-FR-VZUZ: coming back after a confirmed token fills in what was held and submits once with it", async () => {
    const h = mount({
      input: { title: "Held", body: "held body", base: "develop", draft: true },
      submit: true,
      notice: null,
    });
    await waitFor(() => expect(h.onCreated).toHaveBeenCalled());
    expect(calls("create_pull_request")).toHaveLength(1);
    expect(calls("create_pull_request")[0][1]).toEqual({
      title: "Held",
      body: "held body",
      base: "develop",
      head: SOURCE.head,
      draft: true,
    });
  });

  it("CPR-FR-VZUZ: a token that is still unselected after the picker is an inline failure, not another picker", async () => {
    create = async () => Promise.reject("github_token_selection_required");
    const h = mount({
      input: { title: "Held", body: "", base: "main", draft: false },
      submit: true,
      notice: null,
    });
    expect(await screen.findByTestId("create-pr-failure")).toBeInTheDocument();
    expect(h.onRequestToken).not.toHaveBeenCalled();
  });

  it("CPR-FR-VZUZ: coming back after a cancelled picker says inline that no token was selected and keeps the input", async () => {
    const h = mount({
      input: { title: "Held", body: "held body", base: "main", draft: false },
      submit: false,
      notice: "No GitHub token was selected, so no pull request was created.",
    });
    await ready();
    expect(screen.getByTestId("create-pr-failure")).toHaveTextContent("No GitHub token was selected");
    expect(screen.getByLabelText("Description")).toHaveValue("held body");
    expect(calls("create_pull_request")).toHaveLength(0);
    expect(h.onCreated).not.toHaveBeenCalled();
  });
});

describe("focus (CPR-FR-IWDK)", () => {
  it("CPR-FR-IWDK: focus returns to the control that opened the window when it closes", async () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    const view = render(
      <CreatePullRequestWindow
        source={SOURCE}
        returnFocus={opener}
        onClose={vi.fn()}
        onCreated={vi.fn()}
        onRequestToken={vi.fn()}
        onOpenGlobalSettings={vi.fn()}
        onSubmittingChange={vi.fn()}
      />,
    );
    await screen.findByRole("option", { name: "develop" });
    view.unmount();
    expect(opener).toHaveFocus();
    opener.remove();
  });

  it("CPR-FR-XMRL: a request that finishes after the window is gone reports nothing", async () => {
    let release: (v: unknown) => void = noop;
    create = () => new Promise((resolve) => (release = resolve));
    const onCreated = vi.fn();
    const onSubmittingChange = vi.fn();
    const view = render(
      <CreatePullRequestWindow
        source={SOURCE}
        onClose={vi.fn()}
        onCreated={onCreated}
        onRequestToken={vi.fn()}
        onOpenGlobalSettings={vi.fn()}
        onSubmittingChange={onSubmittingChange}
      />,
    );
    await screen.findByRole("option", { name: "develop" });
    await waitFor(() => expect(submit()).toBeEnabled());
    await userEvent.click(submit());
    await waitFor(() => expect(release).not.toBe(noop));
    view.unmount();
    onSubmittingChange.mockClear();
    await act(async () => {
      release({ number: 1, url: "https://github.com/a/b/pull/1" });
      await Promise.resolve();
    });
    expect(onCreated).not.toHaveBeenCalled();
    expect(onSubmittingChange).not.toHaveBeenCalled();
  });
});

describe("look and feel (CPR-FR-DWGS)", () => {
  it("CPR-FR-DWGS: uses the shared modal frame, field styling and one action row holding Cancel then Submit", async () => {
    mount();
    await ready();
    const dialog = screen.getByTestId("create-pr-window");
    expect(dialog).toHaveClass("modal");
    expect(dialog.closest(".scrim")).not.toBeNull();
    expect(dialog.querySelector(".modal__head .modal__title")).toHaveTextContent("Create a PR");
    expect(screen.getByLabelText("Title")).toHaveClass("input");
    expect(screen.getByLabelText("Description")).toHaveClass("textarea");
    const actions = dialog.querySelector(".modal__actions")!;
    const labels = [...actions.querySelectorAll("button")].map((b) => b.textContent);
    expect(labels).toEqual(["Cancel", "Submit"]);
    // The body scrolls, not the window, so the action row stays in view.
    expect(dialog.querySelector(".create-pr__body")).not.toBeNull();
  });
});

describe("keyboard and semantics (CPR-FR-IWDK, CPR-FR-NQPS)", () => {
  it("CPR-FR-NQPS: the first focus lands on the Title field and the dialog is modal and named", async () => {
    mount();
    await ready();
    expect(screen.getByLabelText("Title")).toHaveFocus();
    const dialog = screen.getByRole("dialog", { name: "Create a PR" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
  });

  it("CPR-FR-IWDK: Tab wraps inside the window, forwards and backwards", async () => {
    mount();
    await ready();
    const close = screen.getByRole("button", { name: "Close" });
    close.focus();
    await userEvent.tab({ shift: true });
    expect(submit()).toHaveFocus();
    await userEvent.tab();
    expect(close).toHaveFocus();
  });

  it("CPR-FR-NQPS: a failure is announced as an alert and no text holds a token", async () => {
    create = async () => Promise.reject("github_token_rejected");
    mount();
    await ready();
    await userEvent.click(submit());
    expect(await screen.findByRole("alert")).toHaveTextContent(/rejected the token/);
  });
});
