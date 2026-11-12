/**
 * The OpenSpec archive of a stream merge, as the selector shows it
 * (`../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-VMNV,
 * WSS-FR-BFJZ, WSS-FR-NTGC, WSS-FR-PLVE, WSS-FR-KMHD; `../../specifications/core/OAR-openspec-archive.md`
 * OAR-FR-SOFQ, OAR-FR-BEIK, OAR-FR-QTBK, OAR-FR-EUXZ).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorkStreamSelector } from "./WorkStreamSelector";
import { refusalText } from "./WorkStreamSelector/refusals";
import type { StreamMergeResult, WorkStreamSummary } from "../types";
import { conflictedResult, mergedResult, summary } from "../test/streamFixtures";
import { flushLogs, resetLogBufferForTest } from "../logging";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

/** The commands this file does not expect, each answered with a rejection. */
let unknown: string[] = [];

/**
 * Answer the listing, the merge, and the OpenSpec status. The reads the
 * selector and the merge confirmation make on the way answer with nothing.
 */
function backend(
  streams: WorkStreamSummary[],
  merge: () => Promise<StreamMergeResult>,
  openspecActive: boolean | (() => Promise<unknown>) = false,
) {
  const status =
    typeof openspecActive === "function"
      ? openspecActive
      : async () => ({ setting: "auto", usesOpenspec: true, active: openspecActive, cli: { state: "not_found" } });
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "append_log_records":
        return undefined;
      case "list_work_streams":
        return streams;
      case "merge_work_stream":
        return merge();
      case "get_openspec_status":
        return status();
      // WSS-FR-EPCH: a row reads the pull requests of the repository.
      case "list_pull_requests":
        return [];
      case "list_removed_work_streams":
      case "get_branch_write_guard":
        return undefined;
      default:
        unknown.push(cmd);
        throw new Error(`unexpected invoke ${cmd}`);
    }
  });
}

function renderSelector() {
  render(
    <WorkStreamSelector
      inRepository
      activeBranch="main"
      onOpen={vi.fn()}
      onActivate={vi.fn(async () => ({ ok: true as const }))}
      onRequestMergeCommit={vi.fn(async () => null as string | null)}
      onOpenChanges={vi.fn()}
      onOpenRun={vi.fn()}
    />,
  );
}

const openRow = async () => {
  await userEvent.click(await screen.findByTestId("stream-selector"));
  return within(await screen.findByTestId("stream-menu")).getByTestId("stream-row-w1");
};

const startUncommitted = async () => {
  const r = await openRow();
  await userEvent.click(within(r).getByRole("button", { name: "Merge stream" }));
  await userEvent.click(within(r).getByRole("button", { name: "Leave uncommitted" }));
  return r;
};

const archived = [
  { change: "add-logout", archivedAs: "2026-11-11-add-logout" },
  { change: "add-refunds", archivedAs: "2026-11-11-add-refunds" },
];

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  unknown = [];
});
afterEach(() => {
  cleanup();
  expect(unknown).toEqual([]);
});

