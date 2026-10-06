/**
 * Merging a stream from the selector: the call, its answer, and the merge run a
 * conflict becomes
 * (`../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-HGWL,
 * WSS-FR-OFCU, WSS-FR-RJTN, WSS-FR-GBWE, WSS-FR-NRCQ, WSS-FR-NPXC,
 * WSS-FR-KMHD, WSS-FR-PLVE, WSS-FR-AWRS, WSS-FR-DTYB, WSS-FR-XRHT,
 * WSS-FR-JBYF).
 *
 * Apart from `WorkStreamSelector.test.tsx` because the merge has three
 * stations — the call that is out, the answer of a call that settled, and the
 * merge run the listing links — and one file holding all of them with the
 * rest of the selector outgrows what anyone reads in a sitting.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorkStreamSelector } from "./WorkStreamSelector";
import { STREAM_ERRORS, type GraduationRunState, type WorkStreamSummary } from "../types";
import {
  conflictedResult,
  mergedResult,
  mergeRunLink,
  summary,
} from "../test/streamFixtures";

// The selector reaches the backend only through `invoke` (`../api`).
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type EventHandler = (ev: { payload: unknown }) => void;
let handlers: [string, EventHandler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    handlers.push([name, handler]);
    return Promise.resolve(() => {
      handlers = handlers.filter(([, h]) => h !== handler);
    });
  },
  emit: () => Promise.resolve(),
}));

/** What `list_work_streams` answers next, replaced by a test to move the listing. */
let listing: WorkStreamSummary[] = [];

/** Answer `list_work_streams` from `listing`, a merge with `merge`, the rest with nothing. */
function backend(
  streams: WorkStreamSummary[],
  merge: (args: unknown) => unknown = () => ({ kind: "nothing_to_merge" }),
) {
  listing = streams;
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "list_work_streams") return listing;
    if (cmd === "merge_work_stream") return merge(args);
    // WSS-FR-BRMT: opening a delete reads the stream's uncommitted paths.
    if (cmd === "get_work_stream_uncommitted_paths") return [];
    return undefined;
  });
}

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function renderSelector(over: Partial<Parameters<typeof WorkStreamSelector>[0]> = {}) {
  const onOpenRun = vi.fn();
  const onActivate = vi.fn(async () => ({ ok: true as const }));
  render(
    <WorkStreamSelector
      inRepository
      activeBranch="main"
      onOpen={vi.fn()}
      onActivate={onActivate}
      onRequestMergeCommit={vi.fn(async () => null as string | null)}
      onOpenChanges={vi.fn()}
      onOpenRun={onOpenRun}
      {...over}
    />,
  );
  return { onOpenRun, onActivate };
}

const openMenu = async () => {
  await userEvent.click(await screen.findByTestId("stream-selector"));
  return screen.findByTestId("stream-menu");
};

const row = async () => within(await openMenu()).getByTestId("stream-row-w1");

/** Open the confirmation and take the route that leaves the result uncommitted. */
const startUncommitted = async () => {
  const r = await row();
  await userEvent.click(within(r).getByRole("button", { name: "Merge…" }));
  await userEvent.click(within(r).getByRole("button", { name: "Leave uncommitted" }));
  return r;
};

beforeEach(() => {
  invokeMock.mockReset();
  handlers = [];
  listing = [];
});
afterEach(cleanup);

