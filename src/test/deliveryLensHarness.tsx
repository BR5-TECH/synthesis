/**
 * The backend and the render of the Delivery lens tests (MDL, SDL).
 *
 * A test file mocks `@tauri-apps/api/core` and `@tauri-apps/api/event` with
 * `./flowMocks`, as the Flow canvas tests do. `deliveryBackend` routes every
 * command by name on top of `flowBackend`: it answers the background
 * `append_log_records`, and it rejects a command it does not know.
 */
import { act, render, screen, waitFor } from "@testing-library/react";
import { expect, vi } from "vitest";

import { SpecMap } from "../components/SpecMap";
import type { DeliveryHost } from "../components/DeliveryLens";
import type { GithubPollingController } from "../hooks/useGithubPolling";
import type { SpecMapOps } from "../api/specMap";
import {
  CARD_H,
  CARD_W,
  serializeFlowDocument,
  type FlowDocument,
  type FlowItemKind,
  type StoredLayout,
} from "../state/flowDocument";
import { FlowSessionStore } from "../state/flowSessions";
import { SpecMapSessionStore } from "../state/specMap/session";
import type { SpecificationIndex } from "../state/specMap/types";
import type {
  DeliverySettings,
  DeliverySource,
  GithubDeliveryPlan,
  TrackerFacts,
} from "../types/delivery";
import type { GithubPollingView, ItemDeliveryState } from "../types";
import { flowBackend, type FlowBackend } from "./flowCanvasHarness";
import { setWindowWidth } from "./specMapRender";
import { smallIndex } from "./specMapFixtures";

export const PROJECT_NODE = "PVT_1";

/** DSR-FR-AECK: a GitHub source, with a configuration in `state`. */
export function githubSource(
  state: "valid" | "invalid" | "unchecked" = "valid",
  error: string | null = null,
): DeliverySource {
  return {
    kind: "github",
    project: {
      nodeId: PROJECT_NODE,
      title: "Payments board",
      ownerLogin: "acme",
      number: 7,
      url: "https://github.com/orgs/acme/projects/7",
    },
    repository: { host: "github.com", owner: "acme", name: "payments" },
    configuration: {
      state,
      errorCode: state === "invalid" ? "status_field_missing" : null,
      error,
      missingStatus: null,
      projectTitle: "Payments board",
    },
  };
}

export const fileSource = (flowId: string): DeliverySource => ({ kind: "file", flowId, path: flowId });

/** GDP-FR-QGFO: the tracker facts of an issue item. */
export function issueFacts(itemId: string, number: number, extra: Partial<TrackerFacts> = {}): TrackerFacts {
  return {
    itemId,
    ref: { kind: "issue", number, url: `https://github.com/acme/payments/issues/${number}` },
    state: "open",
    closeReason: null,
    updatedAt: `2026-11-01T00:00:0${number % 10}Z`,
    status: "Todo",
    claimEligible: false,
    externalBlockers: [],
    ...extra,
  };
}

export function milestoneFacts(itemId: string, number: number): TrackerFacts {
  return {
    itemId,
    ref: { kind: "milestone", number, url: `https://github.com/acme/payments/milestone/${number}` },
    state: "open",
    closeReason: null,
    updatedAt: "2026-11-01T00:00:00Z",
    status: null,
    claimEligible: false,
    externalBlockers: [],
  };
}

