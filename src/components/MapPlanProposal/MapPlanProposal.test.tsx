/**
 * The review card of a plan proposal (`MPV-map-proposal-review.md`). The
 * backend is mocked at `invoke`, so every assertion is about what the card
 * renders and which operation it sends.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
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
// No log flush timer may outlive its test, and the card logs are asserted.
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import { logInfo, logWarn } from "../../logging";
import { resetToasts, visibleToasts } from "../../state/toasts";
import { otherCommand } from "../../test/invokeMocks";
import {
  createRow,
  draftRow,
  modifyRow,
  proposal,
  withRow,
} from "../../test/mapPlanFixtures";
import type { MapPlanProposal } from "../../types";
import { MapPlanHostContext, MapPlanProposalCard, type MapPlanHost } from ".";

interface Backend {
  proposal: MapPlanProposal;
  calls: { cmd: string; args: unknown }[];
  /** The answer of `accept_map_plan_row`, or a rejection. */
  accept?: (args: { rowKey: string }) => Promise<MapPlanProposal>;
  reject?: (args: { rowKey: string }) => Promise<MapPlanProposal>;
}

function wire(b: Backend) {
  invokeMock.mockImplementation((cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "list_map_plan_proposals":
        return Promise.resolve([b.proposal]);
      case "accept_map_plan_row":
        return b.accept ? b.accept(args as { rowKey: string }) : Promise.resolve(b.proposal);
      case "reject_map_plan_row":
        return b.reject ? b.reject(args as { rowKey: string }) : Promise.resolve(b.proposal);
      default:
        return otherCommand(cmd);
    }
  });
}

const callsOf = (b: Backend, cmd: string) =>
  b.calls.filter((c) => c.cmd === cmd).map((c) => c.args);

function backend(p: MapPlanProposal): Backend {
  const b: Backend = { proposal: p, calls: [] };
  wire(b);
  return b;
}

async function renderCard(b: Backend, host: MapPlanHost | null = null) {
  render(
    <MapPlanHostContext.Provider value={host}>
      <MapPlanProposalCard discussionId="t1" proposalId={b.proposal.id} />
    </MapPlanHostContext.Provider>,
  );
  await screen.findByTestId("map-plan-proposal");
}

function rowGroup(name: RegExp | string) {
  return screen.getByRole("group", { name });
}

