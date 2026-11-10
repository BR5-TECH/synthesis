import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { findItem } from "../state/flowDocument";
import { resetToasts, visibleToasts } from "../state/toasts";
import { callsOf, clearFlowStores, flowBackend, planOf, renderFlow } from "../test/flowCanvasHarness";
import { HLD_ID, itemEl, panel, selectItem, SKILL_ID } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { item, plan, samplePlan } from "../test/flowPlanFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../test/flowMocks")).listen,
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
});

afterEach(() => {
  cleanup();
  clearFlowStores();
});

const offered = (select: HTMLElement) =>
  within(select)
    .getAllByRole("option")
    .map((o) => (o as HTMLOptionElement).value)
    .filter((v) => v !== "");

describe("item references (FLO-FR-10, FLO-FR-11, FLO-FR-12)", () => {
  it("FLO-FR-10: the picker offers every artifact that is not a Flow, a skill only by its SKILL.md, and none the item has", async () => {
    const b = flowBackend(plan([item("f1", "feature"), item("t1", "task", { parentId: "f1", artifactIds: ["prompts/gather-repo.md"], position: { x: 20, y: 60 } })]));
    await renderFlow(b);

    selectItem("t1");
    const picker = within(panel()).getByRole("combobox", { name: "Add reference" });
    expect(offered(picker).sort()).toEqual([SKILL_ID, HLD_ID, "specifications/FLO-flow.md"].sort());
  });

  it("FLO-FR-10: a reference is added from the picker and removed on its own", async () => {
    const b = flowBackend(plan([item("f1", "feature"), item("t1", "task", { parentId: "f1", artifactIds: ["prompts/gather-repo.md"], position: { x: 20, y: 60 } })]));
    const { flows } = await renderFlow(b);

    selectItem("t1");
    await userEvent.selectOptions(within(panel()).getByRole("combobox", { name: "Add reference" }), SKILL_ID);
    expect(findItem(planOf(flows), "t1")!.artifactIds).toEqual(["prompts/gather-repo.md", SKILL_ID]);
    await userEvent.click(within(panel()).getByRole("button", { name: "Remove reference gather-repo.md" }));
    expect(findItem(planOf(flows), "t1")!.artifactIds).toEqual([SKILL_ID]);
  });

  it("FLO-FR-11: a resolved reference shows the display name and the type chip; one that does not resolve names its id and stays", async () => {
    const b = flowBackend(plan([item("f1", "feature"), item("t1", "task", { parentId: "f1", artifactIds: [SKILL_ID, "gone/old.md"], position: { x: 20, y: 60 } })]));
    const { flows } = await renderFlow(b);

    const card = itemEl("t1");
    const open = within(card).getByRole("button", { name: "Open Code review" });
    expect(open).toHaveTextContent("SKL");
    expect(within(card).getByTestId("flow-ref-unresolved")).toHaveTextContent("unresolved: gone/old.md");
    expect(findItem(planOf(flows), "t1")!.artifactIds).toEqual([SKILL_ID, "gone/old.md"]);
    expect(flows.get("plans/payments.flow")!.dirty).toBe(false);
  });

  it("FLO-FR-12: a resolved reference opens through \"open artifact by id\" in its natural surface; an unresolved one opens nothing", async () => {
    const b = flowBackend(plan([item("f1", "feature"), item("t1", "task", { parentId: "f1", artifactIds: [SKILL_ID, "gone/old.md"], position: { x: 20, y: 60 } })]));
    const { onOpenArtifact } = await renderFlow(b);

    await userEvent.click(within(itemEl("t1")).getByRole("button", { name: "Open Code review" }));
    await waitFor(() => expect(onOpenArtifact).toHaveBeenCalledTimes(1));
    expect(callsOf(b, "open_artifact_by_id")).toEqual([{ id: SKILL_ID }]);
    expect(onOpenArtifact).toHaveBeenCalledWith({ id: SKILL_ID, name: "Code review", artifactType: "skill" });
    await userEvent.click(within(itemEl("t1")).getByTestId("flow-ref-unresolved"));
    expect(callsOf(b, "open_artifact_by_id")).toHaveLength(1);
  });
});

