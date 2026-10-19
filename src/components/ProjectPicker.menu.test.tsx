import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { ProjectPicker } from "./ProjectPicker";
import { placeOnAxis } from "./RecentProjectMenu";
import { resetLogBufferForTest } from "../logging";
import type { RecentProject } from "../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: () => Promise.resolve(null),
}));

// The picker sets its window chrome on mount. These tests do not look at it,
// so every window call resolves and does nothing.
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  const done = async () => {};
  return {
    LogicalSize,
    getCurrentWindow: () => ({
      setResizable: done,
      setMaximizable: done,
      setSize: done,
      isMaximized: async () => false,
      unmaximize: done,
      isFullscreen: async () => false,
      setFullscreen: done,
      onResized: async () => () => {},
    }),
  };
});

const ACME: RecentProject = {
  name: "acme-platform",
  path: "~/dev/acme-platform",
  lastOpenedAt: "2026-05-15T10:00:00Z",
};
const DESIGN: RecentProject = {
  name: "design-skills",
  path: "~/dev/design-skills",
  lastOpenedAt: "2026-05-12T10:00:00Z",
};
const WEBSITE: RecentProject = {
  name: "website",
  path: "~/dev/website",
  lastOpenedAt: "2026-05-01T10:00:00Z",
};
const PINNED: RecentProject = {
  name: "pinned-proj",
  path: "~/dev/pinned-proj",
  lastOpenedAt: "2026-01-01T00:00:00Z",
  pinned: true,
  missing: false,
};
const MISSING: RecentProject = {
  name: "gone-proj",
  path: "~/dev/gone-proj",
  lastOpenedAt: "2026-02-01T00:00:00Z",
  pinned: true,
  missing: true,
};

type Handler = (args: Record<string, unknown> | undefined) => unknown;

/**
 * A strict `invoke` mock routed by command name. It answers the background log
 * flush, and it rejects a command that it does not expect.
 */
function setupInvoke(handlers: Record<string, Handler>) {
  invokeMock.mockImplementation(
    (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "append_log_records") return Promise.resolve();
      const handler = handlers[cmd];
      if (!handler) return Promise.reject(new Error(`unexpected ${cmd}`));
      try {
        return Promise.resolve(handler(args));
      } catch (e) {
        return Promise.reject(e);
      }
    },
  );
}

/** A `list_recent_projects` handler that answers each list in turn. */
function lists(...answers: RecentProject[][]): Handler {
  let at = 0;
  return () => answers[Math.min(at++, answers.length - 1)];
}

function calls(cmd: string) {
  return invokeMock.mock.calls.filter(([c]) => c === cmd);
}

function row(name: string): HTMLElement {
  const el = screen
    .getByText(name)
    .closest<HTMLElement>(".picker-recent-row");
  if (!el) throw new Error(`no row for ${name}`);
  return el;
}

function rowNames(): string[] {
  return Array.from(
    document.querySelectorAll<HTMLElement>("[data-recent-path]"),
  ).map((el) => el.dataset.recentPath ?? "");
}

/** Render the picker and wait until the entry `first` shows. */
async function renderPicker(first: string = ACME.name) {
  const onOpen = vi.fn();
  render(<ProjectPicker onOpen={onOpen} />);
  await screen.findByText(first);
  return onOpen;
}

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
});

