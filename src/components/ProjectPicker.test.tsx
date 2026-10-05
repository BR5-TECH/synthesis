import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { readFileSync } from "node:fs";

import {
  PICKER_WINDOW_HEIGHT,
  PICKER_WINDOW_WIDTH,
  ProjectPicker,
} from "./ProjectPicker";
import type { ProjectHandle, RecentProject } from "../types";
import { readStylesheet } from "../test/readStylesheet";

const invokeMock = vi.fn();
const openDialogMock = vi.fn();

const setResizableMock = vi.fn(async (_v: boolean) => {});
const setMaximizableMock = vi.fn(async (_v: boolean) => {});
const setSizeMock = vi.fn(async (_s: unknown) => {});
const isMaximizedMock = vi.fn(async () => false);
const unmaximizeMock = vi.fn(async () => {});
const isFullscreenMock = vi.fn(async () => false);
const setFullscreenMock = vi.fn(async (_v: boolean) => {});
// Captured `onResized` handler so a test can simulate the resize the platform
// emits when the window leaves full-screen (which is what lets the picker's
// settle wait resolve without hitting its timeout ceiling).
let resizeHandler: (() => void) | null = null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openDialogMock(...args),
}));

vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    width: number;
    height: number;
    constructor(width: number, height: number) {
      this.width = width;
      this.height = height;
    }
  }
  return {
    LogicalSize,
    getCurrentWindow: () => ({
      setResizable: (v: boolean) => setResizableMock(v),
      setMaximizable: (v: boolean) => setMaximizableMock(v),
      setSize: (s: unknown) => setSizeMock(s),
      isMaximized: () => isMaximizedMock(),
      unmaximize: () => unmaximizeMock(),
      isFullscreen: () => isFullscreenMock(),
      setFullscreen: async (v: boolean) => {
        await setFullscreenMock(v);
        // Leaving full-screen changes the outer size, so the platform fires a
        // resize; the picker awaits that before sizing/centering.
        if (v === false && resizeHandler) resizeHandler();
      },
      onResized: async (cb: () => void) => {
        resizeHandler = cb;
        return () => {
          resizeHandler = null;
        };
      },
    }),
  };
});

const RECENTS: RecentProject[] = [
  {
    name: "acme-platform",
    path: "~/dev/acme-platform",
    lastOpenedAt: "2026-05-15T10:00:00Z",
  },
  {
    name: "design-skills",
    path: "~/dev/design-skills",
    lastOpenedAt: "2026-05-12T10:00:00Z",
  },
];

function setupInvoke(impl: (cmd: string, args?: unknown) => unknown) {
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => impl(cmd, args),
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  openDialogMock.mockReset();
  setResizableMock.mockClear();
  setMaximizableMock.mockClear();
  setSizeMock.mockClear();
  isMaximizedMock.mockClear();
  isMaximizedMock.mockResolvedValue(false);
  unmaximizeMock.mockClear();
  isFullscreenMock.mockClear();
  isFullscreenMock.mockResolvedValue(false);
  setFullscreenMock.mockClear();
  setFullscreenMock.mockResolvedValue(undefined);
  resizeHandler = null;
});

afterEach(() => {
  cleanup();
});

describe("ProjectPicker — initial render", () => {
  it("shows loading state then renders recents (PPK-FR-01, PPK-FR-02, PPK-FR-03, PPK-FR-04, PPK-FR-05)", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);

    expect(screen.getByText(/loading/i)).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByText("acme-platform")).toBeInTheDocument(),
    );
    expect(screen.getByText("design-skills")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /browse folder/i })).toHaveLength(2);
    expect(screen.getByPlaceholderText(/git@github.com/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create/i })).toBeInTheDocument();
  });

  it("renders empty-state when recents list is empty (PPK-FR-RQZV)", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return [];
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() =>
      expect(screen.getByText(/No projects yet/i)).toBeInTheDocument(),
    );
    // Other actions remain usable
    expect(screen.getByPlaceholderText(/git@github.com/)).toBeEnabled();
  });

  it("falls back to empty state and shows error when list_recent_projects rejects", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_recent_projects") throw "boom";
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() =>
      expect(screen.getByText(/✗ boom/)).toBeInTheDocument(),
    );
    expect(screen.getByText(/No projects yet/i)).toBeInTheDocument();
  });
});