describe("a merge call that is out (WSS-FR-HGWL)", () => {
  /** Never settles on its own, so the row is looked at while the call is out. */
  const pending = () => {
    let settle: (value: unknown) => void = () => {};
    let refuse: (reason: unknown) => void = () => {};
    backend([summary({}, { aheadOfBase: 2 })], () =>
      new Promise((resolve, reject) => {
        settle = resolve;
        refuse = reject;
      }),
    );
    return { settle: (v: unknown) => settle(v), refuse: (r: unknown) => refuse(r) };
  };

  it("WSS-FR-HGWL: the row says Merging and carries a busy status while the call is out", async () => {
    pending();
    renderSelector();
    const r = await startUncommitted();

    const status = await within(r).findByTestId("stream-merging-w1");
    expect(status).toHaveTextContent("Merging editor work");
    expect(status).toHaveAttribute("role", "status");
    expect(status).toHaveAttribute("aria-busy", "true");
  });

  it("WSS-FR-HGWL: Merge, Update, Delete and selection are disabled, and the row offers no cancel", async () => {
    pending();
    renderSelector();
    const r = await startUncommitted();
    await within(r).findByTestId("stream-merging-w1");

    expect(within(r).getByTestId("stream-merge-w1")).toBeDisabled();
    expect(within(r).getByTestId("stream-update-w1")).toBeDisabled();
    expect(within(r).getByTestId("stream-delete-w1")).toBeDisabled();
    expect(
      within(r).getByRole("menuitem", { name: /Open the work stream/ }),
    ).toBeDisabled();
    // No control of the row stops the call: the merge is Git's check, not a run.
    expect(within(r).queryByRole("button", { name: /Cancel/ })).toBeNull();
  });

  it("WSS-FR-HGWL: the state outlives the dropdown and ends when the call settles", async () => {
    const call = pending();
    renderSelector();
    await startUncommitted();
    await userEvent.keyboard("{Escape}");

    const reopened = within(await openMenu()).getByTestId("stream-row-w1");
    expect(within(reopened).getByTestId("stream-merging-w1")).toBeInTheDocument();

    await act(async () => call.settle({ kind: "nothing_to_merge" }));
    await waitFor(() =>
      expect(within(reopened).queryByTestId("stream-merging-w1")).toBeNull(),
    );
  });

  it("WSS-FR-HGWL: a refusal ends the state too, whatever the call returns", async () => {
    const call = pending();
    renderSelector();
    const r = await startUncommitted();
    await within(r).findByTestId("stream-merging-w1");

    await act(async () => call.refuse(new Error(STREAM_ERRORS.mergeInProgress)));
    await waitFor(() =>
      expect(within(r).queryByTestId("stream-merging-w1")).toBeNull(),
    );
    expect(await within(r).findByRole("alert")).toBeInTheDocument();
  });

  it("WSS-FR-HGWL, WSS-FR-XRHT: a second merge is not started while the first call is out", async () => {
    pending();
    renderSelector();
    const r = await startUncommitted();
    await within(r).findByTestId("stream-merging-w1");

    // The confirmation is gone and Merge is disabled: one call is all there is.
    expect(within(r).queryByRole("button", { name: "Leave uncommitted" })).toBeNull();
    expect(calls("merge_work_stream")).toHaveLength(1);
  });
});

