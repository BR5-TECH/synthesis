import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import App from "./App";
import { resetLogBufferForTest } from "./logging";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { cancelAllScheduledWrites } from "./state/writeSchedule";
import { setWindowProjectId } from "./api/invoke";

// PWN-FR-KQZB: a window that "open project window" created is a project window
// from its start. It shows a loading state that names its project, never the
// Project picker, and it changes the size of no window while it mounts.

const invokeMock = vi.fn();
const windowCalls = vi.hoisted(() => [] as string[]);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  const record =
    (name: string, value?: unknown) =>
    async (...args: unknown[]) => {
      windowCalls.push(`${name}(${args.map((a) => JSON.stringify(a)).join(",")})`);
      return value;
    };
  const win = {
    setResizable: record("setResizable"),
    setMaximizable: record("setMaximizable"),
    setSize: record("setSize"),
    isMaximized: record("isMaximized", false),
    unmaximize: record("unmaximize"),
    maximize: record("maximize"),
    isFullscreen: record("isFullscreen", false),
    setFullscreen: record("setFullscreen"),
    outerSize: async () => ({ width: 1440, height: 900 }),
    onResized: async () => () => {},
    scaleFactor: async () => 1,
    destroy: record("destroy"),
  };
  return { LogicalSize, getCurrentWindow: () => win, availableMonitors: async () => [] };
});

const entry = {
  projectId: "/work/design",
  name: "design",
  windowLabel: "project-1",
  needsAttention: false,
};

let mountResult: () => Promise<unknown>;

function backend(cmd: string): Promise<unknown> {
  switch (cmd) {
    case "append_log_records":
      return Promise.resolve();
    case "current_project_window":
      return Promise.resolve(entry);
    case "list_project_windows":
      return Promise.resolve({ windows: [entry], activeProjectId: entry.projectId });
    case "open_project_at_path":
      return mountResult();
    case "close_project_window":
      return Promise.resolve({ lastClosed: false });
    case "load_app_preferences":
      return Promise.resolve({ theme: "dark" });
    default:
      return Promise.resolve(undefined);
  }
}

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => backend(cmd));
  windowCalls.length = 0;
  mountResult = () => new Promise(() => {});
  resetLogBufferForTest();
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  setWindowProjectId(null);
});

afterEach(() => {
  cleanup();
  cancelAllScheduledWrites();
  setWindowProjectId(null);
  resetLogBufferForTest();
});

describe("a created project window at its start (PWN-FR-KQZB)", () => {
  it("PWN-FR-KQZB, PPK-FR-WCMN: names its project, shows no Project picker, and applies no picker chrome", async () => {
    render(<App projectWindowBoot={{ name: "design" }} />);
    expect(await screen.findByText("Opening design…")).toBeInTheDocument();
    await vi.waitFor(() => expect(calls("open_project_at_path")).toHaveLength(1));
    expect(calls("list_recent_projects")).toHaveLength(0);
    expect(calls("center_picker")).toHaveLength(0);
    expect(windowCalls.filter((c) => !c.startsWith("isFullscreen"))).toEqual([]);
  });

  it("PWN-FR-KQZB: shows the failure of the mount with a control that closes the tab", async () => {
    mountResult = () => Promise.reject(new Error("not a Synthesis project"));
    render(<App projectWindowBoot={{ name: "design" }} />);
    expect(await screen.findByText("Could not open design")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("not a Synthesis project");
    expect(calls("list_recent_projects")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Close this tab" }));
    await vi.waitFor(() => expect(calls("close_project_window")).toHaveLength(1));
    expect(calls("close_project_window")[0][1]).toMatchObject({ projectId: "/work/design" });
  });

  it("PWN-FR-QEGI: the first window, which the backend did not create, shows the Project picker", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "current_project_window" ? Promise.resolve(null) : backend(cmd),
    );
    render(<App />);
    await vi.waitFor(() => expect(calls("list_recent_projects").length).toBeGreaterThan(0));
    expect(screen.queryByText(/^Opening /)).not.toBeInTheDocument();
  });

  it("PWN-FR-KQZB: a created window with no entry closes while another project window is open", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "current_project_window") return Promise.resolve(null);
      if (cmd === "list_project_windows")
        return Promise.resolve({
          windows: [{ ...entry, projectId: "/work/other", windowLabel: "main" }],
          activeProjectId: "/work/other",
        });
      return backend(cmd);
    });
    render(<App projectWindowBoot={{ name: "design" }} />);
    await vi.waitFor(() => expect(windowCalls).toContain("destroy()"));
    expect(calls("list_recent_projects")).toHaveLength(0);
  });
});