describe("ProjectPicker — reflects pinned/missing state (PPK-FR-13)", () => {
  it("renders pinned and missing badges from list_recent_projects without offering management", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects")
        return [
          {
            name: "pinned-proj",
            path: "~/dev/pinned-proj",
            lastOpenedAt: "2026-01-01T00:00:00Z",
            pinned: true,
            missing: false,
          },
          {
            name: "gone-proj",
            path: "~/dev/gone-proj",
            lastOpenedAt: "2026-02-01T00:00:00Z",
            pinned: true,
            missing: true,
          },
        ];
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);

    await waitFor(() =>
      expect(screen.getByText("pinned-proj")).toBeInTheDocument(),
    );
    // Both entries are pinned; only the deleted one is flagged missing.
    expect(screen.getAllByText("pinned")).toHaveLength(2);
    expect(screen.getByText("missing")).toBeInTheDocument();
    // The picker stays view-and-open only: no remove/clear/pin controls.
    expect(
      screen.queryByRole("button", { name: /remove|clear|pin/i }),
    ).not.toBeInTheDocument();
  });

  it("does not render badges for plain entries (no pinned/missing fields)", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() =>
      expect(screen.getByText("acme-platform")).toBeInTheDocument(),
    );
    expect(screen.queryByText("pinned")).not.toBeInTheDocument();
    expect(screen.queryByText("missing")).not.toBeInTheDocument();
  });
});

describe("ProjectPicker — recent row click (PPK-FR-02, PPK-FR-07)", () => {
  it("invokes open_project_at_path with the row path on click", async () => {
    const onOpen = vi.fn();
    const handle: ProjectHandle = {
      name: "acme-platform",
      path: "~/dev/acme-platform",
      activeWorktreePath: "~/dev/acme-platform",
    };
    setupInvoke((cmd, args) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "open_project_at_path") {
        expect(args).toEqual({ path: "~/dev/acme-platform" });
        return handle;
      }
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.click(screen.getByText("acme-platform"));
    await waitFor(() => expect(onOpen).toHaveBeenCalledWith(handle));
  });

  it("shows row-scoped error and keeps picker mounted on failure", async () => {
    const onOpen = vi.fn();
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "open_project_at_path") throw "permission denied";
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.click(screen.getByText("acme-platform"));

    await waitFor(() =>
      expect(screen.getByText(/✗ permission denied/)).toBeInTheDocument(),
    );
    expect(onOpen).not.toHaveBeenCalled();
    // Second row has no error attached
    const otherRow = screen.getByText("design-skills");
    expect(otherRow).toBeInTheDocument();
  });
});

