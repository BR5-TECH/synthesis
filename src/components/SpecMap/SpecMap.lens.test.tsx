/**
 * The lens switch of the Map tab (`../../../specifications/ui/SMP-specification-map.md`
 * SMP-FR-TKUS, SMP-FR-RNLH, SMP-FR-PMCX, SMP-FR-QNUH) and the props the frame
 * gives the Delivery lens.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { DeliveryLensProps } from "../DeliveryLens";
import type { DeliverySource } from "../../types/delivery";

const captured = vi.hoisted(() => ({ props: null as DeliveryLensProps | null }));
vi.mock("../DeliveryLens", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../DeliveryLens")>();
  return {
    ...actual,
    DeliveryLens: (props: DeliveryLensProps) => {
      captured.props = props;
      return actual.DeliveryLens(props);
    },
  };
});
const logInfo = vi.hoisted(() => vi.fn());
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo,
  logWarn: vi.fn(),
  logError: vi.fn(),
}));
// The Delivery lens of a file source hosts the Flow canvas, which reads the
// file and follows the backend's events.
vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { SpecMap } from ".";
import { makeStore, nodeButton, setWindowWidth } from "../../test/specMapRender";
import { flowBackend } from "../../test/flowCanvasHarness";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";

const FILE: DeliverySource = { kind: "file", flowId: "flow-1", path: "plan.flow" };

/** How many times the Delivery lens mounted. */
const lensMounts = () => logInfo.mock.calls.filter(([, message]) => message === "delivery lens mounted").length;

const lensRadio = (name: string) =>
  within(screen.getByRole("radiogroup", { name: "Lens" })).getByRole("radio", { name });

function renderFrame(source: DeliverySource | Promise<DeliverySource> = FILE) {
  setWindowWidth(1440);
  const made = makeStore(undefined, { getDeliverySource: vi.fn(() => Promise.resolve(source)) });
  const props = { onOpenArtifact: vi.fn(), onOverlayOpening: vi.fn() };
  const view = render(<SpecMap store={made.store} {...props} />);
  return { ...made, ...props, ...view };
}

beforeEach(() => {
  captured.props = null;
  logInfo.mockClear();
  resetListeners();
  flowBackend(plan([item("t1", "task")]));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("the default lens (SMP-FR-TKUS)", () => {
  it("SMP-FR-TKUS, SDL-FR-VYIF: a delivery source opens the Delivery lens, which loads the index once its plan shows", async () => {
    const { ops } = renderFrame(FILE);
    const lens = await screen.findByTestId("delivery-lens");
    expect(lens).toHaveTextContent("Delivery");
    expect(lensRadio("Delivery")).toHaveAttribute("aria-checked", "true");
    expect(lensRadio("Specifications")).toHaveAttribute("aria-checked", "false");
    // SMP-FR-TKUS asks once; MDL-FR-YTAP asks again when the lens first renders.
    await waitFor(() => expect(ops.getDeliverySource).toHaveBeenCalledTimes(2));
    await screen.findByTestId("flow-item-t1");
    await waitFor(() => expect(ops.load).toHaveBeenCalledTimes(1));
    expect(screen.queryByTestId("spec-map-canvas")).toBeNull();
  });

  it("SMP-FR-TKUS: while the source is asked for, the switch checks no lens and nothing loads; a none source then opens Specifications", async () => {
    let answer!: (s: DeliverySource) => void;
    const { ops } = renderFrame(new Promise<DeliverySource>((r) => (answer = r)));
    expect(screen.getByRole("status")).toHaveTextContent("Loading the map…");
    expect(lensRadio("Delivery")).toHaveAttribute("aria-checked", "false");
    expect(lensRadio("Specifications")).toHaveAttribute("aria-checked", "false");
    expect(ops.load).not.toHaveBeenCalled();
    await act(async () => answer({ kind: "none", reason: "github_project_unset" }));
    await screen.findByTestId("spec-map-canvas");
    expect(lensRadio("Specifications")).toHaveAttribute("aria-checked", "true");
    expect(ops.load).toHaveBeenCalledTimes(1);
  });

  it("SMP-FR-TKUS: a source that cannot be read shows the Specifications lens, which loads the index", async () => {
    const { ops } = renderFrame(Promise.reject(new TypeError("no backend")));
    await screen.findByTestId("spec-map-canvas");
    expect(lensRadio("Specifications")).toHaveAttribute("aria-checked", "true");
    expect(ops.load).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("delivery-lens")).toBeNull();
  });

  it("SMP-FR-TKUS: a click on a lens before the source answers chooses that lens", async () => {
    let answer!: (s: DeliverySource) => void;
    const { store } = renderFrame(new Promise<DeliverySource>((r) => (answer = r)));
    fireEvent.click(lensRadio("Specifications"));
    await screen.findByTestId("spec-map-canvas");
    await act(async () => answer(FILE));
    expect(store.snapshot().lens).toBe("specifications");
    expect(screen.queryByTestId("delivery-lens")).toBeNull();
  });
});

describe("switching the lens (SMP-FR-RNLH, SMP-FR-PMCX, SMP-FR-QNUH)", () => {
  it("SMP-FR-PMCX, SMP-FR-RNLH: the Specifications lens loads the index when it first renders, and a switch back and forth keeps its selection and loads nothing more", async () => {
    const { store, ops } = renderFrame(FILE);
    await screen.findByTestId("delivery-lens");
    fireEvent.click(lensRadio("Specifications"));
    await screen.findByTestId("spec-map-canvas");
    expect(ops.load).toHaveBeenCalledTimes(1);
    fireEvent.click(nodeButton("d1 label, domain"));

    fireEvent.click(lensRadio("Delivery"));
    expect(screen.getByTestId("delivery-lens")).toBeInTheDocument();
    expect(screen.queryByTestId("spec-map-canvas")).toBeNull();
    // SMP-FR-RNLH: the Delivery lens keeps its selection through the switch.
    fireEvent.pointerDown(await screen.findByTestId("flow-item-t1"), { button: 0, clientX: 5, clientY: 5 });
    fireEvent.pointerUp(screen.getByTestId("flow-item-t1"));
    await waitFor(() => expect(screen.getByTestId("flow-item-t1")).toHaveAttribute("data-selected", "true"));
    fireEvent.click(lensRadio("Specifications"));
    await screen.findByTestId("spec-map-canvas");
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "d1" });
    expect(nodeButton("d1 label, domain")).toHaveAttribute("data-selected", "true");
    expect(ops.load).toHaveBeenCalledTimes(1);
    fireEvent.click(lensRadio("Delivery"));
    expect(await screen.findByTestId("flow-item-t1")).toHaveAttribute("data-selected", "true");
  });

  it("SMP-FR-QNUH: a remount of the tab keeps the active lens and asks for the default lens no more", async () => {
    const { store, ops, onOpenArtifact, unmount } = renderFrame(FILE);
    await screen.findByTestId("delivery-lens");
    fireEvent.click(lensRadio("Specifications"));
    await screen.findByTestId("spec-map-canvas");
    const asked = vi.mocked(ops.getDeliverySource).mock.calls.length;
    unmount();
    render(<SpecMap store={store} onOpenArtifact={onOpenArtifact} />);
    await screen.findByTestId("spec-map-canvas");
    expect(lensRadio("Specifications")).toHaveAttribute("aria-checked", "true");
    expect(ops.getDeliverySource).toHaveBeenCalledTimes(asked);
  });
});

