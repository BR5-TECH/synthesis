/**
 * What the issue view of the Delivery lens logs
 * (`../../../specifications/ui/MIV-map-issue-view.md` MIV-FR-IHRB).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));
vi.mock("../../logging", async (original) => ({
  ...(await original<typeof import("../../logging")>()),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
}));

import { logInfo, logWarn, resetLogBufferForTest } from "../../logging";
import { resetToasts } from "../../state/toasts";
import { canvas, clickItem } from "../../test/flowFixtures";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  clearLensStores,
  deliveryBackend,
  githubAnswer,
  githubSource,
  issueFacts,
  issueRead,
  renderLens,
  settle,
} from "../../test/deliveryLensHarness";

const DOC = plan([
  item("t1", "task", { name: "Secret card name", description: "Secret body" }),
  item("t2", "task", { name: "Other name" }),
]);
const TRACKER = [issueFacts("t1", 51), issueFacts("t2", 52)];

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  vi.mocked(logInfo).mockClear();
  vi.mocked(logWarn).mockClear();
  const b = deliveryBackend("", githubAnswer(DOC, { tracker: TRACKER }));
  b.plan = () => githubAnswer(DOC, { tracker: TRACKER });
  b.issue = (n) => issueRead(n, { title: "Secret card name", body: "Secret body", labels: ["secret-label"] });
});

afterEach(() => {
  cleanup();
  clearLensStores();
});

const records = (message: string) =>
  vi.mocked(logInfo).mock.calls.filter((c) => c[1] === message).map((c) => c[2] as Record<string, unknown>);

describe("the records of the issue view (MIV-FR-IHRB)", () => {
  it("MIV-FR-IHRB: each open and each close logs one record with its cause, and no record holds a name, a body, or a label", async () => {
    await renderLens({ source: async () => githubSource() });
    await screen.findByTestId("flow-item-t1");
    await settle();
    clickItem("t1");
    const view = await screen.findByTestId("map-issue-view");
    await waitFor(() => expect(records("issue detail read")).toHaveLength(1));
    fireEvent.click(within(view).getByRole("button", { name: "Close" }));
    clickItem("t2");
    await screen.findByTestId("map-issue-view");
    fireEvent.pointerDown(canvas(), { pointerId: 1, buttons: 1, clientX: 900, clientY: 900 });
    fireEvent.pointerUp(canvas(), { pointerId: 1, clientX: 900, clientY: 900 });
    await waitFor(() => expect(records("issue view closed")).toHaveLength(2));
    expect(records("issue view opened")).toHaveLength(2);
    expect(records("issue view closed").map((r) => r.cause)).toEqual(["close", "selection"]);
    const all = JSON.stringify([...vi.mocked(logInfo).mock.calls, ...vi.mocked(logWarn).mock.calls]);
    expect(all).not.toMatch(/Secret card name|Secret body|secret-label|Other name/);
  });
});
