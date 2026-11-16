/**
 * The reveal of a Map thread (`SMP-specification-map.md` SMP-FR-QHBV,
 * `CVP-conversation-presentation.md` CVP-FR-06 and CVP-FR-54,
 * `CMP-comments-panel.md` CMP-FR-10).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) =>
    cmd === "append_log_records" ? Promise.resolve() : Promise.reject(new Error(`unexpected command ${cmd}`)),
}));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { consumePendingFocus, registerSurface, resetDiscussionFocus } from "../../state/discussionFocus";
import type { EditSessionStore } from "../../state/editSessions";
import { SpecMapSessionStore } from "../../state/specMap/session";
import { ITEM_NODE, SPEC_NODE } from "../../test/mapPlanFixtures";
import type { Tab } from "../../types";
import { createDiscussionActions } from "./discussionActions";

const MAP = { kind: "map" } as const;

let specMap: SpecMapSessionStore;
let openMap = vi.fn<() => void>();
let setTabs = vi.fn<(update: unknown) => void>();

function shell() {
  const tabsRef = { current: [] as Tab[] };
  setTabs = vi.fn<(update: unknown) => void>();
  openMap = vi.fn<() => void>();
  return createDiscussionActions({
    tabsRef,
    setTabs,
    setActiveTab: vi.fn(),
    activateTab: vi.fn(),
    openArtifact: vi.fn(),
    openDraft: vi.fn(),
    openMap,
    specMap,
    sessions: { update: vi.fn() } as unknown as EditSessionStore,
    setFocusThread: vi.fn(),
  });
}

beforeEach(() => {
  resetDiscussionFocus();
  specMap = new SpecMapSessionStore();
});

describe("a reveal of a Map thread (SMP-FR-QHBV, CVP-FR-06, CVP-FR-54, CMP-FR-10)", () => {
  it("SMP-FR-QHBV: opens or focuses the Map tab, shows the Discussion column, and selects the thread", () => {
    specMap.setDiscussionVisible(false);
    specMap.discuss(SPEC_NODE);
    const actions = shell();
    const outcome = actions.revealDiscussion({
      discussionId: "t2",
      target: MAP,
      fragmentTarget: null,
      ownerLabel: "Decline message",
      subject: "Discussion",
      mapNode: ITEM_NODE,
    });
    expect(outcome).toBe("owner");
    expect(openMap).toHaveBeenCalledTimes(1);
    expect(specMap.snapshot().discussion).toMatchObject({
      visible: true,
      threadId: "t2",
      composing: false,
      showResolved: false,
    });
    // CVP-FR-54: a Map thread opens no conversation tab.
    expect(setTabs).not.toHaveBeenCalled();
  });

  it("SMP-FR-QHBV: a resolved Map thread is selected with the resolved disclosure expanded", () => {
    const actions = shell();
    actions.revealDiscussion({
      discussionId: "t1",
      target: MAP,
      fragmentTarget: null,
      resolved: true,
      ownerLabel: "Map",
      subject: "Discussion",
    });
    expect(openMap).toHaveBeenCalledTimes(1);
    expect(specMap.snapshot().discussion).toMatchObject({ threadId: "t1", showResolved: true });
  });

  it("SMP-FR-QHBV: a reveal with no discussion id only shows the column", () => {
    specMap.setDiscussionVisible(false);
    specMap.selectDiscussionThread("t9");
    const actions = shell();
    actions.revealDiscussion({ target: MAP, fragmentTarget: null, ownerLabel: "Map", subject: "Discussion" });
    expect(openMap).toHaveBeenCalledTimes(1);
    expect(specMap.snapshot().discussion).toMatchObject({ visible: true, threadId: "t9" });
  });

  it("CVP-FR-06: holds the focus request for the surface that mounts with the Map tab", () => {
    const actions = shell();
    actions.revealDiscussion({
      discussionId: "t1",
      target: MAP,
      fragmentTarget: null,
      ownerLabel: "Map",
      subject: "Discussion",
    });
    expect(consumePendingFocus("t1")).toBe("discussion");
  });

  it("CMP-FR-10: a thread already shown in the column is focused there, and the Map tab is not opened again", () => {
    const focus = vi.fn();
    registerSurface("t1", "column", focus);
    const actions = shell();
    const outcome = actions.revealDiscussion({
      discussionId: "t1",
      target: MAP,
      fragmentTarget: null,
      ownerLabel: "Map",
      subject: "Discussion",
    });
    expect(outcome).toBe("focused");
    expect(focus).toHaveBeenCalled();
    expect(openMap).not.toHaveBeenCalled();
  });
});
