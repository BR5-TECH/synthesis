/**
 * The backend and the render of the Flow canvas tests (FLO).
 *
 * A test file mocks `@tauri-apps/api/core` and `@tauri-apps/api/event` with
 * `./flowMocks`, and gets one routed backend per test from `flowBackend`. The backend answers by command name, answers the
 * background `append_log_records`, and rejects a command it does not know, so
 * no timer of the code under test throws into the next test.
 */
import type { ComponentProps } from "react";
import { act, render, waitFor } from "@testing-library/react";
import { expect, vi } from "vitest";

import { FlowCanvas, fileBinding } from "../components/Flow";
import { FlowSessionStore } from "../state/flowSessions";
import {
  resolveLayout,
  serializeFlowDocument,
  type FlowDocument,
  type FlowLayout,
  type StoredLayout,
} from "../state/flowDocument";
import { flowLayoutScope } from "../types/delivery";
import type {
  DraftSummary,
  FlowDeliveryState,
  FlowItemState,
  FlowOverlapReport,
  ItemDeliveryState,
} from "../types";
import { ID, TREE } from "./flowFixtures";
import { invokeMock } from "./flowMocks";
import { treeCommand } from "./treeCommands";

export { emitEvent, invokeMock } from "./flowMocks";

/** One item's delivery state, with the defaults of an item nobody blocks. */
export function itemState(
  itemId: string,
  state: FlowItemState,
  extra: Partial<ItemDeliveryState> = {},
): ItemDeliveryState {
  return {
    itemId,
    state,
    draft: null,
    draftMissing: false,
    run: null,
    blocked: false,
    blockedBy: [],
    leafCounts: null,
    ...extra,
  };
}

export interface FlowBackend {
  /** The body the next load serves. */
  body: string;
  /** Every body `save_artifact_contents` was given. */
  writes: string[];
  /** FLO-FR-PLUZ: the layout `load_delivery_layout` serves. */
  layout: StoredLayout;
  /** FLO-FR-21: every layout `save_delivery_layout` was given, with its scope. */
  layoutWrites: Array<{ scope: string; layout: StoredLayout }>;
  validation: { valid: boolean; violations: Array<Record<string, unknown>> };
  /** The delivery state answer, or a function of the body asked about. */
  delivery: FlowDeliveryState | ((body: string) => FlowDeliveryState | Promise<FlowDeliveryState>);
  overlaps: FlowOverlapReport | ((body: string) => FlowOverlapReport | Promise<FlowOverlapReport>);
  drafts: DraftSummary[];
  /** Every call, in order. */
  calls: Array<{ cmd: string; args: Record<string, unknown> }>;
  /** Answers that replace the defaults, by command name. */
  on: Record<string, (args: Record<string, unknown>) => unknown>;
}

export function flowBackend(
  doc: FlowDocument | string,
  layout: StoredLayout = { items: {} },
): FlowBackend {
  const b: FlowBackend = {
    body: typeof doc === "string" ? doc : serializeFlowDocument(doc),
    writes: [],
    layout,
    layoutWrites: [],
    validation: { valid: true, violations: [] },
    delivery: { items: [] },
    overlaps: { overlaps: [] },
    drafts: [],
    calls: [],
    on: {},
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, raw?: Record<string, unknown>) => {
    const args = raw ?? {};
    b.calls.push({ cmd, args });
    const own = b.on[cmd];
    if (own) return own(args);
    const read = treeCommand(cmd, args, TREE);
    if (read) return read;
    switch (cmd) {
      case "append_log_records":
        return undefined;
      case "validate_flow_document":
        return b.validation;
      case "load_artifact_contents_by_id":
        return { body: b.body, checksum: "ck1" };
      case "save_artifact_contents":
        b.writes.push(String(args.body));
        b.body = String(args.body);
        return { checksum: `ck-w${b.writes.length}` };
      case "open_artifact_by_id":
        return { key: args.id, kind: String(args.id).endsWith(".flow") ? "flow" : "markdown" };
      case "load_flow_delivery_state":
        return typeof b.delivery === "function" ? b.delivery(String(args.body)) : b.delivery;
      case "analyze_flow_overlap":
        return typeof b.overlaps === "function" ? b.overlaps(String(args.body)) : b.overlaps;
      case "list_drafts":
        return { folders: [], drafts: b.drafts };
      case "list_discussions":
        return [];
      case "load_delivery_layout":
        return b.layout;
      case "save_delivery_layout":
        b.layoutWrites.push({ scope: String(args.scope), layout: args.layout as StoredLayout });
        b.layout = args.layout as StoredLayout;
        return undefined;
      default:
        throw new Error(`unexpected invoke ${cmd}`);
    }
  });
  return b;
}

/** Advance the fake clock, and let the promises its timers start settle. */
export async function advance(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

/**
 * Run what the fake clock still holds, then go back to real timers. Call in
 * `afterEach`, before `cleanup`, so no timer that a test started under the
 * fake clock is left behind, never to run.
 */
export async function endFakeTimers() {
  for (let round = 0; round < 5 && vi.isFakeTimers() && vi.getTimerCount() > 0; round += 1) {
    await act(async () => {
      await vi.runOnlyPendingTimersAsync();
    });
  }
  vi.useRealTimers();
}

/** The calls of one command, in order. */
export const callsOf = (b: FlowBackend, cmd: string) =>
  b.calls.filter((c) => c.cmd === cmd).map((c) => c.args);

const stores: FlowSessionStore[] = [];

/** Drop every scheduled write of the stores this file made. Call in `afterEach`. */
export function clearFlowStores() {
  for (const s of stores.splice(0)) s.clear();
}

/**
 * Load the backend's body as the Flow `ID` and render a canvas over it, with
 * the project tree and the layout settled.
 */
export async function renderFlow(
  b: FlowBackend,
  props: Partial<ComponentProps<typeof FlowCanvas>> = {},
) {
  void b;
  const flows = new FlowSessionStore();
  stores.push(flows);
  await act(async () => {
    flows.openTab(ID);
    await flows.get(ID)?.pendingLoad;
  });
  const onOpenArtifact = vi.fn();
  const view = render(
    <FlowCanvas binding={fileBinding(flows, ID)} onOpenArtifact={onOpenArtifact} {...props} />,
  );
  await waitFor(() =>
    expect(invokeMock.mock.calls.some((c) => c[0] === "list_project_artifacts")).toBe(true),
  );
  await act(async () => {});
  return { flows, view, onOpenArtifact };
}

/** The layout entry the canvas last stored for an item, if it stored one. */
export const storedEntry = (b: FlowBackend, id: string) =>
  b.layoutWrites[b.layoutWrites.length - 1]?.layout.items[id];

/** The plan the store holds now. */
export const planOf = (flows: FlowSessionStore) => flows.get(ID)!.doc!;

/** FLO-FR-HBBF: the layout scope of the Flow `ID`. */
export const SCOPE = flowLayoutScope(ID);

/** The layout the canvas draws now: the stored layout of `ID` with the automatic one. */
export const placedOf = (flows: FlowSessionStore): FlowLayout =>
  resolveLayout(planOf(flows), flows.layouts.get(SCOPE) ?? { items: {} });
