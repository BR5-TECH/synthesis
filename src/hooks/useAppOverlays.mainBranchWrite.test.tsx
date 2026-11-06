/**
 * SNV-FR-56 / MBW-FR-GYVD: the main branch write confirmation is one of the
 * main window's floating overlays, and the shell enforces the exclusion.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";

import { useAppOverlays } from "./useAppOverlays";
import {
  currentMainBranchWrite,
  requestMainBranchWrite,
} from "../state/mainBranchWrite";
import { mainGuard } from "../test/mainBranchWriteFixtures";
import { otherCommand } from "../test/invokeMocks";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) => otherCommand(cmd),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

afterEach(cleanup);

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

const request = () =>
  requestMainBranchWrite({ write: "commit", branch: null, guard: mainGuard(), readError: null });

describe("the confirmation among the shell's overlays (SNV-FR-56)", () => {
  it("SNV-FR-56, MBW-FR-GYVD: the confirmation closes an open commit window, and a commit window that opens closes the confirmation", async () => {
    const { result } = renderHook(() => useAppOverlays(shell()));

    let window!: Promise<unknown>;
    act(() => {
      window = result.current.openCommitWindow([{ path: "a.md", untracked: false }], []);
    });
    expect(result.current.commitWindow).not.toBeNull();
    let confirmation!: Promise<boolean>;
    act(() => {
      confirmation = request();
    });
    await expect(window).resolves.toBeNull();
    expect(result.current.commitWindow).toBeNull();
    expect(currentMainBranchWrite()).not.toBeNull();

    act(() => {
      void result.current.openCommitWindow([{ path: "a.md", untracked: false }], []);
    });
    await expect(confirmation).resolves.toBe(false);
    expect(currentMainBranchWrite()).toBeNull();
  });
});
