import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, renderHook, screen, waitFor } from "@testing-library/react";
import { useRef, useState } from "react";

import { resetLogBufferForTest } from "../../logging";
import {
  changeViewOf,
  keepChangeView,
  resetChangeViewsForTest,
} from "../../state/openspec/changeView";
import { openOpenSpecChange } from "../../state/openspec/changeRoutes";
import type { Tab, WorkStreamSummary } from "../../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
/** The listeners the hook subscribed, by event name. */
const listeners = new Map<string, (event: { payload: unknown }) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler);
    return Promise.resolve(() => listeners.delete(name));
  },
  emit: () => Promise.resolve(),
}));
/** VOM-FR-NTKT: the owner voice mode is on, as the test sets it. */
const voice = vi.hoisted(() => ({
  owner: null as import("../../types").DiscussionTarget | null,
  noteOwnerDeleted: vi.fn(),
}));
vi.mock("../../voice/voiceController", async (original) => ({
  ...(await original<typeof import("../../voice/voiceController")>()),
  voiceOwner: () => voice.owner,
  noteOwnerDeleted: voice.noteOwnerDeleted,
}));

import {
  changeTabId,
  createOpenSpecChangeActions,
  useOpenSpecChangeTabs,
} from "./openspecChangeActions";
import { DASHBOARD_TAB } from "./tabRecords";
import { TabStrip } from "../../components/TabStrip";

let streams: Array<{ id: string; name: string }>;

const summary = (id: string, name: string): WorkStreamSummary =>
  ({
    stream: { id, name, isMissing: false },
    queuedRunCount: 0,
    aheadOfBase: 0,
    behindBase: 0,
  }) as unknown as WorkStreamSummary;

beforeEach(() => {
  resetLogBufferForTest();
  resetChangeViewsForTest();
  listeners.clear();
  voice.owner = null;
  voice.noteOwnerDeleted.mockReset();
  streams = [{ id: "s-1", name: "auth-work" }];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "list_work_streams") return streams.map((s) => summary(s.id, s.name));
    throw new Error(`unexpected command ${cmd}`);
  });
});
afterEach(cleanup);

/** The strip, as the shell holds it, with the change actions over it. */
function useStrip() {
  const [tabs, setTabs] = useState<Tab[]>([DASHBOARD_TAB]);
  const [activeTab, setActiveTab] = useState("dashboard");
  const tabsRef = useRef<Tab[]>(tabs);
  tabsRef.current = tabs;
  const actions = createOpenSpecChangeActions({
    tabsRef,
    setTabs,
    setActiveTab,
    activateTab: setActiveTab,
  });
  useOpenSpecChangeTabs(tabs, tabsRef, actions);
  return { tabs, activeTab, ...actions };
}

const STREAM = { kind: "stream" as const, streamId: "s-1" };
const WORKTREE = { kind: "active_worktree" as const };

