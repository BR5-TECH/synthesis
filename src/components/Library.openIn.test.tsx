/**
 * The **Open In** submenu of the Project panel context menu
 * (`../../specifications/ui/LCM-library-context-menu.md` LCM-FR-WBHG …
 * LCM-FR-WNPH).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { flushLogs, resetLogBufferForTest } from "../logging";
import { resetPanelReveals } from "../state/panelReveal";
import { resetTerminalsForTest, terminalsSnapshot } from "../state/terminals";
import { baseTree, expandedAll, file, folder, LibraryHost, makeTreeStubs, waitForTree } from "../test/libraryFixtures";
import { fileManagerLabel } from "../platform";
import { terminalFolderOf } from "./Library/useOpenIn";
import { SUBMENU_INTENT_MS } from "./Library/menuPlacement";
import type { TerminalSummary, TerminalTarget } from "../types";

const { mockInvoke } = makeTreeStubs(invokeMock);

const STREAM_TARGET: TerminalTarget = {
  target: { kind: "work_stream", streamId: "s1" },
  name: "stream",
  isActiveWorktree: true,
  busyRunId: null,
  kinds: [],
};
const OTHER_TARGET: TerminalTarget = {
  target: { kind: "worktree", path: "/repo" },
  name: "repo",
  isActiveWorktree: false,
  busyRunId: null,
  kinds: [],
};

function session(id: string): TerminalSummary {
  return {
    id,
    label: "Host shell",
    target: STREAM_TARGET.target,
    targetName: "stream",
    kind: "host_shell",
    runsIn: "host",
    state: "running",
    pinned: false,
    busyRunId: null,
    hasHistory: false,
    persistent: false,
    createdAt: "2026-01-01T00:00:00Z",
  };
}

type Handlers = Record<string, (args?: unknown) => unknown>;

function serve(handlers: Handlers = {}) {
  mockInvoke(
    {
      tree: () => baseTree(),
      open_in_system_terminal: () => undefined,
      reveal_in_file_manager: () => undefined,
      list_terminal_targets: () => [OTHER_TARGET, STREAM_TARGET],
      create_terminal: () => session("t1"),
      ...handlers,
    },
    expandedAll(baseTree()),
  );
}

function renderPanel(props: { hasActiveWorktree?: boolean; withTerminals?: boolean } = {}) {
  const onOpenTerminal = vi.fn<(s: TerminalSummary) => void>();
  render(
    <LibraryHost
      onOpenTerminal={props.withTerminals === false ? undefined : onOpenTerminal}
      hasActiveWorktree={props.hasActiveWorktree ?? true}
    />,
  );
  return { onOpenTerminal };
}

/** The commands the panel sent for the user, without its own tree and state traffic. */
function actions(): { cmd: string; args: unknown }[] {
  const panel = new Set([
    "list_tree_children",
    "list_project_artifacts",
    "load_library_panel_state",
    "save_library_panel_state",
    "append_log_records",
  ]);
  return invokeMock.mock.calls
    .filter((c) => !panel.has(c[0] as string))
    .map((c) => ({ cmd: c[0] as string, args: c[1] }));
}

/** Right-click `name` and open the Open In submenu by hover. Returns the menu. */
async function openSubmenu(name: string): Promise<HTMLElement> {
  fireEvent.contextMenu(await screen.findByText(name));
  const menu = document.querySelector(".menu") as HTMLElement;
  fireEvent.mouseOver(within(menu).getByText("Open In").closest(".menu-item--parent") as HTMLElement);
  return menu;
}

function subEntries(menu: HTMLElement): string[] {
  const parent = within(menu).getByText("Open In").closest(".menu-item--parent") as HTMLElement;
  const sub = parent.querySelector(".menu--sub") as HTMLElement;
  return Array.from(sub.querySelectorAll<HTMLElement>(".menu-item")).map((e) => e.textContent!.trim());
}

interface SentRecord {
  level: string;
  domains: string[];
  message: string;
  fields: Record<string, unknown>;
}

