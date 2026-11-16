/**
 * The actions of the Delivery lens on a GitHub plan
 * (`../../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-XQGD,
 * MDL-FR-QEXS, MDL-FR-UKGH, MDL-FR-PUKL, MDL-FR-QIXG, MDL-FR-EEDO,
 * MDL-FR-JIKK).
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
import { resetToasts, visibleToasts } from "../../state/toasts";
import { callsOf } from "../../test/flowCanvasHarness";
import { itemEl, panel, selectItem } from "../../test/flowFixtures";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  claimsOf,
  clearLensStores,
  collapsedLayout,
  withItem,
  deliveryBackend,
  expectCentred,
  githubAnswer,
  githubSource,
  issueFacts,
  leafState,
  pollingView,
  renderLens,
  settle,
  type DeliveryBackend,
} from "../../test/deliveryLensHarness";
import { renameItem, type FlowDocument } from "../../state/flowDocument";
import type { GithubDeliveryEdit } from "../../types/delivery";

const DOC = plan([
  item("m1", "milestone"),
  item("f1", "feature", { parentId: "m1" }),
  item("t1", "task", { parentId: "f1" }),
  item("t2", "task", { parentId: "f1" }),
  item("t3", "task"),
]);
const TRACKER = [
  issueFacts("f1", 42),
  issueFacts("t1", 51, { claimEligible: true }),
  issueFacts("t2", 53, { claimEligible: true }),
  issueFacts("t3", 54),
];

let b: DeliveryBackend;
let doc: FlowDocument;

const answerOf = (d: FlowDocument = doc, extra = {}) =>
  githubAnswer(d, { tracker: TRACKER, states: d.items.map((i) => leafState(i.id, "not_started")), ...extra });

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  doc = DOC;
  b = deliveryBackend("", answerOf(DOC));
  b.plan = () => answerOf();
  b.apply = (edit) => {
    const e = edit as unknown as GithubDeliveryEdit;
    if (e.kind === "create_item") {
      doc = withItem(doc, "new-1", e.itemKind, e.name);
    } else if (e.kind === "set_references") {
      doc = { ...doc, items: doc.items.map((i) => (i.id === e.itemId ? { ...i, artifactIds: e.artifactIds } : i)) };
    } else if (e.kind === "rename") {
      doc = renameItem(doc, e.itemId, e.name);
    }
    return answerOf();
  };
});

afterEach(() => {
  cleanup();
  clearLensStores();
});

async function openPlan(host = {}) {
  const r = await renderLens({ source: githubSource(), host });
  await screen.findByTestId("flow-item-t1");
  await settle();
  return r;
}

describe("removing an item of a GitHub plan (MDL-FR-XQGD)", () => {
  it("MDL-FR-XQGD: a removal opens a dialog that names the item and the count under it, and Remove from board applies remove_item", async () => {
    const r = await openPlan();
    selectItem("f1");
    await userEvent.click(within(panel()).getByRole("button", { name: /Remove/ }));
    expect(r.onOverlayOpening).toHaveBeenCalled();
    const dialog = screen.getByTestId("delivery-remove-dialog");
    expect(dialog).toHaveTextContent("Remove “F1”");
    expect(dialog).toHaveTextContent("2 items are under it.");
    expect(dialog).not.toHaveTextContent("milestone closes");
    // The wireframe's order: the two removals, then Cancel last; focus starts on Cancel.
    expect(within(dialog).getAllByRole("button").map((x) => x.textContent)).toEqual([
      "Remove from board",
      "Close as not planned",
      "Cancel",
    ]);
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await userEvent.click(within(dialog).getByRole("button", { name: "Remove from board" }));
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(1));
    expect(callsOf(b, "apply_github_delivery_edit")[0].edit).toEqual({
      kind: "remove_item",
      itemId: "f1",
      mode: "remove_from_board",
      updatedAt: TRACKER[0].updatedAt,
    });
  });

  it("MDL-FR-XQGD: for a milestone the dialog states that the milestone closes, and Close as not planned applies that mode", async () => {
    await openPlan();
    selectItem("m1");
    await userEvent.click(within(panel()).getByRole("button", { name: /Remove/ }));
    const dialog = screen.getByTestId("delivery-remove-dialog");
    expect(dialog).toHaveTextContent("The milestone closes on GitHub.");
    expect(dialog).toHaveTextContent("3 items are under it.");
    await userEvent.click(within(dialog).getByRole("button", { name: "Close as not planned" }));
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(1));
    expect(callsOf(b, "apply_github_delivery_edit")[0].edit).toMatchObject({ mode: "close_not_planned", itemId: "m1" });
  });

  it("MDL-FR-XQGD, MDL-FR-PUCM: a refused remove_item restores the item and the items under it", async () => {
    b.apply = () => Promise.reject({ code: "request_failed", message: "GitHub refused the request." });
    await openPlan();
    selectItem("f1");
    await userEvent.click(within(panel()).getByRole("button", { name: /Remove/ }));
    await userEvent.click(within(screen.getByTestId("delivery-remove-dialog")).getByRole("button", { name: "Remove from board" }));
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(1));
    expect(await screen.findByTestId("flow-item-error-f1")).toHaveTextContent("GitHub refused the request.");
    for (const id of ["f1", "t1", "t2"]) expect(itemEl(id)).toBeInTheDocument();
  });

  it("MDL-FR-XQGD: the dialog of an item with nothing under it says so", async () => {
    await openPlan();
    selectItem("t3");
    await userEvent.click(within(panel()).getByRole("button", { name: /Remove/ }));
    const dialog = screen.getByTestId("delivery-remove-dialog");
    expect(dialog).toHaveTextContent("No item is under it.");
    expect(dialog).not.toHaveTextContent("items are under it");
  });

  it("MDL-FR-XQGD: Cancel and Escape change nothing", async () => {
    await openPlan();
    selectItem("t3");
    await userEvent.click(within(panel()).getByRole("button", { name: /Remove/ }));
    await userEvent.click(within(screen.getByTestId("delivery-remove-dialog")).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByTestId("delivery-remove-dialog")).toBeNull();
    selectItem("t3");
    await userEvent.click(within(panel()).getByRole("button", { name: /Remove/ }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByTestId("delivery-remove-dialog")).toBeNull();
    expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(0);
    expect(itemEl("t3")).toBeInTheDocument();
  });
});

describe("claims (MDL-FR-QEXS, MDL-FR-UKGH)", () => {
  it("MDL-FR-QEXS: a selected task that is eligible and that no shadow draft or pending claim names offers Claim and graduate", async () => {
    const claims = claimsOf();
    await openPlan({ claims });
    selectItem("t1");
    await userEvent.click(within(panel()).getByRole("button", { name: "Claim and graduate" }));
    expect(claims.claim).toHaveBeenCalledWith(51);
  });

  it("MDL-FR-QEXS: a task that is not eligible, or that a shadow draft names, offers no claim", async () => {
    const view = pollingView({
      shadows: [
        {
          draftId: "d-53",
          name: "card-form",
          status: "graduated",
          repositoryHost: "github.com",
          repositoryOwner: "acme",
          repositoryName: "payments",
          issueNumber: 53,
          issueUrl: "u",
          projectNodeId: "PVT_1",
          locked: false,
        },
      ],
    });
    await openPlan({ claims: claimsOf(view) });
    selectItem("t3");
    expect(within(panel()).queryByTestId("delivery-claim")).toBeNull();
    selectItem("t2");
    expect(within(panel()).queryByTestId("delivery-claim")).toBeNull();
  });

  it("MDL-FR-QEXS: a pending claim offers Retry on the terms of GIT-FR-FTHT, and a refusal renders in the panel", async () => {
    const claims = claimsOf(
      pollingView({
        pendingClaims: [
          {
            repositoryHost: "github.com",
            repositoryOwner: "acme",
            repositoryName: "payments",
            issueNumber: 51,
            issueUrl: "u",
            projectNodeId: "PVT_1",
            draftId: null,
            claimedAt: "2026-11-14T09:00:00Z",
          },
        ],
      }),
    );
    (claims.rowErrors as Map<number, string>).set(51, "The claim could not be made.");
    await openPlan({ claims });
    selectItem("t1");
    // t1 is eligible, but its pending claim offers Retry and no new claim.
    expect(within(panel()).queryByRole("button", { name: "Claim and graduate" })).toBeNull();
    await userEvent.click(within(panel()).getByRole("button", { name: "Retry claim" }));
    expect(claims.retry).toHaveBeenCalledWith(51);
    expect(within(panel()).getByRole("alert")).toHaveTextContent("The claim could not be made.");
  });

  it("MDL-FR-UKGH: an item whose draft is a GitHub-shadow draft that no run holds offers Graduate, which opens the start dialog for that draft", async () => {
    b.plan = () =>
      answerOf(doc, {
        states: [leafState("t2", "drafting", { draft: { id: "d-53", name: "card-form", status: "github_shadow" } })],
      });
    const claims = claimsOf();
    await openPlan({ claims });
    selectItem("t2");
    await userEvent.click(within(panel()).getByRole("button", { name: "Graduate" }));
    expect(claims.graduateShadow).toHaveBeenCalledWith("d-53", "card-form");
  });

  it("MDL-FR-UKGH: with no draft in the delivery state, an unlocked shadow row of the issue offers Graduate; a locked one does not", async () => {
    const shadow = (locked: boolean) =>
      pollingView({
        shadows: [
          {
            draftId: "d-53",
            name: "card-form",
            status: "github_shadow",
            repositoryHost: "github.com",
            repositoryOwner: "acme",
            repositoryName: "payments",
            issueNumber: 53,
            issueUrl: "u",
            projectNodeId: "PVT_1",
            locked,
          },
        ],
      });
    const claims = claimsOf(shadow(false));
    await openPlan({ claims });
    selectItem("t2");
    await userEvent.click(within(panel()).getByRole("button", { name: "Graduate" }));
    expect(claims.graduateShadow).toHaveBeenCalledWith("d-53", "card-form");
    cleanup();
    clearLensStores();
    const lockedClaims = claimsOf(shadow(true));
    await openPlan({ claims: lockedClaims });
    selectItem("t2");
    expect(within(panel()).queryByRole("button", { name: "Graduate" })).toBeNull();
    expect(lockedClaims.graduateShadow).not.toHaveBeenCalled();
  });

  it("MDL-FR-UKGH: an item whose draft is not a GitHub-shadow draft offers no Graduate", async () => {
    b.plan = () =>
      answerOf(doc, {
        states: [leafState("t3", "drafting", { draft: { id: "d-54", name: "refunds", status: "drafting" } })],
      });
    const claims = claimsOf();
    await openPlan({ claims });
    selectItem("t3");
    expect(within(panel()).queryByRole("button", { name: "Graduate" })).toBeNull();
    expect(claims.graduateShadow).not.toHaveBeenCalled();
  });

  it("MDL-FR-UKGH: a shadow draft that a graduation run holds offers no Graduate", async () => {
    b.plan = () =>
      answerOf(doc, {
        states: [
          leafState("t2", "in_progress", {
            draft: { id: "d-53", name: "card-form", status: "github_shadow" },
            run: { runId: "r1", state: "working" },
          }),
        ],
      });
    await openPlan({ claims: claimsOf() });
    selectItem("t2");
    expect(within(panel()).queryByRole("button", { name: "Graduate" })).toBeNull();
  });
});

describe("Add existing issue (MDL-FR-PUKL)", () => {
  it("MDL-FR-PUKL: a selected container takes an issue number and applies add_existing_issue into it", async () => {
    await openPlan();
    selectItem("f1");
    const box = within(panel()).getByTestId("delivery-add-issue");
    await userEvent.type(within(box).getByRole("textbox"), "#77");
    await userEvent.click(within(box).getByRole("button", { name: "Add" }));
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(1));
    expect(callsOf(b, "apply_github_delivery_edit")[0].edit).toEqual({ kind: "add_existing_issue", number: 77, containerId: "f1" });
  });

  it("MDL-FR-PUKL: a refusal shows its error text in place, and a task offers no Add existing issue", async () => {
    b.apply = () => Promise.reject({ code: "already_in_plan", message: "Issue #77 is in the plan." });
    await openPlan();
    selectItem("t3");
    expect(within(panel()).queryByTestId("delivery-add-issue")).toBeNull();
    selectItem("m1");
    const box = within(panel()).getByTestId("delivery-add-issue");
    await userEvent.type(within(box).getByRole("textbox"), "77");
    await userEvent.click(within(box).getByRole("button", { name: "Add" }));
    expect(await within(box).findByRole("alert")).toHaveTextContent("Issue #77 is in the plan.");
  });

  it.each(["abc", "0", ""])("MDL-FR-PUKL: the input %j is not an issue number, so Add is disabled and nothing is sent", async (text) => {
    await openPlan();
    selectItem("f1");
    const box = within(panel()).getByTestId("delivery-add-issue");
    const field = within(box).getByRole("textbox");
    if (text) await userEvent.type(field, text);
    expect(within(box).getByRole("button", { name: "Add" })).toBeDisabled();
    await userEvent.type(field, "{Enter}");
    await act(async () => {});
    expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(0);
  });

  it("MDL-FR-PUKL: the add affordance offers Existing issue, which adds an issue at the top level", async () => {
    await openPlan();
    await userEvent.click(screen.getByRole("button", { name: "Add to plan" }));
    await userEvent.click(within(screen.getByTestId("flow-add-menu")).getByRole("menuitem", { name: /Existing issue/ }));
    const box = screen.getByTestId("delivery-add-top");
    await userEvent.type(within(box).getByRole("textbox"), "12{Enter}");
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(1));
    expect(callsOf(b, "apply_github_delivery_edit")[0].edit).toEqual({ kind: "add_existing_issue", number: 12, containerId: null });
    await waitFor(() => expect(screen.queryByTestId("delivery-add-top")).toBeNull());
  });
});

describe("Export to file (MDL-FR-QIXG)", () => {
  it("MDL-FR-QIXG: the band menu offers Export to file, prefilled from the Project title, and Confirm invokes the export", async () => {
    b.on.export_delivery_to_file = (args) => ({ flowId: String(args.path), path: String(args.path) });
    const r = await openPlan();
    await userEvent.click(screen.getByRole("button", { name: "Plan actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Export to file" }));
    expect(r.onOverlayOpening).toHaveBeenCalled();
    const dialog = screen.getByTestId("delivery-export-dialog");
    const field = within(dialog).getByRole("textbox");
    expect(field).toHaveValue("payments-board.flow");
    await userEvent.clear(field);
    await userEvent.type(field, "plans/delivery.txt");
    expect(within(dialog).getByRole("button", { name: "Export" })).toBeDisabled();
    await userEvent.clear(field);
    await userEvent.type(field, "plans/delivery.flow");
    await userEvent.click(within(dialog).getByRole("button", { name: "Export" }));
    await waitFor(() => expect(screen.queryByTestId("delivery-export-dialog")).toBeNull());
    expect(callsOf(b, "export_delivery_to_file")).toEqual([{ path: "plans/delivery.flow" }]);
    expect(visibleToasts()[0]).toMatchObject({ level: "Info", title: "Plan exported" });
  });

  it("MDL-FR-QIXG: a refusal shows its error text in the dialog", async () => {
    b.on.export_delivery_to_file = () => Promise.reject({ code: "path_exists", message: "A file is at that path." });
    await openPlan();
    await userEvent.click(screen.getByRole("button", { name: "Plan actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Export to file" }));
    const dialog = screen.getByTestId("delivery-export-dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Export" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("A file is at that path.");
  });
});

describe("requests of the map (MDL-FR-EEDO, MDL-FR-JIKK)", () => {
  it("MDL-FR-EEDO: a request to plan work adds a top-level task through create_item and then set_references, and selects it", async () => {
    const r = await openPlan();
    act(() => r.store.requestPlanWork("Card form", ["specifications/ui/A1-a1.md", "specifications/ui/A3-a3.md"]));
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(2));
    const edits = callsOf(b, "apply_github_delivery_edit").map((a) => a.edit);
    expect(edits[0]).toEqual({ kind: "create_item", itemKind: "task", name: "Card form", containerId: null });
    expect(edits[1]).toEqual({
      kind: "set_references",
      itemId: "new-1",
      artifactIds: ["specifications/ui/A1-a1.md", "specifications/ui/A3-a3.md"],
    });
    await waitFor(() => expect(itemEl("new-1")).toHaveAttribute("data-selected", "true"));
    // SDL-FR-CFHH: the new task shows centred, as MDL-FR-JIKK does.
    await waitFor(() => expectCentred("new-1"));
    expect(r.store.snapshot().planWork).toBeNull();
  });

  it("MDL-FR-EEDO: a refused create_item sends no set_references, shows the error, and keeps the selection", async () => {
    b.apply = () => Promise.reject({ code: "type_unmapped", message: "No Type maps to task." });
    const r = await openPlan();
    selectItem("t3");
    act(() => r.store.requestPlanWork("Card form", ["specifications/ui/A1-a1.md"]));
    await waitFor(() => expect(visibleToasts()[0]).toMatchObject({ level: "Error", body: "No Type maps to task." }));
    expect(callsOf(b, "apply_github_delivery_edit").map((a) => (a.edit as { kind: string }).kind)).toEqual(["create_item"]);
    expect(r.store.snapshot().planWork).toBeNull();
    expect(itemEl("t3")).toHaveAttribute("data-selected", "true");
    expect(screen.queryByTestId("flow-item-new-1")).toBeNull();
  });

  it("MDL-FR-EEDO: a refused set_references after the create keeps the new task selected and shows the error on it", async () => {
    const created = b.apply;
    b.apply = (edit) =>
      edit.kind === "set_references"
        ? Promise.reject({ code: "request_failed", message: "The references were not set." })
        : created(edit);
    const r = await openPlan();
    act(() => r.store.requestPlanWork("Card form", ["specifications/ui/A1-a1.md"]));
    await waitFor(() => expect(callsOf(b, "apply_github_delivery_edit")).toHaveLength(2));
    expect(await screen.findByTestId("flow-item-error-new-1")).toHaveTextContent("The references were not set.");
    expect(itemEl("new-1")).toHaveAttribute("data-selected", "true");
  });

  it("MDL-FR-JIKK: a request to show an item selects it, expands each collapsed container that holds it, and centres it", async () => {
    b.layout = collapsedLayout(["m1", "f1"]);
    const r = await renderLens({ source: githubSource() });
    await screen.findByTestId("flow-item-m1");
    await settle();
    await waitFor(() => expect(itemEl("m1")).toHaveAttribute("data-collapsed", "true"));
    expect(screen.queryByTestId("flow-item-f1")).toBeNull();
    act(() => r.store.showDeliveryItem("t2"));
    expect(await screen.findByTestId("flow-item-t2")).toHaveAttribute("data-selected", "true");
    expect(itemEl("m1")).toHaveAttribute("data-collapsed", "false");
    expect(itemEl("f1")).toHaveAttribute("data-collapsed", "false");
    expectCentred("t2");
    expect(r.store.snapshot().deliveryReveal).toBeNull();
  });

  it("MDL-FR-JIKK: a request made before the plan answers selects the item once the plan renders", async () => {
    let answer!: (a: ReturnType<typeof answerOf>) => void;
    b.plan = () => new Promise((res) => (answer = res));
    const r = await renderLens({ source: githubSource() });
    expect(await screen.findByText("Loading the delivery plan…")).toBeInTheDocument();
    act(() => r.store.showDeliveryItem("t2"));
    await act(async () => answer(answerOf()));
    await waitFor(() => expect(itemEl("t2")).toHaveAttribute("data-selected", "true"));
    expect(r.store.snapshot().deliveryReveal).toBeNull();
  });

  it("MDL-FR-JIKK: a request for an id the plan does not hold selects nothing, and the request is used up", async () => {
    const r = await openPlan();
    act(() => r.store.showDeliveryItem("gone"));
    await act(async () => {});
    expect(r.store.snapshot().deliveryReveal).toBeNull();
    expect(document.querySelector('[data-selected="true"]')).toBeNull();
    expect(itemEl("t1")).toBeInTheDocument();
  });

  it("NAW-FR-03: Create draft of a plan task goes through the shell's creation, which opens the New Artifact tab", async () => {
    const onCreateDraft = vi.fn(async () => ({ ok: true as const, draft: { id: "d-new", name: "T3" } }));
    const onOpenDraft = vi.fn();
    await openPlan({ onCreateDraft, onOpenDraft });
    selectItem("t3");
    fireEvent.click(within(panel()).getByRole("button", { name: "Create draft" }));
    await waitFor(() => expect(onCreateDraft).toHaveBeenCalledWith("T3"));
    await waitFor(() => expect(onOpenDraft).toHaveBeenCalledWith({ id: "d-new", name: "T3" }));
  });
});