describe("ProjectPicker — git url (PPK-FR-04, PPK-FR-08)", () => {
  it("rejects unparsable URLs client-side without invoking", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      throw new Error(`should not reach ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => screen.getByText("acme-platform"));

    const input = screen.getByPlaceholderText(/git@github.com/);
    await userEvent.type(input, "not-a-url");
    await userEvent.click(screen.getByRole("button", { name: /^open$/i }));

    expect(screen.getByText(/✗ URL is not parsable/)).toBeInTheDocument();
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "open_project_from_git_url"),
    ).toHaveLength(0);
  });

  it("invokes open_project_from_git_url for a valid URL", async () => {
    const onOpen = vi.fn();
    const handle: ProjectHandle = {
      name: "repo",
      path: "~/dev/repo",
      activeWorktreePath: "~/dev/repo",
    };
    setupInvoke((cmd, args) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "open_project_from_git_url") {
        expect(args).toEqual({ url: "https://github.com/org/repo.git" });
        return handle;
      }
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.type(
      screen.getByPlaceholderText(/git@github.com/),
      "https://github.com/org/repo.git",
    );
    await userEvent.click(screen.getByRole("button", { name: /^open$/i }));

    await waitFor(() => expect(onOpen).toHaveBeenCalledWith(handle));
  });

  it("surfaces backend error inline when git open rejects", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "open_project_from_git_url")
        throw "could not derive project name from url";
      throw new Error(`unexpected ${cmd}`);
    });
    const onOpen = vi.fn();
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.type(
      screen.getByPlaceholderText(/git@github.com/),
      "https://x.com/",
    );
    await userEvent.click(screen.getByRole("button", { name: /^open$/i }));

    await waitFor(() =>
      expect(
        screen.getByText(/could not derive project name from url/),
      ).toBeInTheDocument(),
    );
    expect(onOpen).not.toHaveBeenCalled();
  });
});

describe("ProjectPicker — browse folder", () => {
  it("does nothing when the dialog is canceled (returns null)", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      throw new Error(`unexpected ${cmd}`);
    });
    openDialogMock.mockResolvedValue(null);
    const onOpen = vi.fn();
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    const browseButtons = screen.getAllByRole("button", {
      name: /browse folder/i,
    });
    await userEvent.click(browseButtons[0]);

    expect(onOpen).not.toHaveBeenCalled();
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "open_project_at_path"),
    ).toHaveLength(0);
  });

  it("invokes open_project_at_path with the picked folder", async () => {
    const handle: ProjectHandle = {
      name: "picked",
      path: "/tmp/picked",
      activeWorktreePath: "/tmp/picked",
    };
    setupInvoke((cmd, args) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "open_project_at_path") {
        expect(args).toEqual({ path: "/tmp/picked" });
        return handle;
      }
      throw new Error(`unexpected ${cmd}`);
    });
    openDialogMock.mockResolvedValue("/tmp/picked");
    const onOpen = vi.fn();
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    const browseButtons = screen.getAllByRole("button", {
      name: /browse folder/i,
    });
    await userEvent.click(browseButtons[0]);

    await waitFor(() => expect(onOpen).toHaveBeenCalledWith(handle));
  });
});

describe("ProjectPicker — create (PPK-FR-05)", () => {
  it("requires a target folder before submission", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      throw new Error(`should not reach ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.type(screen.getByPlaceholderText("my-project"), "new-proj");
    // Without a target folder, Create button is disabled.
    expect(screen.getByRole("button", { name: /create/i })).toBeDisabled();
  });

  it("invokes create_project with mode=colocated and target path", async () => {
    const handle: ProjectHandle = {
      name: "new-proj",
      path: "/tmp/new",
      activeWorktreePath: "/tmp/new",
    };
    setupInvoke((cmd, args) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "create_project") {
        expect(args).toEqual({
          name: "new-proj",
          mode: "colocated",
          targetPath: "/tmp/new",
        });
        return handle;
      }
      throw new Error(`unexpected ${cmd}`);
    });
    openDialogMock.mockResolvedValue("/tmp/new");
    const onOpen = vi.fn();
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.type(screen.getByPlaceholderText("my-project"), "new-proj");
    await userEvent.click(screen.getByLabelText(/Co-located/));

    // Second "Browse folder…" button (inside Create card)
    const browseButtons = screen.getAllByRole("button", {
      name: /browse folder/i,
    });
    await userEvent.click(browseButtons[1]);

    await waitFor(() =>
      expect(screen.getByText("/tmp/new")).toBeInTheDocument(),
    );

    await userEvent.click(screen.getByRole("button", { name: /create/i }));
    await waitFor(() => expect(onOpen).toHaveBeenCalledWith(handle));
  });

  it("surfaces backend error inline and keeps picker mounted on create failure", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "create_project") throw "name is empty";
      throw new Error(`unexpected ${cmd}`);
    });
    openDialogMock.mockResolvedValue("/tmp/new");
    const onOpen = vi.fn();
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.type(screen.getByPlaceholderText("my-project"), "x");
    const browseButtons = screen.getAllByRole("button", {
      name: /browse folder/i,
    });
    await userEvent.click(browseButtons[1]);
    await userEvent.click(screen.getByRole("button", { name: /create/i }));

    await waitFor(() =>
      expect(screen.getByText(/✗ name is empty/)).toBeInTheDocument(),
    );
    expect(onOpen).not.toHaveBeenCalled();
  });
});

