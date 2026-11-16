import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";
import { useShellSession } from "./useShellSession";
import { setWindowProjectId } from "../api/invoke";
import { resetToasts } from "../state/toasts";
import { resetActiveRootHealth } from "../state/activeRootHealth";
import { handle } from "../test/shellSessionFixtures";

// The close of a project tab (PJT-FR-PCCL, PJT-FR-JSAW, PJT-FR-PEIJ): the window
// of the project closes with it while other tabs stay, and the last tab stays
// and shows the Project picker.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

let closeResult: { lastClosed: boolean } = { lastClosed: false };
const commands = () => invokeMock.mock.calls.map((c) => c[0] as string);

beforeEach(() => {
  resetToasts();
  resetActiveRootHealth();
  invokeMock.mockReset();
  closeResult = { lastClosed: false };
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "close_project_window") return closeResult;
    return undefined;
  });
  setWindowProjectId("~/dev/acme");
});

afterEach(() => {
  cleanup();
  setWindowProjectId(null);
});

describe("closing the project tab of a window", () => {
  it("tears the project down, then closes its window, and keeps the other tabs", async () => {
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
    await act(async () => {
      await result.current.requestCloseProject();
    });
    const names = commands();
    expect(names.indexOf("close_project")).toBeGreaterThanOrEqual(0);
    expect(names.indexOf("close_project_window")).toBeGreaterThan(names.indexOf("close_project"));
    // The window is gone, so this window does not draw the picker.
    expect(result.current.screen).toBe("ide");
  });

  it("shows the Project picker when the last tab closes", async () => {
    closeResult = { lastClosed: true };
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
    await act(async () => {
      await result.current.requestCloseProject();
    });
    expect(result.current.screen).toBe("picker");
  });

  it("shows the Project picker when the window cannot be closed", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "close_project_window") throw new Error("project_not_open");
      return undefined;
    });
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
    await act(async () => {
      await result.current.requestCloseProject();
    });
    expect(result.current.screen).toBe("picker");
  });

  it("closes no window for a window that holds no project id", async () => {
    setWindowProjectId(null);
    const { result } = renderHook(() => useShellSession());
    act(() => result.current.loadProject(handle("acme", "~/dev/acme")));
    await act(async () => {
      await result.current.requestCloseProject();
    });
    expect(commands()).not.toContain("close_project_window");
    expect(result.current.screen).toBe("picker");
  });
});
