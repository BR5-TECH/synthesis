import { beforeEach, describe, expect, it, vi } from "vitest";
import { waitFor } from "@testing-library/react";

import { resetLogBufferForTest } from "../../logging";
import type { EditSessionStore } from "../../state/editSessions";
import type { FlowSessionStore } from "../../state/flowSessions";
import type { SpecMapSessionStore } from "../../state/specMap/session";

let modeActive: boolean | "fails" = true;
vi.mock("@tauri-apps/api/core", () => ({
  invoke: async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "get_openspec_status") {
      if (modeActive === "fails") throw new Error("no project open");
      return { setting: "auto", usesOpenspec: true, active: modeActive, cli: { state: "not_found" } };
    }
    throw new Error(`unexpected command ${cmd}`);
  },
}));

import { createOpenActions } from "./openActions";
import { SPEC_MAP_TAB } from "./tabRecords";

beforeEach(() => {
  resetLogBufferForTest();
  modeActive = true;
});

function actions(activeTab = "dashboard") {
  const deps = {
    tabs: [],
    setTabs: vi.fn(),
    activateTab: vi.fn(),
    sessions: { openTab: vi.fn() } as unknown as EditSessionStore,
    flows: { openTab: vi.fn() } as unknown as FlowSessionStore,
    setPanelSurface: vi.fn(),
    setRevealArtifactId: vi.fn(),
    showBottom: vi.fn(),
    specMap: { selectByPath: vi.fn() } as unknown as SpecMapSessionStore,
    activeTab,
    openDocumentHit: vi.fn(),
    openDraftHit: vi.fn(),
    openDocumentByProjectPath: vi.fn(),
    openWorktreeChange: vi.fn(),
  };
  return { deps, open: createOpenActions(deps) };
}

describe("a Project panel click on a file of a change", () => {
  it("LIB-FR-OKIZ: opens the change workspace in the active worktree with that file selected, and no Editor tab", async () => {
    const { deps, open } = actions();
    open.activateLibraryFile({
      id: "openspec/changes/add-logout/specs/auth/spec.md",
      name: "spec.md",
      artifactType: "spec",
    });
    await waitFor(() =>
      expect(deps.openWorktreeChange).toHaveBeenCalledWith("add-logout", "specs/auth/spec.md"),
    );
    expect(deps.setTabs).not.toHaveBeenCalled();
    expect(deps.sessions.openTab).not.toHaveBeenCalled();
  });

  it.each([
    ["a file of an archived change", "openspec/changes/archive/2025-01-01-old/proposal.md"],
    ["a change folder whose name is not valid", "openspec/changes/Old_One/proposal.md"],
    ["a live spec", "openspec/specs/auth/spec.md"],
  ])("LIB-FR-03 / LIB-FR-OKIZ: %s opens in its natural surface", (_why, path) => {
    const { deps, open } = actions();
    open.activateLibraryFile({ id: path, name: "x.md", artifactType: "spec" });
    expect(deps.openWorktreeChange).not.toHaveBeenCalled();
    expect(deps.setTabs).toHaveBeenCalled();
    expect(deps.activateTab).toHaveBeenCalledWith(`art:${path}`);
  });

  it("LIB-FR-OKIZ / LIB-FR-SJDC: while the Map tab is active, a Spec file of a change selects on the map", () => {
    const { deps, open } = actions(SPEC_MAP_TAB.id);
    open.activateLibraryFile({
      id: "openspec/changes/add-logout/specs/auth/spec.md",
      name: "spec.md",
      artifactType: "spec",
    });
    expect(deps.specMap.selectByPath).toHaveBeenCalled();
    expect(deps.openWorktreeChange).not.toHaveBeenCalled();
  });

  it("LIB-FR-OKIZ: outside the map case, a file of a change opens the workspace while the Map tab is active", async () => {
    const { deps, open } = actions(SPEC_MAP_TAB.id);
    open.activateLibraryFile({
      id: "openspec/changes/add-logout/proposal.md",
      name: "proposal.md",
      artifactType: "doc",
    });
    await waitFor(() => expect(deps.openWorktreeChange).toHaveBeenCalledWith("add-logout", "proposal.md"));
  });

  it.each([
    ["OpenSpec mode is off", false],
    ["the status cannot be read", "fails"],
  ] as const)("LIB-FR-OKIZ, OSM-FR-RMUD: while %s, a file of a change opens in its natural surface", async (_why, mode) => {
    modeActive = mode;
    const { deps, open } = actions();
    open.activateLibraryFile({
      id: "openspec/changes/add-logout/proposal.md",
      name: "proposal.md",
      artifactType: "spec",
    });
    await waitFor(() => expect(deps.activateTab).toHaveBeenCalledWith("art:openspec/changes/add-logout/proposal.md"));
    expect(deps.openWorktreeChange).not.toHaveBeenCalled();
  });
});
