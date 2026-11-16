// The Comments panel row of a Map thread (`specifications/ui/CMP-comments-panel.md`
// CMP-FR-02, CMP-FR-10, CMP-FR-27; `CMT-comments.md` CMT-FR-LVNQ). The backend
// is mocked at `invoke`, so every assertion is about what the panel renders and
// which route it calls.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetAgentRegistry } from "../state/agentRegistry";
import { resetLogBufferForTest } from "../logging";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetOwnerAvailability } from "../state/ownerAvailability";
import { resetProjectIdentity } from "../state/projectIdentity";
import type { DiscussionListItem } from "../types";
import { Harness, localHuman, rows } from "../test/commentsPanelFixtures";
import { otherCommand } from "../test/invokeMocks";
import { ITEM_NODE, SPEC_NODE, mapComment, mapThread } from "../test/mapPlanFixtures";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

function listReturns(items: DiscussionListItem[]) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_all_discussions") return Promise.resolve(items);
    if (cmd === "resolve_comment_author_identity") return Promise.resolve(localHuman);
    return otherCommand(cmd);
  });
}

const item = (d: ReturnType<typeof mapThread>): DiscussionListItem => ({ discussion: d, ownerUnavailable: false });

beforeEach(() => {
  invokeMock.mockReset();
  resetAgentRegistry();
  clearAllDiscussionSessions();
  resetOwnerAvailability();
  resetLogBufferForTest();
  resetProjectIdentity();
});

afterEach(cleanup);

describe("a Map thread in the panel (CMP-FR-02, CMP-FR-27, CMT-FR-LVNQ)", () => {
  it("CMP-FR-02: lists a Map thread beside the discussions of the other owners", async () => {
    listReturns([item(mapThread("m1", ["What is left of the plan?"]))]);
    render(<Harness />);
    expect(await screen.findByText("What is left of the plan?")).toBeInTheDocument();
    expect(rows()).toHaveLength(1);
  });

  it("CMP-FR-27: the Map thread carries the label Map and states that it is a whole-owner discussion", async () => {
    listReturns([item(mapThread("m1", ["What is left of the plan?"]))]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(1));
    const row = screen.getByRole("button", { name: /^Whole discussion on Map/ });
    expect(within(row).getByText("Whole")).toBeInTheDocument();
    expect(within(row).getByText(/about the whole map/i)).toBeInTheDocument();
  });

  it("CMP-FR-27: a node thread carries the label of its node", async () => {
    listReturns([item(mapThread("m2", ["About the node"], ITEM_NODE))]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(screen.getByRole("button", { name: /^Whole discussion on Decline message/ })).toBeInTheDocument();
  });

  it("CMT-FR-LVNQ: a Map thread is listed in the project-wide panel with its opening comment and reply count", async () => {
    const thread = mapThread("m1", ["Opening"]);
    thread.comments.push(mapComment("m1-c1", localHuman, "Reply"));
    listReturns([item(thread)]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(rows()[0]).toHaveTextContent("Opening");
    expect(rows()[0]).toHaveTextContent("1");
  });
});

describe("the click-through of a Map thread (CMP-FR-10, SMP-FR-QHBV, CVP-FR-54)", () => {
  it("CMP-FR-10: activating the row calls the one route with the Map target and no node for the Map thread", async () => {
    const onReveal = vi.fn();
    listReturns([item(mapThread("m1", ["What is left of the plan?"]))]);
    render(<Harness onReveal={onReveal} />);
    await screen.findByText("What is left of the plan?");
    await userEvent.click(rows()[0] as HTMLElement);
    expect(onReveal).toHaveBeenCalledTimes(1);
    expect(onReveal).toHaveBeenCalledWith(
      expect.objectContaining({
        discussionId: "m1",
        target: { kind: "map" },
        fragmentTarget: null,
        ownerLabel: "Map",
        mapNode: null,
        ownerUnavailable: false,
      }),
    );
  });

  it("SMP-FR-QHBV: the row of a node thread hands the node to the route, so the Map tab selects that thread", async () => {
    const onReveal = vi.fn();
    listReturns([item(mapThread("m2", ["About the spec"], SPEC_NODE))]);
    render(<Harness onReveal={onReveal} />);
    await screen.findByText("About the spec");
    await userEvent.click(rows()[0] as HTMLElement);
    expect(onReveal).toHaveBeenCalledWith(
      expect.objectContaining({
        discussionId: "m2",
        target: { kind: "map" },
        ownerLabel: "Specification map",
        mapNode: SPEC_NODE,
      }),
    );
  });

  it("CVP-FR-54: the row of a thread of another owner hands no Map node", async () => {
    const onReveal = vi.fn();
    const other = mapThread("a1", ["About a file"]);
    other.target = { kind: "artifact", artifactId: "specs/a.md" };
    listReturns([item(other)]);
    render(<Harness onReveal={onReveal} />);
    await screen.findByText("About a file");
    await userEvent.click(rows()[0] as HTMLElement);
    expect(onReveal.mock.calls[0][0]).not.toHaveProperty("mapNode");
  });
});
