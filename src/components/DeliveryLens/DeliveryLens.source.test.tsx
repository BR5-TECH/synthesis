/**
 * The source of the Delivery lens and the lens over a Flow file
 * (`../../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-YTAP,
 * MDL-FR-XMQU, MDL-FR-WAOZ, MDL-FR-VLQJ, MDL-FR-NTVL, MDL-FR-JRGK,
 * MDL-FR-WMGU, MDL-FR-QAJN, MDL-FR-EEDO, MDL-FR-GDUP;
 * `ACT-action-control.md` ACT-FR-UNJB).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { renameItem } from "../../state/flowDocument";
import { cancelAllScheduledWrites } from "../../state/writeSchedule";
import { resetToasts, visibleToasts } from "../../state/toasts";
import { callsOf } from "../../test/flowCanvasHarness";
import { itemEl } from "../../test/flowFixtures";
import { emitEvent, resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  claimsOf,
  clearLensStores,
  deliveryBackend,
  expectCentred,
  fileSource,
  githubAnswer,
  githubSource,
  leafState,
  pollingView,
  renderLens,
  type DeliveryBackend,
  type LensRender,
} from "../../test/deliveryLensHarness";
import type { DeliverySource } from "../../types/delivery";

const FLOW = "plans/release.flow";
const DOC = plan([
  item("m1", "milestone"),
  item("t1", "task", { parentId: "m1" }),
  item("t2", "task", { parentId: "m1" }),
  item("x1", "defect"),
]);

/** The last entry of a list. */
const last = <T,>(list: readonly T[]): T | undefined => list[list.length - 1];

let b: DeliveryBackend;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  b = deliveryBackend(DOC);
  b.delivery = {
    items: [
      leafState("t1", "in_progress"),
      leafState("t2", "done"),
      leafState("x1", "not_started", { blocked: true, blockedBy: ["t1"] }),
    ],
  };
});

afterEach(() => {
  cleanup();
  clearLensStores();
});

async function openFile(host = {}) {
  const r = await renderLens({ source: fileSource(FLOW), host });
  await screen.findByTestId("flow-item-t1");
  return r;
}

describe("the source (MDL-FR-YTAP, MDL-FR-XMQU, MDL-FR-WAOZ)", () => {
  it("MDL-FR-YTAP: the lens reads the source when it first renders, and again on delivery source changed, github polling changed, and project tree changed", async () => {
    const r = await openFile();
    await waitFor(() => expect(r.ops.getDeliverySource).toHaveBeenCalledTimes(2));
    act(() => emitEvent("delivery-source-changed", { kind: "none", reason: "no_source" }));
    await waitFor(() => expect(r.ops.getDeliverySource).toHaveBeenCalledTimes(3));
    act(() => emitEvent("github-polling-changed", { newIssues: [] }));
    await waitFor(() => expect(r.ops.getDeliverySource).toHaveBeenCalledTimes(4));
    act(() => emitEvent("project-tree-changed", { paths: [] }));
    await waitFor(() => expect(r.ops.getDeliverySource).toHaveBeenCalledTimes(5));
  });

  it("MDL-FR-YTAP: a source that changes from the file to GitHub replaces the canvas with the GitHub plan", async () => {
    let source: DeliverySource = fileSource(FLOW);
    b.plan = () => githubAnswer(plan([item("g1", "task")]));
    const r = await renderLens({ source: async () => source });
    await screen.findByTestId("flow-item-t1");
    source = githubSource();
    act(() => emitEvent("delivery-source-changed", source));
    expect(await screen.findByTestId("flow-item-g1")).toBeInTheDocument();
    expect(screen.queryByTestId("flow-item-t1")).toBeNull();
    expect(r.store.snapshot().deliverySource).toEqual(source);
  });

  it("MDL-FR-XMQU: with no source the lens says the project has no delivery, states the reason, and Configure delivery opens the Delivery section", async () => {
    const r = await renderLens({ source: githubSource() });
    act(() => r.store.setDeliverySource({ kind: "none", reason: "github_project_unset" }));
    const empty = await screen.findByTestId("delivery-empty");
    expect(empty).toHaveTextContent("This project has no delivery.");
    expect(empty).toHaveTextContent("no GitHub Project is selected");
    await userEvent.click(within(empty).getByRole("button", { name: "Configure delivery" }));
    expect(r.host.onOpenProjectSettings).toHaveBeenCalledWith("delivery");
  });

  it("MDL-FR-XMQU: a missing delivery file is named as the reason", async () => {
    const source: DeliverySource = { kind: "none", reason: "delivery_file_missing", flowId: FLOW };
    const r = await renderLens({ source: githubSource() });
    act(() => r.store.setDeliverySource(source));
    expect(await screen.findByTestId("delivery-empty")).toHaveTextContent("the chosen Flow file is missing");
  });

  it("MDL-FR-WAOZ, GDP-FR-QSHX: an unchecked configuration (as after a restart) loads the plan and needs no visit to the settings", async () => {
    b.plan = async () => githubAnswer(DOC);
    const r = await renderLens({ source: githubSource("unchecked") });
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBeGreaterThan(0));
    expect(await screen.findByTestId("flow-band")).toBeInTheDocument();
    expect(screen.queryByTestId("delivery-config-error")).toBeNull();
    expect(r.host.onOpenProjectSettings).not.toHaveBeenCalled();
  });

  it("MDL-FR-WAOZ, GDP-FR-QSHX, GPP-FR-TYOV: when the load finds an unchecked configuration invalid, the lens shows the configuration error", async () => {
    let state: "unchecked" | "invalid" = "unchecked";
    b.plan = () => Promise.reject({ code: "status_field_missing", message: "No Status field." });
    await renderLens({ source: async () => githubSource(state, state === "invalid" ? "No Status field." : null) });
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBeGreaterThan(0));
    state = "invalid";
    act(() => emitEvent("github-polling-changed", { newIssues: [] }));
    expect(await screen.findByTestId("delivery-config-error")).toHaveTextContent("No Status field.");
  });

  it("MDL-FR-WAOZ: a GitHub source whose configuration is invalid shows the configuration error with a route to its settings, and no plan", async () => {
    const r = await renderLens({ source: githubSource("invalid", "Choose a Status field in GitHub Project settings.") });
    const error = await screen.findByTestId("delivery-config-error");
    expect(error).toHaveTextContent("Choose a Status field in GitHub Project settings.");
    expect(callsOf(b, "load_github_delivery_plan")).toHaveLength(0);
    expect(screen.queryByTestId("flow-band")).toBeNull();
    await userEvent.click(within(error).getByRole("button", { name: "Open GitHub Project settings" }));
    expect(r.host.onOpenProjectSettings).toHaveBeenCalledWith("github-polling");
  });
});

