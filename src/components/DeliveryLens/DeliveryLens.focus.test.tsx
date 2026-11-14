/**
 * Where focus goes when a dialog of the band menu of the Delivery lens
 * closes (`../../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-QIXG,
 * MDL-FR-JRGK, MDL-FR-WMGU). The entry of the menu that opened the dialog is
 * gone, so focus goes to the band menu trigger, as for the other dialogs.
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
import { resetToasts } from "../../state/toasts";
import { resetListeners } from "../../test/flowMocks";
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
  type DeliveryBackend,
} from "../../test/deliveryLensHarness";
import type { DeliverySource } from "../../types/delivery";

const FLOW = "plans/release.flow";
const DOC = plan([item("m1", "milestone"), item("t1", "task", { parentId: "m1" })]);
const GITHUB_DOC = plan([item("g1", "task")]);

let b: DeliveryBackend;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  b = deliveryBackend(DOC, githubAnswer(GITHUB_DOC));
  b.plan = () => githubAnswer(GITHUB_DOC);
});

afterEach(() => {
  cleanup();
  clearLensStores();
});

const trigger = () => screen.getByRole("button", { name: "Plan actions" });

/**
 * Move focus to the page body, as a browser does when the focused button
 * becomes disabled. jsdom keeps focus on a disabled button, and its `blur()`
 * does nothing there, so focus goes to a field that then leaves the page.
 */
function dropFocus() {
  const field = document.createElement("input");
  document.body.append(field);
  field.focus();
  field.remove();
}

async function openFile(source: () => DeliverySource = () => fileSource(FLOW)) {
  const r = await renderLens({ source: async () => source(), host: { claims: claimsOf() } });
  await screen.findByTestId("flow-item-t1");
  return r;
}

async function openGithub(source: () => DeliverySource = () => githubSource()) {
  const r = await renderLens({ source: async () => source() });
  await screen.findByTestId("flow-item-g1");
  await settle();
  return r;
}

async function chooseEntry(name: string) {
  await userEvent.click(trigger());
  await userEvent.click(screen.getByRole("menuitem", { name }));
}

describe("focus after an Export to file dialog (MDL-FR-QIXG)", () => {
  it("MDL-FR-QIXG: Escape closes the dialog and focus goes to the band menu trigger", async () => {
    await openGithub();
    await chooseEntry("Export to file");
    const dialog = screen.getByTestId("delivery-export-dialog");
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByTestId("delivery-export-dialog")).toBeNull();
    expect(trigger()).toHaveFocus();
  });

  it("MDL-FR-QIXG: Cancel closes the dialog and focus goes to the band menu trigger", async () => {
    await openGithub();
    await chooseEntry("Export to file");
    await userEvent.click(within(screen.getByTestId("delivery-export-dialog")).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("delivery-export-dialog")).toBeNull();
    expect(trigger()).toHaveFocus();
  });

  it("MDL-FR-QIXG: where the click did not focus the menu entry, as in WebKit, focus still goes to the band menu trigger", async () => {
    await openGithub();
    await userEvent.click(trigger());
    (document.activeElement as HTMLElement | null)?.blur();
    expect(document.activeElement).toBe(document.body);
    fireEvent.click(screen.getByRole("menuitem", { name: "Export to file" }));
    await userEvent.keyboard("{Escape}");
    expect(trigger()).toHaveFocus();
  });

  it("MDL-FR-QIXG: after an export makes the file the source, focus goes to the band menu trigger of the file's lens", async () => {
    let source: DeliverySource = githubSource();
    b.on.export_delivery_to_file = () => {
      source = fileSource(FLOW);
      return { flowId: FLOW, path: FLOW };
    };
    await openGithub(() => source);
    const githubTrigger = trigger();
    await chooseEntry("Export to file");
    await userEvent.click(within(screen.getByTestId("delivery-export-dialog")).getByRole("button", { name: "Export" }));
    await screen.findByTestId("flow-item-t1");
    expect(githubTrigger).not.toBeInTheDocument();
    await waitFor(() => expect(trigger()).toHaveFocus());
  });
});