describe("the merge confirmation (WSS-FR-VMNV)", () => {
  it("WSS-FR-VMNV: in OpenSpec mode it states that the changes the stream built are archived on it before Git merges", async () => {
    backend([summary({}, { aheadOfBase: 2 })], async () => mergedResult(), true);
    renderSelector();
    const r = await openRow();
    await userEvent.click(within(r).getByRole("button", { name: "Merge stream" }));
    const explainer = within(r).getByTestId("stream-merge-explainer");
    expect(
      await within(r).findByText(/the OpenSpec changes this stream built are archived on it/),
    ).toBe(explainer);
    expect(explainer.textContent).toMatch(/^Git merges first\. Before that/);
    expect(explainer.textContent).toMatch(/The base branch changes only after a review/);
  });

  /** Wait for the status read, and let its answer reach the confirmation. */
  async function statusAnswered() {
    await vi.waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "get_openspec_status")).toBe(true),
    );
    const at = invokeMock.mock.calls.findIndex((c) => c[0] === "get_openspec_status");
    await act(async () => {
      await (invokeMock.mock.results[at].value as Promise<unknown>).catch(() => {});
    });
  }

  it("WSS-FR-VMNV: a project that uses OpenSpec where the mode is not active says nothing about an archive", async () => {
    // `usesOpenspec` is true on purpose: `active` alone decides.
    backend([summary({}, { aheadOfBase: 2 })], async () => mergedResult(), false);
    renderSelector();
    const r = await openRow();
    await userEvent.click(within(r).getByRole("button", { name: "Merge stream" }));
    await statusAnswered();
    const explainer = within(r).getByTestId("stream-merge-explainer");
    expect(explainer.textContent).toMatch(/^Git merges first\. A merge Git settles/);
    expect(explainer.textContent).not.toMatch(/OpenSpec/);
  });

  it("WSS-FR-VMNV: a status read that fails counts as not active, and is logged", async () => {
    backend([summary({}, { aheadOfBase: 2 })], async () => mergedResult(), () =>
      Promise.reject(new Error("cli_unavailable")),
    );
    renderSelector();
    const r = await openRow();
    await userEvent.click(within(r).getByRole("button", { name: "Merge stream" }));
    await statusAnswered();
    await vi.waitFor(() => {
      flushLogs();
      const records = invokeMock.mock.calls
        .filter((c) => c[0] === "append_log_records")
        .flatMap((c) => (c[1] as { records: Array<{ level: string; message: string }> }).records);
      expect(records).toContainEqual(
        expect.objectContaining({
          level: "WARN",
          message: "merge confirmation could not read the openspec status",
        }),
      );
    });
    expect(within(r).getByTestId("stream-merge-explainer").textContent).not.toMatch(/OpenSpec/);
    // The confirmation stays usable.
    expect(within(r).getByRole("button", { name: "Leave uncommitted" })).toBeEnabled();
  });
});

describe("the archived changes of a merge (WSS-FR-BFJZ)", () => {
  it("WSS-FR-BFJZ: a merged result names each archived change with its archive name, in the order of the result", async () => {
    backend([summary({}, { aheadOfBase: 2 })], async () => mergedResult({ archived }));
    renderSelector();
    const r = await startUncommitted();
    const block = await within(r).findByTestId("stream-merge-archived-w1");
    expect(block).toHaveTextContent("Archived 2 OpenSpec changes on editor work");
    const items = within(block).getAllByRole("listitem").map((li) => li.textContent);
    expect(items).toEqual([
      "add-logout as 2026-11-11-add-logout",
      "add-refunds as 2026-11-11-add-refunds",
    ]);
    expect(within(r).getByTestId("stream-merge-result-w1").textContent).toMatch(/No run was made/);
  });

  it("WSS-FR-BFJZ, WSS-FR-PLVE: a conflicted result names its archived changes too, and says the base branch was not written", async () => {
    backend([summary({}, { aheadOfBase: 2 })], async () =>
      conflictedResult({ archived: [archived[1]] }),
    );
    renderSelector();
    const r = await startUncommitted();
    const block = await within(r).findByTestId("stream-merge-archived-w1");
    expect(block).toHaveTextContent("Archived 1 OpenSpec change on editor work");
    expect(within(block).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "add-refunds as 2026-11-11-add-refunds",
    ]);
    expect(within(r).getByTestId("stream-merge-result-w1").textContent).toMatch(
      /The base branch was not written/,
    );
  });

  it("WSS-FR-BFJZ, WSS-FR-KMHD: a merged result with a commit states the commit and the archived changes, both", async () => {
    const commit = "c0ffee".padEnd(40, "0");
    backend([summary({}, { aheadOfBase: 2 })], async () => mergedResult({ archived, commit }));
    renderSelector();
    const r = await startUncommitted();
    const block = await within(r).findByTestId("stream-merge-archived-w1");
    expect(within(block).getAllByRole("listitem")).toHaveLength(2);
    const result = within(r).getByTestId("stream-merge-result-w1");
    expect(within(result).getByLabelText(`Commit ${commit}`)).toBeInTheDocument();
    expect(result.textContent).not.toMatch(/left uncommitted/);
  });

  it("WSS-FR-BFJZ: a result that archived nothing names no archived change", async () => {
    backend([summary({}, { aheadOfBase: 2 })], async () => mergedResult());
    renderSelector();
    const r = await startUncommitted();
    await within(r).findByTestId("stream-merge-result-w1");
    expect(within(r).queryByTestId("stream-merge-archived-w1")).toBeNull();
  });
});