afterEach(() => {
  cleanup();
  resetLogBufferForTest();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("ProjectPicker — recent entry context menu opening", () => {
  it("PPK-FR-SDXU, PPK-FR-02: a right-click opens the menu and does not open the project", async () => {
    setupInvoke({ list_recent_projects: lists([ACME, DESIGN]) });
    const onOpen = await renderPicker();

    // The full right-button sequence (mousedown, contextmenu, mouseup), so an
    // open action on any of those events fails this test.
    await userEvent.pointer({ keys: "[MouseRight]", target: row(DESIGN.name) });

    const menu = await screen.findByRole("menu", {
      name: "Actions for design-skills",
    });
    expect(menu).toBeInTheDocument();
    expect(calls("open_project_at_path")).toHaveLength(0);
    expect(onOpen).not.toHaveBeenCalled();
  });

  it("PPK-FR-SDXU: a pinned entry and a missing pinned entry have the same menu", async () => {
    setupInvoke({ list_recent_projects: lists([PINNED, MISSING]) });
    await renderPicker(PINNED.name);

    fireEvent.contextMenu(row(PINNED.name));
    const first = await screen.findByRole("menu");
    const firstLabels = within(first)
      .getAllByRole("menuitem")
      .map((el) => el.textContent);
    fireEvent.keyDown(first, { key: "Escape" });
    await waitFor(() =>
      expect(screen.queryByRole("menu")).not.toBeInTheDocument(),
    );

    fireEvent.contextMenu(row(MISSING.name));
    const second = await screen.findByRole("menu", {
      name: "Actions for gone-proj",
    });
    expect(
      within(second)
        .getAllByRole("menuitem")
        .map((el) => el.textContent),
    ).toEqual(firstLabels);
    expect(firstLabels).toEqual(["Unpin", "Hide"]);
  });

  it("PPK-FR-SDXU: the context-menu key and Shift+F10 open the menu on the focused entry", async () => {
    setupInvoke({ list_recent_projects: lists([ACME, DESIGN]) });
    await renderPicker();

    const acme = row(ACME.name);
    acme.focus();
    fireEvent.keyDown(acme, { key: "ContextMenu" });
    await screen.findByRole("menu", { name: "Actions for acme-platform" });

    fireEvent.keyDown(document.activeElement ?? document.body, {
      key: "Escape",
    });
    await waitFor(() => expect(document.activeElement).toBe(acme));

    const design = row(DESIGN.name);
    design.focus();
    fireEvent.keyDown(design, { key: "F10", shiftKey: true });
    await screen.findByRole("menu", { name: "Actions for design-skills" });
  });

  it("PPK-FR-SDXU: F10 without Shift does not open the menu", async () => {
    setupInvoke({ list_recent_projects: lists([ACME]) });
    await renderPicker();

    const acme = row(ACME.name);
    acme.focus();
    fireEvent.keyDown(acme, { key: "F10" });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});

describe("ProjectPicker — recent entry context menu entries", () => {
  it("PPK-FR-MHJD, PPK-FR-FINT: an unpinned entry offers Pin then Hide, each with an icon and an accessible name", async () => {
    setupInvoke({ list_recent_projects: lists([ACME]) });
    await renderPicker();

    fireEvent.contextMenu(row(ACME.name));
    const menu = await screen.findByRole("menu");
    const items = within(menu).getAllByRole("menuitem");
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveAccessibleName("Pin");
    expect(items[1]).toHaveAccessibleName("Hide");
    for (const item of items) {
      expect(item.firstElementChild?.tagName.toLowerCase()).toBe("svg");
    }
    expect(within(menu).queryByRole("menuitem", { name: "Unpin" })).toBeNull();
  });

  it("PPK-FR-MHJD: a pinned entry offers Unpin then Hide", async () => {
    setupInvoke({ list_recent_projects: lists([PINNED]) });
    await renderPicker(PINNED.name);

    fireEvent.contextMenu(row(PINNED.name));
    const menu = await screen.findByRole("menu");
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((el) => el.textContent)).toEqual(["Unpin", "Hide"]);
    expect(items[0]).toHaveAccessibleName("Unpin");
    expect(items[0].firstElementChild?.tagName.toLowerCase()).toBe("svg");
  });
});

describe("ProjectPicker — Pin and Unpin", () => {
  it("PPK-FR-YNME, PPK-FR-VFDK: Pin invokes pin_recent_project, reloads, re-orders, and returns focus to the entry", async () => {
    const pinnedDesign = { ...DESIGN, pinned: true };
    setupInvoke({
      list_recent_projects: lists([ACME, DESIGN], [pinnedDesign, ACME]),
      pin_recent_project: () => undefined,
    });
    await renderPicker();
    expect(rowNames()).toEqual([ACME.path, DESIGN.path]);

    fireEvent.contextMenu(row(DESIGN.name));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Pin" }),
    );

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    await waitFor(() =>
      expect(rowNames()).toEqual([DESIGN.path, ACME.path]),
    );
    expect(calls("pin_recent_project")).toEqual([
      ["pin_recent_project", { path: DESIGN.path }],
    ]);
    expect(calls("list_recent_projects")).toHaveLength(2);
    expect(within(row(DESIGN.name)).getByText("pinned")).toBeInTheDocument();
    await waitFor(() => expect(document.activeElement).toBe(row(DESIGN.name)));
    expect(calls("open_project_at_path")).toHaveLength(0);
  });

  it("PPK-FR-YNME: Unpin invokes unpin_recent_project and reloads the list", async () => {
    const unpinned = { ...PINNED, pinned: false };
    setupInvoke({
      list_recent_projects: lists([PINNED, ACME], [ACME, unpinned]),
      unpin_recent_project: () => undefined,
    });
    await renderPicker(PINNED.name);

    fireEvent.contextMenu(row(PINNED.name));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Unpin" }),
    );

    await waitFor(() =>
      expect(rowNames()).toEqual([ACME.path, PINNED.path]),
    );
    expect(calls("unpin_recent_project")).toEqual([
      ["unpin_recent_project", { path: PINNED.path }],
    ]);
    expect(screen.queryByText("pinned")).not.toBeInTheDocument();
    await waitFor(() => expect(document.activeElement).toBe(row(PINNED.name)));
  });
});

describe("ProjectPicker — Hide", () => {
  it("PPK-FR-MQVU, PPK-FR-VFDK: Hide invokes remove_recent_project at once, reloads, and focuses the first entry", async () => {
    setupInvoke({
      list_recent_projects: lists([ACME, DESIGN, WEBSITE], [ACME, WEBSITE]),
      remove_recent_project: () => undefined,
    });
    await renderPicker();

    fireEvent.contextMenu(row(DESIGN.name));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Hide" }),
    );

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByText(DESIGN.name)).not.toBeInTheDocument(),
    );
    // Exactly the remove and the reload: nothing opens, nothing is deleted.
    expect(
      invokeMock.mock.calls
        .map(([c]) => c)
        .filter((c) => c !== "append_log_records" && c !== "center_picker"),
    ).toEqual([
      "list_recent_projects",
      "remove_recent_project",
      "list_recent_projects",
    ]);
    expect(calls("remove_recent_project")).toEqual([
      ["remove_recent_project", { path: DESIGN.path }],
    ]);
    expect(screen.queryByRole("button", { name: /undo/i })).toBeNull();
    // The first entry, not the entry after the hidden one.
    await waitFor(() => expect(document.activeElement).toBe(row(ACME.name)));
  });

  it("PPK-FR-MQVU: Hide on a pinned missing entry uses the same operation", async () => {
    setupInvoke({
      list_recent_projects: lists([MISSING, ACME], [ACME]),
      remove_recent_project: () => undefined,
    });
    await renderPicker(MISSING.name);

    fireEvent.contextMenu(row(MISSING.name));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Hide" }),
    );

    await waitFor(() => expect(rowNames()).toEqual([ACME.path]));
    expect(calls("remove_recent_project")).toEqual([
      ["remove_recent_project", { path: MISSING.path }],
    ]);
  });

  it("PPK-FR-QTWG, PPK-FR-VFDK: hiding the last entry shows the empty state and focuses the list", async () => {
    setupInvoke({
      list_recent_projects: lists([ACME], []),
      remove_recent_project: () => undefined,
    });
    await renderPicker();

    fireEvent.contextMenu(row(ACME.name));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Hide" }),
    );

    expect(await screen.findByText(/No projects yet/i)).toBeInTheDocument();
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByTestId("picker-recents-body"),
      ),
    );
  });
});