function emit(event: string, payload: unknown) {
  act(() => {
    for (const cb of listeners.get(event) ?? []) cb({ payload });
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  resetToasts();
  vi.mocked(logInfo).mockClear();
  vi.mocked(logWarn).mockClear();
});

afterEach(cleanup);

const THREE = () =>
  proposal([
    createRow(
      "r1",
      "Payout schedule",
      {},
      {
        type: "Feature",
        milestone: { number: 2, title: "GA" },
        labels: ["payments"],
        body: "Pay out every Friday.",
      },
    ),
    createRow(
      "r2",
      "Payout retries",
      { blockedBy: { rowKey: "r1", reason: "waiting" } },
      { parent: { row: "r1" } },
    ),
    modifyRow("r3", 47, "Decline message", [
      { field: "status", before: "Todo", after: "In progress" },
      { field: "labels", before: [], after: [], added: ["payments"], removed: ["legacy"] },
      { field: "close", before: null, after: "not_planned", reason: "not_planned" },
    ]),
  ]);

describe("the rows of the card (MPV-FR-IBSF, MPV-FR-OCYF)", () => {
  it("MPV-FR-IBSF: reads the proposal of its discussion and draws one row for each row, in order", async () => {
    const b = backend(THREE());
    await renderCard(b);
    expect(callsOf(b, "list_map_plan_proposals")).toEqual([{ discussionId: "t1" }]);
    const rows = [...document.querySelectorAll(".mpv-row")];
    expect(rows.map((r) => r.getAttribute("aria-label"))).toEqual([
      "New issue Payout schedule, Waiting for you",
      "New issue Payout retries, Waiting for its parent",
      "Change issue Decline message, Waiting for you",
    ]);
  });

  it("MPV-FR-IBSF: the card states no rationale of its own", async () => {
    const b = backend(THREE());
    await renderCard(b);
    expect(screen.queryByText(/rationale/i)).toBeNull();
  });

  it("MPV-FR-IBSF: a card whose proposal cannot be read says so and draws no row", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "list_map_plan_proposals" ? Promise.reject(new Error("gone")) : otherCommand(cmd),
    );
    render(<MapPlanProposalCard discussionId="t1" proposalId="p1" />);
    expect(await screen.findByText("The plan proposal could not be read.")).toBeInTheDocument();
    expect(screen.queryAllByRole("group")).toHaveLength(0);
  });

  it("MPV-FR-OCYF: a new issue shows its kind, title, Type, milestone, labels, and a collapsed body", async () => {
    await renderCard(backend(THREE()));
    const row = rowGroup(/^New issue Payout schedule/);
    expect(within(row).getByText("New issue")).toBeInTheDocument();
    expect(within(row).getByText("Payout schedule")).toBeInTheDocument();
    expect(row).toHaveTextContent("Type Feature");
    expect(row).toHaveTextContent("Milestone GA");
    expect(row).toHaveTextContent("Labels payments");
    const body = row.querySelector("details") as HTMLDetailsElement;
    expect(body.open).toBe(false);
    expect(within(row).getByText("Body")).toBeInTheDocument();
  });

  it("MPV-FR-OCYF: a new issue under a row of the card shows the parent by its title", async () => {
    await renderCard(backend(THREE()));
    expect(rowGroup(/^New issue Payout retries/)).toHaveTextContent("Parent Payout schedule");
  });

  it("MPV-FR-OCYF: a change row shows its issue number, and a new draft its name and a collapsed prompt", async () => {
    const b = backend(
      proposal([
        modifyRow("r1", 47, "Decline message", [{ field: "title", before: "a", after: "b" }]),
        draftRow("r2", "Retry design", "Design the retries."),
      ]),
    );
    await renderCard(b);
    expect(within(rowGroup(/^Change issue/)).getByText("#47")).toBeInTheDocument();
    const draft = rowGroup(/^New draft Retry design/);
    expect(within(draft).getByText("Retry design")).toBeInTheDocument();
    expect((draft.querySelector("details") as HTMLDetailsElement).open).toBe(false);
  });

  it("MPV-FR-OCYF: draws a body as text and never as markup", async () => {
    const b = backend(
      proposal([createRow("r1", "<b>bold</b>", {}, { body: "<img src=x onerror=alert(1)>" })]),
    );
    await renderCard(b);
    expect(screen.getByText("<b>bold</b>")).toBeInTheDocument();
    expect(document.querySelector("img")).toBeNull();
  });
});

describe("a change row (MPV-FR-CCDM, MPV-FR-LYKS)", () => {
  it("MPV-FR-CCDM: a text field shows the text before struck through and the text after", async () => {
    const b = backend(
      proposal([modifyRow("r1", 47, "Decline", [{ field: "title", before: "Old name", after: "New name" }])]),
    );
    await renderCard(b);
    const before = screen.getByTestId("mpv-before");
    const after = screen.getByTestId("mpv-after");
    expect(before.tagName).toBe("DEL");
    expect(before).toHaveTextContent("Old name");
    expect(after.tagName).toBe("INS");
    expect(after).toHaveTextContent("New name");
  });

  it("MPV-FR-CCDM: a label change shows each added label with a plus and each removed label with a minus", async () => {
    await renderCard(backend(THREE()));
    const added = screen.getByTestId("mpv-label-added");
    const removed = screen.getByTestId("mpv-label-removed");
    expect(added).toHaveTextContent("+");
    expect(added).toHaveTextContent("payments");
    expect(removed).toHaveTextContent("−");
    expect(removed).toHaveTextContent("legacy");
  });

  it("MPV-FR-CCDM: a Status change shows both names", async () => {
    await renderCard(backend(THREE()));
    const row = rowGroup(/^Change issue Decline message/);
    expect(row).toHaveTextContent("Todo");
    expect(row).toHaveTextContent("In progress");
  });

  it("MPV-FR-LYKS: a close shows the word Close and its reason in words in the row", async () => {
    await renderCard(backend(THREE()));
    const row = rowGroup(/^Change issue Decline message/);
    expect(row).toHaveTextContent("Close");
    expect(row).toHaveTextContent("not planned");
  });

  it("MPV-FR-LYKS: the reason is part of the accessible name of Accept", async () => {
    await renderCard(backend(THREE()));
    const row = rowGroup(/^Change issue Decline message/);
    expect(within(row).getByRole("button", { name: "Accept, close as not planned" })).toBeInTheDocument();
  });

  it("MPV-FR-LYKS: a close as completed says completed", async () => {
    const b = backend(
      proposal([
        modifyRow("r1", 9, "Done", [{ field: "close", before: null, after: "completed", reason: "completed" }]),
      ]),
    );
    await renderCard(b);
    expect(screen.getByRole("button", { name: "Accept, close as completed" })).toBeInTheDocument();
  });
});

