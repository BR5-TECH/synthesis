import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

import type { ProjectHandle, ProjectWindow, ProjectWindowsSnapshot } from "../types";

const api = vi.hoisted(() => ({
  listProjectWindows: vi.fn(),
  currentProjectWindow: vi.fn(),
  attachProjectToWindow: vi.fn(),
  openProjectAtPath: vi.fn(),
  openProjectWindow: vi.fn(),
  activateProjectWindow: vi.fn(),
  requestProjectWindowClose: vi.fn(),
  closeProject: vi.fn(),
  closeProjectWindow: vi.fn(),
  appendLogRecords: vi.fn(),
}));
vi.mock("../api", () => api);

const prefs = vi.hoisted(() => ({
  loadAppPreferences: vi.fn(),
  patchAppPreferences: vi.fn(),
  resetAppPreferencesCache: vi.fn(),
}));
vi.mock("../state/appPreferences", () => prefs);

const listeners = vi.hoisted(() => ({
  changed: null as null | ((s: ProjectWindowsSnapshot) => void),
  closeRequested: null as null | ((projectId: string) => void),
  menu: new Map<string, () => void>(),
}));
vi.mock("../events", () => ({
  MENU_NEW_TAB: "menu:new-tab",
  onProjectWindowsChanged: (fn: (s: ProjectWindowsSnapshot) => void) => {
    listeners.changed = fn;
    return Promise.resolve(() => {});
  },
  onProjectWindowCloseRequested: (fn: (projectId: string) => void) => {
    listeners.closeRequested = fn;
    return Promise.resolve(() => {});
  },
  onMenuEvent: (event: string, run: () => void) => {
    listeners.menu.set(event, run);
    return Promise.resolve(() => {});
  },
}));

import { setWindowProjectId } from "../api/invoke";
import { resetLogBufferForTest } from "../logging";
import { countRunsInFlight, useProjectTabs, type ProjectTabsDeps } from "./useProjectTabs";

function handle(name: string): ProjectHandle {
  return {
    name,
    path: `/work/${name}`,
    activeWorktreePath: `/work/${name}`,
    rememberedWorktreeUnavailable: false,
  };
}

function win(name: string, over: Partial<ProjectWindow> = {}): ProjectWindow {
  return {
    projectId: `/work/${name}`,
    name,
    windowLabel: `project-${name}`,
    needsAttention: false,
    ...over,
  };
}

function deps(over: Partial<ProjectTabsDeps> = {}): ProjectTabsDeps {
  return {
    screen: "ide",
    loadProject: vi.fn(),
    flushBeforeTeardown: vi.fn().mockResolvedValue(true),
    requestCloseProject: vi.fn().mockResolvedValue(undefined),
    runningOperations: 0,
    ...over,
  };
}

function mount(d: ProjectTabsDeps) {
  return renderHook((props: ProjectTabsDeps) => useProjectTabs(props), { initialProps: d });
}

function setPlatform(platform: string) {
  Object.defineProperty(window.navigator, "platform", { value: platform, configurable: true });
}

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset();
  prefs.loadAppPreferences.mockReset().mockResolvedValue({ theme: "system" });
  prefs.patchAppPreferences.mockReset().mockResolvedValue({ theme: "system" });
  api.listProjectWindows.mockResolvedValue({ windows: [], activeProjectId: null });
  api.currentProjectWindow.mockResolvedValue(null);
  api.attachProjectToWindow.mockResolvedValue(undefined);
  api.openProjectAtPath.mockImplementation(async (path: string) => handle(path.split("/").pop() ?? ""));
  api.openProjectWindow.mockResolvedValue({ projectId: "", windowLabel: "", created: true });
  api.activateProjectWindow.mockResolvedValue(undefined);
  api.requestProjectWindowClose.mockResolvedValue(undefined);
  api.closeProject.mockResolvedValue(undefined);
  api.closeProjectWindow.mockResolvedValue({ lastClosed: false });
  api.appendLogRecords.mockResolvedValue(undefined);
  resetLogBufferForTest();
  listeners.changed = null;
  listeners.closeRequested = null;
  listeners.menu.clear();
  setWindowProjectId(null);
  setPlatform("Linux x86_64");
});

afterEach(() => {
  cleanup();
  setWindowProjectId(null);
  resetLogBufferForTest();
});

