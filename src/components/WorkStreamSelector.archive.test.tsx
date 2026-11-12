/**
 * The OpenSpec archive of a stream merge, as the selector shows it
 * (`../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-VMNV,
 * WSS-FR-BFJZ, WSS-FR-NTGC, WSS-FR-PLVE, WSS-FR-KMHD; `../../specifications/core/OAR-openspec-archive.md`
 * OAR-FR-SOFQ, OAR-FR-BEIK, OAR-FR-QTBK, OAR-FR-EUXZ).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorkStreamSelector } from "./WorkStreamSelector";
import { refusalText } from "./WorkStreamSelector/refusals";
import type { StreamMergeResult, WorkStreamSummary } from "../types";
import { conflictedResult, mergedResult, summary } from "../test/streamFixtures";
import { flushLogs, resetLogBufferForTest } from "../logging";
import { configureOpenSpecChangeRoutes } from "../state/openspec/changeRoutes";
import { configureRunRoute } from "../state/graduation/runRoute";

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
  configureOpenSpecChangeRoutes(null);
  configureRunRoute(null);
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
  const refused = async (reason: unknown) => {
    backend([summary({}, { aheadOfBase: 2 })], () =>
      Promise.reject(typeof reason === "string" ? new Error(reason) : reason),
    );
    renderSelector();
    const r = await startUncommitted();
    return { r, alert: await within(r).findByRole("alert") };
  };

  const FRAME = "The base branch did not move, and the archive commits stay on the stream.";

  it("WSS-FR-NTGC: change_has_pending names the change and each kind of pending item in words, on the row", async () => {
    const { r, alert } = await refused(
      "change_has_pending: add-logout: edit_proposal,new_change_proposal,action_request",
    );
    expect(alert).toHaveTextContent("The OpenSpec change “add-logout” holds");
    expect(alert).toHaveTextContent("an edit proposal that is not decided or not committed");
    expect(alert).toHaveTextContent("a new-change proposal that is not decided or not committed");
    expect(alert).toHaveTextContent("an action request that is not decided");
    expect(alert).not.toHaveTextContent("edit_proposal");
    expect(alert).toHaveTextContent(FRAME);
    // The row routes to no other surface.
    expect(within(alert).queryByRole("button")).toBeNull();
    expect(within(r).queryByRole("button", { name: /Open/ })).toBeNull();
  });

  it("WSS-FR-NTGC, OFN-FR-RLSO: change_not_archivable renders its notice, and Open change opens the change on the stream", async () => {
    const opened = vi.fn();
    configureOpenSpecChangeRoutes({ open: opened });
    const { r, alert } = await refused({
      code: "change_not_archivable",
      cause: "tasks_incomplete",
      change: "add-logout",
      diagnostics: [],
    });
    expect(alert).toHaveTextContent(`The change "add-logout" has tasks that are not done. ${FRAME}`);
    // OFN-FR-PFYW: one statement of the failure, with no code.
    expect(r).not.toHaveTextContent("tasks_incomplete");
    expect(within(r).queryByRole("button", { name: "Retry" })).toBeNull();
    await userEvent.click(within(r).getByRole("button", { name: "Open change" }));
    expect(opened).toHaveBeenCalledWith(
      { kind: "stream", streamId: "w1" },
      "add-logout",
      expect.objectContaining({ thread: null }),
    );
  });

  it("WSS-FR-NTGC, OFN-FR-RCWY, OFN-FR-IHSQ, OFN-FR-HEVB: a repair cause names its repair run, opens its thread and its run, and shows the CLI messages", async () => {
    const opened = vi.fn();
    const openRun = vi.fn();
    configureOpenSpecChangeRoutes({ open: opened });
    configureRunRoute(openRun);
    const { r, alert } = await refused({
      code: "archive_failed",
      cause: "archive_spec_update_failed",
      change: "add-logout",
      threadId: "t-9",
      repairRunId: "g-12",
      diagnostics: [
        {
          severity: "error",
          code: "archive_spec_update_failed",
          message: 'MODIFIED "Session Expiration" not found in auth',
          fix: "Use ADDED for a new requirement",
        },
      ],
    });
    expect(alert).toHaveTextContent(
      `The deltas of "add-logout" do not apply to the live specifications. A rework run started to repair the change. ${FRAME}`,
    );
    await userEvent.click(within(r).getByRole("button", { name: /Details/ }));
    expect(within(r).getByTestId("openspec-failure-diagnostic").textContent).toBe(
      'error archive_spec_update_failed: MODIFIED "Session Expiration" not found in auth\nFix: Use ADDED for a new requirement',
    );
    expect(within(r).getByRole("button", { name: "Open run" })).toBeInTheDocument();
    // SNV-FR-56: the change opens behind the dropdown, so the dropdown closes.
    await userEvent.click(within(r).getByRole("button", { name: "Open change" }));
    expect(opened).toHaveBeenCalledWith(
      { kind: "stream", streamId: "w1" },
      "add-logout",
      expect.objectContaining({ thread: "t-9" }),
    );
    await waitFor(() => expect(screen.queryByTestId("stream-menu")).toBeNull());
  });

  it("WSS-FR-NTGC, OFN-FR-RCWY: Open run opens the repair run and closes the dropdown", async () => {
    const openRun = vi.fn();
    configureRunRoute(openRun);
    const { r } = await refused({
      code: "archive_failed",
      cause: "archive_spec_update_failed",
      change: "add-logout",
      repairRunId: "g-12",
      diagnostics: [],
    });
    await userEvent.click(within(r).getByRole("button", { name: "Open run" }));
    expect(openRun).toHaveBeenCalledWith("g-12");
    await waitFor(() => expect(screen.queryByTestId("stream-menu")).toBeNull());
  });

  it("WSS-FR-NTGC, OFN-FR-LRCZ: a repair start that was refused is named in words", async () => {
    const { alert } = await refused({
      code: "archive_failed",
      cause: "archive_validation_failed",
      change: "add-logout",
      threadId: "t-9",
      repairRefusal: "proposal_pending",
      diagnostics: [],
    });
    expect(alert).toHaveTextContent(
      'The change "add-logout" is not valid. No repair run started: the change holds a pending edit proposal.',
    );
  });

  it("WSS-FR-NTGC, OFN-FR-CBLG: a transient refusal offers Retry, which re-checks the CLI and merges again with the same publication", async () => {
    let merges = 0;
    const statusArgs: unknown[] = [];
    backend(
      [summary({}, { aheadOfBase: 2 })],
      async () => {
        merges += 1;
        if (merges === 1) {
          throw { code: "change_not_archivable", cause: "cli_timed_out", change: "add-logout", diagnostics: [] };
        }
        return mergedResult();
      },
      false,
    );
    const route = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_openspec_status") statusArgs.push(args);
      return route(cmd, args);
    });
    renderSelector();
    const r = await startUncommitted();
    const alert = await within(r).findByRole("alert");
    expect(alert).toHaveTextContent("The OpenSpec CLI did not finish in time, also after one more try.");
    statusArgs.length = 0;
    await userEvent.click(within(r).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(merges).toBe(2));
    await waitFor(() => expect(within(r).queryByRole("alert")).toBeNull());
    expect(statusArgs).toEqual([{ recheck: true }]);
    const mergeCalls = invokeMock.mock.calls.filter((c) => c[0] === "merge_work_stream");
    expect(mergeCalls[1][1]).toEqual(mergeCalls[0][1]);
  });

  it("WSS-FR-NTGC: the row that states a refusal is scrolled into view when the refusal arrives, and not before", async () => {
    const scrollIntoView = vi.fn();
    Element.prototype.scrollIntoView = scrollIntoView;
    try {
      let refuse: (reason: unknown) => void = () => {};
      backend([summary({}, { aheadOfBase: 2 })], () => new Promise((_, reject) => (refuse = reject)));
      renderSelector();
      const r = await startUncommitted();
      await within(r).findByTestId("stream-merging-w1");
      expect(scrollIntoView).not.toHaveBeenCalled();
      await act(async () =>
        refuse({ code: "change_not_archivable", cause: "tasks_incomplete", change: "add-logout", diagnostics: [] }),
      );
      await within(r).findByRole("alert");
      expect(scrollIntoView).toHaveBeenCalledTimes(1);
      expect(scrollIntoView).toHaveBeenCalledWith({ block: "nearest" });
      expect(scrollIntoView.mock.contexts[0]).toBe(r);
    } finally {
      delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
    }
  });

  it("WSS-FR-NTGC, OFN-FR-ONNF: a merge started by keyboard keeps the focus on its row while it runs and when it is refused", async () => {
    let refuse: (reason: unknown) => void = () => {};
    backend([summary({}, { aheadOfBase: 2 })], () => new Promise((_, reject) => (refuse = reject)));
    renderSelector();
    const r = await openRow();
    await userEvent.click(within(r).getByRole("button", { name: "Merge stream" }));
    within(r).getByRole("button", { name: "Leave uncommitted" }).focus();
    await userEvent.keyboard("{Enter}");
    await within(r).findByTestId("stream-merging-w1");
    // The confirmation left the page, so the row holds the focus.
    await waitFor(() => expect(document.activeElement).toBe(r));
    await act(async () =>
      refuse({ code: "change_not_archivable", cause: "cli_failed", change: "add-logout", diagnostics: [] }),
    );
    await within(r).findByRole("alert");
    expect(document.activeElement).toBe(r);
  });

  it("WSS-FR-NTGC, OFN-FR-CBLG: the Retry of the row is disabled while the merge it runs again is in flight", async () => {
    let merges = 0;
    let release: () => void = () => {};
    backend(
      [summary({}, { aheadOfBase: 2 })],
      () => {
        merges += 1;
        if (merges === 1) {
          return Promise.reject({ code: "change_not_archivable", cause: "cli_failed", change: "add-logout", diagnostics: [] });
        }
        return new Promise<StreamMergeResult>((resolve) => {
          release = () => resolve(mergedResult());
        });
      },
      false,
    );
    renderSelector();
    const r = await startUncommitted();
    await within(r).findByRole("alert");
    const retry = within(r).getByRole("button", { name: "Retry" });
    expect(retry).not.toHaveAttribute("aria-disabled");
    await userEvent.click(retry);
    await waitFor(() => expect(merges).toBe(2));
    expect(retry).toHaveAttribute("aria-disabled", "true");
    // A second press while the merge is in flight merges nothing more.
    await userEvent.click(retry);
    expect(merges).toBe(2);
    await act(async () => release());
    await waitFor(() => expect(within(r).queryByRole("alert")).toBeNull());
    expect(merges).toBe(2);
  });

  it("WSS-FR-NTGC, OFN-FR-PVYE: archive_failed with a cause that is not an OpenSpec failure names the code as it came", async () => {
    const { r, alert } = await refused("archive_failed: add-refunds: commit_failed");
    expect(alert).toHaveTextContent(
      `The archive of the OpenSpec change “add-refunds” failed with commit_failed. ${FRAME}`,
    );
    expect(within(r).queryByTestId("openspec-failure-notice")).toBeNull();
  });

  it("WSS-FR-NTGC, OFN-FR-PVYE, OFP-FR-FKRV: archive_failed as an object with a cause that is not an OpenSpec failure names its change and the code as it came", async () => {
    const { r, alert } = await refused({
      code: "archive_failed",
      cause: "commit_failed",
      change: "add-refunds",
      diagnostics: [],
    });
    expect(alert).toHaveTextContent(
      `The archive of the OpenSpec change “add-refunds” failed with commit_failed. ${FRAME}`,
    );
    expect(within(r).queryByTestId("openspec-failure-notice")).toBeNull();
  });

  it("WSS-FR-NTGC, OFP-FR-FKRV: archive_failed as an object with no cause gets no notice", async () => {
    const { r, alert } = await refused({ code: "archive_failed", change: "add-refunds", diagnostics: [] });
    expect(within(r).queryByTestId("openspec-failure-notice")).toBeNull();
    expect(alert).toHaveTextContent(FRAME);
    expect(alert).not.toHaveTextContent("OpenSpec could not archive");
  });

  it("WSS-FR-NTGC: the typed strings of the archive refusals read as the short form of their notice", () => {
    expect(refusalText("change_not_archivable: c: tasks_incomplete")).toBe(
      `The change "c" has tasks that are not done. ${FRAME}`,
    );
    expect(refusalText("archive_failed: c: cli_output_unreadable")).toBe(
      `The OpenSpec CLI gave an answer that the application cannot read. Check the version of the OpenSpec CLI. ${FRAME}`,
    );
    expect(refusalText("archive_failed: c: archive_new_code")).toBe(
      `OpenSpec could not archive "c". ${FRAME}`,
    );
    expect(refusalText("change_has_pending: c: action_request")).toBe(
      `The OpenSpec change “c” holds an action request that is not decided. Decide it first, then merge again. ${FRAME}`,
    );
    // A kind this surface does not know is named as it came.
    expect(refusalText("change_has_pending: c: other_kind")).toContain("other_kind");
  });
});