describe("the decision of a row (MPV-FR-XYDR, MPV-FR-FTXV, MPV-FR-WQKV, MPV-FR-QOPR)", () => {
  it("MPV-FR-XYDR: Accept sends that row alone, and the other rows stay as they were", async () => {
    const b = backend(THREE());
    b.accept = async ({ rowKey }) => {
      b.proposal = withRow(b.proposal, rowKey, {
        state: "accepted",
        result: { issueNumber: 12, issueUrl: "https://example.test/12" },
      });
      return b.proposal;
    };
    await renderCard(b);
    const user = userEvent.setup();
    await user.click(within(rowGroup(/^New issue Payout schedule/)).getByRole("button", { name: "Accept" }));
    await waitFor(() =>
      expect(rowGroup(/^New issue Payout schedule/)).toHaveAttribute("data-state", "accepted"),
    );
    expect(callsOf(b, "accept_map_plan_row")).toEqual([
      { discussionId: "t1", proposalId: "p1", rowKey: "r1" },
    ]);
    expect(callsOf(b, "reject_map_plan_row")).toEqual([]);
    expect(rowGroup(/^Change issue Decline message/)).toHaveAttribute("data-state", "waiting");
  });

  it("MPV-FR-XYDR: Reject sends that row alone", async () => {
    const b = backend(THREE());
    b.reject = async ({ rowKey }) => {
      b.proposal = withRow(b.proposal, rowKey, { state: "rejected" });
      return b.proposal;
    };
    await renderCard(b);
    const user = userEvent.setup();
    await user.click(within(rowGroup(/^Change issue Decline message/)).getByRole("button", { name: /Reject/ }));
    await waitFor(() =>
      expect(rowGroup(/^Change issue Decline message/)).toHaveAttribute("data-state", "rejected"),
    );
    expect(callsOf(b, "reject_map_plan_row")).toEqual([
      { discussionId: "t1", proposalId: "p1", rowKey: "r3" },
    ]);
    expect(callsOf(b, "accept_map_plan_row")).toEqual([]);
  });

  it("MPV-FR-FTXV: a row states its state in words, and a decided row has no Accept and no Reject", async () => {
    const b = backend(
      proposal([
        createRow("r1", "Waits", {}),
        createRow("r2", "Done", { state: "accepted", result: { issueNumber: 12, issueUrl: "https://example.test/12" } }),
        createRow("r3", "Declined", { state: "rejected" }),
      ]),
    );
    await renderCard(b);
    expect(rowGroup(/^New issue Waits/)).toHaveTextContent("Waiting for you");
    expect(rowGroup(/^New issue Done/)).toHaveTextContent("Accepted");
    expect(rowGroup(/^New issue Declined/)).toHaveTextContent("Rejected");
    for (const name of [/^New issue Done/, /^New issue Declined/]) {
      expect(within(rowGroup(name)).queryByRole("button", { name: /Accept|Reject/ })).toBeNull();
    }
  });

  it("MPV-FR-FTXV: an accepted row shows the link of the issue it created, and a draft row a link that opens the draft", async () => {
    const openDraft = vi.fn();
    const b = backend(
      proposal([
        createRow("r1", "Done", { state: "accepted", result: { issueNumber: 12, issueUrl: "https://example.test/12" } }),
        draftRow("r2", "Retry design", "x", { state: "accepted", result: { draftId: "d-9" } }),
      ]),
    );
    await renderCard(b, { itemOnCanvas: () => false, selectItem: vi.fn(), openDraft });
    expect(within(rowGroup(/^New issue Done/)).getByRole("link", { name: "#12" })).toHaveAttribute(
      "href",
      "https://example.test/12",
    );
    const user = userEvent.setup();
    await user.click(within(rowGroup(/^New draft/)).getByRole("button", { name: "Open draft" }));
    expect(openDraft).toHaveBeenCalledWith("d-9");
  });

  it("MPV-FR-WQKV: a failed row shows Failed and its typed error, and offers neither Accept nor Reject", async () => {
    const b = backend(
      proposal([
        createRow("r1", "Broke", { state: "failed", error: { code: "github_unavailable", message: "GitHub did not answer." } }),
        createRow("r2", "Other", {}),
      ]),
    );
    await renderCard(b);
    const row = rowGroup(/^New issue Broke/);
    expect(row).toHaveTextContent("Failed");
    expect(within(row).getByTestId("mpv-row-error")).toHaveTextContent("GitHub did not answer.");
    expect(within(row).queryByRole("button")).toBeNull();
    expect(within(rowGroup(/^New issue Other/)).getByRole("button", { name: "Accept" })).toBeEnabled();
  });

  it("MPV-FR-WQKV: a changed_on_tracker error says the issue changed on GitHub and that nothing was written", async () => {
    const b = backend(
      proposal([
        modifyRow("r1", 47, "Decline", [{ field: "title", before: "a", after: "b" }], {
          state: "failed",
          error: { code: "changed_on_tracker", message: "changed" },
        }),
      ]),
    );
    await renderCard(b);
    const text = screen.getByTestId("mpv-row-error").textContent ?? "";
    expect(text).toMatch(/changed on GitHub after the proposal/);
    expect(text).toMatch(/Nothing was written/);
  });

  it("MPV-FR-WQKV: a refused call shows its typed text beside the row it names and changes no other row", async () => {
    const b = backend(THREE());
    b.accept = () => Promise.reject({ code: "stale_row", message: "The row is not waiting any more." });
    await renderCard(b);
    const user = userEvent.setup();
    await user.click(within(rowGroup(/^Change issue Decline message/)).getByRole("button", { name: /Accept/ }));
    expect(await within(rowGroup(/^Change issue Decline message/)).findByTestId("mpv-row-refusal")).toHaveTextContent(
      "The row is not waiting any more.",
    );
    expect(within(rowGroup(/^New issue Payout schedule/)).queryByTestId("mpv-row-refusal")).toBeNull();
  });

  it("MPV-FR-QOPR: while a decision runs the row is busy with both controls off, and the other rows stay operable", async () => {
    const b = backend(THREE());
    let release: (p: MapPlanProposal) => void = () => {};
    b.accept = () => new Promise<MapPlanProposal>((res) => (release = res));
    await renderCard(b);
    const user = userEvent.setup();
    const first = rowGroup(/^New issue Payout schedule/);
    await user.click(within(first).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(first).toHaveAttribute("aria-busy", "true"));
    expect(within(first).getByRole("button", { name: "Accept" })).toBeDisabled();
    expect(within(first).getByRole("button", { name: "Reject" })).toBeDisabled();
    const other = rowGroup(/^Change issue Decline message/);
    expect(within(other).getByRole("button", { name: /Accept/ })).toBeEnabled();
    expect(within(other).getByRole("button", { name: "Reject" })).toBeEnabled();
    await act(async () => release(withRow(b.proposal, "r1", { state: "accepted" })));
    await waitFor(() => expect(first).not.toHaveAttribute("aria-busy"));
  });
});