describe("the list of project tabs", () => {
  it("shows the tabs that the backend publishes, in its order", async () => {
    const { result } = mount(deps());
    await waitFor(() => expect(listeners.changed).not.toBeNull());
    act(() =>
      listeners.changed?.({ windows: [win("a"), win("b")], activeProjectId: "/work/a" }),
    );
    expect(result.current.stripWindows.map((w) => w.name)).toEqual(["a", "b"]);
  });

  it("keeps the needs-attention flag of a background tab", async () => {
    const { result } = mount(deps());
    await waitFor(() => expect(listeners.changed).not.toBeNull());
    act(() =>
      listeners.changed?.({
        windows: [win("a"), win("b", { needsAttention: true })],
        activeProjectId: "/work/a",
      }),
    );
    expect(result.current.stripWindows[1].needsAttention).toBe(true);
  });

  it("PJT-FR-PBQT: gives the strip no tabs on macOS, where the system draws the tab bar", async () => {
    setPlatform("MacIntel");
    const { result } = mount(deps());
    await waitFor(() => expect(listeners.changed).not.toBeNull());
    act(() =>
      listeners.changed?.({ windows: [win("a"), win("b")], activeProjectId: "/work/a" }),
    );
    expect(result.current.stripWindows).toEqual([]);
  });
});

describe("starting the application", () => {
  it("shows the Project picker when no window holds a project, and restores no tab", async () => {
    const d = deps({ screen: "picker" });
    mount(d);
    await waitFor(() => expect(api.currentProjectWindow).toHaveBeenCalled());
    expect(api.openProjectAtPath).not.toHaveBeenCalled();
    expect(d.loadProject).not.toHaveBeenCalled();
  });

  it("mounts the project of a window that was opened for one", async () => {
    api.currentProjectWindow.mockResolvedValue(win("b"));
    const d = deps({ screen: "picker" });
    mount(d);
    await waitFor(() => expect(d.loadProject).toHaveBeenCalled());
    expect(api.openProjectAtPath).toHaveBeenCalledWith("/work/b");
    expect(api.attachProjectToWindow).toHaveBeenCalledWith("/work/b");
  });
});

describe("choosing a project (PJT-FR-GASB)", () => {
  it("opens it in this window at once while this window shows the picker", async () => {
    const d = deps({ screen: "picker" });
    const { result } = mount(d);
    await act(() => result.current.chooseProject(handle("a")));
    expect(api.attachProjectToWindow).toHaveBeenCalledWith("/work/a");
    expect(d.loadProject).toHaveBeenCalled();
    expect(result.current.pending).toBeNull();
  });

  it("asks This tab or New tab while a project is open", async () => {
    const { result } = mount(deps());
    await act(() => result.current.chooseProject(handle("b")));
    expect(result.current.pending?.name).toBe("b");
    expect(api.openProjectAtPath).not.toHaveBeenCalled();
  });

  it("opens a new project window for New tab and leaves this project alone", async () => {
    const d = deps();
    const { result } = mount(d);
    await act(() => result.current.chooseProject(handle("b")));
    await act(() => result.current.answerDialog("new-tab"));
    expect(api.openProjectWindow).toHaveBeenCalledWith("/work/b");
    expect(api.closeProject).not.toHaveBeenCalled();
    expect(d.loadProject).not.toHaveBeenCalled();
  });

  it("replaces the project of this window for This tab, after the write", async () => {
    const d = deps();
    const { result } = mount(d);
    await act(() => result.current.chooseProject(handle("b")));
    await act(() => result.current.answerDialog("this-tab"));
    expect(d.flushBeforeTeardown).toHaveBeenCalled();
    expect(api.closeProject).toHaveBeenCalled();
    expect(api.attachProjectToWindow).toHaveBeenCalledWith("/work/b");
    expect(d.loadProject).toHaveBeenCalled();
  });

  it("keeps the project open when a write is blocked on This tab", async () => {
    const d = deps({ flushBeforeTeardown: vi.fn().mockResolvedValue(false) });
    const { result } = mount(d);
    await act(() => result.current.chooseProject(handle("b")));
    await act(() => result.current.answerDialog("this-tab"));
    expect(api.closeProject).not.toHaveBeenCalled();
    expect(api.openProjectAtPath).not.toHaveBeenCalled();
    expect(d.loadProject).not.toHaveBeenCalled();
  });

  it("changes nothing on Cancel", async () => {
    const d = deps();
    const { result } = mount(d);
    await act(() => result.current.chooseProject(handle("b")));
    act(() => result.current.cancelDialog());
    expect(result.current.pending).toBeNull();
    expect(api.openProjectWindow).not.toHaveBeenCalled();
    expect(api.closeProject).not.toHaveBeenCalled();
  });

  it("activates the tab of a project that is open already, whatever the choice", async () => {
    const { result } = mount(deps());
    await waitFor(() => expect(listeners.changed).not.toBeNull());
    act(() =>
      listeners.changed?.({ windows: [win("a"), win("b")], activeProjectId: "/work/a" }),
    );
    await act(() => result.current.chooseProject(handle("b")));
    expect(api.activateProjectWindow).toHaveBeenCalledWith("/work/b");
    expect(result.current.pending).toBeNull();
    expect(api.openProjectWindow).not.toHaveBeenCalled();
  });
});

