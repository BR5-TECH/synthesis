/**
 * SNV-FR-56: a dialog of the shared `Modal` is one of the main window's
 * floating overlays, so its opening closes the shell's open overlays.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, renderHook, screen } from "@testing-library/react";

import { useAppOverlays } from "./useAppOverlays";
import { ReworkDialog } from "../components/OpenSpecChange/ReworkDialog";
import { setModalOpening } from "../state/modalOpening";
import { otherCommand } from "../test/invokeMocks";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) => otherCommand(cmd),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

afterEach(() => {
  cleanup();
  setModalOpening(null);
});

/** A shell session whose every member is a stub. */
function shell() {
  const stubs = new Map<PropertyKey, unknown>([
    ["screen", "project"],
    ["projectPath", "/p"],
  ]);
  return new Proxy({} as Parameters<typeof useAppOverlays>[0], {
    get(_target, key) {
      if (!stubs.has(key)) stubs.set(key, vi.fn());
      return stubs.get(key);
    },
  });
}

function drawRework() {
  render(
    <ReworkDialog
      streamId="s-1"
      streamName="auth-work"
      change="add-logout-3f2a"
      returnFocus={null}
      onClose={() => {}}
      onThread={() => {}}
    />,
  );
}

describe("a Modal dialog among the shell's overlays (SNV-FR-56)", () => {
  it("SNV-FR-56: opening the Rework… dialog closes the open agents roster and the open commit window", async () => {
    const { result } = renderHook(() => useAppOverlays(shell()));
    act(() => result.current.setAgentsRosterOpen(true));
    let window!: Promise<unknown>;
    act(() => {
      window = result.current.openCommitWindow([{ path: "a.md", untracked: false }], []);
    });
    expect(result.current.agentsRosterOpen).toBe(true);
    expect(result.current.commitWindow).not.toBeNull();

    drawRework();

    expect(screen.getByRole("dialog", { name: "Rework add-logout-3f2a" })).toBeInTheDocument();
    expect(result.current.agentsRosterOpen).toBe(false);
    expect(result.current.commitWindow).toBeNull();
    await expect(window).resolves.toBeNull();
  });

  it("SNV-FR-56: a dialog with no shell to close anything opens all the same", () => {
    drawRework();
    expect(screen.getByRole("dialog", { name: "Rework add-logout-3f2a" })).toBeInTheDocument();
  });
});
