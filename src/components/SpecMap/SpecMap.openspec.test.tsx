/**
 * The Map tab over an OpenSpec index (`../../../specifications/ui/SMP-specification-map.md`
 * SMP-FR-MNZW, SMP-FR-KVKD, SMP-FR-KBXW; `SMN-specification-map-nodes.md`,
 * `SMI-specification-map-inspector.md`, `SME-specification-map-edges.md`
 * SME-FR-XLUQ, `SMO-specification-map-organization.md` SMO-FR-RVAV,
 * `SMD-specification-map-drafts.md` SMD-FR-YQSR).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const { logWarn } = vi.hoisted(() => ({ logWarn: vi.fn() }));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn,
  logError: vi.fn(),
}));

import { typeChip } from "../../artifactTypes";
import { configureOpenSpecChangeRoutes } from "../../state/openspec/changeRoutes";
import { buildTree } from "../../state/specMap/tree";
import { changeChipKey } from "../../state/specMap/types";
import { layoutMap, scopeOf } from "./layout";
import { openspecIndex } from "../../test/specMapFixtures";
import { canvas, mockCanvasRect, nodeButton, renderMap, setView } from "../../test/specMapRender";

const inspector = () => screen.getByRole("complementary", { name: "Inspector" });
const openRoute = vi.fn();
/** The commands this file does not expect, each answered with a rejection. */
let unknown: string[] = [];

beforeEach(() => {
  unknown = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "open_settings_window" || cmd === "append_log_records") return undefined;
    if (cmd === "get_openspec_status") {
      return { setting: "auto", usesOpenspec: true, active: true, cli: { state: "not_found" } };
    }
    unknown.push(cmd);
    throw new Error(`unexpected invoke ${cmd}`);
  });
  openRoute.mockReset();
  logWarn.mockReset();
  configureOpenSpecChangeRoutes({ open: openRoute });
});
afterEach(() => {
  configureOpenSpecChangeRoutes(null);
  cleanup();
  vi.restoreAllMocks();
  expect(unknown).toEqual([]);
});