describe("ProjectPicker — busy state", () => {
  it("disables action buttons while an open is in flight, re-enables on resolve", async () => {
    let resolve: ((handle: ProjectHandle) => void) | undefined;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "open_project_at_path") {
        return new Promise<ProjectHandle>((r) => {
          resolve = r;
        });
      }
      throw new Error(`unexpected ${cmd}`);
    });
    const onOpen = vi.fn();
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.click(screen.getByText("acme-platform"));

    const browseButtons = screen.getAllByRole("button", {
      name: /browse folder/i,
    });
    await waitFor(() => expect(browseButtons[0]).toBeDisabled());
    expect(browseButtons[1]).toBeDisabled();

    resolve!({
      name: "acme-platform",
      path: "~/dev/acme-platform",
      activeWorktreePath: "~/dev/acme-platform",
    });
    await waitFor(() => expect(onOpen).toHaveBeenCalled());
  });

  it("re-enables action buttons after a failed open", async () => {
    let reject: ((reason: unknown) => void) | undefined;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "open_project_at_path") {
        return new Promise<ProjectHandle>((_, r) => {
          reject = r;
        });
      }
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.click(screen.getByText("acme-platform"));

    const browseDuring = screen.getAllByRole("button", {
      name: /browse folder/i,
    });
    await waitFor(() => expect(browseDuring[0]).toBeDisabled());

    reject!("nope");
    await waitFor(() =>
      expect(screen.getByText(/✗ nope/)).toBeInTheDocument(),
    );
    const browseAfter = screen.getAllByRole("button", {
      name: /browse folder/i,
    });
    expect(browseAfter[0]).toBeEnabled();
    expect(browseAfter[1]).toBeEnabled();
  });
});

describe("ProjectPicker — row-scoped error identity", () => {
  it("renders the row error directly under the failing row, not under others", async () => {
    setupInvoke((cmd, args) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "open_project_at_path") {
        const path = (args as { path: string }).path;
        if (path === "~/dev/acme-platform") throw "permission denied";
      }
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.click(screen.getByText("acme-platform"));
    const errorEl = await screen.findByText(/✗ permission denied/);

    // The error span is rendered as a direct sibling of the failing row,
    // inside the per-row wrapper. The wrapper should contain only that row's
    // name/path, not the other row's.
    const rowWrapper = errorEl.parentElement;
    expect(rowWrapper).not.toBeNull();
    expect(rowWrapper!.textContent).toContain("acme-platform");
    expect(rowWrapper!.textContent).not.toContain("design-skills");
  });

  it("clears the row error when a different row is clicked successfully", async () => {
    setupInvoke((cmd, args) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "open_project_at_path") {
        const path = (args as { path: string }).path;
        if (path === "~/dev/acme-platform") throw "permission denied";
        return { name: "design-skills", path };
      }
      throw new Error(`unexpected ${cmd}`);
    });
    const onOpen = vi.fn();
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.click(screen.getByText("acme-platform"));
    await screen.findByText(/✗ permission denied/);

    await userEvent.click(screen.getByText("design-skills"));
    await waitFor(() => expect(onOpen).toHaveBeenCalled());
    expect(screen.queryByText(/✗ permission denied/)).not.toBeInTheDocument();
  });
});

describe("ProjectPicker — dialog errors", () => {
  it("surfaces an inline browse error when the dialog itself rejects", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      throw new Error(`unexpected ${cmd}`);
    });
    openDialogMock.mockRejectedValue("dialog failed");
    const onOpen = vi.fn();
    render(<ProjectPicker onOpen={onOpen} />);
    await waitFor(() => screen.getByText("acme-platform"));

    const browseButtons = screen.getAllByRole("button", {
      name: /browse folder/i,
    });
    await userEvent.click(browseButtons[0]);

    await waitFor(() =>
      expect(screen.getByText(/✗ dialog failed/)).toBeInTheDocument(),
    );
    expect(onOpen).not.toHaveBeenCalled();
  });
});

describe("ProjectPicker — error isolation", () => {
  it("a git error does not show as a recent-row error", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "open_project_from_git_url") throw "git failed";
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => screen.getByText("acme-platform"));

    await userEvent.type(
      screen.getByPlaceholderText(/git@github.com/),
      "https://x/y.git",
    );
    await userEvent.click(screen.getByRole("button", { name: /^open$/i }));

    await waitFor(() =>
      expect(screen.getByText(/✗ git failed/)).toBeInTheDocument(),
    );
    // No row-attached error appears
    expect(screen.queryByText(/permission denied/)).not.toBeInTheDocument();
  });
});

