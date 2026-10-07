/**
 * The Create a PR action of a stream row
 * (`../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-KHGP,
 * WSS-FR-EPCH, WSS-FR-XZRD, WSS-FR-PSXK, WSS-FR-HGWL, WSS-FR-TKMB).
 *
 * The window itself is the shell's and has its own tests
 * (`CreatePullRequestWindow.test.tsx`); this file is about what the row offers
 * and what it hands the shell.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorkStreamSelector } from "./WorkStreamSelector";
import type { WorkStreamSummary } from "../types";
import { mergeRunLink, summary, updateRecord } from "../test/streamFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

function backend(streams: WorkStreamSummary[]) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_work_streams") return streams;
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
}

const onCreatePullRequest = vi.fn();

async function openRow(streams: WorkStreamSummary[]) {
  backend(streams);
  render(
    <WorkStreamSelector
      inRepository
      activeBranch="main"
      onOpen={vi.fn()}
      onActivate={vi.fn(async () => ({ ok: true as const }))}
      onRequestMergeCommit={vi.fn(async () => null as string | null)}
      onOpenChanges={vi.fn()}
      onOpenRun={vi.fn()}
      onCreatePullRequest={onCreatePullRequest}
    />,
  );
  await userEvent.click(await screen.findByTestId("stream-selector"));
  const menu = await screen.findByTestId("stream-menu");
  return within(menu).getByTestId("stream-row-w1");
}

beforeEach(() => {
  invokeMock.mockReset();
  onCreatePullRequest.mockReset();
});
afterEach(cleanup);

describe("Create a PR on a stream row", () => {
  it("WSS-FR-KHGP, WSS-FR-TKMB: sits between Merge stream and Update stream", async () => {
    const row = await openRow([summary({}, { aheadOfBase: 2, behindBase: 1 })]);
    const labels = within(row)
      .getAllByRole("button")
      .map((b) => b.textContent?.trim())
      .filter((l) => l && /^(Merge stream|Create a PR|Update stream|Delete stream)$/.test(l));
    expect(labels).toEqual(["Merge stream", "Create a PR", "Update stream", "Delete stream"]);
  });

  it("WSS-FR-KHGP, WSS-FR-XZRD: opens the window with the stream's branch, its base branch and its name, and closes the dropdown first", async () => {
    const row = await openRow([summary({}, { aheadOfBase: 2 })]);
    await userEvent.click(within(row).getByTestId("stream-create-pr-w1"));

    expect(onCreatePullRequest).toHaveBeenCalledWith({
      head: "synthesis/stream/editor-work",
      base: "main",
      title: "editor work",
    });
    expect(screen.queryByTestId("stream-menu")).toBeNull();
    // The row starts no request and reads no repository itself.
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("create_pull_request");
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("get_pull_request_head_state");
  });

  it("WSS-FR-EPCH: a stream that holds nothing its base lacks is disabled and says so in words", async () => {
    const row = await openRow([summary({}, { aheadOfBase: 0 })]);
    const button = within(row).getByTestId("stream-create-pr-w1");
    expect(button).toBeDisabled();
    expect(within(row).getByTestId("stream-create-pr-reason-w1")).toHaveTextContent(
      "Nothing to propose: this stream holds no commit that main lacks.",
    );
    await userEvent.click(button);
    expect(onCreatePullRequest).not.toHaveBeenCalled();
  });

  it("WSS-FR-EPCH: stays in place beside the other actions when disabled, so two rows keep one column", async () => {
    const row = await openRow([summary({}, { aheadOfBase: 0 })]);
    expect(within(row).getByTestId("stream-merge-w1")).toBeInTheDocument();
    expect(within(row).getByTestId("stream-create-pr-w1")).toBeInTheDocument();
    expect(within(row).getByTestId("stream-update-w1")).toBeInTheDocument();
  });

  it("WSS-FR-EPCH, WSS-FR-XRHT: a merge run that holds the stream disables it and names the run", async () => {
    const row = await openRow([
      summary({}, { aheadOfBase: 2, mergeRun: mergeRunLink("working") }),
    ]);
    expect(within(row).getByTestId("stream-create-pr-w1")).toBeDisabled();
    expect(within(row).getByTestId("stream-create-pr-reason-w1")).toHaveTextContent(
      "Merge editor work holds this stream. Open it in Runs.",
    );
  });

  it("WSS-FR-PSXK, WSS-FR-EPCH: a stream a run holds shows Create a PR disabled, with the reason in words, and the busy line", async () => {
    const row = await openRow([summary({ busyRunId: "r1" }, { aheadOfBase: 2 })]);
    const button = within(row).getByTestId("stream-create-pr-w1");
    expect(button).toBeDisabled();
    expect(within(row).getByTestId("stream-create-pr-reason-w1")).toHaveTextContent(
      "A run is working in this stream.",
    );
    expect(button).toHaveAccessibleDescription("A run is working in this stream.");
    expect(within(row).getByText(/is working in it/)).toBeInTheDocument();
    expect(within(row).queryByTestId("stream-merge-w1")).toBeNull();
    await userEvent.click(button);
    expect(onCreatePullRequest).not.toHaveBeenCalled();
  });

  it("WSS-FR-EPCH, WSS-FR-BDMU: a stream whose update is running shows Create a PR disabled beside Cancel update", async () => {
    const row = await openRow([
      summary({}, { aheadOfBase: 2, behindBase: 1, update: updateRecord("running") }),
    ]);
    const button = within(row).getByTestId("stream-create-pr-w1");
    expect(button).toBeDisabled();
    expect(within(row).getByTestId("stream-create-pr-reason-w1")).toHaveTextContent(
      "Updating editor work.",
    );
    expect(within(row).getByTestId("stream-update-cancel-w1")).toBeInTheDocument();
  });

  it("WSS-FR-EPCH, WSS-FR-OQYG: a missing stream shows Create a PR disabled and says it is missing", async () => {
    const row = await openRow([summary({ isMissing: true }, { aheadOfBase: 2 })]);
    expect(within(row).getByTestId("stream-create-pr-w1")).toBeDisabled();
    expect(within(row).getByTestId("stream-create-pr-reason-w1")).toHaveTextContent(
      "editor work is missing.",
    );
  });

  it("WSS-FR-RWLB: the disabled reason is the button's accessible description", async () => {
    const row = await openRow([summary({}, { aheadOfBase: 0 })]);
    expect(within(row).getByTestId("stream-create-pr-w1")).toHaveAccessibleDescription(
      /Nothing to propose/,
    );
  });
});