/** One item's delivery state, with the defaults of an item nobody blocks. */
export function leafState(itemId: string, state: ItemDeliveryState["state"], extra: Partial<ItemDeliveryState> = {}): ItemDeliveryState {
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

/** GDP contract surface: an answer over `doc`. */
export function githubAnswer(doc: FlowDocument, extra: Partial<GithubDeliveryPlan> = {}): GithubDeliveryPlan {
  return {
    document: JSON.parse(serializeFlowDocument(doc)),
    tracker: [],
    states: [],
    overlaps: [],
    notices: { unmappedTypes: [], misplaced: [], omittedRelations: [], skipped: 0 },
    readAt: "2026-11-14T10:00:00Z",
    stale: false,
    error: null,
    ...extra,
  };
}

export const DEFAULT_SETTINGS: DeliverySettings = {
  source: { kind: "automatic" },
  createTypes: { epic: "Epic", feature: "Feature", task: "Task", defect: "Bug" },
  readEntries: [{ type: "Spike", kind: "task" }],
  untyped: null,
};

export interface DeliveryBackend extends FlowBackend {
  /** The answers of `load_github_delivery_plan`, by `refresh`. */
  plan: (refresh: boolean) => GithubDeliveryPlan | Promise<GithubDeliveryPlan>;
  /** The answer of each `apply_github_delivery_edit`. */
  apply: (edit: Record<string, unknown>) => GithubDeliveryPlan | Promise<GithubDeliveryPlan>;
  settings: DeliverySettings;
}

/** The Flow backend, with the delivery commands of DSR, GDP, and DMS on top. */
export function deliveryBackend(doc: FlowDocument | string = "", answer?: GithubDeliveryPlan): DeliveryBackend {
  const b = flowBackend(doc) as DeliveryBackend;
  b.settings = structuredClone(DEFAULT_SETTINGS);
  b.plan = () => answer ?? Promise.reject({ code: "not_github_source", message: "Not a GitHub source." });
  b.apply = () => Promise.reject({ code: "not_github_source", message: "Not a GitHub source." });
  b.on.load_github_delivery_plan = (args) => b.plan(Boolean(args.refresh));
  b.on.apply_github_delivery_edit = (args) => b.apply(args.edit as Record<string, unknown>);
  b.on.get_delivery_settings = () => structuredClone(b.settings);
  b.on.set_delivery_settings = (args) => {
    b.settings = structuredClone(args.settings as DeliverySettings);
    return structuredClone(b.settings);
  };
  b.on.open_tracker_link = () => undefined;
  b.on.find_flow_items_for_draft = () => [];
  return b;
}

/** The polling view of a valid configuration, with nothing claimed. */
export function pollingView(extra: Partial<GithubPollingView> = {}): GithubPollingView {
  return {
    settings: { projectNodeId: PROJECT_NODE, intervalMinutes: 15, readyStatus: "Ready", inProgressStatus: "In Progress", blockedStatus: null },
    configuration: {
      state: "valid",
      errorCode: null,
      error: null,
      missingStatus: null,
      projectTitle: "Payments board",
    },
    repository: { host: "github.com", owner: "acme", name: "payments" },
    polling: false,
    tasks: [],
    stale: false,
    lastErrorCode: null,
    lastError: null,
    lastSuccessAt: null,
    pendingClaims: [],
    shadows: [],
    ...extra,
  };
}

/** A stand-in for the shell's claim flow (GIT-FR-NQTZ). */
export function claimsOf(view: GithubPollingView | null = pollingView()): GithubPollingController {
  return {
    view,
    readError: null,
    pollInFlight: false,
    refreshError: null,
    rowBusy: new Map(),
    rowErrors: new Map(),
    reload: vi.fn(async () => {}),
    refresh: vi.fn(async () => {}),
    claim: vi.fn(async () => {}),
    retry: vi.fn(async () => {}),
    graduateShadow: vi.fn(),
    openTaskIssue: vi.fn(),
    openShadowIssue: vi.fn(),
  };
}

export interface LensRender {
  store: SpecMapSessionStore;
  ops: { load: ReturnType<typeof vi.fn>; getDeliverySource: ReturnType<typeof vi.fn> };
  host: DeliveryHost & { flows: FlowSessionStore };
  onOpenArtifact: ReturnType<typeof vi.fn>;
  onOverlayOpening: ReturnType<typeof vi.fn>;
  rerender: () => void;
  unmount: () => void;
}

const stores: FlowSessionStore[] = [];

/** Drop every scheduled write of the stores this file made. Call in `afterEach`. */
export function clearLensStores() {
  for (const s of stores.splice(0)) s.clear();
}

/**
 * Render the Map tab with `source` as the delivery source. The Map opens on
 * the Delivery lens where the source is not `none`.
 */
export async function renderLens(options: {
  source: DeliverySource | (() => Promise<DeliverySource>);
  index?: SpecificationIndex | (() => Promise<SpecificationIndex>);
  host?: Partial<DeliveryHost>;
  store?: SpecMapSessionStore;
  windowWidth?: number;
}): Promise<LensRender> {
  setWindowWidth(options.windowWidth ?? 1440);
  const ops = {
    load: vi.fn<SpecMapOps["load"]>(async () =>
      typeof options.index === "function" ? options.index() : (options.index ?? smallIndex()),
    ),
    getDeliverySource: vi.fn<SpecMapOps["getDeliverySource"]>(async () =>
      typeof options.source === "function" ? options.source() : options.source,
    ),
  };
  const store = options.store ?? new SpecMapSessionStore(ops);
  // SMP-FR-WNLR shows the Discussion column on the first open. A test of
  // another requirement hides it, so its queries meet the lens alone.
  if (!options.store) store.setDiscussionVisible(false);
  const flows = new FlowSessionStore();
  stores.push(flows);
  const host = {
    flows,
    onOpenProjectSettings: vi.fn(),
    onCreateDraft: vi.fn(async () => ({ ok: false as const, error: null })),
    onOpenDraft: vi.fn(),
    onOpenDraftById: vi.fn(),
    ...options.host,
  } as LensRender["host"];
  const onOpenArtifact = vi.fn();
  const onOverlayOpening = vi.fn();
  const ui = () => (
    <SpecMap
      store={store}
      onOpenArtifact={onOpenArtifact}
      onOverlayOpening={onOverlayOpening}
      delivery={host}
    />
  );
  const view = render(ui());
  await act(async () => {});
  return {
    store,
    ops,
    host,
    onOpenArtifact,
    onOverlayOpening,
    rerender: () => view.rerender(ui()),
    unmount: view.unmount,
  };
}

/** The plan with a new top-level item, as GitHub answers a `create_item` edit. */
export function withItem(doc: FlowDocument, id: string, kind: FlowItemKind, name: string): FlowDocument {
  return { ...doc, items: [...doc.items, { id, kind, name }] };
}

/** DMS-FR-DMWD: a stored layout in which the containers `ids` are collapsed. */
export function collapsedLayout(ids: string[]): StoredLayout {
  return { items: Object.fromEntries(ids.map((id) => [id, { collapsed: true }])) };
}

/** Wait until the canvas of the lens shows the item `id`. */
export async function findItem(id: string) {
  return screen.findByTestId(`flow-item-${id}`);
}

/** Wait until the plan's loads have settled. */
export async function settle() {
  await act(async () => {});
  await waitFor(() => expect(screen.queryByText("Loading the delivery plan…")).toBeNull());
}

/**
 * MDL-FR-JIKK / FLO-FR-21: the centre of the item `id` on the screen is the
 * centre of the canvas. jsdom measures no surface, so the canvas reads as
 * 800 × 600, and a card as CARD_W × CARD_H.
 */
export function expectCentred(id: string) {
  const viewport = document.querySelector(".flow-viewport") as HTMLElement;
  const m = /translate\((-?[\d.]+)px, (-?[\d.]+)px\) scale\(([\d.]+)\)/.exec(viewport.style.transform);
  expect(m).not.toBeNull();
  const [x, y, scale] = [Number(m![1]), Number(m![2]), Number(m![3])];
  const card = screen.getByTestId(`flow-item-${id}`);
  const left = parseFloat(card.style.left);
  const top = parseFloat(card.style.top);
  // DBR-FR-LNZI: a card of the board has the size of its slot.
  const width = parseFloat(card.style.width) || CARD_W;
  const height = parseFloat(card.style.height) || CARD_H;
  expect(x + (left + width / 2) * scale).toBeCloseTo(400, 0);
  expect(y + (top + height / 2) * scale).toBeCloseTo(300, 0);
}