describe("the Project picker modal (PJT-FR-IOEV)", () => {
  it("opens with the New Tab menu item while a project is open", async () => {
    const { result } = mount(deps());
    await waitFor(() => expect(listeners.menu.has("menu:new-tab")).toBe(true));
    act(() => listeners.menu.get("menu:new-tab")?.());
    expect(result.current.pickerOpen).toBe(true);
  });

  it("does not open over the Project picker window", async () => {
    const { result } = mount(deps({ screen: "picker" }));
    await waitFor(() => expect(listeners.menu.has("menu:new-tab")).toBe(true));
    act(() => listeners.menu.get("menu:new-tab")?.());
    expect(result.current.pickerOpen).toBe(false);
  });

  it("closes when a choice succeeds", async () => {
    const { result } = mount(deps());
    act(() => result.current.openPicker());
    await act(() => result.current.chooseProject(handle("b")));
    await act(() => result.current.answerDialog("new-tab"));
    expect(result.current.pickerOpen).toBe(false);
  });

  it("stays open, with the error, when the choice fails", async () => {
    api.openProjectWindow.mockRejectedValue("not a project");
    const { result } = mount(deps());
    act(() => result.current.openPicker());
    await act(() => result.current.chooseProject(handle("b")));
    await act(() => result.current.answerDialog("new-tab"));
    expect(result.current.pickerOpen).toBe(true);
    expect(result.current.error).toContain("not a project");
  });
});

describe("closing a project tab (PJT-FR-PCCL)", () => {
  it("closes at once when no run is in flight", async () => {
    const d = deps();
    const { result } = mount(d);
    await act(() => result.current.requestClose());
    expect(d.requestCloseProject).toHaveBeenCalledTimes(1);
    expect(result.current.confirmingClose).toBe(false);
  });

  it("asks first when a run is in flight, and stops nothing on Cancel", async () => {
    const d = deps({ runningOperations: 1 });
    const { result } = mount(d);
    await act(() => result.current.requestClose());
    expect(result.current.confirmingClose).toBe(true);
    expect(d.requestCloseProject).not.toHaveBeenCalled();
    act(() => result.current.cancelClose());
    expect(result.current.confirmingClose).toBe(false);
    expect(d.requestCloseProject).not.toHaveBeenCalled();
  });

  it("closes after the confirmation", async () => {
    const d = deps({ runningOperations: 2 });
    const { result } = mount(d);
    await act(() => result.current.requestClose());
    await act(() => result.current.confirmClose());
    expect(d.requestCloseProject).toHaveBeenCalledTimes(1);
  });

  it("runs the close flow when the backend asks this window for it", async () => {
    setWindowProjectId("/work/a");
    const d = deps();
    mount(d);
    await waitFor(() => expect(listeners.closeRequested).not.toBeNull());
    await act(async () => listeners.closeRequested?.("/work/a"));
    expect(d.requestCloseProject).toHaveBeenCalledTimes(1);
  });

  it("ignores a close request for the project of another window", async () => {
    setWindowProjectId("/work/a");
    const d = deps();
    mount(d);
    await waitFor(() => expect(listeners.closeRequested).not.toBeNull());
    await act(async () => listeners.closeRequested?.("/work/b"));
    expect(d.requestCloseProject).not.toHaveBeenCalled();
  });

  it("closes the own tab from the strip with the same flow", async () => {
    setWindowProjectId("/work/a");
    const d = deps();
    const { result } = mount(d);
    await act(async () => result.current.closeTab("/work/a"));
    expect(d.requestCloseProject).toHaveBeenCalledTimes(1);
  });

  it("activates another tab first, then asks its window to close it", async () => {
    setWindowProjectId("/work/a");
    const d = deps();
    const { result } = mount(d);
    await act(async () => result.current.closeTab("/work/b"));
    await waitFor(() => expect(api.requestProjectWindowClose).toHaveBeenCalledWith("/work/b"));
    expect(api.activateProjectWindow).toHaveBeenCalledWith("/work/b");
    expect(d.requestCloseProject).not.toHaveBeenCalled();
  });

  it("activates a tab on a click in the strip", async () => {
    const { result } = mount(deps());
    act(() => result.current.activateTab("/work/b"));
    expect(api.activateProjectWindow).toHaveBeenCalledWith("/work/b");
  });
});