describe("a row that depends on a parent (MPV-FR-FUQG, MPV-FR-YEIH)", () => {
  it("MPV-FR-FUQG: shows Waiting for its parent with the parent title, and its Accept is disabled with that reason as description", async () => {
    await renderCard(backend(THREE()));
    const row = rowGroup(/^New issue Payout retries/);
    expect(row).toHaveTextContent("Waiting for its parent: Payout schedule");
    const accept = within(row).getByRole("button", { name: "Accept" });
    expect(accept).toBeDisabled();
    expect(accept).toHaveAccessibleDescription("Waiting for its parent: Payout schedule");
  });

  it("MPV-FR-FUQG: when the parent is accepted, the row is accepted as any row is", async () => {
    const b = backend(THREE());
    b.proposal = withRow(THREE(), "r1", { state: "accepted" });
    b.proposal = withRow(b.proposal, "r2", { blockedBy: null });
    wire(b);
    await renderCard(b);
    const accept = within(rowGroup(/^New issue Payout retries/)).getByRole("button", { name: "Accept" });
    expect(accept).toBeEnabled();
    const user = userEvent.setup();
    await user.click(accept);
    await waitFor(() => expect(callsOf(b, "accept_map_plan_row")).toHaveLength(1));
    expect(callsOf(b, "accept_map_plan_row")[0]).toMatchObject({ rowKey: "r2" });
  });

  it("MPV-FR-YEIH: a row whose parent was rejected says so, keeps Accept disabled, and keeps Reject available", async () => {
    const b = backend(
      withRow(withRow(THREE(), "r1", { state: "rejected" }), "r2", {
        blockedBy: { rowKey: "r1", reason: "rejected" },
      }),
    );
    await renderCard(b);
    const row = rowGroup(/^New issue Payout retries/);
    expect(row).toHaveTextContent("Its parent was rejected");
    expect(within(row).getByRole("button", { name: "Accept" })).toBeDisabled();
    expect(within(row).getByRole("button", { name: "Reject" })).toBeEnabled();
  });

  it("MPV-FR-YEIH: a row whose parent failed says so in words", async () => {
    const b = backend(
      withRow(withRow(THREE(), "r1", { state: "failed", error: { code: "x", message: "m" } }), "r2", {
        blockedBy: { rowKey: "r1", reason: "failed" },
      }),
    );
    await renderCard(b);
    expect(rowGroup(/^New issue Payout retries/)).toHaveTextContent("Its parent failed");
  });
});

