/** The draft preview tab (`TAB-tabs.md` TAB-FR-CIGU, TAB-FR-OMWY). */
import { beforeEach, describe, expect, it } from "vitest";

import { createDraftPreviewActions, draftPreviewTabId } from "./draftPreviewActions";
import { resetLogBufferForTest } from "../../logging";
import type { Tab } from "../../types";

function strip(initial: Tab[]) {
  const state = { tabs: initial, active: initial[0]?.id ?? "" };
  const tabsRef = { current: state.tabs };
  const actions = createDraftPreviewActions({
    tabsRef,
    setTabs: (next) => {
      state.tabs = typeof next === "function" ? next(state.tabs) : next;
      tabsRef.current = state.tabs;
    },
    setActiveTab: (next) => {
      state.active = typeof next === "function" ? next(state.active) : next;
    },
    activateTab: (id) => {
      state.active = id;
    },
  });
  return { state, actions };
}

const DRAFT_TAB: Tab = { id: "draft:d1", label: "relay", kind: "draft", draftId: "d1" };

beforeEach(() => resetLogBufferForTest());

describe("the draft preview tab", () => {
  it("TAB-FR-CIGU: one tab per proposal, focused when asked again, carrying no draft id", () => {
    const { state, actions } = strip([DRAFT_TAB]);
    actions.openDraftPreview("p1", "relay-auth");
    actions.openDraftPreview("p1", "relay-auth");
    const previews = state.tabs.filter((t) => t.kind === "draftPreview");
    expect(previews).toHaveLength(1);
    expect(previews[0]).toMatchObject({ id: draftPreviewTabId("p1"), label: "relay-auth", proposalId: "p1" });
    expect(previews[0].draftId).toBeUndefined();
    expect(previews[0].artifactId).toBeUndefined();
    expect(state.active).toBe(draftPreviewTabId("p1"));
  });

  it("TAB-FR-OMWY: closes with no prompt and moves focus to a neighbour", () => {
    const { state, actions } = strip([DRAFT_TAB]);
    actions.openDraftPreview("p1", "relay-auth");
    actions.closeDraftPreview("p1");
    expect(state.tabs.map((t) => t.id)).toEqual(["draft:d1"]);
    expect(state.active).toBe("draft:d1");
    // Closing a preview that is not open changes nothing.
    actions.closeDraftPreview("p2");
    expect(state.tabs.map((t) => t.id)).toEqual(["draft:d1"]);
  });
});
