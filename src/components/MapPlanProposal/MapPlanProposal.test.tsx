/**
 * The summary row and the review sheet of a plan proposal
 * (`MPV-map-proposal-review.md`). The backend is mocked at `invoke`, so every
 * assertion is about what the surface renders and which operation it sends.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: unknown }) => void) => {
    const set = listeners.get(event) ?? new Set();
    set.add(cb);
    listeners.set(event, set);
    return () => set.delete(cb);
  }),
}));
// No log flush timer may outlive its test, and the logs are asserted.
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import { logInfo, logWarn } from "../../logging";
import { resetToasts, visibleToasts } from "../../state/toasts";
import { createRow, draftRow, modifyRow, proposal, withRow } from "../../test/mapPlanFixtures";
import { backend, callsOf, emit, fakeHost, renderReview } from "../../test/mapPlanReviewHarness";
import type { MapPlanProposal, RowChange } from "../../types";

const close: RowChange = { field: "close", before: "open", after: "closed", reason: "not_planned" };

const FOUR = () =>
  proposal([
    modifyRow("r18", 18, "Model serving integration", [
      { field: "body", before: "## Description\n\nOld words here.\n", after: "## Description\n\nNew words here.\n\n- added item\n" },
    ]),
    modifyRow("r11", 11, "Ontological SQL validation", [
      { field: "title", before: "Ontological SQL validat", after: "Ontological SQL validation" },
      { field: "milestone", before: { number: 2, title: "M2" }, after: { number: 1, title: "M1" } },
      { field: "labels", before: ["draft"], after: ["mcp"], added: ["mcp"], removed: ["draft"] },
      { field: "status", before: "Backlog", after: "In progress" },
    ]),
    createRow("rnew", "Model fallback and retry limits", {}, {
      milestone: { number: 2, title: "M2" },
      body: "## Description\n\nRetries.\n",
      type: "Feature",
    }),
    modifyRow("r16", 16, "Slack integration", [close]),
  ]);

async function openSheet() {
  const user = userEvent.setup();
  await user.click(screen.getByTestId("map-plan-entry-toggle"));
  const sheet = await screen.findByTestId("pp-sheet");
  return { user, sheet };
}

const detail = () => screen.getByTestId("pp-detail");

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  resetToasts();
  vi.mocked(logInfo).mockClear();
  vi.mocked(logWarn).mockClear();
});

afterEach(cleanup);

describe("summary row (MPV-FR-IBSF .. MPV-FR-FNIS)", () => {
  it("MPV-FR-IBSF, MPV-FR-KCOB, MPV-FR-WKNB: shows CHG, the name, the summary, the status, and Review", async () => {
    const b = backend(invokeMock, FOUR());
    await renderReview(b);
    const row = screen.getByTestId("map-plan-entry");
    await within(row).findByText("2 changed, 1 added, 1 withdrawn");
    expect(row).toHaveTextContent("CHG");
    expect(row).toHaveTextContent("Plan proposal");
    expect(within(row).getByTestId("pp-status")).toHaveTextContent("pending");
    expect(within(row).getByTestId("map-plan-entry-toggle")).toHaveTextContent("Review");
    expect(callsOf(b, "list_map_plan_proposals")).toEqual([{ discussionId: "t1" }]);
    // MPV-FR-IBSF: the discussion shows no row of the proposal.
    expect(screen.queryByText("Slack integration")).toBeNull();
  });

  it("MPV-FR-GYLV: a pending proposal shows the decided count only after the first decision", async () => {
    const p = FOUR();
    const b = backend(invokeMock, withRow(p, "r18", { state: "accepted" }));
    await renderReview(b);
    expect(await screen.findByText("1 of 4")).toBeInTheDocument();
  });

  it("MPV-FR-GYLV: with no decision there is no count", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    expect(screen.queryByText(/of 4/)).toBeNull();
  });

  it("MPV-FR-RJKZ, MPV-FR-LOSO: a decided proposal offers Show review and its derived status", async () => {
    const p = proposal([
      modifyRow("a", 1, "A", [close], { state: "rejected" }),
      modifyRow("b", 2, "B", [close], { state: "rejected" }),
    ]);
    await renderReview(backend(invokeMock, p));
    await waitFor(() => expect(screen.getByTestId("map-plan-entry-toggle")).toHaveTextContent("Show review"));
    expect(screen.getByTestId("pp-status")).toHaveTextContent("rejected");
  });

  it("MPV-FR-RJKZ, MPV-FR-FNIS: Review shows the Delivery lens, opens the sheet, and becomes Hide review", async () => {
    const showDelivery = vi.fn();
    await renderReview(backend(invokeMock, FOUR()), { host: fakeHost({ showDelivery }) });
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    await openSheet();
    expect(showDelivery).toHaveBeenCalled();
    expect(screen.getByTestId("map-plan-entry-toggle")).toHaveTextContent("Hide review");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    expect(screen.queryByTestId("pp-sheet")).toBeNull();
  });

  it("MPV-FR-IBSF: a row outside the Map tab shows the summary and no review control", async () => {
    await renderReview(backend(invokeMock, FOUR()), { host: null });
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    expect(screen.queryByTestId("map-plan-entry-toggle")).toBeNull();
  });
});

describe("review sheet (MPV-FR-CTTA .. MPV-FR-CAQL)", () => {
  it("MPV-FR-PBAD, MPV-FR-ENVX: the header counts the issues and the list has one entry per row", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { sheet } = await openSheet();
    expect(sheet).toHaveTextContent("4 issues");
    const entries = within(sheet).getAllByTestId("pp-nav");
    expect(entries.map((e) => within(e).getByText(/changed|added|withdrawn/).textContent)).toEqual([
      "changed",
      "changed",
      "added",
      "withdrawn",
    ]);
    expect(within(entries[2]).getByText("new")).toBeInTheDocument();
    expect(within(entries[0]).getByRole("img", { name: "pending" })).toBeInTheDocument();
  });

  it("MPV-FR-CAQL: the sheet opens on the first pending row", async () => {
    const p = withRow(FOUR(), "r18", { state: "accepted" });
    await renderReview(backend(invokeMock, p));
    await screen.findByText("1 of 4");
    await openSheet();
    expect(detail()).toHaveAttribute("data-row", "r11");
  });

  it("MPV-FR-XAOG: Enter and Space on a list entry select its row", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user, sheet } = await openSheet();
    const entries = within(sheet).getAllByTestId("pp-nav");
    entries[2].focus();
    await user.keyboard("{Enter}");
    expect(detail()).toHaveAttribute("data-row", "rnew");
    entries[3].focus();
    await user.keyboard(" ");
    expect(detail()).toHaveAttribute("data-row", "r16");
    expect(entries[3]).toHaveAttribute("aria-selected", "true");
  });

  it("MPV-FR-PBAD: Escape closes the sheet and gives the focus back to the row button", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user } = await openSheet();
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("pp-sheet")).toBeNull();
    expect(screen.getByTestId("map-plan-entry-toggle")).toHaveFocus();
  });

  it("MPV-FR-PBAD: the close control closes the sheet", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user } = await openSheet();
    await user.click(screen.getByRole("button", { name: "Close review" }));
    expect(screen.queryByTestId("pp-sheet")).toBeNull();
  });

  it("MPV-FR-NMYT: the reference of a changed row selects its item on the canvas", async () => {
    const selectItem = vi.fn();
    await renderReview(backend(invokeMock, FOUR()), { host: fakeHost({ selectItem }) });
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user } = await openSheet();
    await user.click(screen.getByTestId("pp-detail-ref"));
    expect(selectItem).toHaveBeenCalledWith(18);
  });

  it("MPV-FR-NMYT: the reference is plain text where the item is not on the canvas", async () => {
    await renderReview(backend(invokeMock, FOUR()), { host: fakeHost({ itemOnCanvas: () => false }) });
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    await openSheet();
    expect(screen.queryByTestId("pp-detail-ref")).toBeNull();
    expect(within(detail()).getByText("#18")).toBeInTheDocument();
  });

  it("MPV-FR-SNCR: a row whose kind the plan does not name shows no kind chip", async () => {
    await renderReview(backend(invokeMock, FOUR()), { host: fakeHost({ kindOfIssue: () => null }) });
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    await openSheet();
    expect(within(detail()).queryByTestId("flow-kind-chip")).toBeNull();
  });
});

describe("field changes (MPV-FR-CCDM .. MPV-FR-OCYF)", () => {
  it("MPV-FR-CCDM, MPV-FR-JBNC: shows each changed field in order, before struck and after added", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user, sheet } = await openSheet();
    await user.click(within(sheet).getAllByTestId("pp-nav")[1]);
    const fields = within(detail()).getByTestId("pp-fields");
    const labels = [...fields.querySelectorAll(".pp-fields__k")].map((e) => e.textContent);
    expect(labels).toEqual(["Title", "Milestone", "Status", "Labels"]);
    expect(within(fields).getByText("Ontological SQL validat")).toHaveClass("pp-text-old");
    expect(within(fields).getByText("M2")).toHaveClass("pp-v--old");
    expect(within(fields).getByText("M1")).toHaveClass("pp-v--new");
    expect(within(fields).getByText("+ mcp")).toHaveClass("pp-v--new");
    expect(within(fields).getByText("draft").closest(".pp-v")).toHaveClass("pp-v--old");
    expect(within(fields).getByText("In progress").querySelector(".dm-dot")).not.toBeNull();
  });

  it("MPV-FR-LYKS, MPV-FR-UXHR: a withdrawn row shows open to closed as not planned, and Accept names it", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user, sheet } = await openSheet();
    await user.click(within(sheet).getAllByTestId("pp-nav")[3]);
    expect(within(detail()).getByText("State")).toBeInTheDocument();
    expect(within(detail()).getByText("closed as not planned")).toHaveClass("pp-v--new");
    expect(within(detail()).getByRole("button", { name: "Accept: closed as not planned" })).toBeEnabled();
  });

  it("MPV-FR-OCYF, MPV-FR-ZFDO, MPV-FR-VMYF: an added row shows its fields as new and a new description", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user, sheet } = await openSheet();
    await user.click(within(sheet).getAllByTestId("pp-nav")[2]);
    expect(within(detail()).getByText("M2")).toHaveClass("pp-v--new");
    const doc = within(detail()).getByTestId("pp-doc");
    expect(doc).toHaveTextContent("new description");
    expect(doc.querySelectorAll('.pp-b[data-m="add"]')).toHaveLength(2);
  });

  it("MPV-FR-OCYF: a draft row shows its name as the title, no field change, and its prompt as new", async () => {
    await renderReview(backend(invokeMock, proposal([draftRow("d", "Payout draft", "Write the payout flow.")])));
    await screen.findByText("1 draft");
    await openSheet();
    expect(within(detail()).getByRole("heading", { name: "Payout draft" })).toBeInTheDocument();
    expect(within(detail()).queryByTestId("pp-fields")).toBeNull();
    expect(within(detail()).getByTestId("pp-doc")).toHaveTextContent("Write the payout flow.");
  });
});

describe("body diff (MPV-FR-ZFDO .. MPV-FR-PVXW)", () => {
  it("MPV-FR-WYTL, MPV-FR-PPHV: marks the added block in the gutter and the edited words inline", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    await openSheet();
    const doc = within(detail()).getByTestId("pp-doc");
    expect(doc).toHaveTextContent("1 block added, 1 edited");
    expect(doc.querySelector('.pp-b[data-m="add"] .pp-b__g')).toHaveTextContent("+");
    expect(doc.querySelector("ins")).toHaveTextContent("New");
    expect(doc.querySelector("del")).toHaveTextContent("Old");
    expect(doc).not.toHaveTextContent("## Description");
  });

  it("MPV-FR-VMYF: a fold of unchanged blocks expands in place", async () => {
    const body = "A one.\n\nB two.\n\nC three.\n\nD four.\n";
    const p = proposal([
      modifyRow("m", 7, "Folded", [{ field: "body", before: body, after: body + "\nE five.\n" }]),
    ]);
    await renderReview(backend(invokeMock, p));
    await screen.findByText("#7, 1 changed");
    const { user } = await openSheet();
    const fold = within(detail()).getByTestId("pp-fold");
    expect(fold).toHaveTextContent("3 unchanged blocks");
    expect(within(detail()).queryByText("A one.")).toBeNull();
    await user.click(fold);
    expect(within(detail()).getByText("A one.")).toBeInTheDocument();
    expect(within(detail()).queryByTestId("pp-fold")).toBeNull();
  });
});

describe("decisions (MPV-FR-XYDR .. MPV-FR-JHCG)", () => {
  it("MPV-FR-XYDR, MPV-FR-CAQL, MPV-FR-WKVL: Accept applies one row, toasts, and moves to the next pending row", async () => {
    const p = FOUR();
    const b = backend(invokeMock, p);
    b.accept = async ({ rowKey }) => (b.proposal = withRow(b.proposal, rowKey, { state: "accepted" }));
    await renderReview(b);
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user } = await openSheet();
    await user.click(within(detail()).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(detail()).toHaveAttribute("data-row", "r11"));
    expect(callsOf(b, "accept_map_plan_row")).toEqual([{ discussionId: "t1", proposalId: "p1", rowKey: "r18" }]);
    expect(visibleToasts().some((t) => t.title === "Plan change applied")).toBe(true);
    expect(screen.getByText("1 of 4")).toBeInTheDocument();
  });

  it("MPV-FR-QOPR, MPV-FR-PIQU: while a decision runs the row is applying and says it is sending", async () => {
    const b = backend(invokeMock, FOUR());
    let release: (p: MapPlanProposal) => void = () => {};
    b.accept = () => new Promise((resolve) => (release = resolve));
    await renderReview(b);
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user } = await openSheet();
    await user.click(within(detail()).getByRole("button", { name: "Accept" }));
    expect(await within(detail()).findByText("Sending to GitHub")).toBeInTheDocument();
    expect(within(screen.getByTestId("map-plan-entry")).getByTestId("pp-status")).toHaveTextContent("applying");
    release(withRow(b.proposal, "r18", { state: "accepted" }));
    await waitFor(() => expect(detail()).toHaveAttribute("data-row", "r11"));
  });

  it("MPV-FR-PIQU: an applied row says so and links its issue", async () => {
    const p = withRow(FOUR(), "r18", { state: "accepted", result: { issueUrl: "https://github.com/acme/app/issues/18" } });
    await renderReview(backend(invokeMock, p));
    await screen.findByText("1 of 4");
    const { user, sheet } = await openSheet();
    await user.click(within(sheet).getAllByTestId("pp-nav")[0]);
    expect(within(detail()).getByText("Applied to GitHub")).toBeInTheDocument();
    expect(within(detail()).getByRole("link", { name: /acme\/app#18/ })).toBeInTheDocument();
  });

  it("MPV-FR-PIQU, MPV-FR-XYDR: a rejected row says nothing was sent, and Undo reopens it", async () => {
    const p = withRow(FOUR(), "r18", { state: "rejected" });
    const b = backend(invokeMock, p);
    b.reopen = async ({ rowKey }) => (b.proposal = withRow(b.proposal, rowKey, { state: "waiting" }));
    await renderReview(b);
    await screen.findByText("1 of 4");
    const { user, sheet } = await openSheet();
    await user.click(within(sheet).getAllByTestId("pp-nav")[0]);
    expect(within(detail()).getByText("Rejected. Nothing was sent to GitHub.")).toBeInTheDocument();
    await user.click(within(detail()).getByRole("button", { name: "Undo" }));
    expect(await within(detail()).findByRole("button", { name: "Accept" })).toBeInTheDocument();
    expect(callsOf(b, "reopen_map_plan_row")).toEqual([{ discussionId: "t1", proposalId: "p1", rowKey: "r18" }]);
  });

  it("MPV-FR-JHCG: a refused Undo shows its text and the row stays rejected", async () => {
    const b = backend(invokeMock, withRow(FOUR(), "r18", { state: "rejected" }));
    b.reopen = () => Promise.reject({ code: "proposal_open", message: "Another proposal is open." });
    await renderReview(b);
    await screen.findByText("1 of 4");
    const { user, sheet } = await openSheet();
    await user.click(within(sheet).getAllByTestId("pp-nav")[0]);
    await user.click(within(detail()).getByRole("button", { name: "Undo" }));
    expect(await within(detail()).findByText("Another proposal is open.")).toBeInTheDocument();
    expect(within(detail()).getByText("Rejected. Nothing was sent to GitHub.")).toBeInTheDocument();
    expect(vi.mocked(logWarn)).toHaveBeenCalledWith(["frontend"], "map plan row decision refused", {
      action: "reopen",
      rowKind: "modify_issue",
      code: "proposal_open",
    });
  });

  it("MPV-FR-WQKV, MPV-FR-AAFF: a failed row shows its error and the re-prepare request", async () => {
    const p = proposal(
      [modifyRow("r18", 18, "M", [close], { state: "failed", error: { code: "changed_on_tracker", message: "x" } })],
      { reprepareRequestedAt: "2026-01-01T00:01:00Z" },
    );
    await renderReview(backend(invokeMock, p), { proposer: "gerald" });
    await screen.findByText("#18, 1 withdrawn");
    await openSheet();
    expect(within(detail()).getByTestId("pp-failure")).toHaveTextContent("changed after it was read");
    expect(within(detail()).getByTestId("pp-failure")).toHaveTextContent("Nothing was sent.");
    expect(within(detail()).getByTestId("pp-reprepare-asked")).toHaveTextContent("@gerald was asked to prepare it again");
    expect(within(detail()).queryByRole("button", { name: "Accept" })).toBeNull();
    expect(within(detail()).queryByRole("button", { name: /Retry/ })).toBeNull();
  });

  it("MPV-FR-AAFF: Ask @agent to re-prepare sends the request; it is disabled while a row waits", async () => {
    const failed = modifyRow("r18", 18, "M", [close], { state: "failed", error: { code: "partial_edit", message: "Half." } });
    const b = backend(invokeMock, proposal([failed, createRow("a", "A")]));
    await renderReview(b, { proposer: "gerald" });
    await screen.findByText("1 of 2");
    const { user, sheet } = await openSheet();
    await user.click(within(sheet).getAllByTestId("pp-nav")[0]);
    expect(within(detail()).getByRole("button", { name: "Ask @gerald to re-prepare" })).toBeDisabled();
    b.proposal = proposal([failed, createRow("a", "A", { state: "rejected" })]);
    b.reprepare = async () => (b.proposal = { ...b.proposal, reprepareRequestedAt: "2026-01-01T00:02:00Z" });
    emit(listeners, "map-plan-proposals-changed", { discussionId: "t1" });
    const ask = await within(detail()).findByRole("button", { name: "Ask @gerald to re-prepare" });
    await waitFor(() => expect(ask).toBeEnabled());
    await user.click(ask);
    expect(await within(detail()).findByTestId("pp-reprepare-asked")).toBeInTheDocument();
    expect(callsOf(b, "request_map_plan_reprepare")).toEqual([{ discussionId: "t1", proposalId: "p1" }]);
  });

  it("MPV-FR-FUQG: a row waiting for its parent cannot be accepted, and says why", async () => {
    const p = proposal([
      createRow("parent", "Payout schedule"),
      createRow("child", "Payout retries", { blockedBy: { rowKey: "parent", reason: "waiting" } }, { parent: { row: "parent" } }),
    ]);
    await renderReview(backend(invokeMock, p));
    await screen.findByText("2 added");
    const { user, sheet } = await openSheet();
    await user.click(within(sheet).getAllByTestId("pp-nav")[1]);
    expect(within(detail()).getByText("Waiting for its parent: Payout schedule")).toBeInTheDocument();
    const accept = within(detail()).getByRole("button", { name: "Accept" });
    expect(accept).toBeDisabled();
    expect(accept).toHaveAccessibleDescription("Waiting for its parent: Payout schedule");
    expect(within(detail()).getByRole("button", { name: "Reject" })).toBeEnabled();
  });

  it("MPV-FR-YEIH: a row whose parent was rejected keeps Accept disabled and Reject available", async () => {
    const p = proposal([
      createRow("parent", "P", { state: "rejected" }),
      createRow("child", "C", { blockedBy: { rowKey: "parent", reason: "rejected" } }, { parent: { row: "parent" } }),
    ]);
    await renderReview(backend(invokeMock, p));
    await screen.findByText("1 of 2");
    await openSheet();
    expect(within(detail()).getByText("Its parent was rejected: P")).toBeInTheDocument();
    expect(within(detail()).getByRole("button", { name: "Accept" })).toBeDisabled();
    expect(within(detail()).getByRole("button", { name: "Reject" })).toBeEnabled();
  });

  it("MPV-FR-PTUT: Discuss asks the host to focus the composer on the issue, and the sheet stays open", async () => {
    const discuss = vi.fn();
    await renderReview(backend(invokeMock, FOUR()), { host: fakeHost({ discuss }) });
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user, sheet } = await openSheet();
    await user.click(within(detail()).getByRole("button", { name: "Discuss" }));
    expect(discuss).toHaveBeenLastCalledWith(18);
    await user.click(within(sheet).getAllByTestId("pp-nav")[2]);
    await user.click(within(detail()).getByRole("button", { name: "Discuss" }));
    expect(discuss).toHaveBeenLastCalledWith(null);
    expect(screen.getByTestId("pp-sheet")).toBeInTheDocument();
  });

  it("MPV-FR-WKVL, MPV-FR-CAQL: a refused decision shows its text beside the row, an Error toast, and keeps the selection", async () => {
    const b = backend(invokeMock, FOUR());
    b.reject = () => Promise.reject({ code: "row_decided", message: "The row is decided." });
    await renderReview(b);
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user } = await openSheet();
    await user.click(within(detail()).getByRole("button", { name: "Reject" }));
    expect(await within(detail()).findByText("The row is decided.")).toBeInTheDocument();
    // MPV-FR-CAQL: a refused decision does not move the selection.
    expect(detail()).toHaveAttribute("data-row", "r18");
    expect(visibleToasts().some((t) => t.level === "Error" && t.title === "Plan change not rejected")).toBe(true);
  });
});

describe("review bar (MPV-FR-MTSP .. MPV-FR-IZLJ)", () => {
  it("MPV-FR-MTSP: Previous and Next step through the rows and stop at the ends", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user } = await openSheet();
    const bar = screen.getByTestId("pp-bar");
    expect(bar).toHaveTextContent("reviewing change 1 of 4");
    expect(within(bar).getByRole("button", { name: "Previous" })).toBeDisabled();
    for (let i = 0; i < 3; i++) await user.click(within(bar).getByRole("button", { name: "Next" }));
    expect(bar).toHaveTextContent("reviewing change 4 of 4");
    expect(within(bar).getByRole("button", { name: "Next" })).toBeDisabled();
  });

  it("MPV-FR-TWSY: Accept all accepts the pending rows that are not blocked, in order, past a failure", async () => {
    const p = proposal([
      modifyRow("a", 1, "A", [close]),
      modifyRow("done", 2, "Done", [close], { state: "accepted" }),
      createRow("parent", "Parent"),
      createRow("child", "Child", { blockedBy: { rowKey: "parent", reason: "waiting" } }, { parent: { row: "parent" } }),
    ]);
    const b = backend(invokeMock, p);
    b.accept = async ({ rowKey }) => {
      if (rowKey === "a") throw { code: "not_github_source", message: "No." };
      return (b.proposal = withRow(b.proposal, rowKey, { state: "accepted" }));
    };
    await renderReview(b);
    await screen.findByText("1 of 4");
    const { user } = await openSheet();
    await user.click(within(screen.getByTestId("pp-bar")).getByRole("button", { name: "Accept all" }));
    // The run ends when no row is applying and the child is still waiting for its parent.
    await waitFor(() => expect(within(screen.getByTestId("map-plan-entry")).getByTestId("pp-status")).toHaveTextContent("pending"));
    await waitFor(() => expect(screen.getByText("2 of 4")).toBeInTheDocument());
    expect(callsOf(b, "accept_map_plan_row").map((c) => c.rowKey)).toEqual(["a", "parent"]);
  });

  it("MPV-FR-TWSY, MPV-FR-VXHC: Reject all rejects every pending row, leaves decided rows alone, and closes the sheet", async () => {
    const p = withRow(FOUR(), "r18", { state: "accepted" });
    const b = backend(invokeMock, p);
    b.reject = async ({ rowKey }) => (b.proposal = withRow(b.proposal, rowKey, { state: "rejected" }));
    await renderReview(b);
    await screen.findByText("1 of 4");
    const { user } = await openSheet();
    await user.click(within(screen.getByTestId("pp-bar")).getByRole("button", { name: "Reject all" }));
    // MPV-FR-VXHC: the run leaves no row pending, so the sheet closes on its own.
    await waitFor(() => expect(screen.queryByTestId("pp-sheet")).toBeNull());
    expect(callsOf(b, "reject_map_plan_row").map((c) => c.rowKey)).toEqual(["r11", "rnew", "r16"]);
    await user.click(screen.getByRole("button", { name: /^Show review/ }));
    expect(screen.getByTestId("pp-bar")).toHaveTextContent("All rows decided: 1 applied, 3 rejected");
  });

  it("MPV-FR-IZLJ, MPV-FR-XYXY: with no pending row the bar is the neutral line, and no row offers Accept or Reject", async () => {
    const p = proposal([
      modifyRow("a", 1, "A", [close], { state: "accepted" }),
      modifyRow("b", 2, "B", [close], { state: "rejected" }),
      modifyRow("c", 3, "C", [close], { state: "failed", error: { code: "x", message: "y" } }),
    ]);
    await renderReview(backend(invokeMock, p));
    await waitFor(() => expect(screen.getByTestId("map-plan-entry-toggle")).toHaveTextContent("Show review"));
    await openSheet();
    const bar = screen.getByTestId("pp-bar");
    expect(bar).toHaveClass("pp-bar--done");
    expect(bar).toHaveTextContent("All rows decided: 1 applied, 1 rejected, 1 failed");
    const user = userEvent.setup();
    for (const entry of screen.getAllByTestId("pp-nav")) {
      await user.click(entry);
      expect(within(detail()).queryByRole("button", { name: /^Accept/ })).toBeNull();
      expect(within(detail()).queryByRole("button", { name: "Reject" })).toBeNull();
    }
    expect(screen.queryByRole("button", { name: "Accept all" })).toBeNull();
  });
});

describe("shared state (MPV-FR-CZDO, MPV-FR-XAOG)", () => {
  it("MPV-FR-CZDO, MPV-FR-XAOG: an event re-reads the proposal and announces the change of status", async () => {
    const b = backend(invokeMock, FOUR());
    await renderReview(b);
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    b.proposal = withRow(b.proposal, "r16", { state: "rejected" });
    emit(listeners, "map-plan-proposals-changed", { discussionId: "t1" });
    expect(await screen.findByText("1 of 4")).toBeInTheDocument();
    expect(screen.getByTestId("map-plan-announcement")).toHaveTextContent("Slack integration: rejected");
  });

  it("MPV-FR-CZDO: an event of another discussion reads nothing", async () => {
    const b = backend(invokeMock, FOUR());
    await renderReview(b);
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    emit(listeners, "map-plan-proposals-changed", { discussionId: "other" });
    expect(callsOf(b, "list_map_plan_proposals")).toHaveLength(1);
  });

  it("MPV-FR-KMJX: a decision logs its action and outcome, and no title", async () => {
    const b = backend(invokeMock, FOUR());
    b.accept = async ({ rowKey }) => (b.proposal = withRow(b.proposal, rowKey, { state: "accepted" }));
    await renderReview(b);
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user } = await openSheet();
    await user.click(within(detail()).getByRole("button", { name: "Accept" }));
    await waitFor(() =>
      expect(vi.mocked(logInfo)).toHaveBeenCalledWith(["frontend"], "map plan row decided", {
        action: "accept",
        rowKind: "modify_issue",
        state: "accepted",
        code: "",
      }),
    );
    const logged = JSON.stringify(vi.mocked(logInfo).mock.calls);
    expect(logged).not.toContain("Model serving integration");
  });
});

describe("keyboard after a decision (MPV-FR-XAOG)", () => {
  it("MPV-FR-XAOG: after Accept the focus stays in the sheet, so Escape still closes it", async () => {
    const b = backend(invokeMock, FOUR());
    b.accept = async ({ rowKey }) => (b.proposal = withRow(b.proposal, rowKey, { state: "accepted" }));
    await renderReview(b);
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user } = await openSheet();
    await user.click(within(detail()).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(detail()).toHaveAttribute("data-row", "r11"));
    await waitFor(() => expect(screen.getByTestId("pp-sheet")).toContainElement(document.activeElement as HTMLElement));
    await user.keyboard("{Escape}");
    expect(screen.queryByTestId("pp-sheet")).toBeNull();
  });

  it("MPV-FR-XAOG: the arrow keys move between the entries of the list", async () => {
    await renderReview(backend(invokeMock, FOUR()));
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user, sheet } = await openSheet();
    const entries = within(sheet).getAllByTestId("pp-nav");
    entries[0].focus();
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(detail()).toHaveAttribute("data-row", "rnew");
    expect(entries[2]).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(detail()).toHaveAttribute("data-row", "r11");
  });
});

describe("more row states (MPV-FR-QOPR, MPV-FR-PIQU, MPV-FR-GYLV)", () => {
  it("MPV-FR-QOPR: while one row is applying the other rows stay operable, and the bar waits", async () => {
    const b = backend(invokeMock, FOUR());
    let release: (p: MapPlanProposal) => void = () => {};
    b.accept = () => new Promise((resolve) => (release = resolve));
    await renderReview(b);
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    const { user, sheet } = await openSheet();
    await user.click(within(detail()).getByRole("button", { name: "Accept" }));
    await within(detail()).findByText("Sending to GitHub");
    expect(within(screen.getByTestId("pp-bar")).getByRole("button", { name: "Accept all" })).toBeDisabled();
    await user.click(within(sheet).getAllByTestId("pp-nav")[1]);
    expect(within(detail()).getByRole("button", { name: "Accept" })).toBeEnabled();
    expect(within(detail()).getByRole("button", { name: "Reject" })).toBeEnabled();
    release(withRow(b.proposal, "r18", { state: "accepted" }));
    await waitFor(() => expect(screen.getByText("1 of 4")).toBeInTheDocument());
  });

  it("MPV-FR-PIQU: an applied draft row says so and opens its draft", async () => {
    const openDraft = vi.fn();
    const p = proposal([draftRow("d", "Payout draft", "Body.", { state: "accepted", result: { draftId: "dr-1" } })]);
    await renderReview(backend(invokeMock, p), { host: fakeHost({ openDraft }) });
    await waitFor(() => expect(screen.getByTestId("map-plan-entry-toggle")).toHaveTextContent("Show review"));
    const { user } = await openSheet();
    expect(within(detail()).getByText("Created the draft")).toBeInTheDocument();
    await user.click(within(detail()).getByRole("button", { name: "Open draft" }));
    expect(openDraft).toHaveBeenCalledWith("dr-1");
  });

  it("MPV-FR-GYLV: with no row pending the status shows the time of the proposal", async () => {
    const p = proposal([modifyRow("a", 1, "A", [close], { state: "accepted" })]);
    await renderReview(backend(invokeMock, p));
    await waitFor(() => expect(screen.getByTestId("map-plan-entry-toggle")).toHaveTextContent("Show review"));
    const status = screen.getByTestId("pp-status");
    expect(status.querySelector(".pp-st__time")?.textContent).not.toBe("");
    expect(status).not.toHaveTextContent(/of 1/);
  });

  it("MPV-FR-WQKV: a failed row shows its own error text and offers no Reject", async () => {
    const p = proposal([
      modifyRow("m", 7, "M", [close], { state: "failed", error: { code: "partial_edit", message: "Only the title was written." } }),
    ]);
    await renderReview(backend(invokeMock, p));
    await waitFor(() => expect(screen.getByTestId("map-plan-entry-toggle")).toHaveTextContent("Show review"));
    await openSheet();
    expect(within(detail()).getByTestId("pp-failure")).toHaveTextContent("Only the title was written.");
    expect(within(detail()).queryByRole("button", { name: "Reject" })).toBeNull();
  });
});

describe("smaller cases (MPV-FR-LYKS, MPV-FR-CCDM, MPV-FR-PVXW, MPV-FR-PBAD, MPV-FR-CTTA, MPV-FR-KMJX)", () => {
  it("MPV-FR-LYKS, MPV-FR-UXHR: a close as completed is a change to `closed as completed`", async () => {
    const p = proposal([modifyRow("c", 5, "Done item", [{ field: "close", before: "open", after: "closed", reason: "completed" }])]);
    await renderReview(backend(invokeMock, p));
    await screen.findByText("#5, 1 changed");
    await openSheet();
    expect(within(detail()).getByText("closed as completed")).toHaveClass("pp-v--new");
    expect(within(detail()).getByRole("button", { name: "Accept: closed as completed" })).toBeEnabled();
  });

  it("MPV-FR-CCDM: all seven fields show in the fixed order, with an arrow between a before and an after", async () => {
    const p = proposal([
      modifyRow("m", 9, "All", [
        { field: "close", before: "open", after: "closed", reason: "completed" },
        { field: "labels", before: [], after: ["a"], added: ["a"], removed: [] },
        { field: "status", before: "Todo", after: "Done" },
        { field: "parent", before: { issue: 1 }, after: { issue: 2 } },
        { field: "milestone", before: null, after: { number: 4, title: "GA" } },
        { field: "type", before: "Task", after: "Bug" },
        { field: "title", before: "Al", after: "All" },
      ]),
    ]);
    await renderReview(backend(invokeMock, p));
    await screen.findByText("#9, 1 changed");
    await openSheet();
    const fields = within(detail()).getByTestId("pp-fields");
    expect([...fields.querySelectorAll(".pp-fields__k")].map((e) => e.textContent)).toEqual([
      "Title",
      "Type",
      "Milestone",
      "Parent",
      "Status",
      "Labels",
      "State",
    ]);
    const type = within(fields).getByText("Task").closest(".pp-fields__v")!;
    expect(type.querySelector(".pp-arrow")).not.toBeNull();
    // A milestone that had none shows the value after alone.
    const milestone = within(fields).getByText("GA").closest(".pp-fields__v")!;
    expect(milestone.querySelector(".pp-arrow")).toBeNull();
  });

  it("MPV-FR-PVXW: emphasis keeps its face and a table row shows as plain text", async () => {
    const p = proposal([createRow("a", "A", {}, { body: "Text with *weight*.\n\n| a | b |\n|---|---|\n| c | d |\n" })]);
    await renderReview(backend(invokeMock, p));
    await screen.findByText("1 added");
    await openSheet();
    const doc = within(detail()).getByTestId("pp-doc");
    expect(doc.querySelector("em")).toHaveTextContent("weight");
    expect(doc).toHaveTextContent("a | b");
    expect(doc).not.toHaveTextContent("|---|");
  });

  it("MPV-FR-XAOG: the fold expands from the keyboard", async () => {
    const body = "A one.\n\nB two.\n\nC three.\n\nD four.\n";
    const p = proposal([modifyRow("m", 7, "F", [{ field: "body", before: body, after: body + "\nE five.\n" }])]);
    await renderReview(backend(invokeMock, p));
    await screen.findByText("#7, 1 changed");
    const { user } = await openSheet();
    within(detail()).getByTestId("pp-fold").focus();
    await user.keyboard("{Enter}");
    expect(within(detail()).getByText("A one.")).toBeInTheDocument();
  });

  it("MPV-FR-PBAD: a proposal of one row says `1 issue` and shows its summary in the header", async () => {
    await renderReview(backend(invokeMock, proposal([modifyRow("m", 18, "M", [close])])));
    await screen.findByText("#18, 1 withdrawn");
    const { sheet } = await openSheet();
    expect(sheet.querySelector(".pp-sheet__meta")).toHaveTextContent("1 issue");
    expect(sheet.querySelector(".pp-sheet__sum")).toHaveTextContent("#18, 1 withdrawn");
  });

  it("MPV-FR-CTTA: opening a second proposal replaces the first sheet", async () => {
    const b = backend(invokeMock, FOUR());
    const second = proposal([createRow("x", "Other")], { id: "p2" });
    invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) =>
      cmd === "list_map_plan_proposals" ? Promise.resolve([b.proposal, second]) : Promise.reject(new Error(cmd)),
    );
    const { store } = await renderReview(b);
    await screen.findByText("2 changed, 1 added, 1 withdrawn");
    await openSheet();
    act(() => store.openReview("t1", "p2"));
    expect(screen.getAllByTestId("pp-sheet")).toHaveLength(1);
    expect(screen.getByTestId("pp-sheet")).toHaveTextContent("Other");
  });

  it("MPV-FR-KMJX: no log record carries a body, a label name, or a draft name", async () => {
    const p = proposal([
      createRow("a", "A", {}, { body: "Secret body", labels: ["secret-label"] }),
      draftRow("d", "Secret draft", "Secret prompt"),
    ]);
    const b = backend(invokeMock, p);
    b.accept = async ({ rowKey }) => (b.proposal = withRow(b.proposal, rowKey, { state: "accepted" }));
    await renderReview(b);
    await screen.findByText("1 added, 1 draft");
    const { user } = await openSheet();
    await user.click(within(screen.getByTestId("pp-bar")).getByRole("button", { name: "Accept all" }));
    await waitFor(() => expect(screen.queryByTestId("pp-sheet")).toBeNull());
    const logged = JSON.stringify([vi.mocked(logInfo).mock.calls, vi.mocked(logWarn).mock.calls]);
    for (const secret of ["Secret body", "secret-label", "Secret draft", "Secret prompt"]) expect(logged).not.toContain(secret);
  });
});
