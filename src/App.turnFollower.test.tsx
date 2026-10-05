/**
 * CVP-FR-HWTN: the shell follows `"agent turn state changed"` for every
 * discussion, so a turn that ends while no surface of its discussion is mounted
 * leaves the session store at once.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetPanelReveals } from "./state/panelReveal";
import {
  getDiscussionSession,
  hasTurnEnded,
  upsertDiscussionTurn,
} from "./state/discussionSession";
import { defaultInvoke, resetAppFixture } from "./test/appFixtures";
import { artifactDiscussionOrigin } from "./test/origins";
import type { AgentTurn } from "./types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<string, Array<(e: { payload?: unknown }) => void>>,
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(async () => {}),
  listen: vi.fn(async (name: string, handler: (e: { payload?: unknown }) => void) => {
    (eventHandlers[name] ??= []).push(handler);
    return () => {
      eventHandlers[name] = (eventHandlers[name] ?? []).filter((h) => h !== handler);
    };
  }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => {
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
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  return { LogicalSize, getCurrentWindow: () => win, availableMonitors: async () => [] };
});

const EVENT = "agent-turn-state-changed";

beforeEach(() => {
  resetPanelReveals();
  resetAppFixture();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) =>
    defaultInvoke(cmd, args),
  );
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  for (const k in eventHandlers) delete eventHandlers[k];
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function turn(id: string, state: AgentTurn["state"], discussionId = "d1"): AgentTurn {
  return {
    id,
    agentId: "agent-helga",
    nickname: "helga",
    origin: artifactDiscussionOrigin(discussionId, "a.md"),
    triggerCommentId: "c1",
    state,
    failure: null,
    retryPermitted: false,
    startedAt: "2026-02-01T00:00:00Z",
    endedAt: state === "running" ? null : "2026-02-01T00:01:00Z",
    activeToolCalls: [],
  } as unknown as AgentTurn;
}

function fire(payload: AgentTurn): void {
  act(() => {
    for (const h of eventHandlers[EVENT] ?? []) h({ payload });
  });
}

describe("CVP-FR-HWTN: the shell follows agent turns", () => {
  it("a turn that ends with no surface mounted leaves the session store", async () => {
    render(<App />);
    await waitFor(() => expect(eventHandlers[EVENT]?.length ?? 0).toBeGreaterThan(0));
    act(() => upsertDiscussionTurn("d1", turn("t1", "running")));
    fire(turn("t1", "delivered"));
    expect(getDiscussionSession("d1").turns).toEqual([]);
    // A dispatch result that arrives after the end adds nothing back.
    act(() => upsertDiscussionTurn("d1", turn("t1", "running")));
    expect(getDiscussionSession("d1").turns).toEqual([]);
  });

  it("a turn of a discussion nobody opened is remembered as ended", async () => {
    render(<App />);
    await waitFor(() => expect(eventHandlers[EVENT]?.length ?? 0).toBeGreaterThan(0));
    fire(turn("t9", "failed", "d9"));
    expect(hasTurnEnded("t9")).toBe(true);
  });

  it("the shell stops following when it unmounts", async () => {
    const view = render(<App />);
    await waitFor(() => expect(eventHandlers[EVENT]?.length ?? 0).toBeGreaterThan(0));
    view.unmount();
    expect(eventHandlers[EVENT] ?? []).toHaveLength(0);
  });
});
