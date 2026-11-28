/**
 * The header, the main column, and the sidebar of the issue view of the
 * Delivery lens, and the edits it holds
 * (`../../../specifications/ui/MIV-map-issue-view.md` MIV-FR-POMI ..
 * MIV-FR-HHQR).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { resetToasts } from "../../state/toasts";
import { callsOf } from "../../test/flowCanvasHarness";
import { clickItem, issueView, itemEl, queryIssueView } from "../../test/flowFixtures";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  claimsOf,
  clearLensStores,
  collapsedLayout,
  deliveryBackend,
  fileSource,
  githubAnswer,
  githubSource,
  issueFacts,
  issueRead,
  leafState,
  milestoneFacts,
  renderLens,
  settle,
  type DeliveryBackend,
} from "../../test/deliveryLensHarness";

const FLOW = "plans/release.flow";
const DOC = plan([
  item("m1", "milestone", { due: "2027-01-31" }),
  item("f1", "feature", { parentId: "m1", description: "Feature body" }),
  item("t1", "task", { parentId: "f1" }),
  item("t2", "task", { parentId: "f1", description: "Body of **T2**" }),
  item("t3", "task"),
]);
const TRACKER = [
  milestoneFacts("m1", 3),
  issueFacts("f1", 42),
  issueFacts("t1", 51),
  issueFacts("t2", 53, {
    assignees: ["mia", "jo"],
    status: "In Progress",
    claimEligible: true,
    externalBlockers: [{ number: 99, title: "Provider contract", url: "https://github.com/acme/legal/issues/99" }],
  }),
  issueFacts("t3", 54),
];
const STATES = [
  ...DOC.items.filter((i) => i.id !== "t2").map((i) => leafState(i.id, "not_started")),
  leafState("t2", "not_started", { blocked: true, blockedBy: ["t1"] }),
];

let b: DeliveryBackend;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  b = deliveryBackend(DOC, githubAnswer(DOC, { tracker: TRACKER, states: STATES }));
  b.plan = () => githubAnswer(DOC, { tracker: TRACKER, states: STATES });
  b.apply = () => githubAnswer(DOC, { tracker: TRACKER, states: STATES });
  b.issue = (n) => issueRead(n, { labels: ["payments"], milestoneTitle: "M1" });
});

afterEach(() => {
  cleanup();
  clearLensStores();
});

async function openGithub(host = {}) {
  const r = await renderLens({ source: async () => githubSource(), host: { claims: claimsOf(), ...host } });
  await screen.findByTestId("flow-item-t1");
  await settle();
  return r;
}

async function openFile() {
  const r = await renderLens({ source: async () => fileSource(FLOW), host: { claims: claimsOf() } });
  await screen.findByTestId("flow-item-t1");
  return r;
}

async function viewOf(id: string) {
  clickItem(id);
  const view = await screen.findByTestId("map-issue-view");
  await waitFor(() => expect(view).toHaveAttribute("data-item", id));
  return view;
}

/** The labels of the sidebar fields, in order. */
const fieldLabels = (view: HTMLElement) =>
  [...within(view).getByTestId("miv-side").querySelectorAll(".miv-field__label, .flow-panel__label")].map(
    (el) => el.textContent,
  );

const edits = () => callsOf(b, "apply_github_delivery_edit").map((c) => c.edit as Record<string, unknown>);

