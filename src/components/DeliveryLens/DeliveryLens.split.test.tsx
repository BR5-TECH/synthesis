/**
 * The split controls in the toolbar of the Delivery lens
 * (`../../../specifications/ui/SMP-specification-map.md` SMP-FR-VDND): the
 * width of the Map tab decides whether the presets and the accelerator hint
 * lead the Discussion toggle.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { resetToasts } from "../../state/toasts";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  clearLensStores,
  deliveryBackend,
  githubAnswer,
  githubSource,
  renderLens,
  settle,
} from "../../test/deliveryLensHarness";
import { mockCanvasRect } from "../../test/specMapRender";

const DOC = plan([item("m1", "milestone"), item("t1", "task", { parentId: "m1" })]);

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  const b = deliveryBackend(DOC, githubAnswer(DOC));
  b.plan = () => githubAnswer(DOC);
});

afterEach(() => {
  cleanup();
  clearLensStores();
  vi.restoreAllMocks();
});

async function openAt(width: number) {
  mockCanvasRect({ width, height: 800 });
  const r = await renderLens({ source: async () => githubSource() });
  await screen.findByTestId("flow-item-t1");
  await settle();
  act(() => r.store.setDiscussionVisible(true));
  return r;
}

describe("the split controls of the Delivery toolbar (SMP-FR-VDND)", () => {
  it("SMP-FR-VDND: a tab of 1180px or more shows the presets and the accelerator hint", async () => {
    await openAt(1180);
    const split = await screen.findByTestId("map-discussion-split");
    expect(split.querySelector("kbd")).not.toBeNull();
  });

  it("SMP-FR-VDND: below 1180px the hint is hidden, and from 1000px the presets stay", async () => {
    await openAt(1179);
    expect((await screen.findByTestId("map-discussion-split")).querySelector("kbd")).toBeNull();
    cleanup();
    await openAt(1000);
    const split = await screen.findByTestId("map-discussion-split");
    expect(split.querySelector("kbd")).toBeNull();
    expect(split.querySelector("[role=radiogroup]")).not.toBeNull();
  });

  it("SMP-FR-VDND: below 1000px the presets are hidden too, and the toggle stays", async () => {
    await openAt(999);
    expect(await screen.findByTestId("map-discussion")).toBeInTheDocument();
    expect(screen.queryByTestId("map-discussion-split")).toBeNull();
    expect(screen.getByRole("button", { name: "Discussion" })).toBeInTheDocument();
  });
});