describe("ProjectPicker — window chrome (PPK-FR-09)", () => {
  it("on mount, sets the picker window non-resizable and applies a fixed size", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "center_picker") return undefined;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);

    await waitFor(() => expect(setResizableMock).toHaveBeenCalledWith(false));
    await waitFor(() => expect(setSizeMock).toHaveBeenCalled());

    // The size passed must be the exact, exported fixed-window dimensions.
    // PPK-FR-09 disallows a user-resizable picker, and the right column layout
    // depends on this exact width.
    const sizeArg = setSizeMock.mock.calls[0]?.[0] as
      | { width: number; height: number }
      | undefined;
    expect(sizeArg).toBeDefined();
    expect(sizeArg!.width).toBe(PICKER_WINDOW_WIDTH);
    expect(sizeArg!.height).toBe(PICKER_WINDOW_HEIGHT);

    // Maximize affordance is disabled as well — PPK-FR-09 forbids maximize.
    await waitFor(() =>
      expect(setMaximizableMock).toHaveBeenCalledWith(false),
    );
  });

  it("actively rejects an already-maximized state by unmaximizing and re-applying the fixed size", async () => {
    // Simulate the picker window starting up already maximized (e.g. an OS
    // window manager slipped a maximize through before setMaximizable(false)
    // could land). PPK-FR-09 forbids the picker from being shown at any size
    // other than the fixed dimensions.
    isMaximizedMock.mockResolvedValue(true);

    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "center_picker") return undefined;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);

    await waitFor(() => expect(unmaximizeMock).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(setSizeMock).toHaveBeenCalled());

    const sizeArg = setSizeMock.mock.calls[0]?.[0] as
      | { width: number; height: number }
      | undefined;
    expect(sizeArg).toBeDefined();
    expect(sizeArg!.width).toBe(PICKER_WINDOW_WIDTH);
    expect(sizeArg!.height).toBe(PICKER_WINDOW_HEIGHT);
  });

  it("leaves OS full-screen before pinning size when the window was full-screen (SNV-FR-27)", async () => {
    // File → Close project (SNV-FR-25) can hand the shared window to the picker
    // while it is still in native full-screen. The picker must exit full-screen
    // so it can be shown as a normal, fixed-size, centered window — and the exit
    // must precede the size pin / centering so those land on a normal window.
    isFullscreenMock.mockResolvedValue(true);

    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "center_picker") return undefined;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);

    await waitFor(() =>
      expect(setFullscreenMock).toHaveBeenCalledWith(false),
    );
    await waitFor(() => expect(setSizeMock).toHaveBeenCalled());

    // Ordering: leaving full-screen happens before the fixed size is applied,
    // otherwise setSize would be swallowed by the full-screen space.
    const exitOrder = setFullscreenMock.mock.invocationCallOrder[0];
    const sizeOrder = setSizeMock.mock.invocationCallOrder[0];
    expect(exitOrder).toBeLessThan(sizeOrder);

    // The picker still ends up at its fixed dimensions and re-centered.
    const sizeArg = setSizeMock.mock.calls[0]?.[0] as
      | { width: number; height: number }
      | undefined;
    expect(sizeArg).toBeDefined();
    expect(sizeArg!.width).toBe(PICKER_WINDOW_WIDTH);
    expect(sizeArg!.height).toBe(PICKER_WINDOW_HEIGHT);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("center_picker"),
    );
  });

  it("leaves the persisted full-screen preference set when it exits (PPK-FR-14 / SNV-FR-27, SNV-FR-39)", async () => {
    // SNV-FR-27: exiting full-screen here is a rule about how the *picker* is
    // presented, not a decision by the user to stop working full-screen. If it
    // wrote `false`, Close project → reopen would silently drop the user out of
    // full-screen and the feature would be dead for anyone who uses Close
    // project.
    isFullscreenMock.mockResolvedValue(true);

    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "center_picker") return undefined;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);

    await waitFor(() =>
      expect(setFullscreenMock).toHaveBeenCalledWith(false),
    );
    await waitFor(() => expect(setSizeMock).toHaveBeenCalled());

    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_app_preferences"),
    ).toHaveLength(0);
  });

  it("never applies the persisted full-screen preference itself (PPK-FR-14, SNV-FR-39)", async () => {
    // PPK-FR-14: the flag governs the main window only. The picker neither
    // reads it nor puts itself into a full-screen space, whatever its value.
    isFullscreenMock.mockResolvedValue(false);

    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "center_picker") return undefined;
      if (cmd === "load_app_preferences")
        return { theme: "system", mainWindowFullscreen: true };
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);

    await waitFor(() => expect(setSizeMock).toHaveBeenCalled());
    // Never asked to enter full-screen...
    expect(setFullscreenMock).not.toHaveBeenCalledWith(true);
    // ...and never even consulted the preference.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_app_preferences"),
    ).toHaveLength(0);
  });

  it("still pins the fixed size and centers when leaving full-screen fails (SNV-FR-27)", async () => {
    // Resilience: if the OS rejects setFullscreen (unsupported platform, or a
    // window not yet fully created), the picker must not be stranded — it still
    // applies its fixed size (PPK-FR-09) and re-centers (PPK-FR-12).
    isFullscreenMock.mockResolvedValue(true);
    setFullscreenMock.mockRejectedValueOnce("unsupported");

    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "center_picker") return undefined;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);

    await waitFor(() => expect(setSizeMock).toHaveBeenCalled());
    const sizeArg = setSizeMock.mock.calls[0]?.[0] as
      | { width: number; height: number }
      | undefined;
    expect(sizeArg).toBeDefined();
    expect(sizeArg!.width).toBe(PICKER_WINDOW_WIDTH);
    expect(sizeArg!.height).toBe(PICKER_WINDOW_HEIGHT);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("center_picker"),
    );
  });

  it("does not toggle full-screen when the window is not full-screen (SNV-FR-27)", async () => {
    // The common case: opening the picker at launch, or closing a project that
    // was not full-screen. We must not force a spurious setFullscreen(false).
    isFullscreenMock.mockResolvedValue(false);

    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "center_picker") return undefined;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);

    // The chrome routine still runs to completion (size is pinned).
    await waitFor(() => expect(setSizeMock).toHaveBeenCalled());
    expect(setFullscreenMock).not.toHaveBeenCalled();
  });

  it("on mount, re-centers the picker window on the active display (PPK-FR-12)", async () => {
    // The launch-time centering runs once in the Rust setup hook; when the
    // picker is re-shown within a session (after closing/switching a project)
    // the window must be re-centered too, otherwise it keeps the main shell's
    // last position. The picker drives this by invoking `center_picker` after
    // pinning its fixed size.
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      if (cmd === "center_picker") return undefined;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("center_picker"),
    );
    // Centering happens only after the fixed size is applied (so it lands on the
    // picker's dimensions, not the main shell's).
    expect(setSizeMock).toHaveBeenCalled();
  });
});