describe("a Flow file source (MDL-FR-VLQJ, MDL-FR-NTVL, MDL-FR-GDUP)", () => {
  it("MDL-FR-VLQJ, DBR-FR-ERHW: the lens hosts the canvas bound to the file as a board, which reads, edits, and asks for its own delivery state", async () => {
    const r = await openFile();
    // DBR-FR-ERHW: the lens lays the file out as the board.
    expect(document.querySelector(".flow")).toHaveAttribute("data-board", "true");
    expect(screen.getByTestId("flow-item-t1")).toHaveClass("dm-card");
    expect(callsOf(b, "load_artifact_contents_by_id")[0]).toMatchObject({ id: FLOW });
    await waitFor(() => expect(callsOf(b, "load_flow_delivery_state").length).toBeGreaterThan(0));
    act(() => {
      r.host.flows.applyEdit(FLOW, (d) => renameItem(d, "x1", "Renamed"));
    });
    expect(screen.getByTestId("flow-item-name-x1")).toHaveTextContent("Renamed");
    expect(r.host.flows.get(FLOW)?.dirty).toBe(true);
    expect(callsOf(b, "load_github_delivery_plan")).toHaveLength(0);
  });

  it("MDL-FR-NTVL, ACT-FR-UNJB: with a file source the canvas carries the action control, bound to the file", async () => {
    await openFile();
    expect(document.querySelector(".flow-actionhost")).not.toBeNull();
    await waitFor(() => expect(callsOf(b, "list_discussions").length).toBeGreaterThan(0));
  });

  it("ACT-FR-UNJB: a file source with the Specifications lens active has no .flow-actionhost", async () => {
    await openFile();
    expect(document.querySelector(".flow-actionhost")).not.toBeNull();
    fireEvent.click(within(screen.getByRole("radiogroup", { name: "Lens" })).getByRole("radio", { name: "Specifications" }));
    await screen.findByTestId("spec-map-canvas");
    expect(document.querySelector(".flow-actionhost")).toBeNull();
  });

  it("SMP-FR-RNLH: a switch to the Specifications lens and back keeps the Flow and reads the file no second time", async () => {
    const r = await openFile();
    act(() => {
      r.host.flows.applyEdit(FLOW, (d) => renameItem(d, "x1", "Kept"));
    });
    cancelAllScheduledWrites();
    const lens = () => within(screen.getByRole("radiogroup", { name: "Lens" }));
    fireEvent.click(lens().getByRole("radio", { name: "Specifications" }));
    await screen.findByTestId("spec-map-canvas");
    fireEvent.click(lens().getByRole("radio", { name: "Delivery" }));
    expect(await screen.findByTestId("flow-item-name-x1")).toHaveTextContent("Kept");
    expect(callsOf(b, "load_artifact_contents_by_id")).toHaveLength(1);
    expect(callsOf(b, "load_github_delivery_plan")).toHaveLength(0);
    expect(r.host.flows.get(FLOW)?.dirty).toBe(true);
  });

  it("MDL-FR-DHHW: the band shows the Flow's path as a link that reveals the file in the Project panel and opens no tab", async () => {
    const onRevealInProject = vi.fn();
    const r = await openFile({ onRevealInProject });
    const link = screen.getByRole("button", { name: FLOW });
    expect(link).toHaveAttribute("data-testid", "delivery-band-path");
    link.focus();
    await userEvent.keyboard("{Enter}");
    expect(onRevealInProject).toHaveBeenCalledWith(FLOW);
    expect(r.onOpenArtifact).not.toHaveBeenCalled();
  });

  it("MDL-FR-GDUP: the roll-up counts the leaves of the file by their state", async () => {
    await openFile();
    const legend = await screen.findByTestId("delivery-rollup-counts");
    expect([...legend.children].map((c) => c.textContent)).toEqual(["1 done", "1 in progress", "1 blocked"]);
  });

  it("MDL-FR-QAJN: Reset layout stores the empty layout of the file's scope", async () => {
    await openFile();
    await userEvent.click(screen.getByRole("button", { name: "Plan actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Reset layout" }));
    await waitFor(() => expect(last(b.layoutWrites)).toEqual({ scope: `flow:${FLOW}`, layout: { items: {} } }));
  });

  it("MDL-FR-EEDO: a request to plan work adds a top-level task to the file as an add of FLO-FR-07, and selects it", async () => {
    const r = await openFile();
    act(() => r.store.requestPlanWork("a1 label", ["specifications/ui/A1-a1.md"]));
    await waitFor(() => expect(r.host.flows.get(FLOW)?.doc?.items).toHaveLength(5));
    const added = r.host.flows.get(FLOW)!.doc!.items[4];
    expect(added).toMatchObject({ kind: "task", name: "a1 label", artifactIds: ["specifications/ui/A1-a1.md"] });
    expect(added.parentId).toBeUndefined();
    await waitFor(() => expect(itemEl(added.id)).toHaveAttribute("data-selected", "true"));
    await waitFor(() => expectCentred(added.id));
  });
});