describe("the keyboard and the announcements (MPV-FR-XAOG)", () => {
  it("MPV-FR-XAOG: a row is a group named by its kind, title, and state", async () => {
    await renderCard(backend(THREE()));
    expect(rowGroup("Change issue Decline message, Waiting for you")).toBeInTheDocument();
  });

  it("MPV-FR-XAOG: the controls are reached with Tab in the order of the rows and operated with the keyboard alone", async () => {
    const b = backend(
      proposal([
        createRow("r1", "First", {}, { body: "text" }),
        createRow("r2", "Second", {}),
      ]),
    );
    b.accept = async ({ rowKey }) => {
      b.proposal = withRow(b.proposal, rowKey, { state: "accepted" });
      return b.proposal;
    };
    await renderCard(b);
    const user = userEvent.setup();
    // The disclosure is a native details element: its summary takes the focus
    // and Enter opens it in a browser (jsdom does not focus a summary).
    expect(screen.getByText("Body").tagName).toBe("SUMMARY");
    expect(screen.getByText("Body").closest("details")).not.toBeNull();
    await user.tab();
    expect(within(rowGroup(/^New issue First/)).getByRole("button", { name: "Accept" })).toHaveFocus();
    await user.tab();
    expect(within(rowGroup(/^New issue First/)).getByRole("button", { name: "Reject" })).toHaveFocus();
    await user.tab();
    expect(within(rowGroup(/^New issue Second/)).getByRole("button", { name: "Accept" })).toHaveFocus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(callsOf(b, "accept_map_plan_row")).toEqual([
      { discussionId: "t1", proposalId: "p1", rowKey: "r2" },
    ]));
  });

  it("MPV-FR-XAOG: a change of state is announced, and the first read is not", async () => {
    const b = backend(proposal([createRow("r1", "First", {})]));
    await renderCard(b);
    const live = screen.getByTestId("mpv-announcement");
    expect(live).toHaveAttribute("aria-live", "polite");
    expect(live).toHaveTextContent("");
    b.proposal = withRow(b.proposal, "r1", { state: "accepted" });
    emit("map-plan-proposals-changed", { discussionId: "t1" });
    await waitFor(() => expect(live).toHaveTextContent("New issue First: Accepted"));
  });
});