describe("ProjectPicker — picker window dimensions match Tauri config", () => {
  it("uses the same fixed-window size declared in src-tauri/tauri.conf.json", () => {
    // PPK-FR-09: the picker is rendered inside the Tauri window at its configured,
    // non-resizable size. If the JS-side constants drift from
    // tauri.conf.json's `app.windows[0]`, the picker layout will be tuned
    // for the wrong viewport and either clip content or trigger a scrollbar
    // on the document body — exactly the bug we're guarding against.
    const conf = JSON.parse(
      readFileSync("src-tauri/tauri.conf.json", "utf8"),
    ) as {
      app: { windows: Array<{ width: number; height: number }> };
    };
    const w = conf.app.windows[0];
    expect(w.width).toBe(PICKER_WINDOW_WIDTH);
    expect(w.height).toBe(PICKER_WINDOW_HEIGHT);
  });
});

describe("ProjectPicker — document-level scroll prevention", () => {
  // Root-cause guard for the "unwanted vertical scrollbar" bug:
  // because the Tauri window is fixed-size, html, body, and #root must
  // not be allowed to scroll. The only permitted scroll region in the
  // picker is .picker-recents-body (asserted by the PPK-FR-11 test below).
  it("declares overflow: hidden on html, body, and #root", () => {
    const colorsCss = readStylesheet("colors_and_type.css");
    const kitCss = readStylesheet("kit.css");

    // html, body rule must include overflow:hidden and zero margin so the
    // browser default 8px body margin can't push content past the viewport.
    const htmlBodyMatch = colorsCss.match(/html,\s*body\s*\{([^}]*)\}/);
    expect(
      htmlBodyMatch,
      "expected `html, body { ... }` rule in colors_and_type.css",
    ).not.toBeNull();
    const htmlBodyDecl = htmlBodyMatch![1];
    expect(htmlBodyDecl).toMatch(/overflow\s*:\s*hidden/);
    expect(htmlBodyDecl).toMatch(/margin\s*:\s*0/);

    // #root must not be allowed to grow past the viewport either.
    const rootMatch = kitCss.match(/#root\s*\{([^}]*)\}/);
    expect(rootMatch, "expected `#root { ... }` rule in kit.css").not.toBeNull();
    const rootDecl = rootMatch![1];
    expect(rootDecl).toMatch(/overflow\s*:\s*hidden/);
    // height: 100% (not min-height) so #root cannot exceed the body.
    expect(rootDecl).toMatch(/height\s*:\s*100%/);
    expect(rootDecl).not.toMatch(/min-height\s*:\s*100vh/);
  });

  it("declares overflow: hidden on .picker so it cannot scroll itself", () => {
    const css = readStylesheet("kit.css");

    // The outer .picker container should clip (not scroll) so that if any
    // layout adjustment ever overflows it, the bug is visible (clipping)
    // rather than silent (a small scrollbar). Per PPK-FR-10/PPK-FR-11 only the
    // recent list may scroll.
    const pickerMatch = css.match(/\.picker\s*\{([^}]*)\}/);
    expect(pickerMatch, "expected `.picker { ... }` rule in kit.css").not.toBeNull();
    const pickerDecl = pickerMatch![1];
    expect(pickerDecl).toMatch(/overflow\s*:\s*hidden/);
  });
});

