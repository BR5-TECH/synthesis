/**
 * The review of a plan proposal in a tab other than the Map tab
 * (`MPV-map-proposal-review.md` MPV-FR-HGLN, MPV-FR-ZQRE, MPV-FR-CUWB): an
 * in-place scope around the shared discussion surface and the content pane of
 * a tab. The backend is mocked at `invoke`.
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
  emit: vi.fn(async () => {}),
}));

import { DiscussionSurface } from "../discussion/DiscussionSurface";
import { resetLogBufferForTest } from "../../logging";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import { clearAllDiscussionSessions } from "../../state/discussionSession";
import { MapPlanReviewStore } from "../../state/mapPlan/reviewStore";
import { resetToasts } from "../../state/toasts";
import { CONTEXT_ESTIMATE, otherCommand } from "../../test/invokeMocks";
import { AGENT, createRow, draftRow, mapComment, modifyRow, proposal, withRow } from "../../test/mapPlanFixtures";
import { emit, fakeHost } from "../../test/mapPlanReviewHarness";
import type { DeliverySettings, DeliverySource, Discussion, MapPlanProposal } from "../../types";
import { InPlaceReviewScope, MapPlanContext, MapPlanEntry, MapPlanReviewLayer } from "./index";

const EVENT = "map-plan-proposals-changed";

const SOURCE: DeliverySource = {
  kind: "github",
  project: { nodeId: "PVT_1", title: "Board", ownerLogin: "acme", number: 7, url: "" },
  repository: { host: "github.com", owner: "acme", name: "payments" },
  configuration: { state: "valid", errorCode: null, error: null, missingStatus: null, projectTitle: "Board" },
} as DeliverySource;

const SETTINGS: DeliverySettings = {
  source: { kind: "automatic" },
  createTypes: { epic: "Epic", feature: "Feature", task: "Task", defect: "Bug" },
  readEntries: [],
  untyped: null,
} as DeliverySettings;

interface Backend {
  proposals: Map<string, MapPlanProposal[]>;
  calls: string[];
  /** The answer of `get_delivery_source`: a source, or an error to reject with. */
  source: DeliverySource | Error;
}

function wire(): Backend {
  const b: Backend = { proposals: new Map(), calls: [], source: SOURCE };
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    b.calls.push(cmd);
    switch (cmd) {
      case "estimate_discussion_context":
        return Promise.resolve(CONTEXT_ESTIMATE);
      case "list_map_plan_proposals":
        return Promise.resolve(b.proposals.get(args.discussionId as string) ?? []);
      case "get_delivery_source":
        return b.source instanceof Error ? Promise.reject(b.source) : Promise.resolve(b.source);
      case "accept_map_plan_row": {
        const id = args.discussionId as string;
        const held = (b.proposals.get(id) ?? []).find((p) => p.id === args.proposalId);
        if (!held) return Promise.reject(new Error("no proposal"));
        const answered = withRow(held, args.rowKey as string, { state: "accepted", result: { issueNumber: 40 } });
        b.proposals.set(id, (b.proposals.get(id) ?? []).map((p) => (p.id === answered.id ? answered : p)));
        return Promise.resolve(answered);
      }
      case "get_delivery_settings":
        return Promise.resolve(SETTINGS);
      default:
        return otherCommand(cmd);
    }
  });
  return b;
}

/** A draft discussion, with an agent comment that carries proposal `p1` where `withProposal`. */
function thread(id = "t1", withProposal = true, proposalId = "p1", more: string[] = []): Discussion {
  const comments = [mapComment(`${id}-c0`, AGENT, "Let us plan.")];
  for (const pid of withProposal ? [proposalId, ...more] : more) {
    comments.push(
      mapComment(`${id}-c-${pid}`, AGENT, "Here is the plan.", {
        attachments: [{ kind: "mapPlanProposal", proposalId: pid }],
      }),
    );
  }
  return {
    id,
    target: { kind: "draft", draftId: "d1" },
    fragmentTarget: null,
    comments,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  };
}

