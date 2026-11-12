/**
 * Where focus goes when a change workspace opens
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-DMGL,
 * OCW-FR-AKVG; `SMI-specification-map-inspector.md` SMI-FR-UKIV).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { OpenSpecChangeWorkspace } from ".";
import {
  changeBackend,
  changeTarget,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";

let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  invokeMock.mockReset();
  backend = changeBackend({ proposals: [], discussions: [] });
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  cleanup();
  document.body.querySelectorAll("[data-test-opener]").forEach((n) => n.remove());
  expect(backend.unknown).toEqual([]);
});

/** A control outside the tab, which holds focus as a panel button does. */
function outsideControl(): HTMLButtonElement {
  const button = document.createElement("button");
  button.dataset.testOpener = "";
  document.body.appendChild(button);
  button.focus();
  return button;
}

describe("focus on open (OCW-FR-DMGL, SMI-FR-UKIV)", () => {
  it("SMI-FR-UKIV, OCW-FR-DMGL: a tab that opens while focus is on the body takes focus, so the keyboard starts in the change", async () => {
    (document.activeElement as HTMLElement | null)?.blur();
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    expect(document.activeElement).toBe(screen.getByTestId("openspec-change"));
  });

  it("OCW-FR-DMGL: a tab that opens while a control outside it holds focus leaves focus there", async () => {
    const opener = outsideControl();
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    expect(document.activeElement).toBe(opener);
  });

  it("SMI-FR-UKIV: a second open of a tab that is already showing takes focus again where the opener went away", async () => {
    const { view } = drawChange();
    await screen.findAllByTestId("openspec-artifact");
    const opener = outsideControl();
    // The opener goes with its surface, as the map inspector does.
    opener.remove();
    view.rerender(
      <OpenSpecChangeWorkspace tabId="tab-1" target={changeTarget({ openSeq: 2 })} onOpenArtifact={vi.fn()} />,
    );
    expect(document.activeElement).toBe(screen.getByTestId("openspec-change"));
  });

  it("OCW-FR-AKVG, OCW-FR-DMGL: the statement of an archived change takes focus the same way", async () => {
    backend.readReject = "change_archived: 2026-11-11-add-logout-3f2a";
    (document.activeElement as HTMLElement | null)?.blur();
    drawChange();
    await screen.findByTestId("openspec-change-archived");
    expect(document.activeElement).toBe(screen.getByTestId("openspec-change"));
  });
});
