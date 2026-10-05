/**
 * Rendering helpers for the Map tab's component tests
 * (`SMP-specification-map.md`). The store is real; only its three stub
 * operations are replaced, so a test can make a load fail or watch a save.
 */
import { act, render, screen, waitFor } from "@testing-library/react";
import { expect, vi } from "vitest";
import { SpecMap } from "../components/SpecMap";
import type { SpecMapOps } from "../api/specMap";
import { SpecMapSessionStore, type MapView } from "../state/specMap/session";
import type { SpecificationIndex } from "../state/specMap/types";
import { smallIndex } from "./specMapFixtures";

export function makeStore(index?: SpecificationIndex, overrides: Partial<SpecMapOps> = {}) {
  const ops = {
    load: vi.fn<SpecMapOps["load"]>(async () => index ?? smallIndex()),
    saveOrganization: vi.fn<SpecMapOps["saveOrganization"]>(async () => {}),
    attachDraft: vi.fn<SpecMapOps["attachDraft"]>(async () => {}),
    ...overrides,
  };
  return { store: new SpecMapSessionStore(ops), ops };
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
    onNewDraft: vi.fn(),
    onOpenDraft: vi.fn(),
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