function Tab({ discussion, name = "tab", openDraft }: { discussion: Discussion; name?: string; openDraft?: (id: string) => void }) {
  return (
    <InPlaceReviewScope openDraft={openDraft}>
      <div data-testid={`${name}-content`}>
        <MapPlanReviewLayer placement="tab" />
      </div>
      <div data-testid={`${name}-column`}>
        <DiscussionSurface
          discussion={discussion}
          owner={`column-${name}`}
          agents={[]}
          liveTurns={false}
          onReply={vi.fn(async () => undefined)}
          onSetResolved={vi.fn(async () => undefined)}
        />
      </div>
    </InPlaceReviewScope>
  );
}

const column = () => screen.getByTestId("tab-column");
const content = () => screen.getByTestId("tab-content");
const composer = () => column().querySelector("textarea") as HTMLTextAreaElement;
const detail = () => screen.getByTestId("pp-detail");

async function openFromRow() {
  const user = userEvent.setup();
  await user.click(await within(column()).findByRole("button", { name: /^Review:/ }));
  return { user, sheet: await within(content()).findByTestId("pp-sheet") };
}

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  resetToasts();
  resetDiscussionFocus();
  clearAllDiscussionSessions();
  resetLogBufferForTest();
});
afterEach(cleanup);

describe("the review in a tab other than the Map tab (MPV-FR-HGLN, MPV-FR-ZQRE)", () => {
  it("MPV-FR-HGLN, MPV-FR-ZQRE: a draft discussion shows Review and opens the sheet over the content pane, not over the column", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([createRow("n1", "New issue")])]);
    render(<Tab discussion={thread()} />);
    const { sheet } = await openFromRow();
    expect(sheet).toHaveAttribute("data-placement", "tab");
    expect(within(column()).queryByTestId("pp-sheet")).toBeNull();
    expect(within(column()).getByRole("button", { name: /^Hide review:/ })).toBeInTheDocument();
  });

  it("MPV-FR-UWQL: a proposal comment that the discussion held when the tab showed it opens nothing", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([createRow("n1", "New issue")])]);
    render(<Tab discussion={thread()} />);
    await within(column()).findByRole("button", { name: /^Review:/ });
    expect(screen.queryByTestId("pp-sheet")).toBeNull();
  });

  it("MPV-FR-UWQL, MPV-FR-KHDP, MPV-FR-FESO: a proposal comment that arrives opens the sheet in place and the composer keeps the focus", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([createRow("n1", "New issue")])]);
    const view = render(<Tab discussion={thread("t1", false)} />);
    composer().focus();
    view.rerender(<Tab discussion={thread("t1", true)} />);
    expect(await within(content()).findByTestId("pp-sheet")).toBeInTheDocument();
    expect(document.activeElement).toBe(composer());
    expect(composer()).not.toBeDisabled();
  });

  it("MPV-FR-PTUT: Discuss puts the focus on the composer of the discussion and keeps the sheet open", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([createRow("n1", "New issue")])]);
    render(<Tab discussion={thread()} />);
    const { user, sheet } = await openFromRow();
    await user.click(within(sheet).getByRole("button", { name: /Discuss/ }));
    expect(document.activeElement).toBe(composer());
    expect(screen.getByTestId("pp-sheet")).toBeInTheDocument();
  });

  it("MPV-FR-NMYT, MPV-FR-CUWB: the reference of a changed row is plain text, and the row shows no kind chip", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([modifyRow("m12", 12, "Old issue", [{ field: "title", before: "Old", after: "New" }])])]);
    render(<Tab discussion={thread()} />);
    await openFromRow();
    expect(within(detail()).getByText("#12")).toBeInTheDocument();
    expect(screen.queryByTestId("pp-detail-ref")).toBeNull();
    expect(within(detail()).queryByTestId("flow-kind-chip")).toBeNull();
  });

  it("MPV-FR-CUWB, MPV-FR-SNCR: a create row shows the kind that its Type maps to in the delivery settings", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([createRow("n1", "New issue", {}, { type: "Bug" })])]);
    render(<Tab discussion={thread()} />);
    await openFromRow();
    expect(await within(detail()).findByTestId("flow-kind-chip")).toHaveAttribute("data-kind", "defect");
  });

  it("MPV-FR-CUWB, MPV-FR-PIQU: the link of an applied issue row names the repository of the delivery source", async () => {
    const b = wire();
    const applied = withRow(
      proposal([modifyRow("m12", 12, "Old issue", [{ field: "title", before: "Old", after: "New" }]), createRow("n1", "Next")]),
      "m12",
      { state: "accepted", result: { issueUrl: "https://github.com/acme/payments/issues/12" } },
    );
    b.proposals.set("t1", [applied]);
    render(<Tab discussion={thread()} />);
    const { user, sheet } = await openFromRow();
    await user.click(within(sheet).getAllByTestId("pp-nav")[0]);
    expect(await within(detail()).findByRole("link", { name: /acme\/payments#12/ })).toBeInTheDocument();
  });

  it("MPV-FR-CUWB, MPV-FR-PIQU: Open draft of an accepted draft row opens the draft", async () => {
    const b = wire();
    const p = withRow(proposal([draftRow("d", "Idea", "Body."), createRow("n1", "Next")]), "d", {
      state: "accepted",
      result: { draftId: "draft-9" },
    });
    b.proposals.set("t1", [p]);
    const openDraft = vi.fn();
    render(<Tab discussion={thread()} openDraft={openDraft} />);
    const { user, sheet } = await openFromRow();
    await user.click(within(sheet).getAllByTestId("pp-nav")[0]);
    await user.click(within(detail()).getByRole("button", { name: "Open draft" }));
    expect(openDraft).toHaveBeenCalledWith("draft-9");
  });

  it("MPV-FR-CUWB: a tab whose discussion shows no proposal reads no delivery source", async () => {
    wire();
    render(<Tab discussion={thread("t1", false)} />);
    await screen.findByTestId("comment-thread-t1");
    expect(invokeMock.mock.calls.map((c) => c[0])).not.toContain("get_delivery_source");
  });

  it("MPV-FR-HGLN, MPV-FR-ZQRE: each tab holds its own sheet", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([createRow("n1", "One")])]);
    b.proposals.set("t2", [proposal([createRow("n2", "Two")], { id: "p2", discussionId: "t2" })]);
    render(
      <>
        <Tab discussion={thread("t1")} name="tab" />
        <Tab discussion={thread("t2", true, "p2")} name="other" />
      </>,
    );
    await openFromRow();
    await within(screen.getByTestId("other-column")).findByRole("button", { name: /^Review:/ });
    expect(within(screen.getByTestId("other-content")).queryByTestId("pp-sheet")).toBeNull();
  });

  it("MPV-FR-CZDO: a decision made in another window shows in the row and the sheet without a reload", async () => {
    const b = wire();
    const p = proposal([createRow("n1", "One"), createRow("n2", "Two")]);
    b.proposals.set("t1", [p]);
    render(<Tab discussion={thread()} />);
    await openFromRow();
    b.proposals.set("t1", [withRow(p, "n1", { state: "accepted", result: { issueNumber: 31 } })]);
    await waitFor(() => expect(listeners.get(EVENT)?.size ?? 0).toBeGreaterThan(0));
    emit(listeners, EVENT, { discussionId: "t1" });
    expect(await within(column()).findByText("1 of 2")).toBeInTheDocument();
    expect(screen.getByTestId("pp-sheet")).toBeInTheDocument();
  });
});