describe("ProjectPicker — failed entry actions", () => {
  it("PPK-FR-LJNO: a failed Hide keeps the list and shows an error attached to the list", async () => {
    setupInvoke({
      list_recent_projects: lists([ACME, DESIGN]),
      remove_recent_project: () => {
        throw "store is read-only";
      },
    });
    await renderPicker();

    fireEvent.contextMenu(row(ACME.name));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Hide" }),
    );

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Could not hide project: store is read-only");
    expect(screen.getByTestId("picker-left")).toContainElement(alert);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(rowNames()).toEqual([ACME.path, DESIGN.path]);
    expect(calls("list_recent_projects")).toHaveLength(1);
    await waitFor(() => expect(document.activeElement).toBe(row(ACME.name)));
  });

  it("PPK-FR-LJNO: a failed reload keeps the list, and the next success removes the message", async () => {
    let reloads = 0;
    const pinnedAcme = { ...ACME, pinned: true };
    setupInvoke({
      list_recent_projects: () => {
        reloads += 1;
        if (reloads === 1) return [ACME, DESIGN];
        if (reloads === 2) throw new Error("disk busy");
        return [pinnedAcme, DESIGN];
      },
      pin_recent_project: () => undefined,
    });
    await renderPicker();

    fireEvent.contextMenu(row(ACME.name));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Pin" }),
    );
    // The pin is stored; only the reload failed, and the message says so.
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not reload recent projects: disk busy",
    );
    expect(rowNames()).toEqual([ACME.path, DESIGN.path]);
    expect(screen.queryByText("pinned")).not.toBeInTheDocument();

    fireEvent.contextMenu(row(ACME.name));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Pin" }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("alert")).not.toBeInTheDocument(),
    );
    expect(within(row(ACME.name)).getByText("pinned")).toBeInTheDocument();
  });
});