describe("the header (MIV-FR-POMI)", () => {
  it("MIV-FR-POMI: an issue shows its kind chip, its address, its status, Discuss, the claim action, and Close", async () => {
    const r = await openGithub();
    const view = await viewOf("t2");
    const head = view.querySelector(".miv-head") as HTMLElement;
    expect(within(head).getByTestId("flow-kind-chip")).toHaveTextContent("TSK");
    const tag = within(head).getByTestId("flow-tracker-tag");
    expect(tag).toHaveTextContent("#53");
    fireEvent.click(tag);
    await waitFor(() =>
      expect(callsOf(b, "open_tracker_link")[0].url).toBe("https://github.com/acme/payments/issues/53"),
    );
    expect(within(head).getByTestId("flow-card-status")).toHaveTextContent("in progress");
    expect(within(head).getByRole("button", { name: "Claim and implement" })).toBeInTheDocument();
    expect(within(head).queryByTestId("miv-pending")).toBeNull();
    await userEvent.click(within(head).getByRole("button", { name: "Discuss" }));
    await waitFor(() => expect(r.store.snapshot().discussion.node).toMatchObject({ kind: "item", id: "t2" }));
    expect(within(head).getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("MIV-FR-POMI: a milestone shows its flag and no kind chip; an item of a file source has no address and no claim", async () => {
    await openGithub();
    const milestone = await viewOf("m1");
    const head = milestone.querySelector(".miv-head") as HTMLElement;
    expect(within(head).getByLabelText("Milestone")).toBeInTheDocument();
    expect(within(head).queryByTestId("flow-kind-chip")).toBeNull();
    cleanup();
    clearLensStores();
    await openFile();
    const file = await viewOf("t2");
    expect(within(file).queryByTestId("flow-tracker-tag")).toBeNull();
    expect(within(file).queryByRole("button", { name: "Claim and implement" })).toBeNull();
  });

  it("MIV-FR-POMI, MDL-FR-PUCM: a pending edit shows the pending mark in the header", async () => {
    let release!: () => void;
    b.apply = () =>
      new Promise((resolve) => {
        release = () => resolve(githubAnswer(DOC, { tracker: TRACKER, states: STATES }));
      });
    await openGithub();
    const view = await viewOf("t3");
    fireEvent.change(within(view).getByRole("combobox", { name: "Kind" }), { target: { value: "defect" } });
    expect(await within(issueView()).findByTestId("miv-pending")).toHaveTextContent("pending");
    release();
    await waitFor(() => expect(within(issueView()).queryByTestId("miv-pending")).toBeNull());
  });
});

describe("the main column (MIV-FR-XMMS, MIV-FR-LENY, MIV-FR-DYXQ)", () => {
  it("MIV-FR-XMMS: the title with Rename, the description, the sub-issues, and Add existing issue, in this order", async () => {
    b.issue = (n) => issueRead(n, { subIssues: [51] });
    await openGithub();
    const view = await viewOf("f1");
    const main = within(view).getByTestId("miv-main");
    const parts = [
      within(main).getByTestId("miv-title"),
      within(main).getByRole("button", { name: "Rename" }),
      within(main).getByTestId("flow-description-section"),
      await within(main).findByTestId("miv-sub-issues"),
      within(main).getByTestId("delivery-add-issue"),
    ];
    for (let i = 1; i < parts.length; i += 1) {
      expect(parts[i - 1].compareDocumentPosition(parts[i]) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
    cleanup();
    clearLensStores();
    await openGithub();
    const leaf = await viewOf("t3");
    expect(within(leaf).queryByTestId("delivery-add-issue")).toBeNull();
  });

  it("MIV-FR-XMMS, MIV-FR-DYXQ: Rename puts the title in a field; Enter commits it once; a card holds no rename field", async () => {
    await openGithub();
    fireEvent.doubleClick(screen.getByTestId("flow-item-name-t3"));
    expect(screen.queryByRole("textbox", { name: "Item name" })).toBeNull();
    const view = await viewOf("t3");
    await userEvent.click(within(view).getByRole("button", { name: "Rename" }));
    const field = within(view).getByRole("textbox", { name: "Item name" });
    expect(field).toHaveFocus();
    await userEvent.clear(field);
    await userEvent.type(field, "Checkout form{Enter}");
    await waitFor(() => expect(edits().filter((e) => e.kind === "rename")).toHaveLength(1));
    expect(edits()[0]).toMatchObject({ kind: "rename", itemId: "t3", name: "Checkout form" });
  });

  it("MIV-FR-XMMS, MIV-FR-YPWK: under StrictMode a rename commits once with Enter, and the focus goes back to the view so Escape still closes it", async () => {
    await renderLens({ source: async () => githubSource(), host: { claims: claimsOf() }, strict: true });
    await screen.findByTestId("flow-item-t1");
    await settle();
    const view = await viewOf("t3");
    await userEvent.click(within(view).getByRole("button", { name: "Rename" }));
    const field = within(view).getByRole("textbox", { name: "Item name" });
    await userEvent.clear(field);
    await userEvent.type(field, "Strict name{Enter}");
    await waitFor(() => expect(edits().filter((e) => e.kind === "rename")).toHaveLength(1));
    expect(within(issueView()).queryByRole("textbox", { name: "Item name" })).toBeNull();
    expect(within(issueView()).getByRole("button", { name: "Rename" })).toBeInTheDocument();
    await waitFor(() => expect(issueView()).toHaveFocus());
    await userEvent.keyboard("{Escape}");
    expect(queryIssueView()).toBeNull();
  });

  it("MIV-FR-XMMS: a blank or unchanged name makes no edit", async () => {
    await openGithub();
    const view = await viewOf("t3");
    await userEvent.click(within(view).getByRole("button", { name: "Rename" }));
    await userEvent.type(within(view).getByRole("textbox", { name: "Item name" }), "{Enter}");
    await userEvent.click(within(view).getByRole("button", { name: "Rename" }));
    await userEvent.clear(within(view).getByRole("textbox", { name: "Item name" }));
    await userEvent.type(within(view).getByRole("textbox", { name: "Item name" }), "{Enter}");
    expect(edits()).toHaveLength(0);
  });

  it("MIV-FR-LENY: the description shows as rich text; Edit shows its source and Done returns; no description says so", async () => {
    await openGithub();
    const view = await viewOf("t2");
    expect(within(view).getByTestId("flow-item-description-t2").querySelector("strong")).toHaveTextContent("T2");
    await userEvent.click(within(view).getByRole("button", { name: "Edit the description" }));
    expect(within(view).getByRole("textbox", { name: "Description" })).toHaveValue("Body of **T2**");
    await userEvent.click(within(view).getByRole("button", { name: "Done editing the description" }));
    expect(within(view).queryByRole("textbox", { name: "Description" })).toBeNull();
    const empty = await viewOf("t3");
    expect(within(empty).getByText("No description.")).toBeInTheDocument();
  });

  it("MIV-FR-XMMS, DBR-FR-CJSG: the error of the item shows below its title", async () => {
    b.apply = () => Promise.reject({ code: "request_failed", message: "GitHub refused the change." });
    await openGithub();
    const view = await viewOf("t3");
    fireEvent.change(within(view).getByRole("combobox", { name: "Kind" }), { target: { value: "defect" } });
    expect(await within(issueView()).findByTestId("flow-panel-error")).toHaveTextContent("GitHub refused the change.");
  });
});

describe("the sidebar (MIV-FR-VHDP, MIV-FR-NBHF, MIV-FR-PGEC)", () => {
  it("MIV-FR-VHDP: the fields of an issue in their order, and Remove at the end on the danger tone", async () => {
    await openGithub();
    const view = await viewOf("t2");
    await within(view).findByText("payments");
    expect(fieldLabels(view)).toEqual([
      "Status",
      "Assignees",
      "Labels",
      "Milestone",
      "Kind",
      "Blocked by",
      "Change",
      "Footprint",
      "References",
    ]);
    const buttons = within(within(view).getByTestId("miv-side")).getAllByRole("button");
    expect(buttons[buttons.length - 1]).toHaveTextContent("Remove");
    expect(buttons[buttons.length - 1]).toHaveClass("btn--danger");
    expect(within(view).getByTestId("miv-field-assignees")).toHaveTextContent("@mia@jo");
  });

  it.each([
    ["an issue with no assignee", "t3", { has: ["Assignees"], none: ["Due", "Blocked by"] }],
    ["a milestone", "m1", { has: ["Due"], none: ["Assignees", "Labels", "Milestone", "Change", "Footprint"] }],
    ["a feature", "f1", { has: ["Labels"], none: ["Change", "Footprint", "Due"] }],
  ])("MIV-FR-NBHF: for %s the fields that apply show and the others are absent", async (_, id, fields) => {
    await openGithub();
    const view = await viewOf(id);
    const labels = fieldLabels(view);
    for (const name of fields.has) expect(labels).toContain(name);
    for (const name of fields.none) expect(labels).not.toContain(name);
    if (id === "t3") expect(within(view).getByTestId("miv-field-assignees")).toHaveTextContent("None");
  });

  it("MIV-FR-NBHF: an item of a file source has no Assignees, Labels, or Milestone", async () => {
    await openFile();
    const labels = fieldLabels(await viewOf("t2"));
    expect(labels).not.toContain("Assignees");
    expect(labels).not.toContain("Labels");
    expect(labels).not.toContain("Milestone");
    expect(labels).toContain("Kind");
  });

  it("MIV-FR-PGEC: a plan blocker opens its own view, revealed; an external blocker opens on GitHub", async () => {
    await openGithub();
    const view = await viewOf("t2");
    const blocked = within(view).getByTestId("miv-field-blocked");
    await userEvent.click(within(blocked).getByRole("button", { name: "Open #99 Provider contract on GitHub" }));
    await waitFor(() =>
      expect(callsOf(b, "open_tracker_link").map((c) => c.url)).toContain("https://github.com/acme/legal/issues/99"),
    );
    const row = within(blocked).getByRole("button", { name: "Open #51 T1" });
    expect(within(row).getByTestId("flow-kind-chip")).toHaveTextContent("TSK");
    await userEvent.click(row);
    await waitFor(() => expect(issueView()).toHaveAttribute("data-item", "t1"));
    expect(itemEl("t1")).toHaveAttribute("data-selected", "true");
  });

  it("MIV-FR-PGEC, MIV-FR-ZQAX: an item inside a collapsed group is shown first when its row opens its view", async () => {
    b.layout = collapsedLayout(["f1"]);
    await renderLens({ source: async () => githubSource() });
    const f1 = await screen.findByTestId("flow-item-f1");
    await settle();
    expect(f1).toHaveAttribute("data-collapsed", "true");
    // The blocked task is in the collapsed group: open it through the group's sub-issue rows.
    b.issue = (n) => issueRead(n, { subIssues: [53] });
    const group = await viewOf("f1");
    await userEvent.click(await within(group).findByRole("button", { name: "Open #53 T2" }));
    await waitFor(() => expect(issueView()).toHaveAttribute("data-item", "t2"));
    await waitFor(() => expect(itemEl("f1")).toHaveAttribute("data-collapsed", "false"));
  });
});

describe("the edits of the issue view (MIV-FR-HHQR)", () => {
  it("MIV-FR-HHQR: a change of kind, a reference, and a footprint path go through the edits of the plan", async () => {
    await openGithub();
    const view = await viewOf("t3");
    fireEvent.change(within(view).getByRole("combobox", { name: "Kind" }), { target: { value: "defect" } });
    await waitFor(() => expect(edits()).toContainEqual(expect.objectContaining({ kind: "set_kind", itemId: "t3" })));
    expect(within(issueView()).getByRole("combobox", { name: "Kind" })).not.toHaveValue("milestone");
    expect(
      [...within(issueView()).getByRole("combobox", { name: "Kind" }).querySelectorAll("option")].map((o) => o.value),
    ).not.toContain("milestone");
  });

  it("MIV-FR-HHQR: a due date of a milestone of a file source is written to the plan", async () => {
    await openFile();
    const view = await viewOf("m1");
    const due = () => issueView().querySelector('input[type="date"]') as HTMLInputElement;
    expect(view).toContainElement(due());
    fireEvent.change(due(), { target: { value: "2027-03-01" } });
    await waitFor(() => expect(due()).toHaveValue("2027-03-01"));
  });

  it("MIV-FR-HHQR: Remove of an item of a file source with nothing under it removes it, and the view closes", async () => {
    await openFile();
    const view = await viewOf("t3");
    await userEvent.click(within(view).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByTestId("flow-item-t3")).toBeNull());
    expect(queryIssueView()).toBeNull();
  });
});
