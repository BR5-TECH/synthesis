/**
 * Rendering helpers for the Map tab's component tests
 * (`SMP-specification-map.md`). The store is real; only its operations are
 * replaced, so a test can make a load fail or choose the delivery source. By
 * default the source is `none`, so the map opens on the Specifications lens.
 */
import { act, render, screen, waitFor } from "@testing-library/react";
import { expect, vi } from "vitest";
import { SpecMap } from "../components/SpecMap";
import type { SpecMapOps } from "../api/specMap";
import { SpecMapSessionStore, type MapView } from "../state/specMap/session";
import type { SpecificationIndex } from "../state/specMap/types";
import { smallIndex } from "./specMapFixtures";
import type { DeliverySource } from "../types/delivery";

/** DSR-FR-ZKGG: the answer of a project with no delivery source. */
export const NO_DELIVERY_SOURCE: DeliverySource = { kind: "none", reason: "no_source" };

export function makeStore(index?: SpecificationIndex, overrides: Partial<SpecMapOps> = {}) {
  const ops = {
    load: vi.fn<SpecMapOps["load"]>(async () => index ?? smallIndex()),
    getDeliverySource: vi.fn<SpecMapOps["getDeliverySource"]>(async () => NO_DELIVERY_SOURCE),
    ...overrides,
  };
  const store = new SpecMapSessionStore(ops);
  // SMP-FR-WNLR shows the Discussion column on the first open. A test of
  // another requirement hides it, so its queries meet the canvas alone.
  store.setDiscussionVisible(false);
  return { store, ops };
}

export function setWindowWidth(width: number): void {
  Object.defineProperty(window, "innerWidth", { configurable: true, writable: true, value: width });
}

/** Every element measures as the canvas would, so the layout sees a real width. */
export function mockCanvasRect(size: { width: number; height: number }) {
  return vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    left: 0,
    top: 0,
    x: 0,
    y: 0,
    width: size.width,
    height: size.height,
    right: size.width,
    bottom: size.height,
    toJSON: () => ({}),
  } as DOMRect);
}

export async function renderMap(
  options: {
    index?: SpecificationIndex;
    windowWidth?: number;
    ops?: Partial<SpecMapOps>;
  } = {},
) {
  setWindowWidth(options.windowWidth ?? 1440);
  const { store, ops } = makeStore(options.index, options.ops);
  const props = {
    onOpenArtifact: vi.fn(),
    onOverlayOpening: vi.fn(),
  };
  const view = render(<SpecMap store={store} {...props} />);
  await waitFor(() => expect(["ready", "error"]).toContain(store.snapshot().status));
  return { store, ops, ...props, ...view };
}

/** A node on the canvas, by the accessible name the node layer gives it. */
export const nodeButton = (name: string) => screen.getByRole("button", { name });

export const canvas = () => screen.getByTestId("spec-map-canvas");

export function setView(store: SpecMapSessionStore, patch: Partial<MapView> & { level: number }) {
  act(() => store.setView({ scale: 1, tx: 0, ty: 0, touched: true, ...patch }));
}