describe("the Map tab keeps its own arrivals (MPV-FR-CTTA)", () => {
  it("MPV-FR-CTTA, MPV-FR-UWQL: a surface under the Map review opens no sheet of its own and draws no tab layer", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([createRow("n1", "New issue")])]);
    const store = new MapPlanReviewStore();
    const arrived = vi.spyOn(store, "arrived");
    const mapTab = (d: Discussion) => (
      <MapPlanContext.Provider value={{ store, host: fakeHost() }}>
        <div data-testid="tab-content">
          <MapPlanReviewLayer placement="tab" />
        </div>
        <DiscussionSurface
          discussion={d}
          owner="column"
          agents={[]}
          liveTurns={false}
          onReply={vi.fn(async () => undefined)}
          onSetResolved={vi.fn(async () => undefined)}
        />
      </MapPlanContext.Provider>
    );
    const view = render(mapTab(thread("t1", false)));
    view.rerender(mapTab(thread("t1", true)));
    await screen.findByRole("button", { name: /^Review:/ });
    expect(arrived).not.toHaveBeenCalled();
    act(() => store.openReview("t1", "p1"));
    expect(within(content()).queryByTestId("pp-sheet")).toBeNull();
    expect(within(content()).queryByTestId("map-plan-announcement")).toBeNull();
  });
});