describe("countRunsInFlight (PJT-FR-OJZG)", () => {
  const op = (id: string, kind: string) => ({
    id,
    kind,
    label: id,
    state: "running" as const,
    sequence: 1,
  });

  it("counts an agent turn and a graduation run", () => {
    expect(countRunsInFlight([op("a", "agent"), op("g", "graduation")])).toBe(2);
  });

  it("does not count indexing, embeddings, searches or stream updates", () => {
    expect(
      countRunsInFlight([
        op("i", "scan"),
        op("e", "embeddings"),
        op("s", "search"),
        op("u", "stream_update"),
      ]),
    ).toBe(0);
  });
});

function openNewTabModal(result: { current: ReturnType<typeof useProjectTabs> }) {
  act(() => listeners.menu.get("menu:new-tab")?.());
  expect(result.current.pickerOpen).toBe(true);
  expect(result.current.pickerIntent).toBe("new-tab");
}

describe("the New Tab modal (PJT-FR-NWTD)", () => {
  it("PJT-FR-NWTD, PJT-FR-PILG: opens the chosen project in a new tab with no dialog", async () => {
    const d = deps();
    const { result } = mount(d);
    await waitFor(() => expect(listeners.menu.has("menu:new-tab")).toBe(true));
    openNewTabModal(result);
    await act(() => result.current.chooseProject(handle("b")));
    expect(result.current.pending).toBeNull();
    expect(api.openProjectWindow).toHaveBeenCalledWith("/work/b");
    expect(api.closeProject).not.toHaveBeenCalled();
    expect(result.current.pickerOpen).toBe(false);
  });

  it("PJT-FR-NWTD: opens a new tab even while the remembered choice is This tab", async () => {
    prefs.loadAppPreferences.mockResolvedValue({ theme: "system", openProjectBehaviour: "this_tab" });
    const d = deps();
    const { result } = mount(d);
    await waitFor(() => expect(listeners.menu.has("menu:new-tab")).toBe(true));
    openNewTabModal(result);
    await act(() => result.current.chooseProject(handle("b")));
    expect(api.openProjectWindow).toHaveBeenCalledWith("/work/b");
    expect(d.flushBeforeTeardown).not.toHaveBeenCalled();
  });

  it("PJT-FR-NWTD, PJT-FR-ZRZD: activates a project that a tab holds already", async () => {
    const { result } = mount(deps());
    await waitFor(() => expect(listeners.changed).not.toBeNull());
    act(() => listeners.changed?.({ windows: [win("a"), win("b")], activeProjectId: "/work/a" }));
    openNewTabModal(result);
    await act(() => result.current.chooseProject(handle("b")));
    expect(api.activateProjectWindow).toHaveBeenCalledWith("/work/b");
    expect(api.openProjectWindow).not.toHaveBeenCalled();
    expect(result.current.pickerOpen).toBe(false);
  });

  it("PJT-FR-IOEV: Open Another Project… still asks, also after a New Tab modal closed", async () => {
    const { result } = mount(deps());
    await waitFor(() => expect(listeners.menu.has("menu:new-tab")).toBe(true));
    openNewTabModal(result);
    act(() => result.current.closePicker());
    act(() => result.current.openPicker());
    expect(result.current.pickerIntent).toBe("ask");
    await act(() => result.current.chooseProject(handle("b")));
    expect(result.current.pending?.name).toBe("b");
    expect(api.openProjectWindow).not.toHaveBeenCalled();
  });
});

