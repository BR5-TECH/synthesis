/**
 * Voice mode ends with its draft, through the shell (`VOM-voice-mode.md`
 * VOM-FR-LSBQ, `CVP-conversation-presentation.md` CVP-FR-51). The Drafts panel
 * reporting the deletion and the controller ending voice mode are wired
 * together only in `AppShell`.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetPanelReveals } from "./state/panelReveal";
import { getVoiceState, resetVoiceStateForTest } from "./state/voiceSession";
import { enterVoiceMode, resetVoiceControllerForTest } from "./voice/voiceController";
import { resetLogBufferForTest } from "./logging";
import {
  createDraftFromPanel,
  defaultInvoke,
  enterIde,
  resetAppFixture,
} from "./test/appFixtures";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(async () => {}),
  listen: vi.fn(async () => () => {}),
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
vi.mock("./voice/playback", () => ({
  isPlaybackIdle: vi.fn(() => true),
  pausePlayback: vi.fn(),
  playFrame: vi.fn(),
  prepareAudio: vi.fn(),
  resumePlayback: vi.fn(),
  setDrainedListener: vi.fn(),
  setPlaybackFailureListener: vi.fn(),
  beginSegment: vi.fn(),
  endSegmentInput: vi.fn(),
  notePlaybackEntry: vi.fn(),
  stopPlayback: vi.fn(),
}));

beforeEach(() => {
  resetPanelReveals();
  resetAppFixture();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "list_drafts")
      return {
        folders: [],
        drafts: [
          {
            id: "d1",
            name: "Untitled",
            promptPath: "Untitled.md",
            status: "active",
            updatedAt: "2026-07-31T10:00:00Z",
          },
        ],
      };
    return defaultInvoke(cmd, args);
  });
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  resetLogBufferForTest();
  resetVoiceControllerForTest();
  resetVoiceStateForTest();
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  resetVoiceControllerForTest();
  resetVoiceStateForTest();
  vi.unstubAllGlobals();
});

describe("voice mode and the deletion of its draft", () => {
  it("VOM-FR-LSBQ, CVP-FR-51: deleting the voice discussion's draft in the Drafts panel ends voice mode", async () => {
    render(<App />);
    await enterIde();
    await createDraftFromPanel();

    act(() => {
      enterVoiceMode("draft:d1", null, { kind: "draft", draftId: "d1" });
    });
    expect(getVoiceState().activeKey).toBe("draft:d1");
    await screen.findByTestId("status-bar-voice");

    fireEvent.contextMenu(await screen.findByRole("treeitem", { name: /^Draft Untitled/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await userEvent.click(
      within(screen.getByRole("dialog", { name: "Delete draft" })).getByRole("button", {
        name: "Delete",
      }),
    );

    await waitFor(() => expect(getVoiceState().activeKey).toBeNull());
    expect(invokeMock.mock.calls.some((c) => c[0] === "delete_draft")).toBe(true);
    expect(screen.queryByTestId("status-bar-voice")).not.toBeInTheDocument();
  });

  it("VOM-FR-LSBQ: deleting another draft keeps voice mode on", async () => {
    render(<App />);
    await enterIde();
    await createDraftFromPanel();

    act(() => {
      enterVoiceMode("draft:other", null, { kind: "draft", draftId: "other" });
    });
    fireEvent.contextMenu(await screen.findByRole("treeitem", { name: /^Draft Untitled/ }));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await userEvent.click(
      within(screen.getByRole("dialog", { name: "Delete draft" })).getByRole("button", {
        name: "Delete",
      }),
    );

    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "delete_draft")).toBe(true),
    );
    await waitFor(() =>
      expect(within(screen.getByTestId("tabstrip")).queryByText("Untitled")).not.toBeInTheDocument(),
    );
    expect(getVoiceState().activeKey).toBe("draft:other");
  });
});
