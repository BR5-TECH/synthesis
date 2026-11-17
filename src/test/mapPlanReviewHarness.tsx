/**
 * The harness of the tests of the review of a plan proposal: a backend mocked
 * at `invoke`, the event bus, and a render of the summary row with the sheet
 * layer under one review store. The test files mock `@tauri-apps/api/core`,
 * `@tauri-apps/api/event`, and `../../logging`, and pass their mocks in.
 */
import { act, render, screen } from "@testing-library/react";
import type { Mock } from "vitest";

import { otherCommand } from "./invokeMocks";
import { MapPlanReviewStore } from "../state/mapPlan/reviewStore";
import type { MapPlanProposal } from "../types";
import { MapPlanContext, MapPlanEntry, MapPlanReviewLayer, type MapPlanHost } from "../components/MapPlanProposal";

export type Listeners = Map<string, Set<(e: { payload: unknown }) => void>>;

export interface Backend {
  proposal: MapPlanProposal;
  calls: { cmd: string; args: Record<string, unknown> }[];
  accept?: (args: { rowKey: string }) => Promise<MapPlanProposal>;
  reject?: (args: { rowKey: string }) => Promise<MapPlanProposal>;
  reopen?: (args: { rowKey: string }) => Promise<MapPlanProposal>;
  reprepare?: () => Promise<MapPlanProposal>;
}

export function wire(invokeMock: Mock, b: Backend) {
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "list_map_plan_proposals":
        return Promise.resolve([b.proposal]);
      case "accept_map_plan_row":
        return b.accept ? b.accept(args as { rowKey: string }) : Promise.resolve(b.proposal);
      case "reject_map_plan_row":
        return b.reject ? b.reject(args as { rowKey: string }) : Promise.resolve(b.proposal);
      case "reopen_map_plan_row":
        return b.reopen ? b.reopen(args as { rowKey: string }) : Promise.resolve(b.proposal);
      case "request_map_plan_reprepare":
        return b.reprepare ? b.reprepare() : Promise.resolve(b.proposal);
      case "open_tracker_link":
        return Promise.resolve();
      default:
        return otherCommand(cmd);
    }
  });
}

export function backend(invokeMock: Mock, p: MapPlanProposal): Backend {
  const b: Backend = { proposal: p, calls: [] };
  wire(invokeMock, b);
  return b;
}

export const callsOf = (b: Backend, cmd: string) => b.calls.filter((c) => c.cmd === cmd).map((c) => c.args);

export function fakeHost(over: Partial<MapPlanHost> = {}): MapPlanHost {
  return {
    itemOnCanvas: () => true,
    selectItem: () => {},
    openDraft: () => {},
    discuss: () => {},
    showDelivery: () => {},
    repository: "acme/app",
    kindOfIssue: () => "feature",
    kindOfType: (type) => (type ? "feature" : null),
    ...over,
  };
}

/** Render the summary row and the sheet layer, and wait for the row. */
export async function renderReview(
  b: Backend,
  options: { host?: MapPlanHost | null; proposer?: string } = {},
) {
  const store = new MapPlanReviewStore();
  const host = options.host === undefined ? fakeHost() : options.host;
  const view = render(
    <MapPlanContext.Provider value={{ store, host }}>
      <MapPlanEntry discussionId="t1" proposalId={b.proposal.id} proposer={options.proposer ?? "arch"} />
      <div data-testid="canvas-area">
        <MapPlanReviewLayer />
      </div>
    </MapPlanContext.Provider>,
  );
  await screen.findByTestId("map-plan-entry");
  return { store, view };
}

export function emit(listeners: Listeners, event: string, payload: unknown) {
  act(() => {
    for (const cb of listeners.get(event) ?? []) cb({ payload });
  });
}
