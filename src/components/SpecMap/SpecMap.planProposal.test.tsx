/**
 * The review of a plan proposal in the real Map tab
 * (`MPV-map-proposal-review.md`, `DBR-delivery-board.md` DBR-FR-AGIN ..
 * DBR-FR-PMGS, `SMP-specification-map.md` SMP-FR-PPJB). The host the tab
 * builds, the shared review store, the Discussion column, the sheet, and the
 * board run together. The plan names its items by its own ids, not
 * `issue-N`, so every lookup goes through the tracker facts.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { resetAgentRegistry } from "../../state/agentRegistry";
import { clearAllDiscussionSessions } from "../../state/discussionSession";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import { resetProjectIdentity } from "../../state/projectIdentity";
import { resetToasts } from "../../state/toasts";
import { ghostItemId } from "../../state/mapPlan/derive";
import { CONTEXT_ESTIMATE } from "../../test/invokeMocks";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  clearLensStores,
  deliveryBackend,
  githubAnswer,
  githubSource,
  issueFacts,
  milestoneFacts,
  renderLens,
  settle,
  type DeliveryBackend,
  type LensRender,
} from "../../test/deliveryLensHarness";
import { AGENT, HUMAN, createRow, mapComment, mapThread, modifyRow, proposal } from "../../test/mapPlanFixtures";
import type { Discussion, MapPlanProposal } from "../../types";

const DOC = plan([
  item("m1", "milestone"),
  item("t1", "task", { parentId: "m1" }),
  item("t2", "task", { parentId: "m1" }),
]);

const PROPOSAL = (): MapPlanProposal =>
  proposal(
    [
      modifyRow("r42", 42, "Tokenize card numbers", [{ field: "title", before: "Tokenize cards", after: "Tokenize card numbers" }]),
      createRow("rnew", "Payout retry limits", {}, { milestone: { number: 3, title: "Beta" }, type: "Task" }),
      modifyRow("r48", 48, "Refund emails", [{ field: "close", before: "open", after: "closed", reason: "not_planned" }]),
    ],
    { discussionId: "t-map", commentId: "t-map-c1" },
  );

let b: DeliveryBackend;
let thread: Discussion;
let current: MapPlanProposal;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  resetAgentRegistry();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetProjectIdentity();
  current = PROPOSAL();
  thread = mapThread("t-map", ["What is left?"]);
  thread.comments.push(
    mapComment("t-map-c1", AGENT, "I propose three changes.", {
      attachments: [{ kind: "mapPlanProposal", proposalId: "p1" }],
    }),
  );
  b = deliveryBackend(
    "",
    githubAnswer(DOC, {
      tracker: [milestoneFacts("m1", 3), issueFacts("t1", 42), issueFacts("t2", 48)],
    }),
  );
  b.on.list_map_discussions = () => [thread];
  // MPR-FR-MYLA: no thread is about a node here, so a node look-up finds none.
  b.on.find_map_discussion = (args) => (args.mapNode ? null : thread);
  b.on.read_discussion = () => thread;
  b.on.list_project_agents = () => [{ agent: { id: "a1", nickname: "arch" }, availability: "ready" }];
  b.on.resolve_comment_author_identity = () => HUMAN;
  b.on.get_active_ai_api_catalog = () => ({ resolution: "none_configured", catalog: null });
  b.on.estimate_discussion_context = () => CONTEXT_ESTIMATE;
  b.on.list_ai_api_catalogs = () => [];
  b.on.list_agent_turns = () => [];
  b.on.list_map_plan_proposals = () => [structuredClone(current)];
  b.on.accept_map_plan_row = (args) => {
    current = {
      ...current,
      rows: current.rows.map((r) => (r.key === args.rowKey ? { ...r, state: "accepted" as const } : r)),
    };
    return structuredClone(current);
  };
});

afterEach(() => {
  cleanup();
  clearLensStores();
  vi.restoreAllMocks();
});

const lensRadio = (name: string) =>
  within(screen.getByRole("radiogroup", { name: "Lens" })).getByRole("radio", { name });

/** The Map tab on the Delivery lens with the Discussion column shown. */
async function openMap(): Promise<LensRender> {
  const r = await renderLens({ source: githubSource() });
  act(() => r.store.setDiscussionVisible(true));
  await screen.findByTestId("flow-item-t1");
  await settle();
  await screen.findByTestId("map-plan-entry");
  return r;
}

async function review() {
  const user = userEvent.setup();
  await user.click(await screen.findByTestId("map-plan-entry-toggle"));
  return { user, sheet: await screen.findByTestId("pp-sheet") };
}

const detail = () => screen.getByTestId("pp-detail");