describe("the frame (SMP-FR-MNZW, SMP-FR-KVKD, SMP-FR-KBXW)", () => {
  it("SMP-FR-MNZW, SME-FR-XLUQ: three levels from the index, no dependencies toggle, and no edge", async () => {
    const { container } = await renderMap({ index: openspecIndex() });
    const levels = within(screen.getByRole("radiogroup", { name: "Detail level" }))
      .getAllByRole("radio")
      .map((r) => r.textContent);
    expect(levels).toEqual(["groups", "capabilities", "requirements"]);
    expect(screen.queryByRole("button", { name: "Dependencies" })).toBeNull();
    expect(screen.getByRole("button", { name: "Gaps only" })).toBeInTheDocument();
    expect(container.querySelectorAll("path.smap-edge")).toHaveLength(0);
  });

  it("SMP-FR-KVKD: no notice shows where the validation ran and every location was read", async () => {
    await renderMap({ index: openspecIndex() });
    expect(screen.queryByTestId("spec-map-notice")).toBeNull();
  });

  it("SMP-FR-KVKD, SMI-FR-QWOP: an unavailable validation states that it could not run, and hides no node", async () => {
    await renderMap({ index: openspecIndex({ validation: "unavailable" }) });
    const notice = screen.getByTestId("spec-map-notice");
    expect(notice).toHaveTextContent("The OpenSpec validation could not run");
    expect(notice).not.toHaveTextContent("could not be read");
    // The line stands outside the canvas, so every root card still renders.
    expect(canvas().contains(notice)).toBe(false);
    // SMI-FR-QWOP: it stands before the body, the box the overlay inspector covers.
    const body = canvas().closest(".smap-body")!;
    expect(body.contains(notice)).toBe(false);
    expect(notice.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(nodeButton("specs, group")).toBeInTheDocument();
    expect(nodeButton("billing, group")).toBeInTheDocument();
  });

  it("SMP-FR-KVKD: unread locations are counted, alone and beside an unavailable validation", async () => {
    await renderMap({ index: openspecIndex({ unreadLocations: 2 }) });
    expect(screen.getByTestId("spec-map-notice")).toHaveTextContent(
      "The changes of 2 locations could not be read.",
    );
    expect(screen.getByTestId("spec-map-notice")).not.toHaveTextContent("validation");
    cleanup();
    await renderMap({ index: openspecIndex({ unreadLocations: 1, validation: "unavailable" }) });
    const both = screen.getAllByTestId("spec-map-notice");
    expect(both).toHaveLength(1);
    expect(both[0]).toHaveTextContent("validation could not run");
    expect(both[0]).toHaveTextContent("The changes of 1 location could not be read.");
  });

  it("SMP-FR-KVKD: more than one unread location beside an unavailable validation shows one line with both", async () => {
    await renderMap({ index: openspecIndex({ unreadLocations: 3, validation: "unavailable" }) });
    const notices = screen.getAllByTestId("spec-map-notice");
    expect(notices).toHaveLength(1);
    expect(notices[0]).toHaveTextContent("The OpenSpec validation could not run");
    expect(notices[0]).toHaveTextContent("The changes of 3 locations could not be read.");
  });

  it("SMP-FR-KBXW, SMP-FR-ONSD, OFN-FR-EPIG, OFN-FR-HEVB: an OpenSpec failure renders its notice in place of the statement, routes to its settings, and keeps Retry", async () => {
    const load = vi
      .fn()
      .mockRejectedValueOnce({ code: "cli_unavailable", detail: "not_found", diagnostics: [] })
      .mockResolvedValue(openspecIndex());
    const { store } = await renderMap({ ops: { load } });
    const notice = screen.getByTestId("openspec-failure-notice");
    expect(within(notice).getByRole("alert")).toHaveTextContent(
      "The OpenSpec CLI is not installed on this machine.",
    );
    expect(screen.queryByText("The specification map could not be loaded.")).toBeNull();
    fireEvent.click(within(notice).getByRole("button", { name: "Open OpenSpec settings" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("open_settings_window", { section: "project/openspec" }),
    );
    // OFN-FR-DPIJ: the notice adds no Retry of its own; the state keeps one.
    expect(within(notice).queryByRole("button", { name: "Retry" })).toBeNull();
    expect(screen.getAllByRole("button", { name: "Retry" })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(store.snapshot().status).toBe("ready"));
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("SMP-FR-KBXW, OFN-FR-DPIJ: Retry after a transient failure re-checks the CLI before the load, and the notice offers no Retry", async () => {
    const order: string[] = [];
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "append_log_records") return undefined;
      if (cmd === "get_openspec_status") {
        order.push(`status ${JSON.stringify(args)}`);
        return { setting: "auto", usesOpenspec: true, active: true, cli: { state: "not_found" } };
      }
      unknown.push(cmd);
      throw new Error(`unexpected invoke ${cmd}`);
    });
    const load = vi.fn(async () => {
      order.push("load");
      if (order.length === 1) throw { code: "cli_timed_out", diagnostics: [{ message: "slow" }] };
      return openspecIndex();
    });
    const { store } = await renderMap({ ops: { load } });
    const notice = screen.getByTestId("openspec-failure-notice");
    expect(notice).toHaveTextContent("The OpenSpec CLI did not finish in time, also after one more try.");
    expect(within(notice).queryByRole("button", { name: "Retry" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(store.snapshot().status).toBe("ready"));
    expect(order).toEqual(["load", 'status {"recheck":true}', "load"]);
  });

  it("SMP-FR-KBXW: a Retry whose load fails again puts the focus on the new Retry, not on the page", async () => {
    const load = vi.fn().mockRejectedValue({ code: "cli_timed_out", diagnostics: [] });
    const { store } = await renderMap({ ops: { load } });
    const first = screen.getByRole("button", { name: "Retry" });
    first.focus();
    fireEvent.click(first);
    await waitFor(() => expect(load).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(store.snapshot().status).toBe("error"));
    const again = await screen.findByRole("button", { name: "Retry" });
    await waitFor(() => expect(document.activeElement).toBe(again));
  });

  it("SMP-FR-KBXW, SMP-FR-ONSD: a failure that is not an OpenSpec failure keeps the statement and Retry, and re-checks nothing", async () => {
    const load = vi.fn().mockRejectedValueOnce(new Error("no_open_project")).mockResolvedValue(openspecIndex());
    const { store } = await renderMap({ ops: { load } });
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("The specification map could not be loaded.");
    expect(within(alert).queryByRole("button", { name: "Open OpenSpec settings" })).toBeNull();
    fireEvent.click(within(alert).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(store.snapshot().status).toBe("ready"));
    expect(invokeMock.mock.calls.filter((c) => c[0] === "get_openspec_status")).toHaveLength(0);
  });

  it("SMP-FR-KVKD, OFN-FR-IHSQ: an unavailable validation states the sentence of its failure in the line, with its Details", async () => {
    await renderMap({
      index: openspecIndex({
        validation: "unavailable",
        validationFailure: {
          code: "cli_rejected",
          diagnostics: [{ severity: "error", path: "specs/auth/spec.md", message: "Requirement has no scenario" }],
        },
      }),
    });
    const notice = screen.getByTestId("spec-map-notice");
    expect(notice).toHaveTextContent(
      "The OpenSpec validation could not run, so no requirement shows as a gap. OpenSpec reported a problem in the OpenSpec files of this change.",
    );
    // The line holds the sentence once, and no action but Details.
    expect(screen.getAllByText(/OpenSpec reported a problem/)).toHaveLength(1);
    const details = within(notice).getByRole("button", { name: /Details/ });
    expect(within(notice).getAllByRole("button")).toEqual([details]);
    fireEvent.click(details);
    expect(within(notice).getByTestId("openspec-failure-diagnostic")).toHaveTextContent(
      "error specs/auth/spec.md: Requirement has no scenario",
    );
    // OFN-FR-ONNF: the sentence alone is live; the line and its Details are not.
    expect(notice).not.toHaveAttribute("role");
    const live = within(notice).getAllByRole("status").find((el) => el.tagName === "SPAN" && !el.closest(".ofn"));
    expect(live).toHaveTextContent("OpenSpec reported a problem in the OpenSpec files of this change.");
    expect(live!.contains(within(notice).getByTestId("openspec-failure-details"))).toBe(false);
    expect(within(notice).getByTestId("openspec-failure-details")).toHaveAttribute("aria-live", "off");
  });
});

describe("the log record of the validation line (OFN-FR-LKAT)", () => {
  const noticeRecords = () =>
    logWarn.mock.calls.filter((c) => c[1] === "an openspec failure notice appeared");

  it("OFN-FR-LKAT, SMP-FR-KVKD: the sentence in the line emits one WARN record, also with no Details", async () => {
    const { store } = await renderMap({
      index: openspecIndex({ validation: "unavailable", validationFailure: { code: "cli_timed_out", diagnostics: [] } }),
    });
    expect(screen.getByTestId("spec-map-notice")).toHaveTextContent("did not finish in time");
    expect(noticeRecords()).toHaveLength(1);
    expect(noticeRecords()[0][2]).toEqual({
      surface: "spec_map_validation",
      code: "cli_timed_out",
      cause: "",
      class: "transient",
    });
    // A render with the same failure is the same notice.
    act(() => store.toggleGaps());
    expect(noticeRecords()).toHaveLength(1);
  });

  it("OFN-FR-LKAT: a line with Details emits one record, not one for the line and one for its Details", async () => {
    await renderMap({
      index: openspecIndex({
        validation: "unavailable",
        validationFailure: { code: "cli_rejected", diagnostics: [{ message: "private message" }] },
      }),
    });
    fireEvent.click(within(screen.getByTestId("spec-map-notice")).getByRole("button", { name: /Details/ }));
    expect(noticeRecords()).toHaveLength(1);
    expect(JSON.stringify(logWarn.mock.calls)).not.toContain("private message");
  });

  it("OFN-FR-LKAT: a line with no validation failure emits no notice record", async () => {
    await renderMap({ index: openspecIndex({ validation: "unavailable" }) });
    expect(screen.getByTestId("spec-map-notice")).toBeInTheDocument();
    expect(noticeRecords()).toHaveLength(0);
  });
});

describe("nodes (SMN)", () => {
  it("SMN-FR-PMZM, SMN-FR-WAGD: a requirement chip shows its badge in the openspec colour pair", async () => {
    const { store } = await renderMap({ index: openspecIndex() });
    setView(store, { level: 2 });
    const chip = nodeButton("auth#1 auth requirement 1");
    const badge = chip.querySelector(".smap-code")!;
    expect(badge).toHaveTextContent("R1");
    expect(badge).not.toHaveTextContent("auth#1");
    expect(badge).toHaveAttribute("data-folder", "openspec");
    expect(nodeButton("auth#2 auth requirement 2").querySelector(".smap-dot")).toHaveAttribute(
      "data-state",
      "gap",
    );
  });

  it("SMN-FR-ZOUZ: change chips follow the spec chips in the same grid, with a change badge and a dot only for an issue", async () => {
    const { store } = await renderMap({ index: openspecIndex() });
    setView(store, { level: 2 });
    const active = nodeButton("change add-logout, active worktree, on Authentication");
    const stream = nodeButton("change add-logout, editor-work, on Authentication");
    expect(active.querySelector(".smap-code--change")).toHaveTextContent("change");
    expect(active).toHaveTextContent("add-logout");
    expect(active.querySelector('.smap-dot[data-state="gap"]')).not.toBeNull();
    expect(stream.querySelector(".smap-dot")).toBeNull();

    const tree = buildTree(store.snapshot().index!);
    const layout = layoutMap(scopeOf(tree, null), 2, 1200);
    const r1 = layout.chip.get("auth#1")!;
    const r2 = layout.chip.get("auth#2")!;
    const chips = tree.nodes.get("capability:auth")!.changes!;
    const third = layout.chip.get(changeChipKey("capability:auth", chips[0]))!;
    const fourth = layout.chip.get(changeChipKey("capability:auth", chips[1]))!;
    // The same change on two capabilities gives two chips, each with its own key.
    expect(nodeButton("change add-logout, editor-work, on ui")).toBeInTheDocument();
    expect([third.w, third.h]).toEqual([96, 22]);
    // One row of three: R1, R2, then the first chip; the second chip opens row two.
    expect(r2.x - r1.x).toBe(third.x - r2.x);
    expect(third.y).toBe(r1.y);
    expect(fourth.x).toBe(r1.x);
    expect(fourth.y).toBe(r1.y + 28);
  });

  it("SMN-FR-VNTJ: a planned capability renders as a leaf group with the word planned, no spec chip and a spec count of 0", async () => {
    const { store } = await renderMap({ index: openspecIndex() });
    setView(store, { level: 1 });
    const ui = nodeButton("ui, capability");
    expect(ui).toHaveAttribute("data-planned", "true");
    expect(ui).toHaveAttribute("data-role", "leaf");
    expect(ui.querySelector(".smap-box__planned")).toHaveTextContent("planned");
    expect(ui.querySelector(".smap-box__meta")).toHaveTextContent(/^0/);
    setView(store, { level: 2 });
    expect(screen.queryAllByRole("button", { name: /^ui#/ })).toHaveLength(0);
    expect(nodeButton("Authentication, capability")).not.toHaveAttribute("data-planned");
  });

  it("SMN-FR-YEVY: a collapsed node counts the distinct changes beneath it, and chips add nothing to the counts", async () => {
    mockCanvasRect({ width: 1400, height: 800 });
    const { store } = await renderMap({ index: openspecIndex() });
    // Level 0: the root group holds add-logout in two locations, on two capabilities.
    expect(nodeButton("specs, group")).toHaveTextContent("2 specs · 2 capabilities · +2 changes");
    // SMN-FR-QHVM: a meta line that truncates keeps its whole text in its title.
    expect(nodeButton("specs, group").querySelector(".smap-root__meta")).toHaveAttribute(
      "title",
      "2 specs · 2 capabilities · +2 changes",
    );
    expect(nodeButton("billing, group")).toHaveTextContent("2 specs · 1 capabilities · +1 change");
    setView(store, { level: 1 });
    expect(nodeButton("Authentication, capability")).toHaveTextContent("2 · +2 changes");
    expect(nodeButton("ui, capability")).toHaveTextContent("0 · +1 change");
    expect(nodeButton("ui, capability").querySelector(".smap-box__meta")).toHaveAttribute(
      "title",
      "0 · +1 change",
    );
    // The counts line covers the four requirement nodes alone.
    expect(screen.getByRole("toolbar", { name: "Map controls" })).toHaveTextContent(
      "0 verified · 2 built · 0 drafted · 2 gaps",
    );
  });

  it("SMN-FR-YEVY: the same change on two streams counts as two, and the same change on one stream counts once however many capabilities hold it", async () => {
    mockCanvasRect({ width: 1400, height: 800 });
    const index = openspecIndex();
    const s2 = { kind: "stream", streamId: "s-2", streamName: "review-work" } as const;
    const [auth, ui] = index.roots[0].children as Array<{ changes?: unknown[] }>;
    auth.changes = [
      { change: "add-logout", location: { kind: "stream", streamId: "s-1", streamName: "editor-work" } },
      { change: "add-logout", location: s2 },
    ];
    ui.changes = [{ change: "add-logout", location: s2 }];
    await renderMap({ index });
    expect(nodeButton("specs, group")).toHaveTextContent("+2 changes");
  });

  it("SMN-FR-EIDC: the hover card of a requirement reads its scenarios and names no citation", async () => {
    const { store } = await renderMap({ index: openspecIndex() });
    setView(store, { level: 2 });
    fireEvent.mouseOver(nodeButton("auth#1 auth requirement 1"));
    const card = screen.getByRole("tooltip");
    expect(card.querySelector(".smap-hover__meta")).toHaveTextContent(/^2 scenarios$/);
    expect(card).not.toHaveTextContent("cites");
    expect(card.querySelector(".smap-code")).toHaveTextContent("R1");
  });
});

describe("the inspector (SMI)", () => {
  it("SMI-FR-BWIY, SMI-FR-CEVL, SMI-FR-BXET: a requirement has no Depends on, a project-relative path, and its issues as level and message", async () => {
    const { store } = await renderMap({ index: openspecIndex() });
    act(() => store.select({ kind: "spec", code: "auth#2" }));
    const pane = inspector();
    expect(within(pane).queryByText("Depends on")).toBeNull();
    expect(pane).toHaveTextContent("openspec/specs/auth/spec.md");
    expect(pane).not.toHaveTextContent("specifications/openspec");
    expect(within(pane).getByRole("note")).toHaveTextContent("WARNING — needs a scenario");
  });

  it("SMI-FR-BXET: an index node lists the issues of itself and beneath it by the label that holds each", async () => {
    const { store } = await renderMap({ index: openspecIndex() });
    act(() => store.select({ kind: "index", id: "group:billing" }));
    const lines = [...inspector().querySelectorAll(".smap-attention__line")].map((l) => l.textContent);
    expect(lines).toEqual(["Payments — purpose is short", "billing/pay requirement 1 — no scenario"]);
    expect(within(inspector()).queryByText("Depends on")).toBeNull();
  });

  it("SMI-FR-BXET: an index node lists at most four issues", async () => {
    const index = openspecIndex();
    const auth = index.roots[0].children[0] as { issues?: unknown[] };
    auth.issues = [1, 2, 3, 4].map((n) => ({ level: "ERROR", message: `issue ${n}` }));
    const { store } = await renderMap({ index });
    act(() => store.select({ kind: "index", id: "group:/" }));
    expect(inspector().querySelectorAll(".smap-attention__line")).toHaveLength(4);
  });

  it("SMI-FR-GAJD: Open in editor opens the capability's spec file by its project-relative path", async () => {
    const { store, onOpenArtifact } = await renderMap({ index: openspecIndex() });
    act(() => store.select({ kind: "spec", code: "billing/pay#2" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Open in editor" }));
    expect(onOpenArtifact).toHaveBeenCalledWith({
      id: "openspec/specs/billing/pay/spec.md",
      name: "spec.md",
      artifactType: "spec",
      chip: typeChip("spec"),
    });
  });

  it("SMI-FR-ZMZB, SMD-FR-YQSR: a click on a change chip shows the change, its location, its capability and its issues, with no New draft", async () => {
    const { store } = await renderMap({ index: openspecIndex() });
    setView(store, { level: 2 });
    fireEvent.click(nodeButton("change add-logout, active worktree, on Authentication"));
    const pane = inspector();
    expect(pane.querySelector(".smap-inspector__name")).toHaveTextContent("add-logout");
    expect(within(pane).getByTestId("spec-map-change-location")).toHaveTextContent("active worktree");
    expect(within(pane).getByTestId("spec-map-change-holder")).toHaveTextContent("Authentication");
    expect(within(pane).getByRole("note")).toHaveTextContent("ERROR — delta has no scenario");
    expect(within(pane).queryByRole("button", { name: /New draft/ })).toBeNull();

    fireEvent.click(nodeButton("change add-logout, editor-work, on Authentication"));
    expect(within(inspector()).getByTestId("spec-map-change-location")).toHaveTextContent("editor-work");
    expect(within(inspector()).queryByRole("note")).toBeNull();
  });

  it("SMI-FR-UKIV, OCW-FR-DMGL: Open change opens the change in the location of its chip", async () => {
    const { store } = await renderMap({ index: openspecIndex() });
    setView(store, { level: 2 });
    fireEvent.click(nodeButton("change add-logout, editor-work, on Authentication"));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Open change" }));
    expect(openRoute).toHaveBeenLastCalledWith(
      { kind: "stream", streamId: "s-1" },
      "add-logout",
      { streamName: "editor-work" },
    );
    fireEvent.click(nodeButton("change add-logout, active worktree, on Authentication"));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Open change" }));
    expect(openRoute).toHaveBeenLastCalledWith({ kind: "active_worktree" }, "add-logout", {});
  });

  it("SMI-FR-QCUC: a spec file selection selects the capability whose requirements have that path, at the last level", async () => {
    mockCanvasRect({ width: 1200, height: 800 });
    const { store, container } = await renderMap({ index: openspecIndex() });
    act(() => store.selectByPath("openspec/specs/billing/pay/spec.md"));
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "capability:billing/pay" });
    await waitFor(() => expect(store.snapshot().view).toMatchObject({ level: 2, touched: true }));
    expect(container.querySelector(".smap-inspector__name")).toHaveTextContent("Payments");
    // A path no capability has changes nothing.
    act(() => store.selectByPath("openspec/specs/none/spec.md"));
    expect(store.snapshot().selection).toEqual({ kind: "index", id: "capability:billing/pay" });
  });
});

describe("no organization (SMO-FR-RVAV, SMD-FR-YQSR)", () => {
  it("SMO-FR-RVAV: the inspector offers no New, Rename, or Delete, with or without a subject", async () => {
    const { store } = await renderMap({ index: openspecIndex() });
    expect(within(inspector()).queryByRole("button", { name: /^New / })).toBeNull();
    act(() => store.select({ kind: "index", id: "group:/" }));
    const pane = inspector();
    expect(within(pane).queryByRole("button", { name: "New capability" })).toBeNull();
    expect(within(pane).queryByRole("button", { name: "Rename" })).toBeNull();
    expect(within(pane).queryByRole("button", { name: "Delete" })).toBeNull();
    expect(within(pane).getByRole("button", { name: "Open group" })).toBeInTheDocument();
  });

  it("SMO-FR-RVAV: no drag starts, and no organization edit is applied or saved", async () => {
    mockCanvasRect({ width: 1200, height: 800 });
    const { store, ops, container } = await renderMap({ index: openspecIndex() });
    setView(store, { level: 2 });
    const before = store.snapshot().index;
    fireEvent.pointerDown(nodeButton("auth#1 auth requirement 1"), {
      button: 0,
      pointerId: 1,
      clientX: 10,
      clientY: 10,
    });
    fireEvent.pointerMove(window, { pointerId: 1, clientX: 400, clientY: 300 });
    expect(container.querySelector(".smap-ghost")).toBeNull();
    fireEvent.pointerUp(window, { pointerId: 1, clientX: 400, clientY: 300 });
    expect(store.snapshot().index).toBe(before);

    let applied = true;
    act(() => {
      applied = store.dispatch({
        kind: "create",
        parentId: null,
        node: { id: "n-1", label: "New", summary: "" },
      });
    });
    expect(applied).toBe(false);
    expect(ops.saveOrganization).not.toHaveBeenCalled();
  });

  it("SMD-FR-YQSR: New draft is offered on a group and on a capability, and a planned chip is placed but not dragged", async () => {
    mockCanvasRect({ width: 1200, height: 800 });
    const { store, ops, onNewDraft, container } = await renderMap({ index: openspecIndex() });
    act(() => store.select({ kind: "index", id: "group:billing" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: /New draft/ }));
    expect(onNewDraft).toHaveBeenLastCalledWith("group:billing");
    act(() => store.select({ kind: "index", id: "capability:auth" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: /New draft/ }));
    expect(onNewDraft).toHaveBeenLastCalledWith("capability:auth");

    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "capability:auth", draft: { draftId: "dr", name: "Plan" } });
    });
    expect(ops.attachDraft).toHaveBeenCalledWith({ draftId: "dr", nodeId: "capability:auth" });
    setView(store, { level: 2 });
    fireEvent.pointerDown(nodeButton("draft Plan"), { button: 0, pointerId: 1, clientX: 10, clientY: 10 });
    fireEvent.pointerMove(window, { pointerId: 1, clientX: 400, clientY: 300 });
    expect(container.querySelector(".smap-ghost")).toBeNull();
    fireEvent.pointerUp(window, { pointerId: 1 });
    expect(store.tree()!.plannedParent.get("dr")).toBe("capability:auth");
  });
});
