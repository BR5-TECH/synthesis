/**
 * MPV-FR-HGLN: the viewport gives each tab that shows a discussion its own
 * in-place review of plan proposals, and gives none to the Map tab and the
 * Dashboard. The surfaces of the tabs are stubs that read the review context.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const unlistens: ReturnType<typeof vi.fn>[] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => {
    const off = vi.fn();
    unlistens.push(off);
    return off;
  }),
  emit: vi.fn(async () => {}),
}));

/** A stub surface: the review context it reads, a summary row, and the tab layer. */
async function probe(name: string) {
  const mp = await import("./MapPlanProposal");
  return function Probe({ tab }: { tab?: { threadId?: string } }) {
    const { inPlace } = mp.useMapPlan();
    return (
      <div data-testid={`${name}-probe`} data-in-place={String(inPlace === true)}>
        <div data-testid="content">
          <mp.MapPlanReviewLayer placement="tab" />
        </div>
        {tab?.threadId && <mp.MapPlanEntry discussionId={tab.threadId} proposalId="p1" proposer="arch" />}
      </div>
    );
  };
}
vi.mock("./ConversationTabView", async () => ({ ConversationTabView: await probe("conversation") }));
vi.mock("./SpecMap", async () => ({ SpecMap: await probe("map") }));
vi.mock("./Dashboard", async () => ({ Dashboard: await probe("dashboard") }));

import { Viewport } from "./Viewport";
import { resetMapPlanFallbackForTest } from "./MapPlanProposal";
import { resetLogBufferForTest } from "../logging";
import { resetToasts } from "../state/toasts";
import { otherCommand } from "../test/invokeMocks";
import { createRow, proposal } from "../test/mapPlanFixtures";
import type { Tab } from "../types";

const conversation = (id: string, threadId: string): Tab => ({
  id,
  label: id,
  tooltip: "",
  kind: "conversation",
  threadId,
  noteId: "n1",
  ownerTarget: { kind: "note", noteId: "n1" },
});
const MAP: Tab = { id: "map", label: "Map", tooltip: "", kind: "map" } as Tab;

function props(activeTab: string, activeT: Tab | undefined) {
  return { activeTab, activeT, onOpenDraft: vi.fn() } as unknown as ComponentProps<typeof Viewport>;
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    if (cmd === "list_map_plan_proposals") {
      return Promise.resolve([proposal([createRow("n1", "One")], { discussionId: args.discussionId as string })]);
    }
    if (cmd === "get_delivery_source") return Promise.resolve({ kind: "none", reason: "no_source" });
    return otherCommand(cmd);
  });
  unlistens.length = 0;
  resetMapPlanFallbackForTest();
  resetToasts();
  resetLogBufferForTest();
});
afterEach(cleanup);

describe("the in-place review scope of the viewport (MPV-FR-HGLN)", () => {
  it("MPV-FR-HGLN, MPV-FR-CTTA: the Map tab and the Dashboard get no in-place scope and draw no tab layer", () => {
    const view = render(<Viewport {...props("map", MAP)} />);
    expect(screen.getByTestId("map-probe")).toHaveAttribute("data-in-place", "false");
    expect(screen.queryByTestId("map-plan-announcement")).toBeNull();
    view.rerender(<Viewport {...props("dashboard", undefined)} />);
    expect(screen.getByTestId("dashboard-probe")).toHaveAttribute("data-in-place", "false");
    expect(screen.queryByTestId("map-plan-announcement")).toBeNull();
  });

  it("MPV-FR-HGLN: a conversation tab gets an in-place scope", () => {
    render(<Viewport {...props("conv:a", conversation("conv:a", "t1"))} />);
    expect(screen.getByTestId("conversation-probe")).toHaveAttribute("data-in-place", "true");
    expect(screen.getAllByTestId("map-plan-announcement")).toHaveLength(1);
  });

  it("MPV-FR-HGLN, MPV-FR-CZDO: a switch to another tab disposes the store of the old tab and closes its sheet", async () => {
    const a = conversation("conv:a", "t1");
    const b = conversation("conv:b", "t2");
    const view = render(<Viewport {...props(a.id, a)} />);
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /^Review:/ }));
    expect(await screen.findByTestId("pp-sheet")).toBeInTheDocument();
    await waitFor(() => expect(unlistens.length).toBeGreaterThan(0));
    const listening = [...unlistens];

    view.rerender(<Viewport {...props(b.id, b)} />);
    expect(screen.queryByTestId("pp-sheet")).toBeNull();
    for (const off of listening) expect(off).toHaveBeenCalled();

    view.rerender(<Viewport {...props(a.id, a)} />);
    expect(await screen.findByRole("button", { name: /^Review:/ })).toBeInTheDocument();
    expect(screen.queryByTestId("pp-sheet")).toBeNull();
  });
});