describe("the change workspace tab", () => {
  it("TAB-FR-02 / OCW-FR-DMGL: an open adds a change workspace tab and focuses it", async () => {
    const { result } = renderHook(useStrip);
    act(() => result.current.openChange(WORKTREE, "add-logout", { file: "proposal.md" }));
    const tab = result.current.tabs[1];
    expect(tab).toMatchObject({
      id: "openspec:worktree:add-logout",
      label: "add-logout",
      kind: "openspecChange",
      openspecChange: { location: WORKTREE, name: "add-logout", file: "proposal.md", openSeq: 1 },
    });
    expect(result.current.activeTab).toBe(tab.id);
  });

  it("TAB-FR-KOTW: the location and the name identify the tab, and a second open focuses it and names its file", async () => {
    const { result } = renderHook(useStrip);
    act(() => result.current.openChange(STREAM, "add-logout", { streamName: "auth-work" }));
    act(() => result.current.openChange(WORKTREE, "add-logout"));
    expect(result.current.tabs).toHaveLength(3);
    expect(changeTabId(STREAM, "add-logout")).not.toBe(changeTabId(WORKTREE, "add-logout"));
    act(() => result.current.openChange(STREAM, "add-logout", { file: "design.md" }));
    expect(result.current.tabs).toHaveLength(3);
    expect(result.current.activeTab).toBe(changeTabId(STREAM, "add-logout"));
    expect(result.current.tabs[1].openspecChange).toMatchObject({
      file: "design.md",
      openSeq: 2,
      streamName: "auth-work",
    });
  });

  it("OCW-FR-XUJN: the label is the change name, and a stream tab reads its stream name where the opener did not give it", async () => {
    const { result } = renderHook(useStrip);
    act(() => result.current.openChange(STREAM, "add-logout"));
    expect(result.current.tabs[1].label).toBe("add-logout");
    await waitFor(() =>
      expect(result.current.tabs[1].openspecChange?.streamName).toBe("auth-work"),
    );
    expect(result.current.tabs[1].tooltip).toBe("add-logout — stream auth-work");
  });

  it("OCW-FR-XUJN: the strip shows the stream name beside the change name", () => {
    const tab: Tab = {
      id: "openspec:stream:s-1:add-logout",
      label: "add-logout",
      kind: "openspecChange",
      openspecChange: { location: STREAM, name: "add-logout", streamName: "auth-work", file: null, openSeq: 1 },
    };
    render(
      <TabStrip
        tabs={[DASHBOARD_TAB, tab]}
        activeId={tab.id}
        onActivate={() => {}}
        onClose={() => {}}
        onHome={() => {}}
      />,
    );
    const shown = screen.getByRole("tab", { name: /add-logout/ });
    expect(shown).toHaveTextContent("add-logout· auth-work");
  });

  it("OCW-FR-DMGL: the route the panels take opens the tab through the shell", async () => {
    const { result } = renderHook(useStrip);
    act(() => openOpenSpecChange(STREAM, "add-logout", { streamName: "auth-work" }));
    expect(result.current.activeTab).toBe(changeTabId(STREAM, "add-logout"));
  });

  it("TAB-FR-DRXS: a tab whose stream leaves the project's streams closes, and a worktree tab stays", async () => {
    const { result } = renderHook(useStrip);
    await waitFor(() => expect(listeners.has("work-streams-changed")).toBe(true));
    act(() => result.current.openChange(WORKTREE, "add-logout"));
    act(() => result.current.openChange(STREAM, "add-logout", { streamName: "auth-work" }));
    // A change that keeps the stream closes nothing.
    act(() => listeners.get("work-streams-changed")!({ payload: {} }));
    await waitFor(() =>
      expect(invokeMock.mock.calls.filter((c) => c[0] === "list_work_streams")).toHaveLength(1),
    );
    expect(result.current.tabs).toHaveLength(3);
    streams = [];
    act(() => listeners.get("work-streams-changed")!({ payload: {} }));
    await waitFor(() => expect(result.current.tabs).toHaveLength(2));
    expect(result.current.tabs.map((t) => t.id)).toEqual([
      "dashboard",
      changeTabId(WORKTREE, "add-logout"),
    ]);
    expect(result.current.activeTab).toBe(changeTabId(WORKTREE, "add-logout"));
  });

  it("TAB-FR-DRXS: with no stream tab open, a streams change reads nothing", async () => {
    const { result } = renderHook(useStrip);
    await waitFor(() => expect(listeners.has("work-streams-changed")).toBe(true));
    act(() => result.current.openChange(WORKTREE, "add-logout"));
    act(() => listeners.get("work-streams-changed")!({ payload: {} }));
    expect(invokeMock.mock.calls.filter((c) => c[0] === "list_work_streams")).toHaveLength(0);
  });

  it("VOM-FR-NTKT: with voice mode on a change and no stream tab open, a streams change lists the streams and ends voice mode when its stream left", async () => {
    voice.owner = { kind: "change", streamId: "s-1", change: "add-logout" };
    const { result } = renderHook(useStrip);
    await waitFor(() => expect(listeners.has("work-streams-changed")).toBe(true));
    act(() => result.current.openChange(WORKTREE, "add-logout"));
    const lists = () => invokeMock.mock.calls.filter((c) => c[0] === "list_work_streams");
    // The first list still holds the stream, and the second does not. The
    // answers land in order, so the first is handled before the second.
    act(() => {
      listeners.get("work-streams-changed")!({ payload: {} });
      streams = [];
      listeners.get("work-streams-changed")!({ payload: {} });
    });
    await waitFor(() =>
      expect(voice.noteOwnerDeleted).toHaveBeenCalledWith({
        kind: "change",
        streamId: "s-1",
        change: "add-logout",
      }),
    );
    expect(voice.noteOwnerDeleted).toHaveBeenCalledTimes(1);
    expect(lists()).toHaveLength(2);
    // The worktree tab is not a tab of the stream, so it stays.
    expect(result.current.tabs.map((t) => t.id)).toEqual(["dashboard", changeTabId(WORKTREE, "add-logout")]);
  });

  it("VOM-FR-NTKT: with voice mode on a discussion that is not of a change and no stream tab open, a streams change reads nothing", async () => {
    voice.owner = { kind: "note", noteId: "n-1" };
    const { result } = renderHook(useStrip);
    await waitFor(() => expect(listeners.has("work-streams-changed")).toBe(true));
    act(() => result.current.openChange(WORKTREE, "add-logout"));
    act(() => listeners.get("work-streams-changed")!({ payload: {} }));
    expect(invokeMock.mock.calls.filter((c) => c[0] === "list_work_streams")).toHaveLength(0);
    expect(voice.noteOwnerDeleted).not.toHaveBeenCalled();
  });

  it("OCW-FR-PFUQ: what a closed tab kept is dropped with it", async () => {
    const { result } = renderHook(useStrip);
    act(() => result.current.openChange(STREAM, "add-logout", { streamName: "auth-work" }));
    const id = changeTabId(STREAM, "add-logout");
    keepChangeView(id, { selected: "design" });
    streams = [];
    await waitFor(() => expect(listeners.has("work-streams-changed")).toBe(true));
    act(() => listeners.get("work-streams-changed")!({ payload: {} }));
    await waitFor(() => expect(result.current.tabs).toHaveLength(1));
    expect(changeViewOf(id).selected).toBeNull();
  });
});