describe("an open in progress (PJT-FR-PNDG)", () => {
  it("PJT-FR-PNDG: names the project while it opens, and accepts no second open", async () => {
    let finish!: () => void;
    api.openProjectWindow.mockImplementation(
      () => new Promise((resolve) => (finish = () => resolve({ projectId: "/work/b", windowLabel: "project-1", created: true }))),
    );
    const { result } = mount(deps());
    await waitFor(() => expect(listeners.menu.has("menu:new-tab")).toBe(true));
    openNewTabModal(result);
    let first!: Promise<void>;
    act(() => {
      first = result.current.chooseProject(handle("b"));
    });
    await waitFor(() => expect(result.current.opening).toBe("b"));
    await act(() => result.current.chooseProject(handle("c")));
    expect(api.openProjectWindow).toHaveBeenCalledTimes(1);
    await act(async () => {
      finish();
      await first;
    });
    expect(result.current.opening).toBe("");
    expect(result.current.pickerOpen).toBe(false);
  });

  it("PJT-FR-PNDG, PJT-FR-RMBR: ignores a second choice while a remembered choice is read", async () => {
    const releases: ((v: unknown) => void)[] = [];
    prefs.loadAppPreferences.mockImplementation(
      () => new Promise((resolve) => releases.push(resolve)),
    );
    const { result } = mount(deps());
    let first!: Promise<void>;
    act(() => {
      first = result.current.chooseProject(handle("b"));
    });
    await act(() => result.current.chooseProject(handle("c")));
    await act(async () => {
      for (const release of releases) release({ theme: "system", openProjectBehaviour: "new_tab" });
      await first;
    });
    expect(api.openProjectWindow).toHaveBeenCalledTimes(1);
    expect(api.openProjectWindow).toHaveBeenCalledWith("/work/b");
  });

  it("PJT-FR-PNDG: a failure shows the error and keeps the modal open", async () => {
    api.openProjectWindow.mockRejectedValue(new Error("not a project"));
    const { result } = mount(deps());
    await waitFor(() => expect(listeners.menu.has("menu:new-tab")).toBe(true));
    openNewTabModal(result);
    await act(() => result.current.chooseProject(handle("b")));
    expect(result.current.error).toContain("not a project");
    expect(result.current.pickerOpen).toBe(true);
    expect(result.current.opening).toBe("");
  });
});

describe("the remembered choice (PJT-FR-RMBR)", () => {
  it("PJT-FR-RMBR: a remembered New tab opens a switcher choice in a new tab with no dialog", async () => {
    prefs.loadAppPreferences.mockResolvedValue({ theme: "system", openProjectBehaviour: "new_tab" });
    const d = deps();
    const { result } = mount(d);
    await act(() => result.current.chooseProject(handle("b")));
    expect(result.current.pending).toBeNull();
    expect(api.openProjectWindow).toHaveBeenCalledWith("/work/b");
  });

  it("PJT-FR-RMBR: a remembered This tab replaces the project here with no dialog", async () => {
    prefs.loadAppPreferences.mockResolvedValue({ theme: "system", openProjectBehaviour: "this_tab" });
    const d = deps();
    const { result } = mount(d);
    await act(() => result.current.chooseProject(handle("b")));
    expect(result.current.pending).toBeNull();
    expect(d.flushBeforeTeardown).toHaveBeenCalled();
    expect(api.attachProjectToWindow).toHaveBeenCalledWith("/work/b");
    expect(api.openProjectWindow).not.toHaveBeenCalled();
  });

  it("PJT-FR-GASB, PJT-FR-RMBR: a record with no value asks", async () => {
    prefs.loadAppPreferences.mockResolvedValue({ theme: "system" });
    const { result } = mount(deps());
    await act(() => result.current.chooseProject(handle("b")));
    expect(result.current.pending?.name).toBe("b");
  });

  it("PJT-FR-RMBR: Remember my choice stores the choice before the project opens", async () => {
    const order: string[] = [];
    prefs.patchAppPreferences.mockImplementation(async () => {
      order.push("store");
      return {};
    });
    api.openProjectWindow.mockImplementation(async () => {
      order.push("open");
      return { projectId: "/work/b", windowLabel: "project-1", created: true };
    });
    const { result } = mount(deps());
    await act(() => result.current.chooseProject(handle("b")));
    await act(() => result.current.answerDialog("new-tab", true));
    expect(prefs.patchAppPreferences).toHaveBeenCalledWith({ openProjectBehaviour: "new_tab" });
    expect(order).toEqual(["store", "open"]);
  });

  it("PJT-FR-RMBR: This tab with Remember my choice stores this_tab", async () => {
    const { result } = mount(deps());
    await act(() => result.current.chooseProject(handle("b")));
    await act(() => result.current.answerDialog("this-tab", true));
    expect(prefs.patchAppPreferences).toHaveBeenCalledWith({ openProjectBehaviour: "this_tab" });
  });

  it("PJT-FR-RMBR: a failed store does not stop the open", async () => {
    prefs.patchAppPreferences.mockRejectedValue(new Error("disk full"));
    const { result } = mount(deps());
    await act(() => result.current.chooseProject(handle("b")));
    await act(() => result.current.answerDialog("new-tab", true));
    expect(api.openProjectWindow).toHaveBeenCalledWith("/work/b");
  });

  it("PJT-FR-RMBR, PJT-FR-GASB: no store without the box, and none on Cancel", async () => {
    const { result } = mount(deps());
    await act(() => result.current.chooseProject(handle("b")));
    await act(() => result.current.answerDialog("new-tab"));
    await act(() => result.current.chooseProject(handle("c")));
    act(() => result.current.cancelDialog());
    expect(prefs.patchAppPreferences).not.toHaveBeenCalled();
  });
});

