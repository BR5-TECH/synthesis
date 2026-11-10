/**
 * The plan tags of a draft's action row
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-TXKN, NAW-FR-SGVS).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { PlanTags } from "./NewArtifactWorkspace/PlanTags";
import { FLUSH_DELAY_MS, resetLogBufferForTest } from "../logging";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetToasts } from "../state/toasts";
import { makeStubs, renderWorkspace } from "../test/newArtifactFixtures";
import { resetMermaidMock } from "../test/mermaidMock";
import { resetMermaidForTest } from "./markdownMermaidRender";
import type { FlowItemMatch } from "../types";

const MATCHES: FlowItemMatch[] = [
  { flowId: "plans/payments.flow", flowName: "Payments v2", itemId: "t1", itemName: "Tokenize", kind: "task" },
  { flowId: "plans/unnamed.flow", flowName: "", itemId: "x1", itemName: "Decline text", kind: "defect" },
];

function answer(items: FlowItemMatch[] | Error | Promise<FlowItemMatch[]>) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "append_log_records") return Promise.resolve();
    if (cmd === "find_flow_items_for_draft") {
      return items instanceof Error ? Promise.reject(items) : Promise.resolve(items);
    }
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
}

/** The records of every log flush, as one text. */
const loggedText = () =>
  JSON.stringify(invokeMock.mock.calls.filter(([cmd]) => cmd === "append_log_records"));

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetToasts();
  clearAllDiscussionSessions();
  resetDraftDiscussions();
  resetMermaidForTest();
  resetMermaidMock();
});
afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe("PlanTags", () => {
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
    let settle!: (items: FlowItemMatch[]) => void;
    const read = new Promise<FlowItemMatch[]>((res) => (settle = res));
    answer(read);
    const { container } = render(<PlanTags draftId="d1" />);
    expect(invokeMock).toHaveBeenCalledWith("find_flow_items_for_draft", { draftId: "d1" });
    // The answer lands before the assertion, so an empty row is the answer's.
    await act(async () => {
      settle([]);
      await read;
    });
    expect(screen.queryByTestId("draft-plan-tag")).toBeNull();
    expect(container).toBeEmptyDOMElement();
  });

  it("NAW-FR-SGVS: a failed read shows no tag and is logged by its code", async () => {
    vi.useFakeTimers();
    answer(Object.assign(new Error("the project at /home/me is gone"), { code: "no_project_open" }));
    const { container } = render(<PlanTags draftId="d1" />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(FLUSH_DELAY_MS);
    });
    expect(container).toBeEmptyDOMElement();
    expect(loggedText()).toContain("plan items of a draft could not be read");
    expect(loggedText()).toContain("no_project_open");
    expect(loggedText()).not.toContain("/home/me");
  });
});

describe("the plan tags in the New Artifact tab (NAW-FR-TXKN)", () => {
  const { stub } = makeStubs(invokeMock);

  it("NAW-FR-TXKN: the tab's action row carries the tag, and activating it calls the shell's open route with the Flow", async () => {
    stub();
    const base = invokeMock.getMockImplementation();
    invokeMock.mockImplementation((cmd: string, args: unknown) =>
      cmd === "find_flow_items_for_draft" ? Promise.resolve([MATCHES[0]]) : base?.(cmd, args),
    );
    const onOpenArtifact = vi.fn();
    renderWorkspace(undefined, { onOpenArtifact });

    const row = await screen.findByTestId("draft-workspace-chrome");
    const tag = await within(row).findByTestId("draft-plan-tag");
    expect(tag).toHaveTextContent("Payments v2 › Tokenize");
    fireEvent.click(tag);
    expect(onOpenArtifact).toHaveBeenCalledWith(
      expect.objectContaining({ id: "plans/payments.flow", artifactType: "flow" }),
    );
    expect(invokeMock).toHaveBeenCalledWith("find_flow_items_for_draft", { draftId: "d1" });
  });
});