describe("ProjectPicker — right-column layout (PPK-FR-10)", () => {
  it("does not apply overflow: auto/scroll to the right column", async () => {
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => screen.getByText("acme-platform"));

    const right = screen.getByTestId("picker-right");
    // Either no overflow declared, or visible — anything but auto/scroll.
    // We assert the *inline* style is not set to a scrolling overflow; the
    // CSS rule set in kit.css uses `overflow: visible` (or absence).
    const style = (right as HTMLElement).style;
    expect(style.overflow === "auto" || style.overflow === "scroll").toBe(
      false,
    );
    expect(style.overflowY === "auto" || style.overflowY === "scroll").toBe(
      false,
    );
    expect(style.overflowX === "auto" || style.overflowX === "scroll").toBe(
      false,
    );

    // And no inline overflow on the panels inside it either.
    const panels = right.querySelectorAll(".picker-card");
    panels.forEach((p) => {
      const s = (p as HTMLElement).style;
      expect(s.overflow === "auto" || s.overflow === "scroll").toBe(false);
      expect(s.overflowY === "auto" || s.overflowY === "scroll").toBe(false);
    });
  });
});

describe("ProjectPicker — recent-list scroll region (PPK-FR-11)", () => {
  it("kit.css declares overflow-y:auto on .picker-recents-body and does NOT declare overflow auto/scroll on .picker-right", async () => {
    // Vitest config has `css: false`, so we can't observe kit.css via
    // getComputedStyle on jsdom-rendered elements. Instead we read kit.css
    // as text and assert the rule blocks directly. This grounds the test in
    // the real stylesheet (any regression to those two rules will fail this
    // test) rather than in a runtime-fabricated <style>.
    const css = readStylesheet("kit.css");

    // Extract the `.picker-recents-body { ... }` rule block.
    const recentsBodyMatch = css.match(/\.picker-recents-body\s*\{([^}]*)\}/);
    expect(
      recentsBodyMatch,
      "expected .picker-recents-body rule in kit.css",
    ).not.toBeNull();
    const recentsBodyDecl = recentsBodyMatch![1];
    expect(recentsBodyDecl).toMatch(/overflow-y\s*:\s*auto/);

    // Extract the `.picker-right { ... }` rule block.
    const rightMatch = css.match(/\.picker-right\s*\{([^}]*)\}/);
    expect(rightMatch, "expected .picker-right rule in kit.css").not.toBeNull();
    const rightDecl = rightMatch![1];
    // The right column must not declare a scroll container in either axis.
    expect(rightDecl).not.toMatch(/overflow(?:-x|-y)?\s*:\s*auto/);
    expect(rightDecl).not.toMatch(/overflow(?:-x|-y)?\s*:\s*scroll/);

    // Sanity: the rendered elements actually carry the asserted class names,
    // so a class rename in the component would still be caught.
    setupInvoke((cmd) => {
      if (cmd === "list_recent_projects") return RECENTS;
      throw new Error(`unexpected ${cmd}`);
    });
    render(<ProjectPicker onOpen={vi.fn()} />);
    await waitFor(() => screen.getByText("acme-platform"));

    const body = screen.getByTestId("picker-recents-body");
    const right = screen.getByTestId("picker-right");
    expect(body.className).toContain("picker-recents-body");
    expect(right.className).toContain("picker-right");
  });
});
