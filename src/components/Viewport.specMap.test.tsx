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

// The Delivery lens of a file source reads the file; this suite answers only
// the background log flush, and refuses every other command.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((cmd: string) =>
    cmd === "append_log_records" ? Promise.resolve() : Promise.reject(new Error(`unexpected command ${cmd}`)),
  ),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

afterEach(cleanup);

async function renderMapTab(ops: Parameters<typeof makeStore>[1] = {}) {
  setWindowWidth(1440);
  const { store } = makeStore(undefined, ops);
  const onOpenArtifact = vi.fn();
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
      onOpenDraft={vi.fn()}
      onCloseTab={vi.fn()}
      onDraftRenamed={vi.fn()}
      specMap={store}
      onOverlayOpening={onOverlayOpening}
    />,
  );
  await waitFor(() => expect(store.snapshot().lens).not.toBeNull());
  if (store.snapshot().lens === "specifications") {
    await waitFor(() => expect(store.snapshot().status).toBe("ready"));
  }
  return { store, onOpenArtifact, onOverlayOpening };
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

  it("SMP-FR-KQTD, SMP-FR-TKUS: with a delivery source the Map tab opens on the Delivery lens", async () => {
    const { store } = await renderMapTab({
      getDeliverySource: vi.fn(async () => ({ kind: "file" as const, flowId: "f", path: "plan.flow" })),
    });
    expect(screen.getByTestId("delivery-lens")).toBeInTheDocument();
    expect(store.snapshot().status).toBe("idle");
  });
});
