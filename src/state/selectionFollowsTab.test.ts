import { afterEach, describe, expect, it } from "vitest";

import {
  followTargetForTab,
  resetSelectionFollowsTab,
  selectionFollowsTabEnabled,
  setSelectionFollowsTab,
} from "./selectionFollowsTab";
import type { Tab } from "../types";
import { DEFAULT_APP_PREFERENCES } from "./appPreferences";

afterEach(() => resetSelectionFollowsTab());

/**
 * SNV-FR-66: which panel a tab selects in, and by what identity.
 *
 * The mapping is pure and is the whole of the decision, so it is tested here
 * rather than through the shell — the shell's own tests are about the *trigger*.
 */
describe("followTargetForTab (SNV-FR-66)", () => {
  it("routes a New Artifact tab to Drafts by the draft's id, not its label", () => {
    const tab: Tab = {
      id: "draft:d1",
      // The label is the draft's name and moves when it is renamed (NAW-FR-04);
      // the id does not, which is why the target is built from the id.
      label: "Agent personas",
      kind: "draft",
      draftId: "d1",
    };
    expect(followTargetForTab(tab)).toEqual({ panel: "drafts", id: "d1" });
  });

  it("routes an Editor tab and a Flow tab to Project by the tab's file", () => {
    const editor: Tab = {
      id: "art:specifications/ui/LIB-library.md",
      label: "LIB-library.md",
      kind: "editor",
      artifactId: "specifications/ui/LIB-library.md",
    };
    const flow: Tab = {
      id: "art:flows/build.flow",
      label: "build.flow",
      kind: "flow",
      artifactId: "flows/build.flow",
    };
    expect(followTargetForTab(editor)).toEqual({
      panel: "library",
      id: "specifications/ui/LIB-library.md",
    });
    expect(followTargetForTab(flow)).toEqual({
      panel: "library",
      id: "flows/build.flow",
    });
  });

  it("routes a Diff tab to Changes by the file half of its identifying pair", () => {
    const tab: Tab = {
      id: "diff:src/App.tsx@uncommitted",
      label: "Diff: App.tsx",
      kind: "diff",
      artifactId: "src/App.tsx",
      diff: {
        path: "src/App.tsx",
        name: "App.tsx",
        scope: { kind: "path", path: "src/App.tsx" },
        comparisonLabel: "Uncommitted",
      },
    };
    // The comparison half names which change set the tab is about, and a reveal
    // never changes the panel's own (CHG-FR-54) — so it is deliberately not part
    // of the target.
    expect(followTargetForTab(tab)).toEqual({
      panel: "changes",
      id: "src/App.tsx",
    });
  });

  it("SNV-FR-66: every other kind of tab selects nowhere", () => {
    const nowhere: Tab[] = [
      { id: "dashboard", label: "Dashboard" },
      { id: "global-settings", label: "Global settings" },
      { id: "settings", label: "Project settings" },
      {
        id: "search:literal_insensitive:x",
        label: "Search: x",
        kind: "search",
        search: { query: "x", mode: "literal_insensitive" },
      },
      { id: "conv:c1", label: "Review", kind: "conversation", threadId: "c1" },
      { id: "history:a.md@v2", label: "a.md @ v2" },
    ];
    for (const tab of nowhere) expect(followTargetForTab(tab)).toBeNull();
  });

  it("SNV-FR-67: a tab whose item cannot be identified names nothing", () => {
    // A Dashboard canned row opens an Editor tab with no backing project file,
    // so there is no node for the Project panel to reveal.
    expect(
      followTargetForTab({ id: "art:Untitled", label: "Untitled", kind: "editor" }),
    ).toBeNull();
    expect(
      followTargetForTab({ id: "draft:?", label: "d", kind: "draft" }),
    ).toBeNull();
    expect(
      followTargetForTab({ id: "diff:?", label: "Diff: d", kind: "diff" }),
    ).toBeNull();
    expect(followTargetForTab(undefined)).toBeNull();
  });
});

describe("the follow gate (GLS-FR-28 / GSS-FR-33)", () => {
  it("is on before anything has been read, so an early activation still follows", () => {
    expect(selectionFollowsTabEnabled()).toBe(true);
  });

  it("agrees with the stored record's default, which is a separate copy of it", () => {
    // GSS-FR-33's default is written out four times — here, in
    // `DEFAULT_APP_PREFERENCES`, in the Rust `Default` impl, and in that
    // struct's serde field default. The two on this side are pinned together
    // here; the two on the Rust side are pinned to each other by
    // `selection_follows_tab_defaults_to_on_including_for_a_record_written_before_the_field`.
    // A copy that drifts turns the behaviour off for a whole class of user
    // without failing anything else.
    expect(DEFAULT_APP_PREFERENCES.selectionFollowsTab).toBe(true);
    expect(selectionFollowsTabEnabled()).toBe(
      DEFAULT_APP_PREFERENCES.selectionFollowsTab,
    );
  });

  it("takes the value it is set to, in both directions", () => {
    setSelectionFollowsTab(false);
    expect(selectionFollowsTabEnabled()).toBe(false);
    setSelectionFollowsTab(true);
    expect(selectionFollowsTabEnabled()).toBe(true);
  });
});
