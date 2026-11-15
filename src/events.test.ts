import { beforeEach, describe, expect, it, vi } from "vitest";

type Callback = (event: { payload: unknown }) => void;
const callbacks = new Map<string, Callback>();

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(() => Promise.resolve()),
  listen: vi.fn((name: string, callback: Callback) => {
    callbacks.set(name, callback);
    return Promise.resolve(() => {});
  }),
}));

import { setWindowProjectId } from "./api/invoke";
import { onExportRequestChanged, onOperationProgress, onProjectTreeChanged } from "./events";
import { onNewDraftProposalsChanged } from "./newDraftEvents";

function deliver(name: string, payload: unknown) {
  const callback = callbacks.get(name);
  if (!callback) throw new Error(`no listener for ${name}`);
  callback({ payload });
}

describe("project event listeners (PWN-FR-AINZ)", () => {
  beforeEach(() => {
    callbacks.clear();
    setWindowProjectId("project-a");
  });

  it("applies an event of its own project", async () => {
    const handler = vi.fn();
    await onProjectTreeChanged(handler);
    deliver("project-tree-changed", { projectId: "project-a", changed: 1 });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("ignores an event of another project", async () => {
    const handler = vi.fn();
    await onProjectTreeChanged(handler);
    deliver("project-tree-changed", { projectId: "project-b", changed: 1 });
    expect(handler).not.toHaveBeenCalled();
  });

  it("ignores the progress of another project", async () => {
    const handler = vi.fn();
    await onOperationProgress(handler);
    deliver("operation-progress", { projectId: "project-b", id: "1" });
    expect(handler).not.toHaveBeenCalled();
    deliver("operation-progress", { projectId: "project-a", id: "2" });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("applies an event that carries no project id", async () => {
    const handler = vi.fn();
    await onProjectTreeChanged(handler);
    deliver("project-tree-changed", { changed: 1 });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("applies every event in a window that holds no project", async () => {
    setWindowProjectId(null);
    const handler = vi.fn();
    await onProjectTreeChanged(handler);
    deliver("project-tree-changed", { projectId: "project-b" });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("ignores an export request of another project", async () => {
    const handler = vi.fn();
    await onExportRequestChanged(handler);
    deliver("export-request-changed", { projectId: "project-b", id: "r1" });
    expect(handler).not.toHaveBeenCalled();
    deliver("export-request-changed", { projectId: "project-a", id: "r2" });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("ignores the new draft proposals of another project", async () => {
    const handler = vi.fn();
    await onNewDraftProposalsChanged(handler);
    deliver("new-draft-proposals-changed", { projectId: "project-b", draftId: "d1" });
    expect(handler).not.toHaveBeenCalled();
    deliver("new-draft-proposals-changed", { projectId: "project-a", draftId: "d1" });
    expect(handler).toHaveBeenCalledTimes(1);
  });
});