describe("opening a reference (FLO-FR-12)", () => {
  const OTHER_FLOW = "flows/other.flow";
  const withRef = (id: string) =>
    plan([item("f1", "feature"), item("t1", "task", { parentId: "f1", artifactIds: [id], position: { x: 20, y: 60 } })]);

  it("FLO-FR-12: a reference to another Flow opens it in a Flow tab", async () => {
    const b = flowBackend(withRef(OTHER_FLOW));
    const { onOpenArtifact } = await renderFlow(b);

    await userEvent.click(within(itemEl("t1")).getByRole("button", { name: "Open other.flow" }));
    await waitFor(() => expect(onOpenArtifact).toHaveBeenCalledTimes(1));
    expect(callsOf(b, "open_artifact_by_id")).toEqual([{ id: OTHER_FLOW }]);
    expect(onOpenArtifact).toHaveBeenCalledWith(expect.objectContaining({ id: OTHER_FLOW, artifactType: "flow" }));
  });

  it("FLO-FR-12: a refused open opens nothing and shows the error", async () => {
    const b = flowBackend(withRef(HLD_ID));
    b.on.open_artifact_by_id = () => Promise.reject({ code: "not_found", message: "The file is not in the project." });
    const { flows, onOpenArtifact } = await renderFlow(b);

    await userEvent.click(within(itemEl("t1")).getByRole("button", { name: "Open payments-hld.md" }));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(visibleToasts()[0]).toMatchObject({
      level: "Error",
      title: "payments-hld.md not opened",
      body: "The file is not in the project.",
    });
    expect(onOpenArtifact).not.toHaveBeenCalled();
    expect(flows.get("plans/payments.flow")!.dirty).toBe(false);
  });
});

describe("Flow references (FLO-FR-YDWT)", () => {
  it("FLO-FR-YDWT: a Flow with no reference shows Add HLD in the band, which adds the HLD", async () => {
    const b = flowBackend(samplePlan());
    const { flows } = await renderFlow(b);

    const add = within(screen.getByTestId("flow-band")).getByRole("combobox", { name: "Add HLD" });
    await userEvent.selectOptions(add, HLD_ID);
    expect(planOf(flows).references).toEqual([HLD_ID]);
    expect(within(screen.getByTestId("flow-band")).queryByRole("combobox", { name: "Add HLD" })).toBeNull();
    expect(within(screen.getByTestId("flow-band")).getByRole("combobox", { name: "Add Flow reference" })).toBeInTheDocument();
  });

  it("FLO-FR-YDWT, FLO-FR-11, FLO-FR-12: the band's references render as rows that open, and each is removed on its own", async () => {
    const b = flowBackend(plan([], [], { references: [HLD_ID, "gone/spec.md"] }));
    const { flows, onOpenArtifact } = await renderFlow(b);

    const band = screen.getByTestId("flow-references");
    expect(within(band).getByRole("button", { name: "Open payments-hld.md" })).toHaveTextContent("DOC");
    expect(within(band).getByTestId("flow-ref-unresolved")).toHaveTextContent("gone/spec.md");
    await userEvent.click(within(band).getByRole("button", { name: "Open payments-hld.md" }));
    await waitFor(() => expect(onOpenArtifact).toHaveBeenCalledWith(expect.objectContaining({ id: HLD_ID })));
    await userEvent.click(within(band).getByRole("button", { name: "Remove reference gone/spec.md" }));
    expect(planOf(flows).references).toEqual([HLD_ID]);
  });

  it("FLO-FR-YDWT: the band's picker offers no Flow and no reference the Flow already has", async () => {
    const b = flowBackend(plan([], [], { references: [HLD_ID] }));
    await renderFlow(b);

    const picker = within(screen.getByTestId("flow-band")).getByRole("combobox", { name: "Add Flow reference" });
    expect(offered(picker)).not.toContain(HLD_ID);
    expect(offered(picker)).not.toContain("flows/other.flow");
    expect(offered(picker)).toContain("specifications/FLO-flow.md");
  });
});
