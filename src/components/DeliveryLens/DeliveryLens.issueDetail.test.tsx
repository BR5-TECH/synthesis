/**
 * The detail read of the issue view of a GitHub plan: labels, milestone, and
 * sub-issues (`../../../specifications/ui/MIV-map-issue-view.md`
 * MIV-FR-EDKQ .. MIV-FR-TZKJ).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../../test/flowMocks")).listen,
}));

import { resetLogBufferForTest } from "../../logging";
import { resetToasts } from "../../state/toasts";
import type { GithubPlanIssueRead } from "../../types/delivery";
import { callsOf } from "../../test/flowCanvasHarness";
import { clickItem, issueView } from "../../test/flowFixtures";
import { resetListeners } from "../../test/flowMocks";
import { item, plan } from "../../test/flowPlanFixtures";
import {
  claimsOf,
  clearLensStores,
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
  item("m1", "milestone"),
  item("f1", "feature", { parentId: "m1" }),
  item("t1", "task", { parentId: "f1" }),
  item("t2", "task", { parentId: "f1" }),
]);
const trackerAt = (updatedAt: string) => [
  milestoneFacts("m1", 3),
  issueFacts("f1", 42, { updatedAt }),
  issueFacts("t1", 51, { status: "Done", state: "closed", closeReason: "completed" }),
  issueFacts("t2", 53),
];
const STATES = DOC.items.map((i) => leafState(i.id, "not_started"));

let b: DeliveryBackend;
let updatedAt: string;

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
  updatedAt = "2026-11-01T00:00:00Z";
  b = deliveryBackend(DOC, githubAnswer(DOC, { tracker: trackerAt(updatedAt), states: STATES }));
  b.plan = () => githubAnswer(DOC, { tracker: trackerAt(updatedAt), states: STATES });
});

afterEach(() => {
  cleanup();
  clearLensStores();
});

async function openGithub() {
  const r = await renderLens({ source: async () => githubSource(), host: { claims: claimsOf() } });
  await screen.findByTestId("flow-item-t1");
  await settle();
  return r;
}

async function viewOf(id: string) {
  clickItem(id);
  const view = await screen.findByTestId("map-issue-view");
  await waitFor(() => expect(view).toHaveAttribute("data-item", id));
  return view;
}

/** A read that answers when the test says so. */
function deferred() {
  let resolve!: (read: GithubPlanIssueRead) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<GithubPlanIssueRead>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

const reads = () => callsOf(b, "read_github_plan_issue").map((c) => c.number);

describe("the detail read (MIV-FR-EDKQ, MIV-FR-DFVC, MIV-FR-BCOP)", () => {
  it("MIV-FR-EDKQ: an issue reads its detail once when its view opens; a milestone reads nothing", async () => {
    await openGithub();
    await viewOf("f1");
    await waitFor(() => expect(reads()).toEqual([42]));
    await viewOf("m1");
    await viewOf("t1");
    await waitFor(() => expect(reads()).toEqual([42, 51]));
  });

  it("MIV-FR-EDKQ: an item of a file source reads nothing", async () => {
    await renderLens({ source: async () => fileSource(FLOW), host: { claims: claimsOf() } });
    await screen.findByTestId("flow-item-t1");
    await viewOf("t1");
    expect(within(issueView()).queryByTestId("miv-field-labels")).toBeNull();
    expect(within(issueView()).queryByTestId("miv-sub-issues")).toBeNull();
    expect(reads()).toEqual([]);
  });

  it("MIV-FR-EDKQ: a new updated_at of the item reads again, once; the same one does not", async () => {
    await openGithub();
    await viewOf("f1");
    await waitFor(() => expect(reads()).toHaveLength(1));
    const loads = callsOf(b, "load_github_delivery_plan").length;
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(callsOf(b, "load_github_delivery_plan").length).toBeGreaterThan(loads));
    await settle();
    expect(reads()).toHaveLength(1);
    updatedAt = "2026-11-02T00:00:00Z";
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(reads()).toHaveLength(2));
    await settle();
    expect(reads()).toEqual([42, 42]);
  });

  it("MIV-FR-DFVC, MIV-FR-HSRA: the fields show Loading… while the read runs, and the title and body show at once", async () => {
    const read = deferred();
    b.issue = () => read.promise;
    await openGithub();
    const view = await viewOf("f1");
    expect(within(view).getByTestId("miv-title")).toHaveTextContent("F1");
    expect(within(view).getByTestId("miv-field-labels")).toHaveTextContent("Loading…");
    expect(within(view).getByTestId("miv-field-milestone")).toHaveTextContent("Loading…");
    expect(within(view).getByTestId("miv-sub-issues")).toHaveTextContent("Loading…");
    read.resolve(issueRead(42, { labels: ["payments", "p1"], milestoneTitle: "Beta" }));
    await waitFor(() => expect(within(view).getByTestId("miv-field-milestone")).toHaveTextContent("Beta"));
    const labels = within(within(view).getByTestId("miv-field-labels")).getAllByText(/payments|p1/);
    expect(labels.map((l) => l.textContent)).toEqual(["payments", "p1"]);
    for (const l of labels) expect(l.className).toBe("badge miv-label");
  });

  it("MIV-FR-HSRA: no label and no milestone show None", async () => {
    b.issue = (n) => issueRead(n);
    await openGithub();
    const view = await viewOf("f1");
    await waitFor(() => expect(within(view).getByTestId("miv-field-labels")).toHaveTextContent("None"));
    expect(within(view).getByTestId("miv-field-milestone")).toHaveTextContent("None");
  });

  it("MIV-FR-DFVC: an answer for an issue the view no longer shows is discarded", async () => {
    const first = deferred();
    const second = deferred();
    b.issue = (n) => (n === 42 ? first.promise : second.promise);
    await openGithub();
    await viewOf("f1");
    const view = await viewOf("t2");
    second.resolve(issueRead(53, { labels: ["second"] }));
    await waitFor(() => expect(within(view).getByTestId("miv-field-labels")).toHaveTextContent("second"));
    first.resolve(issueRead(42, { labels: ["first"] }));
    await settle();
    expect(within(issueView()).getByTestId("miv-field-labels")).toHaveTextContent("second");
    expect(within(issueView()).getByTestId("miv-field-labels")).not.toHaveTextContent("first");
  });

  it("MIV-FR-BCOP: a stale answer shows its milestone and sub-issues, and says the labels could not be read", async () => {
    b.issue = (n) =>
      issueRead(
        n,
        { milestoneTitle: "Beta", subIssues: [51], labels: [] },
        { stale: true, error: { code: "github_unreachable", message: "GitHub could not be reached." }, readAt: "x" },
      );
    await openGithub();
    const view = await viewOf("f1");
    await waitFor(() =>
      expect(within(view).getByTestId("miv-field-labels")).toHaveTextContent("Labels could not be read"),
    );
    expect(within(view).getByTestId("miv-field-milestone")).toHaveTextContent("Beta");
    expect(within(within(view).getByTestId("miv-sub-issues")).getByRole("button", { name: "Open #51 T1" })).toBeInTheDocument();
  });

  it("MIV-FR-BCOP: a refusal shows Could not be read in Labels, Milestone, and Sub-issues", async () => {
    b.issue = () => Promise.reject({ code: "tracker_item_not_found", message: "Not in the plan." });
    await openGithub();
    const view = await viewOf("f1");
    for (const id of ["miv-field-labels", "miv-field-milestone", "miv-sub-issues"]) {
      await waitFor(() => expect(within(view).getByTestId(id)).toHaveTextContent("Could not be read"));
    }
  });
});