describe("ProjectPicker — entry actions and other work", () => {
  it("PPK-FR-SDXU, PPK-FR-06: while a project opens, a right-click opens no menu", async () => {
    let finishOpen: (v: unknown) => void = () => {};
    setupInvoke({
      list_recent_projects: lists([ACME, DESIGN]),
      open_project_at_path: () =>
        new Promise((resolve) => {
          finishOpen = resolve;
        }),
    });
    const onOpen = await renderPicker();

    await userEvent.click(screen.getByText(ACME.name));
    await waitFor(() => expect(calls("open_project_at_path")).toHaveLength(1));
    fireEvent.contextMenu(row(DESIGN.name), { clientX: 40, clientY: 60 });

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    finishOpen({ name: ACME.name, path: ACME.path, activeWorktreePath: ACME.path });
    await waitFor(() => expect(onOpen).toHaveBeenCalledTimes(1));
  });

  it("PPK-FR-VFDK: when the user moves focus before the reload ends, focus stays there", async () => {
    let finishPin: (v: unknown) => void = () => {};
    setupInvoke({
      list_recent_projects: lists([ACME, DESIGN], [{ ...DESIGN, pinned: true }, ACME]),
      pin_recent_project: () =>
        new Promise((resolve) => {
          finishPin = resolve;
        }),
    });
    await renderPicker();

    fireEvent.contextMenu(row(DESIGN.name), { clientX: 40, clientY: 60 });
    await userEvent.click(await screen.findByRole("menuitem", { name: "Pin" }));
    await waitFor(() => expect(calls("pin_recent_project")).toHaveLength(1));

    const name = screen.getByPlaceholderText("my-project");
    name.focus();
    finishPin(undefined);

    await waitFor(() => expect(rowNames()).toEqual([DESIGN.path, ACME.path]));
    expect(document.activeElement).toBe(name);
  });
});

describe("ProjectPicker — failed Unpin", () => {
  it("PPK-FR-LJNO: a failed Unpin keeps the entry pinned and shows the error", async () => {
    setupInvoke({
      list_recent_projects: lists([PINNED, ACME]),
      unpin_recent_project: () => {
        throw "store is locked";
      },
    });
    await renderPicker(PINNED.name);

    fireEvent.contextMenu(row(PINNED.name));
    await userEvent.click(
      await screen.findByRole("menuitem", { name: "Unpin" }),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Could not unpin project: store is locked",
    );
    expect(within(row(PINNED.name)).getByText("pinned")).toBeInTheDocument();
    expect(rowNames()).toEqual([PINNED.path, ACME.path]);
  });
});