describe("the props of the Delivery lens", () => {
  it("SMP-FR-TKUS: the Delivery lens gets the lens switch for its toolbar, the session, and the routes of the frame", async () => {
    const { store, onOpenArtifact, onOverlayOpening } = renderFrame(FILE);
    const lens = await screen.findByTestId("delivery-lens");
    expect(within(lens).getByRole("toolbar", { name: "Map controls" }).firstElementChild).toHaveAttribute(
      "aria-label",
      "Lens",
    );
    expect(captured.props).toMatchObject({ isActive: true, store, onOpenArtifact, onOverlayOpening });
  });

  it("SMP-FR-TKUS: while Specifications is active the Delivery lens stays mounted, inactive, and renders nothing", async () => {
    renderFrame(FILE);
    await screen.findByTestId("delivery-lens");
    expect(lensMounts()).toBe(1);
    fireEvent.click(lensRadio("Specifications"));
    await screen.findByTestId("spec-map-canvas");
    expect(captured.props?.isActive).toBe(false);
    expect(screen.queryByTestId("delivery-lens")).toBeNull();
    fireEvent.click(lensRadio("Delivery"));
    await screen.findByTestId("delivery-lens");
    // The lens mounted once: the switch there and back kept it mounted.
    expect(lensMounts()).toBe(1);
  });

  it("SMP-FR-TKUS, SMI-FR-PRSL: a request of the Delivery lens for a spec file shows the Specifications lens with that spec node selected", async () => {
    const { store } = renderFrame(FILE);
    await screen.findByTestId("delivery-lens");
    act(() => captured.props!.onRequestSpecifications({ specPath: "specifications/ui/C1-c1.md" }));
    await screen.findByTestId("spec-map-canvas");
    expect(lensRadio("Specifications")).toHaveAttribute("aria-checked", "true");
    expect(store.snapshot().selection).toEqual({ kind: "spec", code: "C1" });
  });

  it("SMP-FR-TKUS: a request with no spec file shows the Specifications lens and keeps the selection", async () => {
    const { store } = renderFrame(FILE);
    await screen.findByTestId("delivery-lens");
    act(() => store.select({ kind: "index", id: "d2" }));
    act(() => captured.props!.onRequestSpecifications());
    await screen.findByTestId("spec-map-canvas");
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "d2" });
  });
});