describe("the answer of a call that settled (WSS-FR-KMHD, WSS-FR-PLVE)", () => {
  it("WSS-FR-KMHD: a clean merge left uncommitted says what merged and that no run was made", async () => {
    backend([summary({}, { aheadOfBase: 2 })], () => mergedResult());
    renderSelector();
    const r = await startUncommitted();

    const line = await within(r).findByTestId("stream-merge-result-w1");
    expect(line.textContent).toMatch(/Merged editor work: 2 paths into main/);
    expect(line.textContent).toMatch(/left uncommitted in the base worktree/);
    expect(line.textContent).toMatch(/No run was made/);
    // WSS-FR-KMHD: neither a run nor a link to one.
    expect(within(r).queryByRole("button", { name: /Open in Runs/ })).toBeNull();
    expect(calls("merge_work_stream")[0][1]).toEqual({
      streamId: "w1",
      publication: { kind: "uncommitted" },
    });
  });

  it("WSS-FR-KMHD: a committed merge names the commit in short form with the whole id in semantics", async () => {
    const id = "9f2c1ab4d5e6f708192a3b4c5d6e7f8091a2b3c4";
    backend([summary({}, { aheadOfBase: 2 })], () => mergedResult({ commit: id }));
    renderSelector();
    const r = await startUncommitted();

    const line = await within(r).findByTestId("stream-merge-result-w1");
    expect(line.textContent).toMatch(/committed as 9f2c1ab/);
    expect(line.textContent).not.toContain(id);
    const commit = within(line).getByText("9f2c1ab");
    expect(commit).toHaveAttribute("title", id);
    expect(commit).toHaveAttribute("aria-label", `Commit ${id}`);
  });

  it("WSS-FR-KMHD: nothing to merge says nothing was written and makes no run", async () => {
    backend([summary({}, { aheadOfBase: 1 })], () => ({ kind: "nothing_to_merge" }));
    renderSelector();
    const r = await startUncommitted();

    const line = await within(r).findByTestId("stream-merge-result-w1");
    expect(line.textContent).toMatch(/holds nothing main does not/);
    expect(line.textContent).toMatch(/Nothing was written/);
    expect(within(r).queryByRole("button", { name: /Open in Runs/ })).toBeNull();
  });

  it("WSS-FR-KMHD: the answer stays until the dropdown closes", async () => {
    backend([summary({}, { aheadOfBase: 2 })], () => mergedResult());
    renderSelector();
    const r = await startUncommitted();
    await within(r).findByTestId("stream-merge-result-w1");
    // While the dropdown stays open the answer stays.
    expect(within(r).getByTestId("stream-merge-result-w1")).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    const reopened = within(await openMenu()).getByTestId("stream-row-w1");
    expect(within(reopened).queryByTestId("stream-merge-result-w1")).toBeNull();
  });

  it("WSS-FR-KMHD: starting another act on the stream drops the answer", async () => {
    let answered = 0;
    backend([summary({}, { aheadOfBase: 2 })], () => {
      answered += 1;
      if (answered > 1) throw new Error(STREAM_ERRORS.busy);
      return mergedResult();
    });
    renderSelector();
    const r = await startUncommitted();
    await within(r).findByTestId("stream-merge-result-w1");

    await userEvent.click(within(r).getByRole("button", { name: "Merge…" }));
    await userEvent.click(within(r).getByRole("button", { name: "Leave uncommitted" }));
    // The second call was refused, so the first answer is gone and the refusal
    // stands in its place.
    await waitFor(() => expect(calls("merge_work_stream")).toHaveLength(2));
    await waitFor(() =>
      expect(within(r).queryByTestId("stream-merge-result-w1")).toBeNull(),
    );
    expect(within(r).getByRole("alert").textContent).toMatch(
      /A run holds this stream or waits in its queue/,
    );
  });

  it("WSS-FR-KMHD: starting a delete of the stream drops the answer", async () => {
    backend([summary({}, { aheadOfBase: 2 })], () => {
      // The merge landed, so the reloaded listing says the stream holds nothing
      // its base does not, and a delete of it is not refused as unmerged.
      listing = [summary({}, { aheadOfBase: 0 })];
      return mergedResult();
    });
    renderSelector();
    const r = await startUncommitted();
    await within(r).findByTestId("stream-merge-result-w1");

    await userEvent.click(within(r).getByTestId("stream-delete-w1"));
    const confirm = within(r).getByRole("button", { name: "Delete" });
    await waitFor(() => expect(confirm).toBeEnabled());
    await userEvent.click(confirm);
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    await waitFor(() =>
      expect(within(r).queryByTestId("stream-merge-result-w1")).toBeNull(),
    );
  });

  it("WSS-FR-PLVE, WSS-FR-JMWA: a conflict says Git could not settle it, names the first paths and counts the rest", async () => {
    const paths = Array.from({ length: 12 }, (_, i) => `src/thing-${i}.ts`);
    backend([summary({}, { aheadOfBase: 2 })], () =>
      conflictedResult({ conflictedPaths: paths }),
    );
    renderSelector();
    const r = await startUncommitted();

    const line = await within(r).findByTestId("stream-merge-result-w1");
    expect(line.textContent).toMatch(/Git could not settle 12 paths/);
    expect(line.textContent).toMatch(/Neither branch was written/);
    expect(r.textContent).toMatch(/thing-0\.ts, src\/thing-1\.ts, src\/thing-2\.ts and 9 more/);
    expect(r.textContent).not.toMatch(/thing-11/);
    // WSS-FR-JMWA: the whole path set stands in the row's accessible semantics.
    const whole = within(r).getByLabelText(/Paths Git could not settle/);
    expect(whole.getAttribute("aria-label")).toContain("src/thing-11.ts");
    expect(whole.getAttribute("aria-label")).toContain("src/thing-0.ts");
  });

  it("WSS-FR-PLVE: the listing is reloaded and the merge run's status comes from it", async () => {
    backend([summary({}, { aheadOfBase: 2 })], () => {
      // The conflict made a run, and the next listing links it.
      listing = [
        summary(
          {},
          { aheadOfBase: 2, mergeRun: mergeRunLink("queued", { runId: "run-9" }) },
        ),
      ];
      return conflictedResult({ runId: "run-9" });
    });
    renderSelector();
    const r = await startUncommitted();

    const status = await within(r).findByTestId("stream-merge-run-w1");
    expect(status).toHaveTextContent("Merge editor work: waiting");
    expect(
      within(r).getByRole("button", { name: "Open in Runs…" }),
    ).toBeInTheDocument();
    expect(calls("list_work_streams").length).toBeGreaterThanOrEqual(2);
  });

  it("WSS-FR-PLVE: no merge status is rendered from the call's result alone", async () => {
    // The listing never links a run, so the row has nothing but the answer.
    backend([summary({}, { aheadOfBase: 2 })], () => conflictedResult());
    renderSelector();
    const r = await startUncommitted();

    await within(r).findByTestId("stream-merge-result-w1");
    expect(within(r).queryByTestId("stream-merge-run-w1")).toBeNull();
    expect(within(r).queryByRole("button", { name: "Open in Runs…" })).toBeNull();
  });
});

