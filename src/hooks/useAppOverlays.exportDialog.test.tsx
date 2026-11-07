/**
 * SNV-FR-56 / EXD-FR-XNLZ / EXD-FR-OOTQ: the Export dialog is one of the main
 * window's floating overlays, and the shell enforces the exclusion in both
 * directions.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";

import { useAppOverlays } from "./useAppOverlays";
import {
  exportDialogRequest,
  openExportDialog,
  resetExportDialogForTest,
} from "../state/exportDialog";
import { otherCommand } from "../test/invokeMocks";
import { resetLogBufferForTest } from "../logging";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) => otherCommand(cmd),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetExportDialogForTest();
});
afterEach(() => {
  cleanup();
  resetExportDialogForTest();
});

/** A shell session whose every member is a stub, with `stubs` to change it. */
function shell(stubs = new Map<PropertyKey, unknown>([["screen", "project"], ["projectPath", "/p"]])) {
  const session = new Proxy({} as Parameters<typeof useAppOverlays>[0], {
    get(_target, key) {
      if (!stubs.has(key)) stubs.set(key, vi.fn());
      return stubs.get(key);
    },
  });
  return { session, stubs };
}

describe("the Export dialog among the shell's overlays (SNV-FR-56)", () => {
  it("SNV-FR-56, EXD-FR-XNLZ: the dialog closes an open commit window, and the About panel that opens closes the dialog", async () => {
    const { session } = shell();
    const { result } = renderHook(() => useAppOverlays(session));

    let window!: Promise<unknown>;
    act(() => {
      window = result.current.openCommitWindow([{ path: "a.md", untracked: false }], []);
    });
    expect(result.current.commitWindow).not.toBeNull();
    await act(async () => {
      await openExportDialog("docs/guide.md");
    });
    await expect(window).resolves.toBeNull();
    expect(result.current.commitWindow).toBeNull();
    expect(exportDialogRequest()).not.toBeNull();

    act(() => result.current.openAbout());
    expect(exportDialogRequest()).toBeNull();
    expect(result.current.aboutOpen).toBe(true);
  });

  it("SNV-FR-56, EXD-FR-OOTQ: a project switch closes the dialog", async () => {
    const { session, stubs } = shell();
    const { rerender } = renderHook(() => useAppOverlays(session));
    await act(async () => {
      await openExportDialog("docs/guide.md");
    });
    expect(exportDialogRequest()).not.toBeNull();
    stubs.set("projectPath", "/q");
    rerender();
    expect(exportDialogRequest()).toBeNull();
  });
});
