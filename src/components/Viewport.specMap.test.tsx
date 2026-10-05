// The Viewport's routing of the Map tab to the map, and of the map's outward
// actions to the shell (`SMP-specification-map.md`).
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import { Viewport } from "./Viewport";
import { EditSessionStore } from "../state/editSessions";
import { DraftSessionStore } from "../state/draftSessions";
import { FlowSessionStore } from "../state/flowSessions";
import { SearchSessionStore } from "../state/searchSessions";
import { SPEC_MAP_TAB } from "../hooks/shell/tabRecords";
import { makeStore, setWindowWidth } from "../test/specMapRender";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

afterEach(cleanup);

async function renderMapTab() {
  setWindowWidth(1440);
  const { store } = makeStore();
  const onOpenArtifact = vi.fn();
  const onOpenDraft = vi.fn();
  const onNewDraftForMapNode = vi.fn();
  const onOverlayOpening = vi.fn();
  render(
    <Viewport
      activeTab={SPEC_MAP_TAB.id}
      activeT={SPEC_MAP_TAB}
      onOpenArtifact={onOpenArtifact}
      onOpenRuns={vi.fn()}
      onOpenGit={vi.fn()}
      sessions={new EditSessionStore()}
      flows={new FlowSessionStore()}
      drafts={new DraftSessionStore()}
      onActivateSearchHit={vi.fn()}
      searches={new SearchSessionStore()}
      onDraftChanged={vi.fn()}
      draftsRevision={0}
      onGraduationStarted={vi.fn()}
      onOpenRun={vi.fn()}
      onOpenDraft={onOpenDraft}
      onCloseTab={vi.fn()}
      onDraftRenamed={vi.fn()}
      specMap={store}
      onNewDraftForMapNode={onNewDraftForMapNode}
      onOverlayOpening={onOverlayOpening}
    />,
  );
  await waitFor(() => expect(store.snapshot().status).toBe("ready"));
  return { store, onOpenArtifact, onOpenDraft, onNewDraftForMapNode, onOverlayOpening };
}

const inspector = () => screen.getByRole("complementary", { name: "Inspector" });

describe("the Map tab in the viewport (SMP-FR-KQTD)", () => {
  it("SMP-FR-KQTD: a Map tab renders the map from the shell's session", async () => {
    const { store } = await renderMapTab();
    expect(screen.getByRole("toolbar", { name: "Map controls" })).toBeInTheDocument();
    expect(screen.getByTestId("spec-map-canvas")).toBeInTheDocument();
    expect(store.snapshot().index).not.toBeNull();
  });

  it("SMI-FR-GAJD: Open in editor reaches the shell's open route", async () => {
    const { store, onOpenArtifact } = await renderMapTab();
    act(() => store.select({ kind: "spec", code: "C1" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "Open in editor" }));
    expect(onOpenArtifact).toHaveBeenCalledWith(
      expect.objectContaining({ id: "specifications/ui/C1-c1.md", artifactType: "spec" }),
    );
  });

  it("SMD-FR-HVBE: New draft reaches the shell with the node it ran on", async () => {
    const { store, onNewDraftForMapNode } = await renderMapTab();
    act(() => store.select({ kind: "index", id: "f2" }));
    fireEvent.click(within(inspector()).getByRole("button", { name: "New draft" }));
    expect(onNewDraftForMapNode).toHaveBeenCalledWith("f2");
  });

  it("SMD-FR-XEPS: Open draft opens the draft by its id", async () => {
    const { store, onOpenDraft } = await renderMapTab();
    act(() => {
      store.dispatch({ kind: "attachDraft", nodeId: "g2", draft: { draftId: "d-7", name: "Plan" } });
      store.select({ kind: "planned", draftId: "d-7" });
    });
    fireEvent.click(within(inspector()).getByRole("button", { name: "Open draft" }));
    expect(onOpenDraft).toHaveBeenCalledWith("d-7");
  });

  it("SNV-FR-56: opening a map dialog closes the shell's other floating overlays", async () => {
    const { onOverlayOpening } = await renderMapTab();
    fireEvent.click(within(inspector()).getByRole("button", { name: "New domain" }));
    expect(onOverlayOpening).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("dialog")).toHaveAccessibleName("New domain");
  });
});