describe("the review in the Map tab (SMP-FR-PPJB, MPV-FR-CTTA, MPV-FR-FNIS)", () => {
  it("SMP-FR-PPJB, MPV-FR-FNIS, MPV-FR-CTTA: Review on the Specifications lens shows the Delivery lens and the sheet over its canvas", async () => {
    await openMap();
    fireEvent.click(lensRadio("Specifications"));
    await screen.findByTestId("spec-map-canvas");
    expect(screen.queryByTestId("pp-sheet")).toBeNull();
    const { sheet } = await review();
    expect(lensRadio("Delivery")).toBeChecked();
    expect(sheet.closest(".dlens-main")).not.toBeNull();
    // The sheet covers the canvas area only: the column is outside it.
    expect(screen.getByTestId("map-discussion").contains(sheet)).toBe(false);
  });

  it("MPV-FR-CTTA, MPV-FR-FESO: the composer takes text while the sheet shows", async () => {
    await openMap();
    const { user } = await review();
    const composer = within(screen.getByTestId("map-discussion")).getByRole("textbox");
    await user.type(composer, "one question");
    expect(composer).toHaveValue("one question");
    expect(screen.getByTestId("pp-sheet")).toBeInTheDocument();
  });

  it("MPV-FR-AAFF: the proposer is the agent that wrote the comment", async () => {
    current = { ...current, rows: current.rows.map((r) => ({ ...r, state: "failed" as const, error: { code: "x", message: "No." } })) };
    await openMap();
    await review();
    expect(within(detail()).getByRole("button", { name: "Ask @arch to re-prepare" })).toBeInTheDocument();
  });
});

describe("the board under the review (DBR-FR-AGIN, DBR-FR-WKNI, DBR-FR-PMGS)", () => {
  it("DBR-FR-AGIN: the cards of the changed and the withdrawn issue take their marks, through the plan's own ids", async () => {
    await openMap();
    await waitFor(() =>
      expect(within(screen.getByTestId("flow-item-t1")).getByTestId("flow-marker-proposal")).toHaveTextContent("proposed"),
    );
    expect(within(screen.getByTestId("flow-item-t2")).getByTestId("flow-marker-proposal")).toHaveTextContent("withdrawn");
  });

  it("DBR-FR-WKNI, MPV-FR-SNCR: the added issue shows as a ghost card with the kind its Type maps to", async () => {
    await openMap();
    const ghost = await screen.findByTestId(`flow-item-${ghostItemId("p1", "rnew")}`);
    expect(ghost).toHaveAttribute("data-ghost", "true");
    expect(ghost).toHaveTextContent("Payout retry limits");
    expect(within(ghost).getByTestId("flow-kind-chip")).toHaveAccessibleName(/task/i);
  });

  it("DBR-FR-PMGS: the card of the selected row takes the selected ring while the sheet shows", async () => {
    await openMap();
    const { user, sheet } = await review();
    expect(screen.getByTestId("flow-item-t1")).toHaveAttribute("data-selected", "true");
    await user.click(within(sheet).getAllByTestId("pp-nav")[2]);
    expect(screen.getByTestId("flow-item-t2")).toHaveAttribute("data-selected", "true");
    expect(screen.getByTestId("flow-item-t1")).toHaveAttribute("data-selected", "false");
  });

  it("DBR-FR-PMGS, DBR-FR-AGIN: an applied row takes away its mark", async () => {
    await openMap();
    const { user } = await review();
    await user.click(within(detail()).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(within(screen.getByTestId("flow-item-t1")).queryByTestId("flow-marker-proposal")).toBeNull());
    expect(b.calls.filter((c) => c.cmd === "accept_map_plan_row").map((c) => c.args.rowKey)).toEqual(["r42"]);
  });
});

describe("the host of the Map tab (MPV-FR-NMYT, MPV-FR-PTUT)", () => {
  it("MPV-FR-NMYT: the reference of a changed row selects its card through the tracker facts", async () => {
    const r = await openMap();
    const { user } = await review();
    await user.click(within(detail()).getByTestId("pp-detail-ref"));
    await waitFor(() => expect(r.store.snapshot().deliveryNode?.id).toBe("t1"));
  });

  it("MPV-FR-PTUT: Discuss on a changed row makes its card the focus node of the composer", async () => {
    const r = await openMap();
    const { user } = await review();
    await user.click(within(detail()).getByRole("button", { name: "Discuss" }));
    await waitFor(() => expect(r.store.snapshot().discussion.node?.id).toBe("t1"));
    expect(screen.getByTestId("pp-sheet")).toBeInTheDocument();
  });

  it("MPV-FR-PTUT: Discuss on an added row keeps the focus of the thread and asks for the composer", async () => {
    const r = await openMap();
    const { user, sheet } = await review();
    await user.click(within(sheet).getAllByTestId("pp-nav")[1]);
    await user.click(within(detail()).getByRole("button", { name: "Discuss" }));
    expect(r.store.snapshot().discussion.node).toBeNull();
    await waitFor(() => expect(within(screen.getByTestId("map-discussion")).getByRole("textbox")).toHaveFocus());
  });
});