/**
 * FLO-FR-27: an edit starts the autosave rest on a real timer, and a slow
 * runner can take longer than the rest for the steps of a test. Make the edit
 * and then stop the rest, so the Flow stays dirty and the only write is the one
 * the publish asks for. A fake clock cannot hold it here: the Testing Library
 * waits for a real `setTimeout` after each event.
 */
function editPending(r: LensRender) {
  act(() => {
    r.host.flows.applyEdit(FLOW, (d) => renameItem(d, "x1", "Pending"));
  });
  expect(r.host.flows.hasPendingWrite(FLOW)).toBe(true);
  cancelAllScheduledWrites();
  expect(r.host.flows.get(FLOW)?.dirty).toBe(true);
}

describe("Publish to GitHub (MDL-FR-JRGK, MDL-FR-WMGU)", () => {
  it("MDL-FR-JRGK: the band menu offers Publish to GitHub only while the GitHub configuration is valid", async () => {
    await openFile({ claims: claimsOf(pollingView({ configuration: { state: "invalid", errorCode: "x", error: "x", missingStatus: null, projectTitle: null } })) });
    await userEvent.click(screen.getByRole("button", { name: "Plan actions" }));
    expect(screen.queryByRole("menuitem", { name: "Publish to GitHub" })).toBeNull();
  });

  it("MDL-FR-JRGK, MDL-FR-WMGU: Confirm writes the pending changes, then publishes; the canvas is inert while it runs", async () => {
    let finish!: (v: unknown) => void;
    b.on.publish_delivery_to_github = () =>
      new Promise((r) => {
        finish = r;
      });
    const r = await openFile({ claims: claimsOf() });
    editPending(r);
    await userEvent.click(screen.getByRole("button", { name: "Plan actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Publish to GitHub" }));
    expect(r.onOverlayOpening).toHaveBeenCalled();
    const dialog = screen.getByTestId("delivery-publish-dialog");
    expect(dialog).toHaveTextContent("GitHub becomes the source of the delivery");
    expect(dialog).toHaveTextContent("The Flow file stays on disk unchanged");
    // Nothing is written before the author confirms.
    expect(callsOf(b, "save_artifact_contents")).toHaveLength(0);
    expect(r.host.flows.get(FLOW)?.dirty).toBe(true);
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(callsOf(b, "publish_delivery_to_github")).toHaveLength(1));
    // Exactly one write, and it came first.
    const order = b.calls.map((c) => c.cmd).filter((c) => c === "save_artifact_contents" || c === "publish_delivery_to_github");
    expect(order).toEqual(["save_artifact_contents", "publish_delivery_to_github"]);
    expect(b.writes).toHaveLength(1);
    expect(last(b.writes)).toContain("Pending");
    // MDL layout notes: the notice line sits below the band.
    const progress = screen.getByTestId("delivery-publish-progress");
    expect(screen.getByTestId("flow-band").compareDocumentPosition(progress) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(document.querySelector(".flow-surface")).toHaveAttribute("data-inert", "true");
    // MDL-FR-WMGU: no edit affordance acts while the publish runs.
    fireEvent.doubleClick(screen.getByTestId("flow-item-name-t1"));
    expect(screen.queryByLabelText("Item name")).toBeNull();
    expect(screen.getByRole("button", { name: "Add to plan" })).toBeDisabled();
    await act(async () => finish({ milestones: 1, issues: 3, relations: 0, createdUrls: [] }));
    await waitFor(() => expect(screen.queryByTestId("delivery-publish-dialog")).toBeNull());
    expect(visibleToasts()[0]).toMatchObject({ level: "Info", title: "Plan published to GitHub" });
    expect(callsOf(b, "save_artifact_contents")).toHaveLength(1);
  });

  it("MDL-FR-WMGU: a failed publish shows the error and the links of the items it made, and Retry publishes again", async () => {
    let attempt = 0;
    b.on.publish_delivery_to_github = () => {
      attempt += 1;
      return attempt === 1
        ? Promise.reject({
            code: "request_failed",
            message: "GitHub refused the request.",
            createdUrls: ["https://github.com/acme/payments/issues/1"],
          })
        : { milestones: 1, issues: 3, relations: 0, createdUrls: [] };
    };
    await openFile({ claims: claimsOf() });
    await userEvent.click(screen.getByRole("button", { name: "Plan actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Publish to GitHub" }));
    const dialog = screen.getByTestId("delivery-publish-dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    expect(await within(dialog).findByText("GitHub refused the request.")).toBeInTheDocument();
    const created = within(dialog).getByTestId("delivery-publish-created");
    await userEvent.click(within(created).getByRole("button", { name: "https://github.com/acme/payments/issues/1" }));
    expect(callsOf(b, "open_tracker_link")).toEqual([{ url: "https://github.com/acme/payments/issues/1" }]);
    await userEvent.click(within(dialog).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.queryByTestId("delivery-publish-dialog")).toBeNull());
    expect(callsOf(b, "publish_delivery_to_github")).toHaveLength(2);
  });

  it("MDL-FR-JRGK: Cancel publishes nothing and writes nothing", async () => {
    b.on.publish_delivery_to_github = () => ({ milestones: 0, issues: 0, relations: 0, createdUrls: [] });
    const r = await openFile({ claims: claimsOf() });
    editPending(r);
    await userEvent.click(screen.getByRole("button", { name: "Plan actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Publish to GitHub" }));
    await userEvent.click(within(screen.getByTestId("delivery-publish-dialog")).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("delivery-publish-dialog")).toBeNull();
    await act(async () => {});
    expect(callsOf(b, "publish_delivery_to_github")).toHaveLength(0);
    expect(callsOf(b, "save_artifact_contents")).toHaveLength(0);
    expect(r.host.flows.get(FLOW)?.dirty).toBe(true);
  });

  it("MDL-FR-JRGK: a write that is refused stops the publish", async () => {
    b.on.publish_delivery_to_github = () => ({ milestones: 0, issues: 0, relations: 0, createdUrls: [] });
    b.on.save_artifact_contents = () => Promise.reject({ code: "write_refused", message: "Refused." });
    const r = await openFile({ claims: claimsOf() });
    editPending(r);
    await userEvent.click(screen.getByRole("button", { name: "Plan actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Publish to GitHub" }));
    await userEvent.click(within(screen.getByTestId("delivery-publish-dialog")).getByRole("button", { name: "Publish" }));
    expect(await screen.findByText("The Flow file was not written, so nothing was published.")).toBeInTheDocument();
    expect(callsOf(b, "save_artifact_contents")).toHaveLength(1);
    expect(callsOf(b, "publish_delivery_to_github")).toHaveLength(0);
  });
});
