/**
 * Activating a row of the status bar's in-flight overlay, through the shell:
 * each destination opens the surface that owns its target.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { OPERATION_PROGRESS } from "./events";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { makeQueue, makeRun } from "./test/graduationFixtures";
import { resetPanelReveals } from "./state/panelReveal";
import {
  defaultInvoke,
  enterIde,
  resetAppFixture,
  tabLabels,
} from "./test/appFixtures";
import type { Operation } from "./types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
// The Library panel (mounted in the IDE) subscribes to the backend
// `"project tree changed"` event, and the File menu reaches the frontend via
// Tauri events. Stub the event bus with a capturing registry so the real Tauri
// runtime is never required and tests can fire menu events deterministically.
const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<
    string,
    Array<(e: { payload?: unknown }) => void>
  >,
}));
const { emitMock } = vi.hoisted(() => ({ emitMock: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  // SWN-FR-01: three windows now, so this window both announces and listens.
  emit: (...args: unknown[]) => emitMock(...args),
  listen: vi.fn(
    async (name: string, handler: (e: { payload?: unknown }) => void) => {
      (eventHandlers[name] ??= []).push(handler);
      return () => {
        eventHandlers[name] = (eventHandlers[name] ?? []).filter(
          (h) => h !== handler,
        );
      };
    },
  ),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  const win = {
    setResizable: async () => {},
    setMaximizable: async () => {},
    setSize: async () => {},
    isMaximized: async () => false,
    unmaximize: async () => {},
    maximize: async () => {},
    outerSize: async () => ({ width: 1440, height: 900 }),
    onResized: async () => () => {},
    scaleFactor: async () => 1,
  };
  return {
    LogicalSize,
    getCurrentWindow: () => win,
    availableMonitors: async () => [
      {
        name: "primary",
        size: { width: 2560, height: 1440 },
        position: { x: 0, y: 0 },
        workArea: { position: { x: 0, y: 0 }, size: { width: 2560, height: 1400 } },
        scaleFactor: 1,
      },
    ],
  };
});
function fireBusEvent(name: string, payload?: unknown) {
  for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
}


/** What the backend reports as in flight when the window mounts. */
let inFlight: Operation[];

function op(sequence: number, overrides: Partial<Operation> = {}): Operation {
  return {
    id: `op-${sequence}`,
    kind: "x",
    label: `Operation ${sequence}`,
    state: "running",
    sequence,
    ...overrides,
  };
}

beforeEach(() => {
  resetPanelReveals();
  resetAppFixture();
  inFlight = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "list_in_flight_operations") return inFlight;
    if (cmd === "list_graduation_queue")
      return makeQueue([makeRun("r-1", "working"), makeRun("r-2", "working")]);
    if (cmd === "get_graduation_capacity")
      return { limit: "unlimited", inUse: 2, waitingForSlot: [] };
    if (cmd === "read_discussion")
      return {
        id: "d-9",
        target: { kind: "note", noteId: "n-1" },
        fragmentTarget: null,
        comments: [],
        locked: false,
        resolved: false,
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
      };
    return defaultInvoke(cmd, args);
  });
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  emitMock.mockReset();
  for (const k in eventHandlers) delete eventHandlers[k];
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function openOverlay() {
  await userEvent.click(await screen.findByTestId("status-bar-progress"));
  return screen.findByTestId("in-flight-overlay");
}

