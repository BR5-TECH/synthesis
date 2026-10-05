import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Git } from "./Git";
import type {
  PullRequestDetail,
  PullRequestListState,
  PullRequestSummary,
  PullRequestTimeline,
} from "../types";
import {
  FULL_TIMELINE,
  deferred,
  pullRequest,
  pullRequestDetail,
} from "../test/gitPanelFixtures";

// The PRs section (GIT-FR-OBZW, GIT-FR-FNQA, GIT-FR-LKRX, GIT-FR-EPSV).

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

let listRead: (state: PullRequestListState) => Promise<PullRequestSummary[]>;
let detailRead: (id: number) => Promise<PullRequestDetail>;
let timelineRead: (id: number) => Promise<PullRequestTimeline>;

beforeEach(() => {
  listRead = async (state) =>
    state === "open"
      ? [pullRequest(1), pullRequest(2, { isDraft: true })]
      : [
          pullRequest(7, { state: "merged" }),
          pullRequest(8, { state: "closed" }),
        ];
  detailRead = async (id) => pullRequestDetail(id);
  timelineRead = async () => FULL_TIMELINE;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "list_pull_requests":
        return listRead(args.state as PullRequestListState);
      case "get_pull_request_detail":
        return detailRead(args.id as number);
      case "list_pull_request_timeline":
        return timelineRead(args.id as number);
      case "get_project_github_token_binding":
        return { tokenId: "t1", resolution: "bound" };
      case "get_upstream_sync_state":
        return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const rows = () => screen.getAllByTestId("git-pr-row");

const openPrs = async (token?: () => Promise<boolean>) => {
  render(
    <Git
      onSwitchWorktree={vi.fn(async () => ({ ok: true as const }))}
      canCheckOutBranches
      onRequestGithubToken={token}
    />,
  );
  await userEvent.click(screen.getByRole("tab", { name: "PRs" }));
};

describe("the pull request list (GIT-FR-OBZW)", () => {
  it("GIT-FR-OBZW: opens on Open, and each row shows the number, the title, the author and the state in words", async () => {
    await openPrs();
    await screen.findAllByTestId("git-pr-row");

    expect(screen.getByRole("radio", { name: "Open" })).toBeChecked();
    expect(calls("list_pull_requests")[0][1]).toEqual({ state: "open" });
    expect(rows()).toHaveLength(2);
    expect(rows()[0]).toHaveTextContent("#1");
    expect(rows()[0]).toHaveTextContent("Pull request 1");
    expect(rows()[0]).toHaveTextContent("dev1");
    expect(within(rows()[0]).getByText("open")).toBeInTheDocument();
    expect(within(rows()[1]).getByText("draft")).toBeInTheDocument();
  });

  it("GIT-FR-OBZW: Closed lists closed pull requests and shows merged ones as merged", async () => {
    await openPrs();
    await screen.findAllByTestId("git-pr-row");
    await userEvent.click(screen.getByRole("radio", { name: "Closed" }));
    await waitFor(() => expect(rows()[0]).toHaveTextContent("Pull request 7"));

    expect(calls("list_pull_requests")[1][1]).toEqual({ state: "closed" });
    expect(within(rows()[0]).getByText("merged")).toBeInTheDocument();
    expect(within(rows()[1]).getByText("closed")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Closed" })).toBeChecked();
  });

  it("GIT-FR-05, GIT-FR-OBZW: keeps Create PR for current branch at the top of the rail", async () => {
    await openPrs();
    await screen.findAllByTestId("git-pr-row");
    const create = screen.getByRole("button", { name: /Create PR for current branch/ });
    const rail = screen.getByTestId("git-prs-rail");
    expect(
      create.compareDocumentPosition(rail) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    await userEvent.click(create);
    expect(await screen.findByTestId("git-auth-note-prs")).toHaveTextContent(
      "not wired to a remote",
    );
  });

  it("GIT-FR-OBZW: shows loading, empty and failure states in words, with a retry", async () => {
    const pending = deferred<PullRequestSummary[]>();
    listRead = () => pending.promise;
    await openPrs();
    expect(await screen.findByTestId("git-prs-loading")).toHaveTextContent(
      "Loading pull requests…",
    );
    await act(async () => pending.resolve([]));
    expect(await screen.findByTestId("git-prs-empty")).toHaveTextContent(
      "There are no open pull requests.",
    );
    cleanup();

    listRead = async () => {
      throw "github_unreachable";
    };
    await openPrs();
    expect(await screen.findByTestId("git-prs-error")).toHaveTextContent(
      "Could not reach GitHub.",
    );
    expect(screen.queryByTestId("git-prs-empty")).toBeNull();
    listRead = async () => [pullRequest(3)];
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findAllByTestId("git-pr-row");
    expect(screen.queryByTestId("git-prs-error")).toBeNull();
  });

  it("GIT-FR-LKRX, GIT-FR-EPSV: a list response of an older filter is discarded", async () => {
    const open = deferred<PullRequestSummary[]>();
    const closed = deferred<PullRequestSummary[]>();
    listRead = (state) => (state === "open" ? open.promise : closed.promise);
    await openPrs();
    // The filter switch stays usable while the first read runs.
    await userEvent.click(screen.getByRole("radio", { name: "Closed" }));

    await act(async () => closed.resolve([pullRequest(8, { state: "closed" })]));
    expect(await screen.findByText("Pull request 8")).toBeInTheDocument();
    await act(async () => open.resolve([pullRequest(1)]));
    expect(screen.queryByText("Pull request 1")).toBeNull();
    expect(screen.getByText("Pull request 8")).toBeInTheDocument();
  });
});

describe("the pull request view (GIT-FR-FNQA)", () => {
  const selectFirst = async () => {
    await openPrs();
    await screen.findAllByTestId("git-pr-row");
    await userEvent.click(rows()[0]);
  };

  it("GIT-FR-05, GIT-FR-FNQA: loads the detail and the timeline together and shows title, state, branches, link and the description as Markdown", async () => {
    const detail = deferred<PullRequestDetail>();
    const timeline = deferred<PullRequestTimeline>();
    detailRead = () => detail.promise;
    timelineRead = () => timeline.promise;
    await openPrs();
    await screen.findAllByTestId("git-pr-row");
    await userEvent.click(rows()[0]);

    // Both reads started before either answered.
    expect(calls("get_pull_request_detail")[0][1]).toEqual({ id: 1 });
    expect(calls("list_pull_request_timeline")[0][1]).toEqual({ id: 1 });
    expect(rows()[0]).toHaveAttribute("aria-selected", "true");
    expect(within(rows()[0]).getByText("selected")).toBeInTheDocument();
    expect(await screen.findByTestId("git-pr-loading")).toBeInTheDocument();
    expect(screen.getByTestId("git-pr-timeline-loading")).toBeInTheDocument();

    await act(async () => {
      detail.resolve(pullRequestDetail(1));
      timeline.resolve(FULL_TIMELINE);
    });
    const header = await screen.findByTestId("git-pr-header");
    expect(header).toHaveTextContent("Pull request 1");
    expect(within(header).getByText("open")).toBeInTheDocument();
    expect(within(header).getByText("topic-1 → main")).toBeInTheDocument();
    expect(within(header).getByText("https://github.com/acme/app/pull/1")).toBeInTheDocument();
    const description = screen.getByTestId("git-pr-description");
    expect(within(description).getByText("pull request 1").tagName).toBe("STRONG");
  });

  it("GIT-FR-FNQA: renders Markdown safely and never as raw HTML", async () => {
    detailRead = async (id) =>
      pullRequestDetail(id, {
        body: '<img src=x onerror="alert(1)"> and <script>alert(2)</script> text',
      });
    await selectFirst();
    const description = await screen.findByTestId("git-pr-description");
    expect(description.querySelector("img")).toBeNull();
    expect(description.querySelector("script")).toBeNull();
    expect(description).toHaveTextContent("text");
  });

  it("GIT-FR-FNQA: a description that is empty says so", async () => {
    detailRead = async (id) => pullRequestDetail(id, { body: "" });
    await selectFirst();
    expect(await screen.findByTestId("git-pr-description")).toHaveTextContent(
      "This pull request has no description.",
    );
  });

  it("GIT-FR-FNQA: shows the timeline in the order it arrived, each kind in its own form", async () => {
    await selectFirst();
    await screen.findByTestId("git-pr-item-comment");
    const items = within(screen.getByLabelText("Timeline")).getAllByRole("listitem");
    expect(items.map((i) => i.getAttribute("data-kind"))).toEqual([
      "comment",
      "review",
      "review_comment",
      "commit",
      "event",
    ]);

    const comment = screen.getByTestId("git-pr-item-comment");
    expect(within(comment).getByText("Comment")).toBeInTheDocument();
    expect(within(comment).getByText("ann")).toBeInTheDocument();
    expect(within(comment).getByText("comment").tagName).toBe("STRONG");
    expect(comment.querySelector("time")).toHaveAttribute("datetime", "2026-09-01T11:00:00Z");

    const review = screen.getByTestId("git-pr-item-review");
    expect(within(review).getByText("Review")).toBeInTheDocument();
    expect(within(review).getByText("Changes requested")).toBeInTheDocument();
    expect(review).toHaveTextContent("Please change this.");

    const reviewComment = screen.getByTestId("git-pr-item-review_comment");
    expect(within(reviewComment).getByText("Review comment")).toBeInTheDocument();
    expect(within(reviewComment).getByText("src/lib.rs")).toBeInTheDocument();
    expect(reviewComment).toHaveTextContent("This line needs a test.");

    const commit = screen.getByTestId("git-pr-item-commit");
    expect(within(commit).getByText("abcdef1")).toBeInTheDocument();
    expect(within(commit).getByText("Add the missing test")).toBeInTheDocument();

    const event = screen.getByTestId("git-pr-item-event");
    expect(within(event).getByText("Event")).toBeInTheDocument();
    expect(within(event).getByText("ready for review")).toBeInTheDocument();
  });

  it("GIT-FR-FNQA: a truncated timeline says so, and an empty one says so", async () => {
    timelineRead = async () => ({ ...FULL_TIMELINE, truncated: true });
    await selectFirst();
    expect(await screen.findByTestId("git-pr-truncated")).toHaveTextContent("truncated");
    cleanup();

    timelineRead = async () => ({ items: [], truncated: false });
    await selectFirst();
    expect(await screen.findByTestId("git-pr-timeline-empty")).toHaveTextContent(
      "no activity yet",
    );
    expect(screen.queryByTestId("git-pr-truncated")).toBeNull();
  });

  it("GIT-FR-FNQA: the view scrolls inside the panel", async () => {
    await selectFirst();
    const scroll = await screen.findByTestId("git-pr-scroll");
    expect(scroll.className).toContain("git-pr__scroll");
    expect(scroll.closest(".git__right")).not.toBeNull();
  });

  it("GIT-FR-FNQA: a failed detail read shows in words with a retry and leaves the timeline and the rail usable", async () => {
    detailRead = async () => {
      throw "pull_request_not_found";
    };
    await selectFirst();
    expect(await screen.findByTestId("git-pr-error")).toHaveTextContent(
      "That pull request does not exist any more.",
    );
    expect(await screen.findByTestId("git-pr-item-comment")).toBeInTheDocument();
    expect(rows()).toHaveLength(2);

    detailRead = async (id) => pullRequestDetail(id);
    await userEvent.click(within(screen.getByTestId("git-pr-error")).getByRole("button", { name: "Retry" }));
    expect(await screen.findByTestId("git-pr-header")).toBeInTheDocument();
    expect(screen.queryByTestId("git-pr-error")).toBeNull();
  });

  it("GIT-FR-FNQA, GIT-FR-LKRX: a failed timeline read shows beside a good description", async () => {
    timelineRead = async () => {
      throw "github_unreachable";
    };
    await selectFirst();
    expect(await screen.findByTestId("git-pr-timeline-error")).toHaveTextContent(
      "Could not reach GitHub.",
    );
    expect(screen.getByTestId("git-pr-header")).toBeInTheDocument();
  });

  it("GIT-FR-LKRX, GIT-FR-EPSV: the response of an older selection is discarded", async () => {
    const first = deferred<PullRequestDetail>();
    const second = deferred<PullRequestDetail>();
    detailRead = (id) => (id === 1 ? first.promise : second.promise);
    await selectFirst();
    await userEvent.click(rows()[1]);

    await act(async () => second.resolve(pullRequestDetail(2)));
    expect(await screen.findByTestId("git-pr-header")).toHaveTextContent("Pull request 2");
    await act(async () => first.resolve(pullRequestDetail(1)));
    expect(screen.getByTestId("git-pr-header")).toHaveTextContent("Pull request 2");
    expect(screen.getByTestId("git-pr-header")).not.toHaveTextContent("Pull request 1");
  });

  it("GIT-FR-OBZW: another filter clears the selection", async () => {
    await selectFirst();
    await screen.findByTestId("git-pr-header");
    await userEvent.click(screen.getByRole("radio", { name: "Closed" }));
    expect(await screen.findByTestId("git-pr-none")).toBeInTheDocument();
    expect(screen.queryByTestId("git-pr-header")).toBeNull();
  });
});

describe("a failed refresh keeps what was read (GIT-FR-LKRX)", () => {
  it("GIT-FR-LKRX: a failed list refresh keeps the rows, marks them stale and shows the error above them", async () => {
    await openPrs();
    await screen.findAllByTestId("git-pr-row");
    listRead = async () => {
      throw "github_unreachable";
    };
    await userEvent.click(screen.getByRole("button", { name: "Refresh pull requests" }));

    const error = await screen.findByTestId("git-prs-error");
    expect(error).toHaveTextContent("Could not reach GitHub.");
    expect(screen.getByTestId("git-prs-stale")).toHaveTextContent("stale");
    expect(rows()).toHaveLength(2);
    expect(screen.queryByTestId("git-prs-empty")).toBeNull();
    // The error stands above the rows.
    expect(
      error.compareDocumentPosition(rows()[0]) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    // A later good read clears both.
    listRead = async () => [pullRequest(1)];
    await userEvent.click(within(error).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.queryByTestId("git-prs-error")).toBeNull());
    expect(screen.queryByTestId("git-prs-stale")).toBeNull();
  });

  it("GIT-FR-LKRX: a failed detail refresh keeps the description, marks it stale and shows the error above it", async () => {
    await openPrs();
    await screen.findAllByTestId("git-pr-row");
    await userEvent.click(rows()[0]);
    await screen.findByTestId("git-pr-header");
    detailRead = async () => {
      throw "github_unreachable";
    };
    await userEvent.click(screen.getByRole("button", { name: "Refresh pull request" }));

    const error = await screen.findByTestId("git-pr-error");
    expect(screen.getByTestId("git-pr-stale")).toBeInTheDocument();
    expect(screen.getByTestId("git-pr-header")).toBeInTheDocument();
    expect(
      error.compareDocumentPosition(screen.getByTestId("git-pr-header")) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});

describe("the GitHub token (GIT-FR-10, GIT-FR-LKRX)", () => {
  it("GIT-FR-10, GIT-FR-LKRX: a selection-required failure opens the picker and the read runs again on confirmation", async () => {
    listRead = vi
      .fn<(state: PullRequestListState) => Promise<PullRequestSummary[]>>()
      .mockRejectedValueOnce("github_token_selection_required")
      .mockResolvedValue([pullRequest(1)]);
    const picker = vi.fn(async () => true);
    await openPrs(picker);
    await screen.findAllByTestId("git-pr-row");
    expect(picker).toHaveBeenCalledOnce();
    expect(calls("list_pull_requests")).toHaveLength(2);
  });

  it("GIT-FR-10, GIT-FR-LKRX: the detail and the timeline of one selection ask for the token once", async () => {
    detailRead = vi
      .fn<(id: number) => Promise<PullRequestDetail>>()
      .mockRejectedValueOnce("github_token_selection_required")
      .mockImplementation(async (id) => pullRequestDetail(id));
    timelineRead = vi
      .fn<(id: number) => Promise<PullRequestTimeline>>()
      .mockRejectedValueOnce("github_token_selection_required")
      .mockResolvedValue(FULL_TIMELINE);
    const picker = vi.fn(async () => true);
    await openPrs(picker);
    await screen.findAllByTestId("git-pr-row");
    await userEvent.click(rows()[0]);

    expect(await screen.findByTestId("git-pr-header")).toBeInTheDocument();
    expect(await screen.findByTestId("git-pr-item-comment")).toBeInTheDocument();
    expect(picker).toHaveBeenCalledOnce();
  });

  it("GIT-FR-10: cancelling the picker abandons the read and says so, with a retry", async () => {
    listRead = async () => {
      throw "github_token_selection_required";
    };
    const picker = vi.fn(async () => false);
    await openPrs(picker);
    expect(await screen.findByTestId("git-prs-error")).toHaveTextContent(
      "No GitHub token was selected",
    );
    expect(calls("list_pull_requests")).toHaveLength(1);
    expect(screen.queryByTestId("git-prs-empty")).toBeNull();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  it("GIT-FR-10: a missing token renders inline with the route to the Global settings GitHub section and no picker", async () => {
    listRead = async () => {
      throw "github_token_missing";
    };
    const picker = vi.fn(async () => true);
    await openPrs(picker);
    expect(await screen.findByTestId("git-prs-error")).toHaveTextContent(
      "No GitHub token is stored. Add one in Global settings → GitHub.",
    );
    expect(picker).not.toHaveBeenCalled();
  });
});