describe("arrivals, the delivery source, and the routes of the scope (MPV-FR-UWQL, MPV-FR-CUWB)", () => {
  const sourceReads = () => invokeMock.mock.calls.filter((c) => c[0] === "get_delivery_source").length;
  const settingsReads = () => invokeMock.mock.calls.filter((c) => c[0] === "get_delivery_settings").length;
  const APPLIED = () =>
    withRow(
      proposal([modifyRow("m12", 12, "Old issue", [{ field: "title", before: "Old", after: "New" }]), createRow("n1", "Next")]),
      "m12",
      { state: "accepted", result: { issueUrl: "https://github.com/acme/payments/issues/12" } },
    );

  it("MPV-FR-UWQL: a proposal that arrives with every row decided opens no sheet", async () => {
    const b = wire();
    const p = proposal([createRow("n1", "One")]);
    b.proposals.set("t1", [withRow(p, "n1", { state: "rejected" })]);
    const view = render(<Tab discussion={thread("t1", false)} />);
    view.rerender(<Tab discussion={thread("t1", true)} />);
    await within(column()).findByRole("button", { name: /^Show review:/ });
    expect(screen.queryByTestId("pp-sheet")).toBeNull();
  });

  it("MPV-FR-ZQRE, MPV-FR-UWQL: a second proposal that arrives while the sheet shows the first replaces it in the one sheet", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([createRow("n1", "One")])]);
    const view = render(<Tab discussion={thread("t1", true)} />);
    await openFromRow();
    b.proposals.set("t1", [proposal([createRow("n1", "One")]), proposal([createRow("n2", "Two")], { id: "p2" })]);
    view.rerender(<Tab discussion={thread("t1", true, "p1", ["p2"])} />);
    await waitFor(() => expect(within(detail()).getByText("Two")).toBeInTheDocument());
    expect(screen.getAllByTestId("pp-sheet")).toHaveLength(1);
  });

  it("MPV-FR-CUWB: a source that is not GitHub reads no settings, and the applied link names no repository", async () => {
    const b = wire();
    b.source = { kind: "none", reason: "no_source" } as DeliverySource;
    b.proposals.set("t1", [APPLIED()]);
    render(<Tab discussion={thread()} />);
    const { user, sheet } = await openFromRow();
    await waitFor(() => expect(sourceReads()).toBe(1));
    await user.click(within(sheet).getAllByTestId("pp-nav")[0]);
    const link = within(detail()).getByRole("link", { name: /#12/ });
    expect(link).toHaveTextContent(/^#12$/);
    expect(settingsReads()).toBe(0);
  });

  it("MPV-FR-CUWB: a refused source read leaves the review usable, with no repository and no settings read", async () => {
    const b = wire();
    b.source = new Error("refused");
    b.proposals.set("t1", [APPLIED()]);
    render(<Tab discussion={thread()} />);
    const { user, sheet } = await openFromRow();
    await waitFor(() => expect(sourceReads()).toBe(1));
    await user.click(within(sheet).getAllByTestId("pp-nav")[0]);
    expect(within(detail()).getByRole("link", { name: /#12/ })).toHaveTextContent(/^#12$/);
    expect(settingsReads()).toBe(0);
  });

  it("MPV-FR-CUWB: the delivery source is read once, however many changes the proposals have", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([createRow("n1", "One")])]);
    render(<Tab discussion={thread()} />);
    await within(column()).findByRole("button", { name: /^Review:/ });
    await waitFor(() => expect(listeners.get(EVENT)?.size ?? 0).toBeGreaterThan(0));
    for (let i = 0; i < 3; i++) emit(listeners, EVENT, { discussionId: "t1" });
    await waitFor(() => expect(invokeMock.mock.calls.filter((c) => c[0] === "list_map_plan_proposals").length).toBe(4));
    await waitFor(() => expect(settingsReads()).toBe(1));
    expect(sourceReads()).toBe(1);
  });

  it("MPV-FR-CUWB: a tab with no draft route offers no Open draft", async () => {
    const b = wire();
    b.proposals.set("t1", [
      withRow(proposal([draftRow("d", "Idea", "Body."), createRow("n1", "Next")]), "d", {
        state: "accepted",
        result: { draftId: "draft-9" },
      }),
    ]);
    render(<Tab discussion={thread()} />);
    const { user, sheet } = await openFromRow();
    await user.click(within(sheet).getAllByTestId("pp-nav")[0]);
    expect(within(detail()).getByText("Created the draft")).toBeInTheDocument();
    expect(within(detail()).queryByRole("button", { name: "Open draft" })).toBeNull();
  });

  it("MPV-FR-CUWB: Open draft takes the newest route that the shell gave", async () => {
    const b = wire();
    b.proposals.set("t1", [
      withRow(proposal([draftRow("d", "Idea", "Body."), createRow("n1", "Next")]), "d", {
        state: "accepted",
        result: { draftId: "draft-9" },
      }),
    ]);
    const first = vi.fn();
    const second = vi.fn();
    const view = render(<Tab discussion={thread()} openDraft={first} />);
    const { user, sheet } = await openFromRow();
    view.rerender(<Tab discussion={thread()} openDraft={second} />);
    await user.click(within(sheet).getAllByTestId("pp-nav")[0]);
    await user.click(within(detail()).getByRole("button", { name: "Open draft" }));
    expect(second).toHaveBeenCalledWith("draft-9");
    expect(first).not.toHaveBeenCalled();
  });

  it("MPV-FR-HGLN, MPV-FR-CZDO: an arrival and a decision in one tab leave the review of another tab as it was", async () => {
    const b = wire();
    b.proposals.set("t1", [proposal([createRow("n1", "One"), createRow("n2", "Two")])]);
    const pair = (d: Discussion) => (
      <>
        <Tab discussion={d} name="tab" />
        <InPlaceReviewScope>
          <div data-testid="other-content">
            <MapPlanReviewLayer placement="tab" />
          </div>
          <div data-testid="other-column">
            <MapPlanEntry discussionId="t1" proposalId="p1" proposer="arch" />
          </div>
        </InPlaceReviewScope>
      </>
    );
    const view = render(pair(thread("t1", false)));
    await within(screen.getByTestId("other-column")).findByRole("button", { name: /^Review:/ });
    view.rerender(pair(thread("t1", true)));
    const sheet = await within(content()).findByTestId("pp-sheet");
    expect(within(screen.getByTestId("other-content")).queryByTestId("pp-sheet")).toBeNull();
    const user = userEvent.setup();
    await user.click(within(sheet).getByRole("button", { name: "Accept" }));
    expect(await within(column()).findByText("1 of 2")).toBeInTheDocument();
    const other = screen.getByTestId("other-column");
    expect(within(other).queryByText("1 of 2")).toBeNull();
    expect(within(other).getByRole("button", { name: /^Review:/ })).toBeInTheDocument();
  });
});
