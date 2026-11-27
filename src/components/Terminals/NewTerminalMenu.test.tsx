// The New terminal menu (`TRM-terminals.md` TRM-FR-JINY … TRM-FR-IVGX). The
// backend is mocked at `invoke`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { logWarn } from "../../logging";
import { refreshTerminals, resetTerminalsForTest, terminalsSnapshot } from "../../state/terminals";
import { NewTerminalMenu } from "./NewTerminalMenu";
import { TerminalsPanel } from "./TerminalsPanel";
import { commands, resetTerminalFixtures, serve, session, target } from "./terminalFixtures";

vi.mock("@tauri-apps/api/core", async () => {
  const f = await import("./terminalFixtures");
  return { invoke: f.invokeMock, Channel: f.FakeChannel };
});
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./terminalFixtures")).listenMock,
}));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

beforeEach(() => {
  resetTerminalFixtures();
  resetTerminalsForTest();
  vi.mocked(logWarn).mockClear();
});
const anchors: HTMLElement[] = [];
afterEach(() => {
  cleanup();
  for (const a of anchors.splice(0)) a.remove();
  vi.restoreAllMocks();
});

const stream = target({
  target: { kind: "work_stream", streamId: "st" },
  name: "auth",
  branch: "synthesis/stream/auth",
  isActiveWorktree: false,
  busyRunId: "run-7",
  kinds: [
    { kind: "microsandbox_shell", runsIn: "microsandbox", available: true },
    { kind: "host_shell", runsIn: "host", available: true },
  ],
});
const beta = target({ target: { kind: "worktree", path: "/b" }, name: "beta", branch: "feat/b", isActiveWorktree: false });
const alpha = target({ target: { kind: "worktree", path: "/a" }, name: "alpha", isActiveWorktree: false });

function renderMenu() {
  const anchor = document.createElement("button");
  anchor.textContent = "anchor";
  document.body.appendChild(anchor);
  anchors.push(anchor);
  const props = { anchor, onClose: vi.fn(), onCreated: vi.fn() };
  const view = render(<NewTerminalMenu {...props} />);
  return { ...props, unmount: view.unmount };
}

const topMenu = () => screen.getByRole("menu", { name: "New terminal" });
const activeSection = () => screen.getByRole("group", { name: "repo" });
/** The entries of one menu, not of a submenu inside it. */
const ownEntries = (menu: HTMLElement) =>
  within(menu)
    .getAllByRole("menuitem")
    .filter((el) => el.closest('[role="menu"]') === menu);
const submenuEntry = (name: RegExp) => within(topMenu()).getByRole("menuitem", { name });