describe("the card follows the backend (MPV-FR-CZDO, MPV-FR-GYLV, MPV-FR-FESO, MPV-FR-XYXY)", () => {
  it("MPV-FR-CZDO: re-reads the proposal on the change event of its discussion, and ignores another discussion", async () => {
    const b = backend(proposal([createRow("r1", "First", {})]));
    await renderCard(b);
    await waitFor(() => expect(listeners.get("map-plan-proposals-changed")?.size).toBe(1));
    emit("map-plan-proposals-changed", { discussionId: "other" });
    expect(callsOf(b, "list_map_plan_proposals")).toHaveLength(1);
    b.proposal = withRow(b.proposal, "r1", { state: "rejected" });
    emit("map-plan-proposals-changed", { discussionId: "t1" });
    await waitFor(() => expect(rowGroup(/^New issue First/)).toHaveAttribute("data-state", "rejected"));
    expect(callsOf(b, "list_map_plan_proposals")).toHaveLength(2);
  });

  it("MPV-FR-GYLV: the header states the count of rows and the count that wait", async () => {
    await renderCard(backend(THREE()));
    expect(screen.getByTestId("mpv-count")).toHaveTextContent("3 rows · 3 waiting");
  });

  it("MPV-FR-GYLV: when no row waits, the header says that all rows are decided", async () => {
    const b = backend(proposal([createRow("r1", "First", { state: "rejected" })]));
    await renderCard(b);
    expect(screen.getByTestId("mpv-count")).toHaveTextContent("1 row · all rows decided");
  });

  it("MPV-FR-FESO: while a row waits the card tells the author that the agent proposes again after every row is decided", async () => {
    await renderCard(backend(THREE()));
    expect(screen.getByText("The agent proposes again after every row is decided.")).toBeInTheDocument();
  });

  it("MPV-FR-XYXY: a card whose rows are all decided stays readable and offers no control", async () => {
    const b = backend(
      proposal([
        createRow("r1", "First", { state: "accepted", result: { issueNumber: 3 } }),
        createRow("r2", "Second", { state: "rejected" }),
      ]),
    );
    await renderCard(b);
    expect(screen.getByTestId("map-plan-proposal")).toHaveAttribute("data-settled", "true");
    expect(screen.getByText("First")).toBeInTheDocument();
    expect(within(screen.getByTestId("map-plan-proposal")).queryByRole("button")).toBeNull();
    expect(screen.queryByText(/proposes again/)).toBeNull();
  });
});