describe("a window that open project window created (PWN-FR-KQZB)", () => {
  it("PWN-FR-KQZB: names its project while it mounts it, then shows the project", async () => {
    api.currentProjectWindow.mockResolvedValue(win("b"));
    const d = deps({ screen: "picker", boot: { name: "b" } });
    const { result } = mount(d);
    expect(result.current.boot).toEqual({ name: "b", error: "" });
    await waitFor(() => expect(d.loadProject).toHaveBeenCalled());
    await waitFor(() => expect(result.current.boot).toBeNull());
  });

  it("PWN-FR-KQZB: shows the failure of the mount, and closes its own entry on request", async () => {
    api.currentProjectWindow.mockResolvedValue(win("b"));
    api.openProjectAtPath.mockRejectedValue(new Error("not a project"));
    const { result } = mount(deps({ screen: "picker", boot: { name: "b" } }));
    await waitFor(() => expect(result.current.boot?.error).toContain("not a project"));
    await act(() => result.current.closeBootWindow());
    expect(api.closeProjectWindow).toHaveBeenCalledWith("/work/b");
    expect(result.current.boot).not.toBeNull();
  });

  it("PWN-FR-KQZB, PWN-FR-QEGI: the last window stays and shows the Project picker", async () => {
    api.currentProjectWindow.mockResolvedValue(win("b"));
    api.openProjectAtPath.mockRejectedValue(new Error("not a project"));
    api.closeProjectWindow.mockResolvedValue({ lastClosed: true });
    const { result } = mount(deps({ screen: "picker", boot: { name: "b" } }));
    await waitFor(() => expect(result.current.boot?.error).not.toBe(""));
    await act(() => result.current.closeBootWindow());
    expect(result.current.boot).toBeNull();
  });

  it("PWN-FR-KQZB: a window with no entry in the list shows the Project picker", async () => {
    api.currentProjectWindow.mockResolvedValue(null);
    const { result } = mount(deps({ screen: "picker", boot: { name: "b" } }));
    await waitFor(() => expect(result.current.boot).toBeNull());
    expect(api.openProjectAtPath).not.toHaveBeenCalled();
  });

  it("PWN-FR-KQZB: a window that the backend did not create has no loading state", () => {
    const { result } = mount(deps({ screen: "picker" }));
    expect(result.current.boot).toBeNull();
  });
});

describe("Cancel in the open project dialog (PJT-FR-GASB, PPK-FR-KRPD)", () => {
  it("PJT-FR-GASB, PPK-FR-KRPD: returns to the modal of Open Another Project…, and stores nothing", async () => {
    const { result } = mount(deps());
    act(() => result.current.openPicker());
    await act(() => result.current.chooseProject(handle("b")));
    expect(result.current.pending?.name).toBe("b");
    act(() => result.current.cancelDialog());
    expect(result.current.pending).toBeNull();
    expect(result.current.pickerOpen).toBe(true);
    expect(result.current.pickerIntent).toBe("ask");
    expect(prefs.patchAppPreferences).not.toHaveBeenCalled();
    expect(api.openProjectWindow).not.toHaveBeenCalled();
  });
});