describe("an archive refusal (WSS-FR-NTGC)", () => {
  const refused = async (reason: string) => {
    backend([summary({}, { aheadOfBase: 2 })], () => Promise.reject(new Error(reason)));
    renderSelector();
    const r = await startUncommitted();
    return { r, alert: await within(r).findByRole("alert") };
  };

  it("WSS-FR-NTGC: change_has_pending names the change and each kind of pending item in words, on the row", async () => {
    const { r, alert } = await refused(
      "change_has_pending: add-logout: edit_proposal,new_change_proposal,action_request",
    );
    expect(alert).toHaveTextContent("The OpenSpec change “add-logout” holds");
    expect(alert).toHaveTextContent("an edit proposal that is not decided or not committed");
    expect(alert).toHaveTextContent("a new-change proposal that is not decided or not committed");
    expect(alert).toHaveTextContent("an action request that is not decided");
    expect(alert).not.toHaveTextContent("edit_proposal");
    expect(alert).toHaveTextContent("Nothing was archived or merged.");
    // The row routes to no other surface.
    expect(within(alert).queryByRole("button")).toBeNull();
    expect(within(r).queryByRole("button", { name: /Open/ })).toBeNull();
  });

  it("WSS-FR-NTGC: change_not_archivable names the change and its reason in words", async () => {
    const { alert } = await refused("change_not_archivable: add-logout: tasks_incomplete");
    expect(alert).toHaveTextContent(
      "The OpenSpec change “add-logout” cannot be archived: not all of its tasks are done.",
    );
    expect(alert).not.toHaveTextContent("tasks_incomplete");
  });

  it("WSS-FR-NTGC: archive_failed names the change and the code", async () => {
    const { alert } = await refused("archive_failed: add-refunds: commit_failed");
    expect(alert).toHaveTextContent("The archive of the OpenSpec change “add-refunds” failed with commit_failed.");
    expect(alert).toHaveTextContent("The base branch was not written.");
  });

  it("WSS-FR-NTGC: archive_failed shows the code of the CLI status as it came, also where it is a word of another refusal", async () => {
    const { alert } = await refused("archive_failed: add-refunds: tasks_incomplete");
    expect(alert).toHaveTextContent("The archive of the OpenSpec change “add-refunds” failed with tasks_incomplete.");
    expect(alert).not.toHaveTextContent("not all of its tasks are done");
    expect(refusalText("archive_failed: c: cli_output_unreadable")).toBe(
      "The archive of the OpenSpec change “c” failed with cli_output_unreadable. The base branch was not written.",
    );
  });

  it("WSS-FR-NTGC: every reason of change_not_archivable and every pending kind has its words", () => {
    const reasons: Record<string, string> = {
      openspec_mode_inactive: "OpenSpec mode is not active for this project",
      openspec_cli_unavailable: "the OpenSpec CLI is not available on this machine",
      apply_progress_unavailable: "its task progress could not be read",
      tasks_incomplete: "not all of its tasks are done",
    };
    for (const [reason, words] of Object.entries(reasons)) {
      expect(refusalText(`change_not_archivable: c: ${reason}`)).toContain(words);
    }
    expect(refusalText("change_has_pending: c: action_request")).toBe(
      "The OpenSpec change “c” holds an action request that is not decided. Decide it first, then merge again. Nothing was archived or merged.",
    );
    // A kind or a reason this surface does not know is named as it came.
    expect(refusalText("change_has_pending: c: other_kind")).toContain("other_kind");
    expect(refusalText("change_not_archivable: c: new_reason")).toContain("new_reason");
  });
});