describe("the stream's merge run (WSS-FR-OFCU, WSS-FR-DTYB, WSS-FR-RJTN, WSS-FR-GBWE)", () => {
  const withRun = async (state: GraduationRunState, ahead = 2) => {
    backend([summary({}, { aheadOfBase: ahead, mergeRun: mergeRunLink(state) })]);
    const handle = renderSelector();
    return { ...handle, r: await row() };
  };

  it.each<[GraduationRunState, string]>([
    ["queued", "waiting"],
    ["working", "reconciling"],
    ["reviewing", "being reviewed"],
    ["blocked", "blocked"],
    ["interrupted", "interrupted"],
    ["awaiting_author", "waiting on the author"],
    ["failed", "failed"],
    ["completed", "merged"],
  ])("WSS-FR-DTYB: %s reads as '%s' beside the run's name", async (state, word) => {
    const { r } = await withRun(state);
    expect(within(r).getByTestId("stream-merge-run-w1")).toHaveTextContent(
      `Merge editor work: ${word}`,
    );
    // The typed state is a word set, never its own name.
    expect(within(r).getByTestId("stream-merge-run-w1").textContent).not.toMatch(
      /awaiting_author/,
    );
  });

  it.each<GraduationRunState>([
    "queued",
    "working",
    "reviewing",
    "blocked",
    "interrupted",
    "awaiting_author",
    "failed",
    "completed",
  ])("WSS-FR-DTYB: a %s merge run offers Open in Runs…", async (state) => {
    const { r } = await withRun(state);
    expect(within(r).getByRole("button", { name: "Open in Runs…" })).toBeInTheDocument();
  });

  it("WSS-FR-RJTN: a run waiting on the author says the answer is made in Runs and offers no Answer… or Continue", async () => {
    const { r } = await withRun("awaiting_author");
    expect(r.textContent).toMatch(/Merge editor work waits on you/);
    expect(r.textContent).toMatch(/answer or the decision is made in Runs/);
    expect(within(r).queryByRole("button", { name: /Answer…/ })).toBeNull();
    expect(within(r).queryByRole("button", { name: /Continue/ })).toBeNull();
  });

  it("WSS-FR-GBWE: a failed run says neither branch was written, points to Runs, and keeps Merge… offered", async () => {
    const { r } = await withRun("failed");
    expect(r.textContent).toMatch(/Merge editor work failed/);
    expect(r.textContent).toMatch(/Neither branch was written/);
    expect(r.textContent).toMatch(/cause is in Runs/);
    // The author may start the merge again.
    expect(within(r).getByTestId("stream-merge-w1")).toBeEnabled();
  });

  it("WSS-FR-GBWE: the failed status stays across a reopen until the listing drops the run", async () => {
    const { r } = await withRun("failed");
    expect(within(r).getByTestId("stream-merge-run-w1")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    const reopened = within(await openMenu()).getByTestId("stream-row-w1");
    expect(within(reopened).getByTestId("stream-merge-run-w1")).toBeInTheDocument();
  });

  it.each<GraduationRunState>([
    "queued",
    "working",
    "reviewing",
    "blocked",
    "interrupted",
    "awaiting_author",
  ])("WSS-FR-XRHT: Merge… and Update… are disabled while the merge run is %s", async (state) => {
    // A stream the run holds is busy, which hides the action row; one it does
    // not hold (queued, interrupted, awaiting the author) shows both disabled.
    backend([
      summary({}, { aheadOfBase: 2, behindBase: 1, mergeRun: mergeRunLink(state) }),
    ]);
    renderSelector();
    const r = await row();
    expect(within(r).getByTestId("stream-merge-w1")).toBeDisabled();
    expect(within(r).getByTestId("stream-update-w1")).toBeDisabled();
  });

  it("WSS-FR-XRHT: a completed merge run lets the next merge start", async () => {
    const { r } = await withRun("completed");
    expect(within(r).getByTestId("stream-merge-w1")).toBeEnabled();
  });

  it("WSS-FR-PSXK: a stream a merge run holds is flagged busy, naming the run", async () => {
    backend([
      summary(
        { busyRunId: "run-merge-1" },
        { aheadOfBase: 2, mergeRun: mergeRunLink("working") },
      ),
    ]);
    renderSelector();
    const r = await row();
    expect(r.textContent).toMatch(/Busy — Merge editor work is working in it/);
  });

  it("WSS-FR-OFCU: a stream whose listing holds no merge run renders no merge status", async () => {
    backend([summary({}, { aheadOfBase: 2 })]);
    renderSelector();
    const r = await row();
    expect(within(r).queryByTestId("stream-merge-run-w1")).toBeNull();
    expect(within(r).queryByRole("button", { name: "Open in Runs…" })).toBeNull();
  });

  it("WSS-FR-OFCU: a run that changed state while the dropdown was closed renders its state when it opens", async () => {
    backend([summary({}, { aheadOfBase: 2, mergeRun: mergeRunLink("queued") })]);
    renderSelector();
    await openMenu();
    await userEvent.keyboard("{Escape}");

    // Another window moved the run on while this dropdown was shut.
    listing = [summary({}, { aheadOfBase: 2, mergeRun: mergeRunLink("reviewing") })];
    const reopened = within(await openMenu()).getByTestId("stream-row-w1");
    await waitFor(() =>
      expect(within(reopened).getByTestId("stream-merge-run-w1")).toHaveTextContent(
        "being reviewed",
      ),
    );
  });

  it("WSS-FR-JBYF: a work-streams-changed event reloads the listing and the row follows the run", async () => {
    backend([summary({}, { aheadOfBase: 2, mergeRun: mergeRunLink("queued") })]);
    renderSelector();
    const r = await row();
    expect(within(r).getByTestId("stream-merge-run-w1")).toHaveTextContent("waiting");
    const reads = calls("list_work_streams").length;

    listing = [summary({}, { aheadOfBase: 2, mergeRun: mergeRunLink("working") })];
    const entry = handlers.find(([name]) => name === "work-streams-changed");
    expect(entry, "the surface listens on work-streams-changed").toBeTruthy();
    await act(async () => entry![1]({ payload: { projectKey: "p" } }));

    await waitFor(() =>
      expect(within(r).getByTestId("stream-merge-run-w1")).toHaveTextContent(
        "reconciling",
      ),
    );
    expect(calls("list_work_streams").length).toBeGreaterThan(reads);
  });
});

describe("Open in Runs and the controls a merge run does not get here (WSS-FR-AWRS, WSS-FR-NRCQ)", () => {
  it("WSS-FR-AWRS: Open in Runs… names the run, closes the dropdown and starts nothing", async () => {
    backend([
      summary({}, { aheadOfBase: 2, mergeRun: mergeRunLink("working", { runId: "run-42" }) }),
    ]);
    const { onOpenRun } = renderSelector();
    const r = await row();
    await userEvent.click(within(r).getByRole("button", { name: "Open in Runs…" }));

    expect(onOpenRun).toHaveBeenCalledTimes(1);
    expect(onOpenRun).toHaveBeenCalledWith("run-42");
    expect(screen.queryByTestId("stream-menu")).toBeNull();
    expect(calls("merge_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-AWRS: it is reachable by the keyboard alone", async () => {
    backend([summary({}, { aheadOfBase: 2, mergeRun: mergeRunLink("awaiting_author") })]);
    const { onOpenRun } = renderSelector();
    const r = await row();
    const open = within(r).getByRole("button", { name: "Open in Runs…" });
    open.focus();
    await userEvent.keyboard("{Enter}");
    expect(onOpenRun).toHaveBeenCalledWith("run-merge-1");
  });

  it.each<GraduationRunState>([
    "queued",
    "working",
    "blocked",
    "interrupted",
    "awaiting_author",
    "failed",
    "completed",
  ])("WSS-FR-NRCQ: a %s merge run offers no Continue, Answer, Discard, Pause or Cancel", async (state) => {
    backend([summary({}, { aheadOfBase: 2, mergeRun: mergeRunLink(state) })]);
    renderSelector();
    const r = await row();
    for (const name of [/Continue/, /Answer/, /Discard/, /Pause/, /Cancel/, /Resume/]) {
      expect(within(r).queryByRole("button", { name })).toBeNull();
    }
  });

  it("WSS-FR-NRCQ, WSS-FR-PMYA: nothing the row does reaches an operation on a run", async () => {
    backend([summary({}, { aheadOfBase: 2, mergeRun: mergeRunLink("awaiting_author") })]);
    renderSelector();
    const r = await row();
    await userEvent.click(within(r).getByRole("button", { name: "Open in Runs…" }));

    for (const cmd of [
      "continue_graduation_run",
      "pause_graduation_run",
      "discard_graduation_run",
      "answer_graduation_escalation",
    ]) {
      expect(calls(cmd)).toHaveLength(0);
    }
  });
});

describe("the commit route and the typed refusals (WSS-FR-TQBN, WSS-FR-NPXC, WSS-FR-OMAP)", () => {
  it("WSS-FR-TQBN: the commit route starts the merge with the message the window returned", async () => {
    backend([summary({}, { aheadOfBase: 2 })], () => mergedResult({ commit: "a".repeat(40) }));
    renderSelector({ onRequestMergeCommit: vi.fn(async () => "merge the specs") });
    const r = await row();
    await userEvent.click(within(r).getByRole("button", { name: "Merge…" }));
    await userEvent.click(within(r).getByRole("button", { name: "Commit…" }));

    await waitFor(() => expect(calls("merge_work_stream")).toHaveLength(1));
    expect(calls("merge_work_stream")[0][1]).toEqual({
      streamId: "w1",
      publication: { kind: "commit", message: "merge the specs" },
    });
  });

  it("WSS-FR-TQBN: a dismissed message window starts no merge", async () => {
    backend([summary({}, { aheadOfBase: 2 })]);
    renderSelector({ onRequestMergeCommit: vi.fn(async () => null) });
    const r = await row();
    await userEvent.click(within(r).getByRole("button", { name: "Merge…" }));
    await userEvent.click(within(r).getByRole("button", { name: "Commit…" }));

    await waitFor(() => expect(screen.queryByTestId("stream-menu")).toBeNull());
    expect(calls("merge_work_stream")).toHaveLength(0);
  });

  it.each<[string, RegExp]>([
    [STREAM_ERRORS.busy, /A run holds this stream or waits in its queue/],
    [STREAM_ERRORS.missing, /working copy is gone/],
    [STREAM_ERRORS.unknownStream, /not there any more/],
    [STREAM_ERRORS.notAGitRepository, /not inside a Git repository/],
    [STREAM_ERRORS.mergeInProgress, /already merging or updating/],
    [STREAM_ERRORS.updateInProgress, /already updating a stream/],
    [STREAM_ERRORS.baseNotCheckedOut, /No worktree holds the base branch/],
    [STREAM_ERRORS.vendorImageUnconfigured, /no agent image configured/],
    [STREAM_ERRORS.vendorImageInvalid, /image cannot carry a run/],
    [STREAM_ERRORS.vendorExecutionUnsupported, /cannot be executed in a container/],
    [STREAM_ERRORS.dockerBackendUnverified, /Docker backend has not been verified/],
    [STREAM_ERRORS.unsupportedConflict, /a text edit cannot settle/],
    [STREAM_ERRORS.artifactGenerationFailed, /update turn reads could not be written/],
  ])("WSS-FR-NPXC, WKS-FR-UCMR: %s renders on the stream's own row as a sentence", async (code, words) => {
    backend([summary({}, { aheadOfBase: 2 })], () => {
      throw new Error(code);
    });
    renderSelector();
    const r = await startUncommitted();

    const alert = await within(r).findByRole("alert");
    expect(alert.textContent).toMatch(words);
    expect(alert.textContent).not.toContain(code);
    // The refusal is on the row, and nowhere else.
    expect(screen.getAllByRole("alert")).toHaveLength(1);
    expect(screen.queryByTestId("stream-merging-w1")).toBeNull();
  });

  it.each<[string, string]>([
    [STREAM_ERRORS.mergeBranchMoved, "moved after the merge was handed off"],
    [STREAM_ERRORS.mergeDirtySide, "holds uncommitted changes"],
    [STREAM_ERRORS.mergeGuardHeld, "holds the repository"],
    [STREAM_ERRORS.mergeApplyFailed, "could not write the merge result"],
  ])("WSS-FR-NPXC: the merge run code %s has author-readable text", async (code, words) => {
    backend([summary({}, { aheadOfBase: 2 })], () => {
      throw new Error(code);
    });
    renderSelector();
    const r = await startUncommitted();

    const alert = await within(r).findByRole("alert");
    expect(alert.textContent).toContain(words);
    expect(alert.textContent).toMatch(/Nothing was written/);
  });

  it("WSS-FR-OMAP: a dirty refusal names the complete path set and routes to Changes", async () => {
    backend([summary({}, { aheadOfBase: 2 })], () => {
      throw new Error(`${STREAM_ERRORS.baseDirty}: a.ts, b/c.md`);
    });
    renderSelector();
    const r = await startUncommitted();

    expect(await within(r).findByRole("alert")).toHaveTextContent(/not committed/i);
    expect(within(r).getByText("a.ts")).toBeInTheDocument();
    expect(within(r).getByText("b/c.md")).toBeInTheDocument();
    expect(within(r).getByRole("button", { name: "Open Changes" })).toBeInTheDocument();
  });
});
