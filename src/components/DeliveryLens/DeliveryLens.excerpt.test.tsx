/**
 * An item of the Delivery lens reads as its name: no card and no container
 * shows the excerpt of its description, for a GitHub plan and for a Flow file
 * (`../../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-RILW). A Flow
 * tab still shows the excerpt (`Flow.canvas.test.tsx`, FLO-FR-XCRP).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { resetToasts } from "../../state/toasts";
import { resetListeners } from "../../test/flowMocks";
import { itemEl, selectItem } from "../../test/flowFixtures";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  claimsOf,
  clearLensStores,
  deliveryBackend,
  fileSource,
  githubAnswer,
  githubSource,
  renderLens,
  settle,
} from "../../test/deliveryLensHarness";

const FLOW = "plans/release.flow";
const TASK_BODY = "The decline message names the reason and the next step.";
const FEATURE_BODY = "## Description\nCapture the card once and keep a token.";
const DOC = plan([
  item("m1", "milestone", { description: "The beta release." }),
  item("f1", "feature", { parentId: "m1", description: FEATURE_BODY }),
  item("t1", "task", { parentId: "f1", description: TASK_BODY }),
]);

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
});

function expectNoExcerpt() {
  for (const id of ["m1", "f1", "t1"]) {
    expect(screen.queryByTestId(`flow-item-excerpt-${id}`)).toBeNull();
  }
  expect(itemEl("t1")).not.toHaveTextContent(TASK_BODY);
  expect(itemEl("f1").querySelector(".flow-ctr__about")).toBeNull();
}

describe("an item reads as its name (MDL-FR-RILW)", () => {
  it("MDL-FR-RILW: in a GitHub plan no card and no container shows an excerpt, and the panel still shows the full body", async () => {
    await renderLens({ source: async () => githubSource() });
    await screen.findByTestId("flow-item-t1");
    await settle();
    expect(within(itemEl("t1")).getByText("T1")).toBeInTheDocument();
    expectNoExcerpt();
    selectItem("t1");
    expect(await screen.findByText(TASK_BODY)).toBeInTheDocument();
  });

  it("MDL-FR-RILW: with a Flow file as the source no card and no container shows an excerpt", async () => {
    await renderLens({ source: async () => fileSource(FLOW), host: { claims: claimsOf() } });
    await screen.findByTestId("flow-item-t1");
    expectNoExcerpt();
  });
});