describe("activating a status bar row through the shell (STB-FR-RWPD, STB-FR-DNLC)", () => {
  it("STB-FR-RWPD, GRU-FR-VQJB: a graduation run row opens the Runs panel on that run and closes the overlay", async () => {
    inFlight = [
      op(2, {
        kind: "graduation",
        label: "Graduating Editor scroll fix…",
        activation: { type: "graduation_run", runId: "r-2" },
      }),
      op(1, {
        kind: "graduation",
        label: "Graduating Empty state…",
        activation: { type: "graduation_run", runId: "r-1" },
      }),
    ];
    render(<App />);
    await enterIde();
    const overlay = await openOverlay();
    await userEvent.click(
      within(overlay).getByRole("button", {
        name: "Graduating Empty state… — open graduation run",
      }),
    );

    const region = await screen.findByLabelText("The selected run");
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r-1"));
    expect(screen.queryByTestId("in-flight-overlay")).not.toBeInTheDocument();
  });

  it("STB-FR-RWPD, GRU-FR-VQJB: a later graduation row moves the selection from the run the author had open", async () => {
    inFlight = [
      op(2, {
        kind: "graduation",
        label: "Graduating Editor scroll fix…",
        activation: { type: "graduation_run", runId: "r-2" },
      }),
      op(1, {
        kind: "graduation",
        label: "Graduating Empty state…",
        activation: { type: "graduation_run", runId: "r-1" },
      }),
    ];
    render(<App />);
    await enterIde();
    await userEvent.click(
      within(await openOverlay()).getByRole("button", {
        name: "Graduating Editor scroll fix… — open graduation run",
      }),
    );
    const region = await screen.findByLabelText("The selected run");
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r-2"));

    await userEvent.click(
      within(await openOverlay()).getByRole("button", {
        name: "Graduating Empty state… — open graduation run",
      }),
    );
    await waitFor(() =>
      expect(screen.getByLabelText("The selected run")).toHaveAttribute("data-run", "r-1"),
    );
  });

  it("STB-FR-RWPD, GIT-FR-FZMS: a Git push row opens the Git panel's Logs section with that branch selected", async () => {
    inFlight = [
      op(1, {
        kind: "git",
        label: "Pushing branch",
        activation: { type: "git_push", branch: "develop" },
      }),
    ];
    render(<App />);
    await enterIde();
    const overlay = await openOverlay();
    await userEvent.click(
      within(overlay).getByRole("button", {
        name: "Pushing branch — open branch develop in Git",
      }),
    );

    // GIT-FR-FZMS: the transcript shows first, and the branch is selected for
    // when the author opens the branches section.
    expect(await screen.findByTestId("git-transfer-output")).toBeInTheDocument();
    expect(screen.queryByTestId("in-flight-overlay")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Branches" }));
    const selected = await screen.findByTestId("git-branch-selected");
    expect(selected).toHaveTextContent("develop");
  });

  it("STB-FR-RWPD, CVP-FR-06: a discussion row reveals that discussion through the one route", async () => {
    inFlight = [
      op(1, {
        kind: "agent",
        label: "@Helga is thinking…",
        activation: { type: "discussion", discussionId: "d-9" },
      }),
    ];
    render(<App />);
    await enterIde();
    const overlay = await openOverlay();
    await userEvent.click(
      within(overlay).getByRole("button", {
        name: "@Helga is thinking… — open discussion",
      }),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("read_discussion", {
        discussionId: "d-9",
      }),
    );
    // A note's discussion opens its conversation tab (CVP-FR-54).
    await waitFor(() => expect(tabLabels().some((l) => l.startsWith("Chat:"))).toBe(true));
    expect(screen.queryByTestId("in-flight-overlay")).not.toBeInTheDocument();
  });

  it("STB-FR-DNLC: a row with no destination opens nothing and the overlay stays open", async () => {
    inFlight = [
      op(2, { kind: "git", label: "Fetching remote branches" }),
      op(1, {
        kind: "git",
        label: "Pushing branch",
        activation: { type: "git_push", branch: "develop" },
      }),
    ];
    render(<App />);
    await enterIde();
    const overlay = await openOverlay();
    const before = tabLabels();
    const plain = within(overlay)
      .getAllByRole("listitem")
      .find((row) => row.textContent === "Fetching remote branches")!;
    await userEvent.click(plain);

    expect(screen.getByTestId("in-flight-overlay")).toBeInTheDocument();
    expect(screen.queryByTestId("git-branch-selected")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("The selected run")).not.toBeInTheDocument();
    expect(tabLabels()).toEqual(before);
  });

  it("STB-FR-LKRN, STB-FR-HJVM: the overlay keeps up with events and lists every operation beyond five", async () => {
    inFlight = Array.from({ length: 6 }, (_, i) => op(6 - i));
    render(<App />);
    await enterIde();
    const overlay = await openOverlay();
    expect(within(overlay).getAllByRole("listitem")).toHaveLength(6);

    act(() => {
      fireBusEvent(OPERATION_PROGRESS, { ...op(7), label: "Operation 7" });
    });
    await waitFor(() =>
      expect(within(overlay).getAllByRole("listitem")).toHaveLength(7),
    );
    expect(within(overlay).getAllByRole("listitem")[0]).toHaveTextContent("Operation 7");

    act(() => {
      fireBusEvent(OPERATION_PROGRESS, { ...op(7), state: "finished" });
    });
    await waitFor(() =>
      expect(within(overlay).getAllByRole("listitem")).toHaveLength(6),
    );
    expect(within(overlay).queryByText("Operation 7")).not.toBeInTheDocument();
  });
});