describe("New terminal menu", () => {
  it("TRM-FR-JINY: it shows Reading targets until the targets are read", async () => {
    let answer: (v: unknown) => void = () => {};
    serve("list_terminal_targets", () => new Promise((r) => (answer = r)));
    renderMenu();
    expect(within(topMenu()).getByRole("status")).toHaveTextContent("Reading targets…");
    await act(async () => answer([target()]));
    expect(within(topMenu()).queryByRole("status")).toBeNull();
    expect(commands("list_terminal_targets")).toHaveLength(1);
  });

  it("TRM-FR-QJSW: a failed read shows the message and logs a WARN", async () => {
    serve("list_terminal_targets", () => {
      throw { code: "io" };
    });
    const props = renderMenu();
    expect(await screen.findByRole("alert")).toHaveTextContent("The targets could not be read.");
    expect(logWarn).toHaveBeenCalledWith(["frontend"], "terminal targets read failed", { code: "io" });
    expect(within(topMenu()).queryByRole("status")).toBeNull();
    expect(topMenu()).not.toHaveAttribute("aria-busy");
    expect(topMenu()).toHaveFocus();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(props.onClose).toHaveBeenCalledWith(true);
  });

  it("TRM-FR-AASW, TRM-FR-GYIR: the active worktree's kinds come first, then a separator, then worktrees and streams by name", async () => {
    const zeta = target({ target: { kind: "work_stream", streamId: "z" }, name: "zeta", branch: "z", isActiveWorktree: false });
    serve("list_terminal_targets", [zeta, stream, beta, target(), alpha]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    const head = activeSection().querySelector(".new-terminal-menu__head")!;
    expect(head).toHaveTextContent("repo");
    expect(head).toHaveTextContent("main");
    expect(head).toHaveTextContent("active worktree");
    const names = ownEntries(topMenu()).map((el) => el.getAttribute("aria-label"));
    expect(names).toEqual([
      "Host shell",
      "Container shell",
      "Microsandbox shell",
      "Codex",
      "alpha, main, worktree",
      "beta, feat/b, worktree",
      "auth, synthesis/stream/auth, work stream, busy",
      "zeta, z, work stream",
    ]);
    expect(within(topMenu()).getAllByRole("separator")).toHaveLength(1);
    // The separator sits between the active section and the first submenu entry.
    const sep = within(topMenu()).getByRole("separator");
    expect(sep.previousElementSibling).toBe(activeSection());
    expect(submenuEntry(/^beta/)).toHaveTextContent("feat/b · worktree");
    expect(submenuEntry(/^auth/)).toHaveTextContent("synthesis/stream/auth · work stream");
    // The branch shows also where it is the same as the name.
    expect(submenuEntry(/^zeta/)).toHaveTextContent("z · work stream");
    expect(submenuEntry(/^auth/)).toHaveAttribute("aria-haspopup", "menu");
    expect(submenuEntry(/^auth/).querySelector(".terminal-busy")).not.toBeNull();
    expect(submenuEntry(/^alpha/).querySelector(".terminal-busy")).toBeNull();
  });

  it("TRM-FR-GYIR: a menu with no other target shows no separator and no submenu entry", async () => {
    serve("list_terminal_targets", [target()]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    expect(within(topMenu()).queryByRole("separator")).toBeNull();
    expect(topMenu().querySelectorAll('[aria-haspopup="menu"]')).toHaveLength(0);
  });

  it("TRM-FR-CZTR, TRM-FR-HJUD: kinds come in fixed order and name where they run; a greyed one shows its reason and does nothing", async () => {
    serve("list_terminal_targets", [
      target({
        kinds: [
          { kind: "agent_cli", vendor: "codex", runsIn: "host", available: true },
          { kind: "microsandbox_shell", runsIn: "microsandbox", available: false, reason: "no_vm_running" },
          { kind: "container_shell", runsIn: "devcontainer", available: true },
          { kind: "host_shell", runsIn: "host", available: true },
        ],
      }),
    ]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    const kinds = within(activeSection()).getAllByRole("menuitem");
    expect(kinds.map((k) => k.getAttribute("aria-label"))).toEqual([
      "Host shell",
      "Container shell",
      "Microsandbox shell",
      "Codex",
    ]);
    expect(kinds[0]).toHaveTextContent("host");
    expect(kinds[1]).toHaveTextContent("dev container");
    expect(kinds[2]).toHaveTextContent("No VM runs for this target");
    expect(kinds[2]).toHaveAttribute("aria-disabled", "true");
    expect(kinds[0]).not.toHaveAttribute("aria-disabled");
    await userEvent.click(kinds[2]);
    fireEvent.keyDown(kinds[2], { key: "Enter" });
    expect(commands("create_terminal")).toHaveLength(0);
  });

  it("TRM-FR-CZTR: each Agent CLI vendor is its own entry, after the shells, and creates with its vendor", async () => {
    serve("list_terminal_targets", [
      target({
        kinds: [
          { kind: "agent_cli", vendor: "claude_code", runsIn: "docker", available: true },
          { kind: "agent_cli", vendor: "codex", runsIn: "host", available: true },
          { kind: "microsandbox_shell", runsIn: "microsandbox", available: true },
          { kind: "container_shell", runsIn: "docker", available: true },
        ],
      }),
    ]);
    serve("create_terminal", (args) => session({ id: `c-${String(args.vendor)}` }));
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    const kinds = within(activeSection()).getAllByRole("menuitem");
    expect(kinds.map((k) => k.getAttribute("aria-label"))).toEqual(["Container shell", "Microsandbox shell", "Claude Code", "Codex"]);
    expect(kinds[0]).toHaveTextContent("docker");
    expect(kinds[1]).toHaveTextContent("vm");
    await userEvent.click(kinds[2]);
    await waitFor(() => expect(commands("create_terminal")).toHaveLength(1));
    expect(commands("create_terminal")[0].args.vendor).toBe("claude_code");
  });

  it("TRM-FR-PVTT: the first available kind of the active worktree takes the focus, and the arrows move and wrap", async () => {
    serve("list_terminal_targets", [
      target({
        kinds: [
          { kind: "host_shell", runsIn: "host", available: false, reason: "no_vm_running" },
          { kind: "container_shell", runsIn: "docker", available: true },
        ],
      }),
      alpha,
    ]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    const [host, container, alphaEntry] = ownEntries(topMenu());
    await waitFor(() => expect(container).toHaveFocus());
    await userEvent.keyboard("{ArrowDown}");
    expect(alphaEntry).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(host).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(alphaEntry).toHaveFocus();
  });

  it("TRM-FR-PVTT: with no active worktree, the first entry takes the focus", async () => {
    serve("list_terminal_targets", [beta, alpha]);
    renderMenu();
    await waitFor(() => expect(submenuEntry(/^alpha/)).toHaveFocus());
    expect(screen.queryByRole("group")).toBeNull();
    expect(within(topMenu()).queryByRole("separator")).toBeNull();
  });

  it("TRM-FR-PVTT: where the active worktree has no available kind, its first entry takes the focus", async () => {
    serve("list_terminal_targets", [
      target({ kinds: [{ kind: "host_shell", runsIn: "host", available: false, reason: "no_vm_running" }] }),
      alpha,
    ]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    await waitFor(() => expect(within(activeSection()).getByRole("menuitem", { name: "Host shell" })).toHaveFocus());
  });

  it("TRM-FR-VJXC: a pointer opens a submenu, and one submenu is open at a time", async () => {
    serve("list_terminal_targets", [target(), alpha, stream]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    fireEvent.mouseEnter(submenuEntry(/^alpha/));
    expect(screen.getByRole("menu", { name: "alpha" })).toBeInTheDocument();
    expect(submenuEntry(/^alpha/)).toHaveAttribute("aria-expanded", "true");
    fireEvent.mouseEnter(submenuEntry(/^auth/));
    expect(screen.queryByRole("menu", { name: "alpha" })).toBeNull();
    const sub = screen.getByRole("menu", { name: "auth" });
    expect(within(sub).getAllByRole("menuitem").map((k) => k.getAttribute("aria-label"))).toEqual([
      "Host shell",
      "Microsandbox shell",
    ]);
    // A pointer over the active worktree's section closes the submenu.
    fireEvent.mouseEnter(activeSection());
    expect(screen.queryByRole("menu", { name: "auth" })).toBeNull();
  });

  it("TRM-FR-VJXC: Right opens a submenu and focuses its first entry; Left and Escape close it and focus its entry", async () => {
    serve("list_terminal_targets", [target(), alpha]);
    const props = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    submenuEntry(/^alpha/).focus();
    await userEvent.keyboard("{ArrowRight}");
    const sub = screen.getByRole("menu", { name: "alpha" });
    await waitFor(() => expect(within(sub).getAllByRole("menuitem")[0]).toHaveFocus());
    // The arrows stay inside the submenu.
    await userEvent.keyboard("{ArrowUp}");
    const subEntries = within(sub).getAllByRole("menuitem");
    expect(subEntries[subEntries.length - 1]).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(screen.queryByRole("menu", { name: "alpha" })).toBeNull();
    expect(submenuEntry(/^alpha/)).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(within(screen.getByRole("menu", { name: "alpha" })).getAllByRole("menuitem")[0]).toHaveFocus());
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu", { name: "alpha" })).toBeNull();
    expect(submenuEntry(/^alpha/)).toHaveFocus();
    expect(props.onClose).not.toHaveBeenCalled();
  });

  it("TRM-FR-VJXC: a pointer that closes the submenu with the focus keeps the focus in the menu, and the keys still work", async () => {
    serve("list_terminal_targets", [target(), alpha, beta]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    submenuEntry(/^alpha/).focus();
    await userEvent.keyboard("{ArrowRight}");
    await waitFor(() => expect(within(screen.getByRole("menu", { name: "alpha" })).getAllByRole("menuitem")[0]).toHaveFocus());
    // The entry under the pointer takes the focus.
    fireEvent.mouseEnter(submenuEntry(/^beta/));
    expect(screen.queryByRole("menu", { name: "alpha" })).toBeNull();
    expect(submenuEntry(/^beta/)).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(submenuEntry(/^alpha/)).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(submenuEntry(/^beta/)).toHaveFocus();
    // The same for a pointer over the active worktree's section.
    await userEvent.keyboard("{ArrowRight}");
    await waitFor(() => expect(within(screen.getByRole("menu", { name: "beta" })).getAllByRole("menuitem")[0]).toHaveFocus());
    fireEvent.mouseEnter(activeSection());
    expect(screen.queryByRole("menu", { name: "beta" })).toBeNull();
    expect(submenuEntry(/^beta/)).toHaveFocus();
  });

  it("TRM-FR-FYYB: Space on a kind creates the terminal too", async () => {
    serve("list_terminal_targets", [target()]);
    serve("create_terminal", session({ id: "n6" }));
    const props = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    await waitFor(() => expect(within(activeSection()).getByRole("menuitem", { name: "Host shell" })).toHaveFocus());
    await userEvent.keyboard(" ");
    await waitFor(() => expect(props.onCreated).toHaveBeenCalled());
    expect(commands("create_terminal")).toHaveLength(1);
  });

  it("TRM-FR-VJXC: a click opens a submenu without moving the focus into it", async () => {
    serve("list_terminal_targets", [target(), alpha]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    submenuEntry(/^alpha/).focus();
    fireEvent.click(submenuEntry(/^alpha/));
    expect(screen.getByRole("menu", { name: "alpha" })).toBeInTheDocument();
    expect(submenuEntry(/^alpha/)).toHaveAttribute("aria-expanded", "true");
    expect(submenuEntry(/^alpha/)).toHaveFocus();
  });

  it("TRM-FR-JZPT: Escape in the menu closes it with the focus going back, and an outside activation closes it", async () => {
    serve("list_terminal_targets", [target()]);
    const first = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(first.onClose).toHaveBeenCalledWith(true);
    first.unmount();

    const second = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    fireEvent.mouseDown(document.body);
    expect(second.onClose).toHaveBeenCalledWith(false);
    // The opening button toggles the menu itself.
    second.onClose.mockClear();
    fireEvent.mouseDown(second.anchor);
    expect(second.onClose).not.toHaveBeenCalled();
    expect(commands("create_terminal")).toHaveLength(0);
  });

  it("TRM-FR-JZPT: a context menu or a focus outside closes the menu, also with a submenu open", async () => {
    serve("list_terminal_targets", [target(), alpha]);
    const outside = document.createElement("input");
    document.body.appendChild(outside);
    anchors.push(outside);
    const first = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    fireEvent.mouseEnter(submenuEntry(/^alpha/));
    fireEvent.contextMenu(document.body);
    expect(first.onClose).toHaveBeenCalledWith(false);
    first.unmount();

    const second = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    fireEvent.mouseEnter(submenuEntry(/^alpha/));
    // A focus inside the submenu does not count as outside.
    within(screen.getByRole("menu", { name: "alpha" })).getAllByRole("menuitem")[0].focus();
    expect(second.onClose).not.toHaveBeenCalled();
    outside.focus();
    expect(second.onClose).toHaveBeenCalledWith(false);
  });

  it("TRM-FR-JZPT: a menu that closes while a create runs opens no tab", async () => {
    serve("list_terminal_targets", [target()]);
    let answer: (v: unknown) => void = () => {};
    serve("create_terminal", () => new Promise((r) => (answer = r)));
    const props = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    await userEvent.click(within(activeSection()).getByRole("menuitem", { name: "Host shell" }));
    props.unmount();
    await act(async () => answer(session({ id: "late" })));
    expect(props.onCreated).not.toHaveBeenCalled();
    // The backend made the session, so it shows as a row.
    expect(terminalsSnapshot().sessions.map((s) => s.id)).toEqual(["late"]);
  });

  it("TRM-FR-JINY: a read that ends after the menu closed changes nothing", async () => {
    let answer: (v: unknown) => void = () => {};
    serve("list_terminal_targets", () => new Promise((r) => (answer = r)));
    const props = renderMenu();
    props.unmount();
    await act(async () => answer([target()]));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(logWarn).not.toHaveBeenCalled();
  });

  it("TRM-FR-FYYB: a click on a kind creates the terminal for its target and hands the session on", async () => {
    serve("list_terminal_targets", [target()]);
    const created = session({ id: "n1", kind: "agent_cli", vendor: "codex", label: "Codex 1" });
    serve("create_terminal", created);
    const props = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    await userEvent.click(within(activeSection()).getByRole("menuitem", { name: "Codex" }));
    await waitFor(() => expect(props.onCreated).toHaveBeenCalledWith(created));
    expect(commands("create_terminal")[0].args).toEqual({
      target: { kind: "worktree", path: "/repo" },
      kind: "agent_cli",
      vendor: "codex",
      startFolder: null,
    });
    expect(terminalsSnapshot().sessions.map((s) => s.id)).toEqual(["n1"]);
  });

  it("TRM-FR-FYYB: Enter on a submenu kind creates the terminal for that target", async () => {
    serve("list_terminal_targets", [target(), stream]);
    serve("create_terminal", session({ id: "n2" }));
    const props = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    submenuEntry(/^auth/).focus();
    await userEvent.keyboard("{ArrowRight}");
    const sub = screen.getByRole("menu", { name: "auth" });
    await waitFor(() => expect(within(sub).getByRole("menuitem", { name: "Host shell" })).toHaveFocus());
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(props.onCreated).toHaveBeenCalled());
    expect(commands("create_terminal")[0].args).toEqual({
      target: { kind: "work_stream", streamId: "st" },
      kind: "host_shell",
      vendor: null,
      startFolder: null,
    });
  });

  it("TRM-FR-FYYB: while a create runs, the menu ignores other choices", async () => {
    serve("list_terminal_targets", [target()]);
    let answer: (v: unknown) => void = () => {};
    serve("create_terminal", () => new Promise((r) => (answer = r)));
    const props = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    await userEvent.click(within(activeSection()).getByRole("menuitem", { name: "Host shell" }));
    await userEvent.click(within(activeSection()).getByRole("menuitem", { name: "Codex" }));
    expect(commands("create_terminal")).toHaveLength(1);
    await act(async () => answer(session({ id: "n4" })));
    expect(props.onCreated).toHaveBeenCalledTimes(1);
  });

  it("TRM-FR-YVPJ: a failed create keeps the menu open, shows the error under the target's kinds, and logs a WARN", async () => {
    serve("list_terminal_targets", [target(), stream]);
    serve("create_terminal", () => {
      throw { code: "terminal_limit_reached" };
    });
    const props = renderMenu();
    await screen.findByRole("group", { name: "repo" });
    fireEvent.mouseEnter(submenuEntry(/^auth/));
    const sub = screen.getByRole("menu", { name: "auth" });
    await userEvent.click(within(sub).getByRole("menuitem", { name: "Host shell" }));
    expect(await within(sub).findByRole("alert")).toHaveTextContent("16");
    expect(within(activeSection()).queryByRole("alert")).toBeNull();
    expect(props.onCreated).not.toHaveBeenCalled();
    expect(props.onClose).not.toHaveBeenCalled();
    expect(logWarn).toHaveBeenCalledWith(
      ["frontend"],
      "terminal create failed",
      expect.objectContaining({ code: "terminal_limit_reached" }),
    );
    // A second try clears the old error first.
    serve("create_terminal", session({ id: "n5" }));
    await userEvent.click(within(sub).getByRole("menuitem", { name: "Host shell" }));
    await waitFor(() => expect(props.onCreated).toHaveBeenCalled());
    expect(within(sub).queryByRole("alert")).toBeNull();
  });

  it.each([
    [{ code: "terminal_limit_reached" }, "The project holds the most terminals it can hold (16)."],
    [{ code: "target_gone" }, "This target is gone."],
    [{ code: "kind_unavailable", reason: "no_vm_running" }, "No VM runs for this target"],
    [{ code: "platform_unsupported" }, "Terminals are not available on this platform."],
    [{ code: "spawn_failed", message: "docker exited" }, "The terminal did not start: docker exited"],
    [{}, "The terminal did not start."],
  ])("TRM-FR-YVPJ: a failed create in the active worktree shows its error there (%o)", async (error, text) => {
    serve("list_terminal_targets", [target(), alpha]);
    serve("create_terminal", () => {
      throw error;
    });
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    await userEvent.click(within(activeSection()).getByRole("menuitem", { name: "Host shell" }));
    expect((await within(activeSection()).findByRole("alert")).textContent).toBe(text);
  });

  it("TRM-FR-IVGX: a busy target shows the busy line in its section or submenu and disables nothing", async () => {
    serve("list_terminal_targets", [target({ busyRunId: "run-1" }), stream, alpha]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    expect(within(activeSection()).getByRole("note")).toHaveTextContent("A graduation run is working in this target.");
    fireEvent.mouseEnter(submenuEntry(/^auth/));
    const sub = screen.getByRole("menu", { name: "auth" });
    expect(within(sub).getByRole("note")).toHaveTextContent("A graduation run is working in this target.");
    for (const kind of within(sub).getAllByRole("menuitem")) expect(kind).not.toHaveAttribute("aria-disabled");
    const note = within(sub).getByRole("note");
    const firstKind = within(sub).getAllByRole("menuitem")[0];
    expect(note.compareDocumentPosition(firstKind) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    serve("create_terminal", session({ id: "busy-1" }));
    await userEvent.click(within(sub).getByRole("menuitem", { name: "Host shell" }));
    await waitFor(() => expect(commands("create_terminal")).toHaveLength(1));
    expect(commands("create_terminal")[0].args.target).toEqual({ kind: "work_stream", streamId: "st" });
  });
});

describe("New terminal menu position", () => {
  function stubLayout(box: Partial<DOMRect>, size: { width: number; height: number }) {
    vi.spyOn(window, "innerWidth", "get").mockReturnValue(1000);
    vi.spyOn(window, "innerHeight", "get").mockReturnValue(600);
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(size.width);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(size.height);
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      left: 0, top: 0, right: 0, bottom: 0, width: 0, height: 0, x: 0, y: 0, toJSON: () => ({}), ...box,
    } as DOMRect);
  }

  it("TRM-FR-JINY: the menu aligns its trailing edge with the button, under it", async () => {
    stubLayout({ left: 280, right: 300, top: 40, bottom: 60 }, { width: 250, height: 200 });
    serve("list_terminal_targets", [target()]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    expect(topMenu().style.left).toBe("50px");
    expect(topMenu().style.top).toBe("64px");
    expect(topMenu().style.visibility).toBe("");
  });

  it("TRM-FR-JINY: where the menu fits neither below nor above, it goes up until it is inside the window", async () => {
    stubLayout({ left: 100, right: 300, top: 350, bottom: 374 }, { width: 250, height: 340 });
    serve("list_terminal_targets", [target()]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    expect(topMenu().style.top).toBe("252px");
  });

  it("TRM-FR-JINY: the menu stays inside the window, and opens above a button near the bottom", async () => {
    stubLayout({ left: 100, right: 120, top: 560, bottom: 580 }, { width: 250, height: 200 });
    serve("list_terminal_targets", [target()]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    expect(topMenu().style.left).toBe("8px");
    expect(topMenu().style.top).toBe("356px");
  });

  it("TRM-FR-VJXC: a submenu with no space on the trailing side opens on the leading side", async () => {
    stubLayout({ left: 700, right: 900, top: 40, bottom: 60 }, { width: 250, height: 100 });
    serve("list_terminal_targets", [target(), alpha]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    fireEvent.mouseEnter(submenuEntry(/^alpha/));
    const sub = screen.getByRole("menu", { name: "alpha" });
    expect(sub).toHaveAttribute("data-flip", "true");
    // The stub gives the entry the box of the button: the submenu ends at its leading edge.
    expect(sub.style.left).toBe("450px");
  });

  it("TRM-FR-VJXC: a submenu with no space on either side stays inside the window", async () => {
    stubLayout({ left: 100, right: 900, top: 550, bottom: 570 }, { width: 250, height: 100 });
    serve("list_terminal_targets", [target(), alpha]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    fireEvent.mouseEnter(submenuEntry(/^alpha/));
    const sub = screen.getByRole("menu", { name: "alpha" });
    expect(sub).not.toHaveAttribute("data-flip");
    expect(sub.style.left).toBe("742px");
    expect(sub.style.top).toBe("492px");
  });

  it("TRM-FR-VJXC: a scroll of the menu closes the open submenu", async () => {
    serve("list_terminal_targets", [target(), alpha]);
    renderMenu();
    await screen.findByRole("group", { name: "repo" });
    fireEvent.mouseEnter(submenuEntry(/^alpha/));
    fireEvent.scroll(topMenu());
    expect(screen.queryByRole("menu", { name: "alpha" })).toBeNull();
  });
});

describe("New terminal menu in the panel", () => {
  async function renderPanel() {
    serve("list_terminals", []);
    await act(() => refreshTerminals());
    const onOpenTerminal = vi.fn();
    const onOverlayOpening = vi.fn();
    render(<TerminalsPanel activeWorktreePath="/repo" onOpenTerminal={onOpenTerminal} onOverlayOpening={onOverlayOpening} />);
    return { onOpenTerminal, onOverlayOpening };
  }
  const headerButton = () => document.querySelector<HTMLButtonElement>(".panel-header .terminals-panel__new")!;

  it("TRM-FR-JINY, TRM-FR-FYYB: from the panel, a created session appears as a row, its tab opens, and the menu closes", async () => {
    serve("list_terminal_targets", [target()]);
    serve("create_terminal", session({ id: "n3", label: "Host shell 1" }));
    const { onOpenTerminal, onOverlayOpening } = await renderPanel();
    await userEvent.click(headerButton());
    expect(onOverlayOpening).toHaveBeenCalled();
    expect(headerButton()).toHaveAttribute("aria-expanded", "true");
    await screen.findByRole("group", { name: "repo" });
    await userEvent.click(within(activeSection()).getByRole("menuitem", { name: "Host shell" }));
    await waitFor(() => expect(onOpenTerminal).toHaveBeenCalledWith(expect.objectContaining({ id: "n3" })));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("option", { name: /^Host shell 1,/ })).toBeInTheDocument();
  });

  it("TRM-FR-JINY: a second activation of the button closes the menu, and a new open reads the targets again", async () => {
    serve("list_terminal_targets", [target()]);
    await renderPanel();
    await userEvent.click(headerButton());
    await screen.findByRole("group", { name: "repo" });
    await userEvent.click(headerButton());
    expect(screen.queryByRole("menu")).toBeNull();
    expect(headerButton()).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(headerButton());
    await screen.findByRole("group", { name: "repo" });
    expect(commands("list_terminal_targets")).toHaveLength(2);
  });

  it("TRM-FR-JINY: a menu opened from the empty state moves to the header button when that one is activated", async () => {
    serve("list_terminal_targets", [target()]);
    await renderPanel();
    const action = screen.getAllByRole("button", { name: "New terminal" }).find((b) => b !== headerButton())!;
    await userEvent.click(action);
    await screen.findByRole("group", { name: "repo" });
    expect(headerButton()).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(headerButton());
    await screen.findByRole("group", { name: "repo" });
    expect(screen.getAllByRole("menu")).toHaveLength(1);
    expect(headerButton()).toHaveAttribute("aria-expanded", "true");
  });

  it("SNV-FR-56, TRM-FR-JINY: the New terminal menu and the row context menu close each other", async () => {
    serve("list_terminals", [session({ id: "r1", label: "Host shell 1" })]);
    await act(() => refreshTerminals());
    serve("list_terminal_targets", [target()]);
    render(<TerminalsPanel activeWorktreePath="/repo" onOpenTerminal={vi.fn()} />);
    fireEvent.contextMenu(screen.getByRole("option", { name: /^Host shell 1,/ }));
    expect(screen.getByRole("menu", { name: "Host shell 1 actions" })).toBeInTheDocument();
    await userEvent.click(headerButton());
    await within(await screen.findByRole("menu", { name: "New terminal" })).findByRole("group", { name: "repo" });
    expect(screen.queryByRole("menu", { name: "Host shell 1 actions" })).toBeNull();
    fireEvent.contextMenu(screen.getByRole("option", { name: /^Host shell 1,/ }));
    expect(screen.queryByRole("menu", { name: "New terminal" })).toBeNull();
    expect(screen.getByRole("menu", { name: "Host shell 1 actions" })).toBeInTheDocument();
  });

  it("TRM-FR-JINY, TRM-FR-JZPT: the empty-state action opens the menu too, and Escape puts the focus back on that button", async () => {
    serve("list_terminal_targets", [target()]);
    await renderPanel();
    const action = screen.getAllByRole("button", { name: "New terminal" }).find((b) => b !== headerButton())!;
    await userEvent.click(action);
    await screen.findByRole("group", { name: "repo" });
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(action).toHaveFocus();
  });
});
