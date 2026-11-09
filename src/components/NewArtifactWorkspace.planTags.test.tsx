/**
 * The plan tags of a draft's action row
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-TXKN, NAW-FR-SGVS).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { PlanTags } from "./NewArtifactWorkspace/PlanTags";
import { resetLogBufferForTest } from "../logging";
import type { FlowItemMatch } from "../types";

const MATCHES: FlowItemMatch[] = [
  { flowId: "plans/payments.flow", flowName: "Payments v2", itemId: "t1", itemName: "Tokenize", kind: "task" },
  { flowId: "plans/unnamed.flow", flowName: "", itemId: "x1", itemName: "Decline text", kind: "defect" },
];

function answer(items: FlowItemMatch[] | Error) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "append_log_records") return Promise.resolve();
    if (cmd === "find_flow_items_for_draft") {
      return items instanceof Error ? Promise.reject(items) : Promise.resolve(items);
    }
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
}

describe("PlanTags", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    resetLogBufferForTest();
  });
  afterEach(cleanup);

  it("NAW-FR-TXKN: one tag per plan item names the Flow and the item with its kind glyph", async () => {
    answer(MATCHES);
    render(<PlanTags draftId="d1" />);
    const tags = await screen.findAllByTestId("draft-plan-tag");
    expect(tags).toHaveLength(2);
    expect(tags[0]).toHaveTextContent("☐ Payments v2 › Tokenize");
    expect(tags[0]).toHaveAccessibleName("Open Task Tokenize in the plan Payments v2");
    // A Flow with no name is named by its file.
    expect(tags[1]).toHaveTextContent("✱ unnamed.flow › Decline text");
    expect(invokeMock).toHaveBeenCalledWith("find_flow_items_for_draft", { draftId: "d1" });
  });

  it("NAW-FR-TXKN: activating a tag opens that Flow through the open route", async () => {
    answer(MATCHES);
    const open = vi.fn();
    render(<PlanTags draftId="d1" onOpenArtifact={open} />);
    fireEvent.click((await screen.findAllByTestId("draft-plan-tag"))[0]);
    expect(open).toHaveBeenCalledWith({
      id: "plans/payments.flow",
      name: "payments.flow",
      artifactType: "flow",
    });
  });

  it("NAW-FR-TXKN: the items are read again for each mount, so a tab made active again reads them anew", async () => {
    answer(MATCHES);
    const first = render(<PlanTags draftId="d1" />);
    await screen.findAllByTestId("draft-plan-tag");
    first.unmount();
    answer([MATCHES[0]]);
    render(<PlanTags draftId="d1" />);
    await waitFor(() => expect(screen.getAllByTestId("draft-plan-tag")).toHaveLength(1));
    expect(
      invokeMock.mock.calls.filter(([cmd]) => cmd === "find_flow_items_for_draft"),
    ).toHaveLength(2);
  });

  it("NAW-FR-SGVS: a draft that belongs to no plan item carries no tag and no placeholder", async () => {
    answer([]);
    const { container } = render(<PlanTags draftId="d1" />);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("find_flow_items_for_draft", { draftId: "d1" }),
    );
    expect(screen.queryByTestId("draft-plan-tag")).toBeNull();
    expect(container).toBeEmptyDOMElement();
  });

  it("NAW-FR-SGVS: a failed read shows no tag and is logged", async () => {
    answer(Object.assign(new Error("boom"), { code: "no_project_open" }));
    const { container } = render(<PlanTags draftId="d1" />);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("find_flow_items_for_draft", { draftId: "d1" }),
    );
    expect(container).toBeEmptyDOMElement();
  });
});
