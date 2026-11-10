/**
 * The shell's half of a change discussion: the location of an open in a
 * stream's working copy (OCW-FR-PLDG), the reveal of a change owner
 * (CVP-FR-TFRD), and the target key (CVP-FR-FWOB).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  },
}));

import { resetLogBufferForTest } from "../../logging";
import { discussionColumnState, changeColumn, resetDiscussionColumns, setColumnHidden } from "../../state/discussionColumn";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import {
  configureOpenSpecChangeRoutes,
  resetChangeDiscussionRequests,
  takeChangeDiscussion,
} from "../../state/openspec/changeRoutes";
import { discussionTargetKey, type Tab, type WorktreeEntry } from "../../types";
import type { EditSessionStore } from "../../state/editSessions";
import { createDiscussionActions } from "./discussionActions";
import { worktreeChangeLocation } from "./openspecChangeActions";

const TARGET = { kind: "change", streamId: "s-1", change: "add-logout" } as const;

beforeEach(() => {
  resetLogBufferForTest();
  resetDiscussionColumns();
  resetDiscussionFocus();
  resetChangeDiscussionRequests();
  configureOpenSpecChangeRoutes(null);
});

function worktree(over: Partial<WorktreeEntry> = {}): WorktreeEntry {
  return {
    path: "/repo/.wt/a",
    name: "a",
    branch: "work/a",
    headShortHash: "abc1234",
    isDetached: false,
    isActive: true,
    isPrimary: false,
    isMissing: false,
    ...over,
  };
}

describe("an open in the active worktree", () => {
  it("OCW-FR-PLDG: a worktree that is a stream's working copy opens that stream's location, with its name", () => {
    expect(
      worktreeChangeLocation(worktree({ stream: { streamId: "s-9", streamName: "auth-work" } })),
    ).toEqual({ location: { kind: "stream", streamId: "s-9" }, streamName: "auth-work" });
  });

  it("OCW-FR-PLDG: any other worktree, or none known, opens the active worktree location", () => {
    expect(worktreeChangeLocation(worktree())).toEqual({ location: { kind: "active_worktree" } });
    expect(worktreeChangeLocation(null)).toEqual({ location: { kind: "active_worktree" } });
  });
});

describe("the target key of a change owner", () => {
  it("CVP-FR-FWOB: is change:<stream-id>:<change>", () => {
    expect(discussionTargetKey(TARGET)).toBe("change:s-1:add-logout");
  });
});

describe("a reveal of a change discussion", () => {
  function shell() {
    const tabsRef = { current: [] as Tab[] };
    const open = vi.fn();
    configureOpenSpecChangeRoutes({ open });
    const actions = createDiscussionActions({
      tabsRef,
      setTabs: vi.fn(),
      setActiveTab: vi.fn(),
      activateTab: vi.fn(),
      openArtifact: vi.fn(),
      openDraft: vi.fn(),
      sessions: { update: vi.fn() } as unknown as EditSessionStore,
      setFocusThread: vi.fn(),
    });
    return { actions, open };
  }

  it("CVP-FR-TFRD: opens or focuses the change workspace on its stream, shows its column, and selects the discussion", () => {
    const { actions, open } = shell();
    setColumnHidden(changeColumn("s-1", "add-logout"), true);
    const outcome = actions.revealDiscussion({
      discussionId: "t2",
      target: TARGET,
      fragmentTarget: { owner: TARGET, path: "specs/auth/spec.md", start: 0, end: 2, quote: "##" },
      ownerLabel: "add-logout",
      subject: "##",
    });
    expect(outcome).toBe("owner");
    expect(open).toHaveBeenCalledWith({ kind: "stream", streamId: "s-1" }, "add-logout", {
      file: "specs/auth/spec.md",
    });
    expect(discussionColumnState(changeColumn("s-1", "add-logout")).hidden).toBe(false);
    expect(takeChangeDiscussion(discussionTargetKey(TARGET))).toBe("t2");
  });

  it("CVP-FR-TFRD: a whole-change discussion opens the tab with no file named", () => {
    const { actions, open } = shell();
    actions.revealDiscussion({
      discussionId: "t1",
      target: TARGET,
      fragmentTarget: null,
      ownerLabel: "add-logout",
      subject: "Discussion",
    });
    expect(open).toHaveBeenCalledWith({ kind: "stream", streamId: "s-1" }, "add-logout", { file: null });
  });
});