describe("ProjectPicker — recent entry menu keyboard and dismissal", () => {
  it("PPK-FR-FINT: focus moves to the first entry, arrows wrap, and Enter chooses", async () => {
    setupInvoke({
      list_recent_projects: lists([ACME, DESIGN], [DESIGN]),
      remove_recent_project: () => undefined,
    });
    await renderPicker();

    fireEvent.contextMenu(row(ACME.name));
    const menu = await screen.findByRole("menu");
    const [pin, hide] = within(menu).getAllByRole("menuitem");
    await waitFor(() => expect(document.activeElement).toBe(pin));

    fireEvent.keyDown(pin, { key: "ArrowDown" });
    expect(document.activeElement).toBe(hide);
    fireEvent.keyDown(hide, { key: "ArrowDown" });
    expect(document.activeElement).toBe(pin);
    fireEvent.keyDown(pin, { key: "ArrowUp" });
    expect(document.activeElement).toBe(hide);

    fireEvent.keyDown(hide, { key: "Enter" });
    await waitFor(() =>
      expect(screen.queryByText(ACME.name)).not.toBeInTheDocument(),
    );
    expect(calls("remove_recent_project")).toEqual([
      ["remove_recent_project", { path: ACME.path }],
    ]);
    expect(calls("open_project_at_path")).toHaveLength(0);
  });

  it("PPK-FR-VFDK: one menu at most is open", async () => {
    setupInvoke({ list_recent_projects: lists([ACME, DESIGN]) });
    await renderPicker();

    fireEvent.contextMenu(row(ACME.name));
    await screen.findByRole("menu", { name: "Actions for acme-platform" });
    fireEvent.mouseDown(row(DESIGN.name), { button: 2 });
    fireEvent.contextMenu(row(DESIGN.name));

    await screen.findByRole("menu", { name: "Actions for design-skills" });
    expect(screen.getAllByRole("menu")).toHaveLength(1);
  });

  it("PPK-FR-VFDK: Escape closes the menu, starts nothing, and returns focus to the entry", async () => {
    setupInvoke({ list_recent_projects: lists([ACME, DESIGN]) });
    await renderPicker();

    fireEvent.contextMenu(row(DESIGN.name));
    const menu = await screen.findByRole("menu");
    fireEvent.keyDown(menu, { key: "Escape" });

    await waitFor(() =>
      expect(screen.queryByRole("menu")).not.toBeInTheDocument(),
    );
    await waitFor(() => expect(document.activeElement).toBe(row(DESIGN.name)));
    expect(invokeMock.mock.calls.map(([c]) => c)).not.toContain(
      "remove_recent_project",
    );
  });

  it("PPK-FR-VFDK: a click outside the menu closes it and starts nothing", async () => {
    setupInvoke({ list_recent_projects: lists([ACME]) });
    await renderPicker();

    fireEvent.contextMenu(row(ACME.name));
    await screen.findByRole("menu");
    await userEvent.click(screen.getByText("Create new project"));

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(calls("pin_recent_project")).toHaveLength(0);
    expect(calls("remove_recent_project")).toHaveLength(0);
  });

  it("PPK-FR-VFDK: a scroll of the recent list closes the menu", async () => {
    setupInvoke({ list_recent_projects: lists([ACME, DESIGN]) });
    await renderPicker();

    fireEvent.contextMenu(row(ACME.name));
    await screen.findByRole("menu");
    fireEvent.scroll(screen.getByTestId("picker-recents-body"));

    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});

describe("ProjectPicker — recent entry menu placement", () => {
  it("PPK-FR-VEID: the menu floats outside the scrolling list and opens at the pointer", async () => {
    setupInvoke({ list_recent_projects: lists([ACME]) });
    await renderPicker();

    fireEvent.contextMenu(row(ACME.name), { clientX: 120, clientY: 90 });
    const menu = await screen.findByRole("menu");

    expect(screen.getByTestId("picker-recents-body")).not.toContainElement(
      menu,
    );
    expect(menu.parentElement).toBe(document.body);
    expect(menu.style.position).toBe("fixed");
    expect(menu.style.left).toBe("120px");
    expect(menu.style.top).toBe("90px");
  });

  it("PPK-FR-VEID: a keyboard opening places the menu at the focused entry", async () => {
    setupInvoke({ list_recent_projects: lists([ACME]) });
    await renderPicker();

    const acme = row(ACME.name);
    vi.spyOn(acme, "getBoundingClientRect").mockReturnValue(
      DOMRect.fromRect({ x: 10, y: 30, width: 400, height: 20 }),
    );
    acme.focus();
    fireEvent.keyDown(acme, { key: "F10", shiftKey: true });

    const menu = await screen.findByRole("menu");
    expect(menu.style.left).toBe("34px");
    expect(menu.style.top).toBe("50px");
  });

  it("PPK-FR-VEID: near the bottom-right corner the rendered menu flips inside the window", async () => {
    vi.stubGlobal("innerWidth", 800);
    vi.stubGlobal("innerHeight", 600);
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(
      function (this: HTMLElement) {
        return this.getAttribute("role") === "menu"
          ? DOMRect.fromRect({ x: 0, y: 0, width: 150, height: 60 })
          : DOMRect.fromRect({ x: 0, y: 0, width: 0, height: 0 });
      },
    );
    setupInvoke({ list_recent_projects: lists([ACME]) });
    await renderPicker();

    fireEvent.contextMenu(row(ACME.name), { clientX: 700, clientY: 580 });

    const menu = await screen.findByRole("menu");
    await waitFor(() => expect(menu.style.left).toBe("550px"));
    expect(menu.style.top).toBe("520px");
  });

  it("PPK-FR-VEID: a menu that fits stays at the pointer", () => {
    expect(placeOnAxis(100, 150, 800)).toBe(100);
  });

  it("PPK-FR-VEID: a menu that overhangs the far edge flips to the other side of the pointer", () => {
    expect(placeOnAxis(700, 150, 800)).toBe(550);
  });

  it("PPK-FR-VEID: a menu that cannot flip shifts inside the window and never past the near edge", () => {
    expect(placeOnAxis(100, 150, 200)).toBe(4);
    expect(placeOnAxis(0, 150, 800)).toBe(4);
    expect(placeOnAxis(190, 150, 200)).toBe(40);
  });
});