describe("the sub-issues (MIV-FR-ZQAX, MIV-FR-TZKJ)", () => {
  it("MIV-FR-ZQAX: each plan sub-issue shows its chip, address, name, and status, and opens its view; another one opens on GitHub", async () => {
    b.issue = (n) => issueRead(n, { subIssues: [51, 77] });
    await openGithub();
    const view = await viewOf("f1");
    const subs = await within(view).findByTestId("miv-sub-issues");
    await waitFor(() => expect(subs.querySelector(".miv-count")).toHaveTextContent("2"));
    const own = within(subs).getByRole("button", { name: "Open #51 T1" });
    expect(within(own).getByTestId("flow-kind-chip")).toHaveTextContent("TSK");
    expect(within(own).getByTestId("flow-card-status")).toHaveTextContent("done");
    await userEvent.click(within(subs).getByRole("button", { name: "Open acme/payments#77 on GitHub" }));
    await waitFor(() =>
      expect(callsOf(b, "open_tracker_link").map((c) => c.url)).toEqual(["https://github.com/acme/payments/issues/77"]),
    );
    await userEvent.click(own);
    await waitFor(() => expect(issueView()).toHaveAttribute("data-item", "t1"));
  });

  it("MIV-FR-TZKJ: the section is absent where the answer holds no sub-issue, and for a milestone", async () => {
    b.issue = (n) => issueRead(n, { labels: ["x"] });
    await openGithub();
    const view = await viewOf("f1");
    await waitFor(() => expect(within(view).getByTestId("miv-field-labels")).toHaveTextContent("x"));
    expect(within(view).queryByTestId("miv-sub-issues")).toBeNull();
    const milestone = await viewOf("m1");
    expect(within(milestone).queryByTestId("miv-sub-issues")).toBeNull();
    expect(within(milestone).queryByTestId("miv-field-labels")).toBeNull();
  });
});