describe("the toasts and the log (MPV-FR-WKVL, MPV-FR-KMJX)", () => {
  it("MPV-FR-WKVL: an accepted row that wrote to GitHub shows an Info toast, and the row keeps its state", async () => {
    const b = backend(THREE());
    b.accept = async ({ rowKey }) => {
      b.proposal = withRow(b.proposal, rowKey, { state: "accepted", result: { issueNumber: 12 } });
      return b.proposal;
    };
    await renderCard(b);
    const user = userEvent.setup();
    await user.click(within(rowGroup(/^New issue Payout schedule/)).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(visibleToasts()[0]).toMatchObject({ level: "Info" });
    expect(rowGroup(/^New issue Payout schedule/)).toHaveAttribute("data-state", "accepted");
  });

  it("MPV-FR-WKVL: a decision that failed shows an Error toast and the row keeps its own error", async () => {
    const b = backend(THREE());
    b.accept = async ({ rowKey }) => {
      b.proposal = withRow(b.proposal, rowKey, {
        state: "failed",
        error: { code: "changed_on_tracker", message: "m" },
      });
      return b.proposal;
    };
    await renderCard(b);
    const user = userEvent.setup();
    await user.click(within(rowGroup(/^Change issue Decline message/)).getByRole("button", { name: /Accept/ }));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(visibleToasts()[0]).toMatchObject({ level: "Error" });
    expect(screen.getByTestId("mpv-row-error")).toBeInTheDocument();
  });

  it("MPV-FR-WKVL: a refused call shows an Error toast", async () => {
    const b = backend(THREE());
    b.accept = () => Promise.reject(new Error("not allowed"));
    await renderCard(b);
    const user = userEvent.setup();
    await user.click(within(rowGroup(/^New issue Payout schedule/)).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(visibleToasts()[0]).toMatchObject({ level: "Error" });
  });

  it("MPV-FR-KMJX: logs a decision and its outcome, and no record carries a title, a body, a label, or a draft name", async () => {
    const b = backend(
      proposal([
        createRow("r1", "Secret title", {}, { body: "Secret body", labels: ["secret-label"] }),
        draftRow("r2", "Secret draft", "Secret prompt"),
      ]),
    );
    b.accept = async ({ rowKey }) => {
      b.proposal = withRow(b.proposal, rowKey, { state: "accepted", result: { issueNumber: 5 } });
      return b.proposal;
    };
    b.reject = () => Promise.reject({ code: "stale_row", message: "Secret message" });
    await renderCard(b);
    const user = userEvent.setup();
    await user.click(within(rowGroup(/^New issue Secret title/)).getByRole("button", { name: "Accept" }));
    await waitFor(() =>
      expect(logInfo).toHaveBeenCalledWith(["frontend"], "map plan row decided", expect.objectContaining({ state: "accepted" })),
    );
    await user.click(within(rowGroup(/^New draft Secret draft/)).getByRole("button", { name: "Reject" }));
    await waitFor(() =>
      expect(logWarn).toHaveBeenCalledWith(["frontend"], "map plan row decision refused", expect.objectContaining({ code: "stale_row" })),
    );
    const records = JSON.stringify([...vi.mocked(logInfo).mock.calls, ...vi.mocked(logWarn).mock.calls]);
    for (const secret of ["Secret title", "Secret body", "secret-label", "Secret draft", "Secret prompt", "Secret message"]) {
      expect(records).not.toContain(secret);
    }
  });
});

describe("the item chip of a change row (MPV-FR-NMYT, MDL-FR-WTTQ)", () => {
  const CHANGE = () =>
    proposal([modifyRow("r1", 47, "Decline", [{ field: "title", before: "a", after: "b" }])]);

  it("MPV-FR-NMYT: names the item as a chip that selects it on the canvas where the item is on the canvas", async () => {
    const selectItem = vi.fn();
    await renderCard(backend(CHANGE()), { itemOnCanvas: (n) => n === 47, selectItem });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Select issue #47 on the canvas" }));
    expect(selectItem).toHaveBeenCalledWith(47);
  });

  it("MDL-FR-WTTQ: the chip hands the issue number to the canvas, which selects and centres the item", async () => {
    const selectItem = vi.fn();
    await renderCard(backend(CHANGE()), { itemOnCanvas: () => true, selectItem });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /Select issue #47/ }));
    expect(selectItem).toHaveBeenCalledTimes(1);
  });

  it("MPV-FR-NMYT: where the item is not on the canvas the chip is plain text", async () => {
    await renderCard(backend(CHANGE()), { itemOnCanvas: () => false, selectItem: vi.fn() });
    expect(screen.queryByRole("button", { name: /Select issue/ })).toBeNull();
    expect(screen.getByText("#47")).toBeInTheDocument();
  });
});
