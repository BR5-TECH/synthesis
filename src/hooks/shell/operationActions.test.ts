import { beforeEach, describe, expect, it, vi } from "vitest";

import { createOperationActions } from "./operationActions";
import type { Discussion, Operation } from "../../types";

// STB-FR-RWPD, STB-FR-DNLC: what activating a status bar row opens, by the
// owning surface's own route.

const readDiscussion = vi.fn();
vi.mock("../../api", () => ({
  readDiscussion: (...args: unknown[]) => readDiscussion(...args),
}));

const deps = {
  openGraduationRun: vi.fn(),
  revealDiscussion: vi.fn(),
  openGitBranch: vi.fn(),
};

function operation(overrides: Partial<Operation> = {}): Operation {
  return {
    id: "op-1",
    kind: "x",
    label: "Something",
    state: "running",
    sequence: 1,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("activating an operation (STB-FR-RWPD)", () => {
  it("STB-FR-RWPD, GRU-FR-VQJB: a graduation run opens the Runs panel's graduation section on that run", () => {
    const { activateOperation } = createOperationActions(deps);
    const opened = activateOperation(
      operation({ activation: { type: "graduation_run", runId: "g-42" } }),
    );
    expect(opened).toBe(true);
    expect(deps.openGraduationRun).toHaveBeenCalledWith("g-42");
    expect(deps.revealDiscussion).not.toHaveBeenCalled();
    expect(deps.openGitBranch).not.toHaveBeenCalled();
  });

  it("STB-FR-RWPD, GIT-FR-FZMS: a Git push opens the Git panel on that branch", () => {
    const { activateOperation } = createOperationActions(deps);
    expect(
      activateOperation(
        operation({ activation: { type: "git_push", branch: "feature/x" } }),
      ),
    ).toBe(true);
    expect(deps.openGitBranch).toHaveBeenCalledWith("feature/x");
    expect(deps.openGraduationRun).not.toHaveBeenCalled();
  });

  it("STB-FR-RWPD, CVP-FR-06: a discussion is read by its id and revealed through the one route", async () => {
    const discussion: Discussion = {
      id: "d-9",
      target: { kind: "artifact", artifactId: "specs/a.md" },
      fragmentTarget: null,
      comments: [],
      locked: false,
      resolved: false,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    };
    readDiscussion.mockResolvedValue(discussion);
    const { activateOperation } = createOperationActions(deps);
    expect(
      activateOperation(
        operation({ activation: { type: "discussion", discussionId: "d-9" } }),
      ),
    ).toBe(true);
    await vi.waitFor(() => expect(deps.revealDiscussion).toHaveBeenCalledTimes(1));
    expect(readDiscussion).toHaveBeenCalledWith("d-9");
    expect(deps.revealDiscussion.mock.calls[0][0]).toMatchObject({
      discussionId: "d-9",
      target: { kind: "artifact", artifactId: "specs/a.md" },
      ownerLabel: "specs/a.md",
    });
  });

  it("STB-FR-RWPD: a discussion that cannot be read reveals nothing and does not throw", async () => {
    readDiscussion.mockRejectedValue("discussion_not_found");
    const { activateOperation } = createOperationActions(deps);
    expect(
      activateOperation(
        operation({ activation: { type: "discussion", discussionId: "gone" } }),
      ),
    ).toBe(true);
    await vi.waitFor(() => expect(readDiscussion).toHaveBeenCalled());
    await Promise.resolve();
    expect(deps.revealDiscussion).not.toHaveBeenCalled();
  });
});

describe("an operation with no destination (STB-FR-DNLC)", () => {
  it("STB-FR-DNLC: opens nothing, whatever its kind and label say", () => {
    const { activateOperation } = createOperationActions(deps);
    for (const [kind, label] of [
      ["graduation", "Graduating run g-1"],
      ["agent", "@Helga is thinking about Push button…"],
      ["git", "Pushing feature/x"],
    ]) {
      expect(activateOperation(operation({ kind, label }))).toBe(false);
    }
    expect(deps.openGraduationRun).not.toHaveBeenCalled();
    expect(deps.revealDiscussion).not.toHaveBeenCalled();
    expect(deps.openGitBranch).not.toHaveBeenCalled();
    expect(readDiscussion).not.toHaveBeenCalled();
  });

  it("STB-FR-RWPD: an unknown destination type opens nothing", () => {
    const { activateOperation } = createOperationActions(deps);
    expect(
      activateOperation(
        operation({ activation: { type: "elsewhere", id: "1" } as never }),
      ),
    ).toBe(false);
    expect(deps.openGraduationRun).not.toHaveBeenCalled();
  });
});