describe("focus after a Publish to GitHub dialog (MDL-FR-JRGK, MDL-FR-WMGU)", () => {
  it("MDL-FR-JRGK: Escape closes the dialog and focus goes to the band menu trigger", async () => {
    await openFile();
    await chooseEntry("Publish to GitHub");
    const dialog = screen.getByTestId("delivery-publish-dialog");
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByTestId("delivery-publish-dialog")).toBeNull();
    expect(trigger()).toHaveFocus();
  });

  it("MDL-FR-WMGU: when a publish fails, focus goes to Retry in the dialog, not to the page body", async () => {
    b.on.publish_delivery_to_github = () => {
      // A browser takes focus from the disabled Publish button; jsdom does not.
      dropFocus();
      return Promise.reject({ code: "request_failed", message: "GitHub refused the request.", createdUrls: [] });
    };
    await openFile();
    await chooseEntry("Publish to GitHub");
    const dialog = screen.getByTestId("delivery-publish-dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    expect(await within(dialog).findByText("GitHub refused the request.")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Retry" })).toHaveFocus();
  });

  it("MDL-FR-WMGU: a failed publish keeps focus where the author put it in the dialog", async () => {
    let fail!: () => void;
    b.on.publish_delivery_to_github = () =>
      new Promise((_, reject) => {
        fail = () => reject({ code: "request_failed", message: "GitHub refused the request.", createdUrls: [] });
      });
    await openFile();
    await chooseEntry("Publish to GitHub");
    const dialog = screen.getByTestId("delivery-publish-dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Publishing…" })).toBeDisabled());
    const title = within(dialog).getByRole("heading", { name: "Publish to GitHub" });
    title.tabIndex = -1;
    title.focus();
    await act(async () => fail());
    expect(await within(dialog).findByText("GitHub refused the request.")).toBeInTheDocument();
    expect(title).toHaveFocus();
  });

  it("MDL-FR-JRGK: after Retry succeeds and GitHub is the source, focus goes to the band menu trigger of the GitHub lens", async () => {
    let source: DeliverySource = fileSource(FLOW);
    let attempt = 0;
    b.on.publish_delivery_to_github = () => {
      attempt += 1;
      dropFocus();
      if (attempt === 1) {
        return Promise.reject({ code: "request_failed", message: "GitHub refused the request.", createdUrls: [] });
      }
      source = githubSource();
      return { milestones: 1, issues: 1, relations: 0, createdUrls: [] };
    };
    await openFile(() => source);
    await chooseEntry("Publish to GitHub");
    const dialog = screen.getByTestId("delivery-publish-dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await userEvent.click(await within(dialog).findByRole("button", { name: "Retry" }));
    await screen.findByTestId("flow-item-g1");
    expect(screen.queryByTestId("delivery-publish-dialog")).toBeNull();
    await waitFor(() => expect(trigger()).toHaveFocus());
  });

  it("MDL-FR-JRGK: a source read that changes nothing moves no focus later", async () => {
    let source: DeliverySource = fileSource(FLOW);
    b.on.publish_delivery_to_github = () => ({ milestones: 1, issues: 1, relations: 0, createdUrls: [] });
    const r = await openFile(() => source);
    await chooseEntry("Publish to GitHub");
    await userEvent.click(within(screen.getByTestId("delivery-publish-dialog")).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(screen.queryByTestId("delivery-publish-dialog")).toBeNull());
    await waitFor(() => expect(r.ops.getDeliverySource.mock.calls.length).toBeGreaterThanOrEqual(2));
    await act(async () => {});
    // The source stayed the file. A later change of source from elsewhere
    // does not take focus from the page.
    (document.activeElement as HTMLElement | null)?.blur();
    source = githubSource();
    await act(async () => {
      await r.store.resolveDeliverySource();
    });
    await screen.findByTestId("flow-item-g1");
    await act(async () => {});
    expect(document.activeElement).toBe(document.body);
  });
});
