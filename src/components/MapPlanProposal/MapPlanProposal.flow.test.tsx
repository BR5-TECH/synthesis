/**
 * The review sheet as the agent and the author steer it
 * (`MPV-map-proposal-review.md`): the summary row of an update comment, the
 * sheet that opens on its own when a proposal arrives, its redraw on an update,
 * and its close after the last decision. The backend is mocked at `invoke`.
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
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import { resetToasts } from "../../state/toasts";
import { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";
import { draftRow, proposal, withRow } from "../../test/mapPlanFixtures";
import { backend, emit, fakeHost, type Backend } from "../../test/mapPlanReviewHarness";
import type { ProposalUpdateCounts } from "../../types";
import { MapPlanContext, MapPlanEntry, MapPlanReviewLayer, type MapPlanHost } from "./index";

const EVENT = "map-plan-proposals-changed";

const THREE = () => proposal([draftRow("a", "Alpha", "A."), draftRow("b", "Beta", "B."), draftRow("c", "Gamma", "C.")]);

function renderPair(
  b: Backend,
  options: { host?: MapPlanHost; update?: ProposalUpdateCounts } = {},
) {
  const store = new MapPlanReviewStore();
  const host = options.host ?? fakeHost();
  render(
    <MapPlanContext.Provider value={{ store, host }}>
      <input aria-label="Composer" />
      <div data-testid="first">
        <MapPlanEntry discussionId="t1" proposalId={b.proposal.id} proposer="arch" />
      </div>
      {options.update && (
        <div data-testid="second">
          <MapPlanEntry discussionId="t1" proposalId={b.proposal.id} proposer="arch" update={options.update} />
        </div>
      )}
      <MapPlanReviewLayer />
    </MapPlanContext.Provider>,
  );
  return { store, host };
}

const nav = () => screen.getAllByTestId("pp-nav");

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  resetToasts();
});

afterEach(cleanup);

describe("update comments (MPV-FR-QZUD)", () => {
  it("MPV-FR-QZUD: an update comment shows the counts of the update, with zero left out, and opens the same sheet", async () => {
    const b = backend(invokeMock, THREE());
    renderPair(b, { update: { changed: 1, added: 0, withdrawn: 2 } });
    const second = screen.getByTestId("second");
    expect(await within(second).findByText("Updated: 1 changed, 2 removed")).toBeInTheDocument();
    await within(screen.getByTestId("first")).findByText("3 draft");
    const user = userEvent.setup();
    await user.click(within(second).getByTestId("map-plan-entry-toggle"));
    expect(await screen.findByTestId("pp-sheet")).toBeInTheDocument();
    expect(nav()).toHaveLength(3);
  });

  it("MPV-FR-PBAD, MPV-FR-QZUD: closing the sheet gives the focus to the newest summary row", async () => {
    const b = backend(invokeMock, THREE());
    renderPair(b, { update: { changed: 1, added: 0, withdrawn: 0 } });
    await screen.findByText("Updated: 1 changed");
    const user = userEvent.setup();
    await user.click(within(screen.getByTestId("first")).getByTestId("map-plan-entry-toggle"));
    await screen.findByTestId("pp-sheet");
    await user.keyboard("{Escape}");
    expect(document.activeElement).toBe(within(screen.getByTestId("second")).getByTestId("map-plan-entry-toggle"));
  });
});

describe("the sheet opens on its own (MPV-FR-UWQL, MPV-FR-KHDP)", () => {
  it("MPV-FR-UWQL, MPV-FR-KHDP: an arrived proposal with a pending row opens the sheet on the Delivery lens and leaves the focus", async () => {
    const b = backend(invokeMock, THREE());
    // MPV-FR-UWQL: the lens switches first, so the sheet is not open yet.
    const openWhenShown: boolean[] = [];
    const showDelivery = vi.fn(() => openWhenShown.push(screen.queryByTestId("pp-sheet") !== null));
    const { store } = renderPair(b, { host: fakeHost({ showDelivery }) });
    await screen.findByText("3 draft");
    const composer = screen.getByLabelText("Composer");
    composer.focus();
    act(() => store.arrived("t1", "p1", showDelivery));
    expect(await screen.findByTestId("pp-sheet")).toBeInTheDocument();
    await screen.findByTestId("pp-detail");
    expect(openWhenShown).toEqual([false]);
    expect(document.activeElement).toBe(composer);
  });

  it("MPV-FR-UWQL: a proposal that is not read yet opens once its read lands", async () => {
    const b = backend(invokeMock, proposal([draftRow("x", "Other", "X.")], { id: "p0" }));
    const { store } = renderPair(b);
    await screen.findByText("1 draft");
    b.proposal = THREE();
    act(() => store.arrived("t1", "p1"));
    expect(await screen.findByTestId("pp-sheet")).toBeInTheDocument();
    expect(nav()).toHaveLength(3);
  });

  it("MPV-FR-UWQL: a proposal with no pending row opens nothing", async () => {
    const decided = withRow(withRow(withRow(THREE(), "a", { state: "rejected" }), "b", { state: "rejected" }), "c", {
      state: "rejected",
    });
    const b = backend(invokeMock, decided);
    const showDelivery = vi.fn();
    const { store } = renderPair(b, { host: fakeHost({ showDelivery }) });
    await screen.findByText("Show review");
    act(() => store.arrived("t1", "p1", showDelivery));
    expect(screen.queryByTestId("pp-sheet")).toBeNull();
    expect(showDelivery).not.toHaveBeenCalled();
  });
});

describe("an update redraws the sheet (MPV-FR-GRTP)", () => {
  it("MPV-FR-GRTP: a changed row redraws in place, an added row joins the list, and the selection stays", async () => {
    const b = backend(invokeMock, THREE());
    renderPair(b);
    await screen.findByText("3 draft");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    await screen.findByTestId("pp-sheet");
    await user.click(nav()[1]);
    expect(nav()[1]).toHaveAttribute("aria-selected", "true");
    b.proposal = {
      ...THREE(),
      rows: [draftRow("a", "Alpha two", "A."), draftRow("b", "Beta", "B."), draftRow("c", "Gamma", "C."), draftRow("d", "Delta", "D.")],
    };
    emit(listeners, EVENT, { discussionId: "t1" });
    await waitFor(() => expect(nav()).toHaveLength(4));
    expect(nav()[0]).toHaveTextContent("Alpha two");
    expect(nav()[1]).toHaveAttribute("aria-selected", "true");
  });

  it("MPV-FR-GRTP, MPV-FR-CAQL: a withdrawn selected row leaves the list, and the first pending row is selected", async () => {
    const b = backend(invokeMock, withRow(THREE(), "a", { state: "rejected" }));
    renderPair(b);
    await screen.findByText("1 of 3");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    await screen.findByTestId("pp-sheet");
    await user.click(nav()[2]);
    b.proposal = { ...b.proposal, rows: b.proposal.rows.filter((r) => r.key !== "c") };
    emit(listeners, EVENT, { discussionId: "t1" });
    await waitFor(() => expect(nav()).toHaveLength(2));
    expect(nav()[1]).toHaveAttribute("aria-selected", "true");
    expect(nav()[1]).toHaveTextContent("Beta");
  });
});

describe("a withdrawn selected row in the middle (MPV-FR-GRTP)", () => {
  it("MPV-FR-GRTP, MPV-FR-CAQL: the selection moves to the next pending row after the withdrawn one", async () => {
    const four = proposal([
      draftRow("a", "Alpha", "A."),
      draftRow("b", "Beta", "B."),
      draftRow("c", "Gamma", "C."),
      draftRow("d", "Delta", "D."),
    ]);
    const b = backend(invokeMock, four);
    renderPair(b);
    await screen.findByText("4 draft");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    await screen.findByTestId("pp-sheet");
    await user.click(nav()[1]);
    b.proposal = { ...four, rows: four.rows.filter((r) => r.key !== "b") };
    emit(listeners, EVENT, { discussionId: "t1" });
    await waitFor(() => expect(nav()).toHaveLength(3));
    expect(nav()[1]).toHaveTextContent("Gamma");
    expect(nav()[1]).toHaveAttribute("aria-selected", "true");
  });
});

describe("the sheet closes on its own (MPV-FR-BBQY, MPV-FR-VXHC)", () => {
  it("MPV-FR-BBQY: the decision that leaves no row pending closes the sheet and focuses the summary row", async () => {
    const b = backend(invokeMock, withRow(withRow(THREE(), "a", { state: "accepted" }), "b", { state: "rejected" }));
    b.accept = async ({ rowKey }) => (b.proposal = withRow(b.proposal, rowKey, { state: "accepted" }));
    renderPair(b);
    await screen.findByText("2 of 3");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    await screen.findByTestId("pp-sheet");
    await user.click(within(screen.getByTestId("pp-detail")).getByRole("button", { name: /^Accept/ }));
    await waitFor(() => expect(screen.queryByTestId("pp-sheet")).toBeNull());
    expect(document.activeElement).toBe(screen.getByTestId("map-plan-entry-toggle"));
  });

  it("MPV-FR-BBQY: a decision that leaves a pending row keeps the sheet open", async () => {
    const b = backend(invokeMock, THREE());
    b.reject = async ({ rowKey }) => (b.proposal = withRow(b.proposal, rowKey, { state: "rejected" }));
    renderPair(b);
    await screen.findByText("3 draft");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    await screen.findByTestId("pp-sheet");
    await user.click(within(screen.getByTestId("pp-detail")).getByRole("button", { name: "Reject" }));
    await screen.findByText("1 of 3");
    expect(screen.getByTestId("pp-sheet")).toBeInTheDocument();
  });

  it("MPV-FR-BBQY: a last decision that failed keeps the sheet open on the failed row", async () => {
    const b = backend(invokeMock, withRow(withRow(THREE(), "a", { state: "accepted" }), "b", { state: "rejected" }));
    b.accept = async ({ rowKey }) =>
      (b.proposal = withRow(b.proposal, rowKey, { state: "failed", error: { code: "draft_create_failed", message: "No draft." } }));
    renderPair(b);
    await screen.findByText("2 of 3");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    await screen.findByTestId("pp-sheet");
    await user.click(within(screen.getByTestId("pp-detail")).getByRole("button", { name: /^Accept/ }));
    expect(await within(screen.getByTestId("pp-detail")).findByText(/No draft\./)).toBeInTheDocument();
    expect(screen.getByTestId("pp-sheet")).toBeInTheDocument();
    expect(nav()[2]).toHaveAttribute("aria-selected", "true");
  });

  it("MPV-FR-VXHC: Accept all that ends with a failed row keeps the sheet open on that row", async () => {
    const b = backend(invokeMock, THREE());
    b.accept = async ({ rowKey }) =>
      (b.proposal = withRow(b.proposal, rowKey, rowKey === "b" ? { state: "failed", error: { code: "x", message: "No." } } : { state: "accepted" }));
    renderPair(b);
    await screen.findByText("3 draft");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    await screen.findByTestId("pp-sheet");
    await user.click(within(screen.getByTestId("pp-bar")).getByRole("button", { name: "Accept all" }));
    await waitFor(() => expect(screen.getByTestId("pp-bar")).toHaveTextContent("All rows decided"));
    expect(screen.getByTestId("pp-sheet")).toBeInTheDocument();
    await waitFor(() => expect(nav()[1]).toHaveAttribute("aria-selected", "true"));
    expect(within(screen.getByTestId("pp-detail")).getByText(/No\./)).toBeInTheDocument();
  });

  it("MPV-FR-VXHC: a decision made in another window does not close the sheet", async () => {
    const b = backend(invokeMock, withRow(withRow(THREE(), "a", { state: "accepted" }), "b", { state: "rejected" }));
    renderPair(b);
    await screen.findByText("2 of 3");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    await screen.findByTestId("pp-sheet");
    b.proposal = withRow(b.proposal, "c", { state: "rejected" });
    emit(listeners, EVENT, { discussionId: "t1" });
    await waitFor(() => expect(screen.getByTestId("pp-bar")).toHaveTextContent("All rows decided"));
    expect(screen.getByTestId("pp-sheet")).toBeInTheDocument();
  });
});

describe("code blocks in the body diff (MPV-FR-ZCWQ, MPV-FR-JVZN)", () => {
  it("MPV-FR-ZCWQ: a Mermaid block shows its source with each line and indentation, and its language", async () => {
    const body = "Intro.\n\n```mermaid\nflowchart LR\n    A --> B\n```\n";
    const b = backend(invokeMock, proposal([draftRow("a", "Diagram", body)]));
    renderPair(b);
    await screen.findByText("1 draft");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    const code = await screen.findByTestId("pp-code");
    expect(code).toHaveAttribute("data-language", "mermaid");
    expect(within(code).getByText("mermaid")).toBeInTheDocument();
    const pre = code.querySelector("pre")!;
    expect(pre.textContent).toBe("flowchart LR\n    A --> B\n");
  });

  it("MPV-FR-JVZN: an edited code block marks each added and removed line", async () => {
    const before = "```mermaid\nflowchart LR\n    A --> B\n```\n";
    const after = "```mermaid\nflowchart LR\n    A --> C\n```\n";
    const row = {
      ...draftRow("m", "Diagram", ""),
      kind: "modify_issue",
      fields: {
        number: 4,
        title: "Diagram",
        baseUpdatedAt: "2026-01-01T00:00:00Z",
        changes: [{ field: "body", before, after }],
      },
    } as unknown as ReturnType<typeof draftRow>;
    const b = backend(invokeMock, proposal([row]));
    renderPair(b);
    await screen.findByText("#4, 1 changed");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("map-plan-entry-toggle"));
    const code = await screen.findByTestId("pp-code");
    const lines = [...code.querySelectorAll(".pp-code__line")];
    expect(lines.map((l) => l.getAttribute("data-mark"))).toEqual(["ctx", "del", "ins"]);
    expect(lines[1].querySelector(".pp-code__g")).toHaveTextContent("−");
    expect(lines[2].querySelector(".pp-code__g")).toHaveTextContent("+");
    expect(within(screen.getByTestId("pp-doc")).getByText("1 block edited")).toBeInTheDocument();
  });
});