async function loggedRecords(): Promise<string> {
  flushLogs();
  await waitFor(() => expect(invokeMock.mock.calls.some((c) => c[0] === "append_log_records")).toBe(true));
  return JSON.stringify(invokeMock.mock.calls.filter((c) => c[0] === "append_log_records").map((c) => c[1]));
}

/** LCM-FR-WNPH: the one WARN record of a failed entry, with its exact fields. */
async function expectOneFailureRecord(fields: { entry: string; error: string }) {
  flushLogs();
  const failures = () =>
    invokeMock.mock.calls
      .filter((c) => c[0] === "append_log_records")
      .flatMap((c) => (c[1] as { records: SentRecord[] }).records)
      .filter((r) => r.message === "project panel: open in failed");
  await waitFor(() => expect(failures()).toHaveLength(1));
  expect(failures()[0]).toMatchObject({ level: "WARN", domains: ["frontend"], fields });
  expect(Object.keys(failures()[0].fields).sort()).toEqual(["entry", "error"]);
}

function setPlatform(value: string) {
  vi.spyOn(window.navigator, "platform", "get").mockReturnValue(value);
}

beforeEach(() => {
  resetLogBufferForTest();
  resetPanelReveals();
  resetTerminalsForTest();
  invokeMock.mockReset();
  setPlatform("MacIntel");
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("Open In submenu", () => {
  it("LCM-FR-01, LCM-FR-WBHG: file and folder menus offer Open In between Rename and Artifact Type", async () => {
    serve();
    renderPanel();
    await waitForTree();
    for (const name of ["AGENTS.md", "specifications"]) {
      fireEvent.contextMenu(screen.getByText(name));
      const menu = document.querySelector(".menu") as HTMLElement;
      const top = Array.from(menu.children)
        .map((el) => (el.classList.contains("menu-item--parent") ? el.firstElementChild : el))
        .map((el) => el?.textContent?.trim() ?? "")
        .filter((text) => text !== "");
      const rename = top.indexOf("Rename");
      expect(top.slice(rename, rename + 3)).toEqual(["Rename", "Open In", "Artifact Type"]);
      const parent = within(menu).getByText("Open In").closest(".menu-item--parent") as HTMLElement;
      expect(parent.previousElementSibling).toHaveClass("menu-sep");
      fireEvent.keyDown(window, { key: "Escape" });
    }
  });

  it("LCM-FR-WBHG, LCM-FR-07: hover opens the three entries in order, each with a leading icon", async () => {
    serve();
    renderPanel();
    const menu = await openSubmenu("AGENTS.md");
    expect(subEntries(menu)).toEqual(["System Terminal", "Terminal Tab", "Finder"]);
    const parent = within(menu).getByText("Open In").closest(".menu-item--parent") as HTMLElement;
    for (const item of Array.from(parent.querySelectorAll<HTMLElement>(".menu-item"))) {
      expect(item.firstElementChild).toHaveClass("icon");
    }
  });

  it("LCM-FR-WBHG: a click also opens the submenu, and only one submenu is open at a time", async () => {
    serve();
    renderPanel();
    fireEvent.contextMenu(await screen.findByText("AGENTS.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Open In"));
    expect(within(menu).getByText("System Terminal")).toBeInTheDocument();
    fireEvent.click(within(menu).getByText("Artifact Type"));
    expect(within(menu).getByText("Scratchpad")).toBeInTheDocument();
    expect(within(menu).queryByText("System Terminal")).not.toBeInTheDocument();
    fireEvent.click(within(menu).getByText("Open In"));
    expect(within(menu).queryByText("Scratchpad")).not.toBeInTheDocument();
  });
});

describe("System Terminal", () => {
  it("LCM-FR-PAUK: sends the node path for a file and a folder, closes the menu, and asks no write confirmation", async () => {
    serve();
    renderPanel();
    await waitForTree();
    for (const name of ["onboarding.md", "specifications"]) {
      const menu = await openSubmenu(name);
      fireEvent.click(within(menu).getByText("System Terminal"));
      expect(document.querySelector(".menu")).toBeNull();
    }
    await waitFor(() => expect(actions()).toHaveLength(2));
    expect(actions()).toEqual([
      { cmd: "open_in_system_terminal", args: { path: ".claude/skills/onboarding.md" } },
      { cmd: "open_in_system_terminal", args: { path: "specifications" } },
    ]);
  });

  it("LCM-FR-WNPH: a failure shows inline, keeps the tree, and logs one WARN with no path", async () => {
    serve({
      open_in_system_terminal: () => {
        throw "no_terminal_found";
      },
    });
    renderPanel();
    const menu = await openSubmenu("onboarding.md");
    fireEvent.click(within(menu).getByText("System Terminal"));
    expect(await screen.findByText("No terminal application was found.")).toBeInTheDocument();
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();
    expect(actions().map((a) => a.cmd)).toEqual(["open_in_system_terminal"]);
    await expectOneFailureRecord({ entry: "system_terminal", error: "no_terminal_found" });
    expect(await loggedRecords()).not.toContain("onboarding");
  });

  it("LCM-FR-WNPH: an untyped error is logged as unknown, so a path in it does not reach the log", async () => {
    serve({
      open_in_system_terminal: () => {
        throw "io error at /Users/me/secret/project";
      },
    });
    renderPanel();
    const menu = await openSubmenu("AGENTS.md");
    fireEvent.click(within(menu).getByText("System Terminal"));
    expect(await screen.findByText(/The item did not open/)).toBeInTheDocument();
    await expectOneFailureRecord({ entry: "system_terminal", error: "unknown" });
    expect(await loggedRecords()).not.toContain("/Users/me");
  });
});

describe("File manager entry", () => {
  it.each([
    ["MacIntel", "Finder"],
    ["Win32", "Explorer"],
    ["Linux x86_64", "File Manager"],
  ])("LCM-FR-VTLR: on %s the entry reads %s", async (platform, label) => {
    setPlatform(platform);
    serve();
    renderPanel();
    const menu = await openSubmenu("AGENTS.md");
    expect(subEntries(menu).slice(-1)).toEqual([label]);
  });

  it("LCM-FR-VTLR: sends the node path itself, also for a file inside a folder", async () => {
    serve();
    renderPanel();
    const menu = await openSubmenu("onboarding.md");
    fireEvent.click(within(menu).getByText("Finder"));
    expect(document.querySelector(".menu")).toBeNull();
    await waitFor(() =>
      expect(actions()).toEqual([
        { cmd: "reveal_in_file_manager", args: { path: ".claude/skills/onboarding.md" } },
      ]),
    );
  });

  it("LCM-FR-WNPH: a reveal of a path that is gone shows inline", async () => {
    serve({
      reveal_in_file_manager: () => {
        throw "path not found";
      },
    });
    renderPanel();
    const menu = await openSubmenu("AGENTS.md");
    fireEvent.click(within(menu).getByText("Finder"));
    expect(await screen.findByText("This item is no longer on disk.")).toBeInTheDocument();
    await expectOneFailureRecord({ entry: "file_manager", error: "path not found" });
  });
});

describe("Terminal Tab", () => {
  it.each([
    ["onboarding.md", ".claude/skills"],
    ["specifications", "specifications"],
    ["AGENTS.md", null],
  ])(
    "LCM-FR-XWFO, LCM-FR-PKIH: %s creates a Host shell on the active target in %s and opens its tab",
    async (name, startFolder) => {
      serve();
      const { onOpenTerminal } = renderPanel();
      await waitForTree();
      const menu = await openSubmenu(name);
      fireEvent.click(within(menu).getByText("Terminal Tab"));
      expect(document.querySelector(".menu")).toBeNull();
      await waitFor(() => expect(onOpenTerminal).toHaveBeenCalledTimes(1));
      expect(onOpenTerminal.mock.calls[0][0].id).toBe("t1");
      expect(terminalsSnapshot().sessions.map((t) => t.id)).toEqual(["t1"]);
      expect(actions()).toEqual([
        { cmd: "list_terminal_targets", args: undefined },
        {
          cmd: "create_terminal",
          args: { target: STREAM_TARGET.target, kind: "host_shell", vendor: null, startFolder },
        },
      ]);
    },
  );

  it("LCM-FR-NJVW: on Windows the entry is not rendered", async () => {
    setPlatform("Win32");
    serve();
    renderPanel();
    const menu = await openSubmenu("AGENTS.md");
    expect(subEntries(menu)).toEqual(["System Terminal", "Explorer"]);
  });

  it.each([
    ["no active worktree", { hasActiveWorktree: false }],
    ["no terminal surface", { withTerminals: false }],
  ])("LCM-FR-NJVW: with %s the entry is greyed and does nothing", async (_, props) => {
    serve();
    renderPanel(props);
    const menu = await openSubmenu("AGENTS.md");
    const entry = within(menu).getByText("Terminal Tab").closest(".menu-item") as HTMLElement;
    expect(entry).toHaveClass("menu-item--disabled");
    expect(entry).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(entry);
    expect(document.querySelector(".menu")).not.toBeNull();
    expect(actions()).toEqual([]);
  });

  it("LCM-FR-XWFO, LCM-FR-WNPH: no target for the active worktree is an inline error, and nothing is created", async () => {
    serve({ list_terminal_targets: () => [OTHER_TARGET] });
    const { onOpenTerminal } = renderPanel();
    const menu = await openSubmenu("AGENTS.md");
    fireEvent.click(within(menu).getByText("Terminal Tab"));
    expect(await screen.findByText("This worktree has no terminal target.")).toBeInTheDocument();
    expect(actions().map((a) => a.cmd)).toEqual(["list_terminal_targets"]);
    expect(onOpenTerminal).not.toHaveBeenCalled();
    await expectOneFailureRecord({ entry: "terminal_tab", error: "no_active_target" });
  });

  it("LCM-FR-WNPH: a failed create shows the terminal error inline and logs its code", async () => {
    serve({
      create_terminal: () => {
        throw { code: "terminal_limit_reached" };
      },
    });
    const { onOpenTerminal } = renderPanel();
    const menu = await openSubmenu("AGENTS.md");
    fireEvent.click(within(menu).getByText("Terminal Tab"));
    expect(await screen.findByText("The project holds the most terminals it can hold (16).")).toBeInTheDocument();
    expect(onOpenTerminal).not.toHaveBeenCalled();
    expect(terminalsSnapshot().sessions).toEqual([]);
    await expectOneFailureRecord({ entry: "terminal_tab", error: "terminal_limit_reached" });
  });
});

describe("pure helpers", () => {
  it("LCM-FR-PKIH: the terminal folder is the folder itself, the folder of a file, or the root", () => {
    expect(terminalFolderOf(folder("docs/specs", true, []))).toBe("docs/specs");
    expect(terminalFolderOf(file("docs/specs/a.md"))).toBe("docs/specs");
    expect(terminalFolderOf(file("a.md"))).toBe("");
    expect(terminalFolderOf(folder("", true, []))).toBe("");
  });

  it("LCM-FR-VTLR: the file manager label follows the platform", () => {
    expect(fileManagerLabel("MacIntel")).toBe("Finder");
    expect(fileManagerLabel("Win32")).toBe("Explorer");
    expect(fileManagerLabel("Linux x86_64")).toBe("File Manager");
    expect(fileManagerLabel("")).toBe("File Manager");
  });
});

describe("Open In errors", () => {
  it("LCM-FR-WNPH: a refused start folder shows the folder text", async () => {
    serve({
      create_terminal: () => {
        throw { code: "invalid_start_folder" };
      },
    });
    renderPanel();
    const menu = await openSubmenu("specifications");
    fireEvent.click(within(menu).getByText("Terminal Tab"));
    expect(await screen.findByText("This folder is not in the worktree.")).toBeInTheDocument();
    await expectOneFailureRecord({ entry: "terminal_tab", error: "invalid_start_folder" });
  });

  it("LCM-FR-WNPH: a later entry that succeeds clears the inline error", async () => {
    let fail = true;
    serve({
      open_in_system_terminal: () => {
        if (fail) throw "no_terminal_found";
        return undefined;
      },
    });
    renderPanel();
    let menu = await openSubmenu("AGENTS.md");
    fireEvent.click(within(menu).getByText("System Terminal"));
    expect(await screen.findByText("No terminal application was found.")).toBeInTheDocument();
    fail = false;
    menu = await openSubmenu("AGENTS.md");
    expect(screen.getByText("No terminal application was found.")).toBeInTheDocument();
    fireEvent.click(within(menu).getByText("System Terminal"));
    await waitFor(() => expect(screen.queryByText("No terminal application was found.")).not.toBeInTheDocument());
  });
});

describe("submenu placement and hover intent", () => {
  function parentOf(menu: HTMLElement, label: string): HTMLElement {
    return within(menu).getByText(label).closest(".menu-item--parent") as HTMLElement;
  }

  it("LCM-FR-PDXH: a menu opened near the window bottom and right edge moves inside the window", async () => {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      const width = this.classList.contains("menu") ? 200 : 0;
      const height = this.classList.contains("menu") ? 300 : 0;
      return { x: 0, y: 0, top: 0, left: 0, right: width, bottom: height, width, height, toJSON: () => ({}) } as DOMRect;
    });
    serve();
    renderPanel();
    fireEvent.contextMenu(await screen.findByText("AGENTS.md"), { clientX: window.innerWidth - 10, clientY: window.innerHeight - 10 });
    const menu = document.querySelector(".menu") as HTMLElement;
    await waitFor(() => expect(menu.style.top).toBe(`${window.innerHeight - 300 - 4}px`));
    expect(menu.style.left).toBe(`${window.innerWidth - 200 - 4}px`);
  });

  it("LCM-FR-PDXH: a submenu that would overhang the window bottom moves up", async () => {
    const subTop = window.innerHeight - 100;
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      const isSub = this.classList.contains("menu--sub");
      const top = isSub ? subTop + (parseFloat(this.style.top) || 0) : 0;
      const height = isSub ? 300 : 0;
      return { x: 0, y: top, top, left: 0, right: 0, bottom: top + height, width: 0, height, toJSON: () => ({}) } as DOMRect;
    });
    serve();
    renderPanel();
    const menu = await openSubmenu("AGENTS.md");
    const sub = parentOf(menu, "Open In").querySelector(".menu--sub") as HTMLElement;
    await waitFor(() => expect(sub.style.top).toBe(`${-(subTop + 300 - (window.innerHeight - 4))}px`));
  });

  it("LCM-FR-UCJY: crossing Artifact Type on the way into Open In keeps Open In open", async () => {
    serve();
    renderPanel();
    const menu = await openSubmenu("AGENTS.md");
    vi.useFakeTimers();
    try {
      const openIn = parentOf(menu, "Open In");
      const assign = parentOf(menu, "Artifact Type");
      fireEvent.mouseOut(openIn, { relatedTarget: assign });
      act(() => vi.advanceTimersByTime(SUBMENU_INTENT_MS - 100));
      expect(within(menu).getByText("Finder")).toBeInTheDocument();
      fireEvent.mouseOut(assign, { relatedTarget: within(menu).getByText("Finder") });
      act(() => vi.advanceTimersByTime(SUBMENU_INTENT_MS * 2));
      expect(within(menu).getByText("Finder")).toBeInTheDocument();
      expect(within(menu).queryByText("Scratchpad")).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("LCM-FR-UCJY: resting on Artifact Type switches the submenu, and leaving both closes it", async () => {
    serve();
    renderPanel();
    const menu = await openSubmenu("AGENTS.md");
    vi.useFakeTimers();
    try {
      const openIn = parentOf(menu, "Open In");
      const assign = parentOf(menu, "Artifact Type");
      fireEvent.mouseOut(openIn, { relatedTarget: assign });
      act(() => vi.advanceTimersByTime(SUBMENU_INTENT_MS));
      expect(within(menu).getByText("Scratchpad")).toBeInTheDocument();
      expect(within(menu).queryByText("Finder")).not.toBeInTheDocument();
      fireEvent.mouseOut(assign, { relatedTarget: within(menu).getByText("Rename") });
      act(() => vi.advanceTimersByTime(SUBMENU_INTENT_MS - 1));
      expect(within(menu).getByText("Scratchpad")).toBeInTheDocument();
      act(() => vi.advanceTimersByTime(1));
      expect(within(menu).queryByText("Scratchpad")).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });
});
