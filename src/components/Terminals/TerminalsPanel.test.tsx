// The Terminals panel and its row context menu (`TRM-terminals.md`). The
// backend is mocked at `invoke`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { refreshTerminals } from "../../state/terminals";
import type { TerminalSummary } from "../../types";
import { TerminalsPanel } from "./TerminalsPanel";
import { commands, resetTerminalFixtures, serve, session } from "./terminalFixtures";

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
});
afterEach(cleanup);

async function seed(list: TerminalSummary[]) {
  serve("list_terminals", () => list);
  await act(() => refreshTerminals());
}

function renderPanel(extra: Partial<Parameters<typeof TerminalsPanel>[0]> = {}) {
  const props = {
    activeWorktreePath: "/repo",
    onOpenTerminal: vi.fn(),
    onOverlayOpening: vi.fn(),
    ...extra,
  };
  render(<TerminalsPanel {...props} />);
  return props;
}

const row = (label: string) => screen.getByRole("option", { name: new RegExp(`^${label},`) });

function labels(): string[] {
  return screen.getAllByRole("option").map((o) => o.getAttribute("aria-label")!.split(",")[0]);
}

describe("Terminals panel rows", () => {
  it("TRM-FR-QSTC: with no session the panel shows the empty state with New terminal", async () => {
    await seed([]);
    renderPanel();
    expect(screen.getByText("No terminals")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "New terminal" }).length).toBe(2);
  });

  it("TRM-FR-LAZH, TRM-FR-EAUY, TRM-FR-BEYN: rows are grouped by target and name label, kind, place, state", async () => {
    await seed([
      session({ id: "s", label: "Microsandbox shell 1", kind: "microsandbox_shell", runsIn: "microsandbox", environmentName: "vm-auth", target: { kind: "work_stream", streamId: "st" }, targetName: "auth", state: "exited", exitCode: 0 }),
      session({ id: "a", label: "Host shell 1" }),
      session({ id: "c", label: "Claude Code 1", kind: "agent_cli", vendor: "claude_code", runsIn: "docker", environmentName: "syn-c" }),
    ]);
    renderPanel();
    const groups = screen.getAllByRole("group");
    expect(groups.map((g) => g.getAttribute("aria-label"))).toEqual(["repo", "auth"]);
    expect(within(groups[0]).getByText("active worktree")).toBeInTheDocument();
    expect(within(groups[1]).getByText("work stream")).toBeInTheDocument();
    expect(within(row("Claude Code 1")).getByText("Agent CLI · Claude Code · docker · syn-c")).toBeInTheDocument();
    expect(within(row("Microsandbox shell 1")).getByText("exited 0")).toBeInTheDocument();
    expect(within(row("Host shell 1")).getByText("running")).toBeInTheDocument();
  });

  it("TRM-FR-ZQPC, TRM-FR-DYKW: a pinned row sorts first in its group and shows the marker", async () => {
    await seed([session({ id: "a", label: "Host shell 1" }), session({ id: "b", label: "Host shell 2", pinned: true })]);
    renderPanel();
    expect(labels()).toEqual(["Host shell 2", "Host shell 1"]);
    expect(row("Host shell 2")).toHaveAttribute("data-pinned", "true");
    expect(row("Host shell 1")).not.toHaveAttribute("data-pinned");
  });

  it("TRM-FR-CTGK: a row of a busy target shows the busy mark", async () => {
    await seed([session({ id: "a", busyRunId: "run-1" })]);
    renderPanel();
    expect(within(row("Host shell 1")).getByLabelText("A graduation run is working in this target")).toBeInTheDocument();
  });

  it("TRM-FR-OXKC: at 16 sessions New terminal is disabled and says why", async () => {
    await seed(Array.from({ length: 16 }, (_, i) => session({ id: `t${i}`, label: `Host shell ${i + 1}` })));
    renderPanel();
    const button = screen.getByRole("button", { name: "New terminal" });
    expect(button).toBeDisabled();
    expect(button.parentElement).toHaveAttribute("title", "The limit of 16 terminals is reached");
  });

  it("TRM-FR-PZJI: a click or Enter opens the session's tab and selects the row", async () => {
    await seed([session({ id: "a" })]);
    const props = renderPanel();
    await userEvent.click(row("Host shell 1"));
    expect(props.onOpenTerminal).toHaveBeenCalledWith(expect.objectContaining({ id: "a" }));
    expect(row("Host shell 1")).toHaveAttribute("aria-selected", "true");
    fireEvent.keyDown(row("Host shell 1"), { key: "Enter" });
    expect(props.onOpenTerminal).toHaveBeenCalledTimes(2);
  });

  it("TRM-FR-UIWG: the arrow keys move the focus across groups", async () => {
    await seed([
      session({ id: "a", label: "Host shell 1" }),
      session({ id: "s", label: "Host shell 2", target: { kind: "work_stream", streamId: "st" }, targetName: "auth" }),
    ]);
    renderPanel();
    row("Host shell 1").focus();
    fireEvent.keyDown(row("Host shell 1"), { key: "ArrowDown" });
    expect(row("Host shell 2")).toHaveFocus();
    fireEvent.keyDown(row("Host shell 2"), { key: "ArrowUp" });
    expect(row("Host shell 1")).toHaveFocus();
  });

  it("TRM-FR-RCCE: a reveal of a terminal selects its row", async () => {
    await seed([session({ id: "a" })]);
    const { rerender } = render(<TerminalsPanel activeWorktreePath="/repo" onOpenTerminal={vi.fn()} reveal={null} />);
    rerender(
      <TerminalsPanel activeWorktreePath="/repo" onOpenTerminal={vi.fn()} reveal={{ panel: "terminals", id: "a", nonce: 1, optimistic: false, focus: false }} />,
    );
    expect(row("Host shell 1")).toHaveAttribute("aria-selected", "true");
  });
});

describe("Terminals row context menu", () => {
  const menuItems = () => screen.getAllByRole("menuitem").map((m) => m.getAttribute("aria-label"));

  it("TRM-FR-EUMZ, TRM-FR-KFCS: a right-click opens four entries in order, with a separator after the pin action", async () => {
    await seed([session({ id: "a" }), session({ id: "b", label: "Host shell 2" })]);
    const props = renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    expect(props.onOverlayOpening).toHaveBeenCalled();
    expect(props.onOpenTerminal).not.toHaveBeenCalled();
    expect(menuItems()).toEqual(["Pin terminal", "Close terminal", "Close other terminals", "Close all terminals"]);
    const menu = screen.getByRole("menu");
    expect(within(menu).getAllByRole("separator")).toHaveLength(1);
    expect(menu.querySelectorAll("svg").length).toBe(4);
  });

  it("TRM-FR-EUMZ: Shift+F10 and the context-menu key open it; Escape closes it and returns focus to the row", async () => {
    await seed([session({ id: "a" })]);
    renderPanel();
    fireEvent.keyDown(row("Host shell 1"), { key: "F10", shiftKey: true });
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(row("Host shell 1")).toHaveFocus();
    fireEvent.keyDown(row("Host shell 1"), { key: "ContextMenu" });
    expect(screen.getByRole("menu")).toBeInTheDocument();
    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("TRM-FR-EUMZ: the arrow keys move through the entries and Enter chooses", async () => {
    await seed([session({ id: "a" })]);
    serve("close_terminal", null);
    renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    expect(screen.getByRole("menuitem", { name: "Pin terminal" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "ArrowDown" });
    expect(screen.getByRole("menuitem", { name: "Close terminal" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    // The row is running, so the choice asks first.
    expect(await screen.findByRole("dialog", { name: "End running terminals" })).toBeInTheDocument();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("TRM-FR-KFCS, TRM-FR-AHXU: a pinned row offers Unpin terminal, and the pin changes nothing else", async () => {
    const pinned = session({ id: "a", pinned: true });
    await seed([pinned]);
    serve("set_terminal_pinned", (args) => ({ ...pinned, pinned: Boolean(args.pinned) }));
    const props = renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    expect(menuItems()).toContain("Unpin terminal");
    expect(menuItems()).not.toContain("Pin terminal");
    await userEvent.click(screen.getByRole("menuitem", { name: "Unpin terminal" }));
    await waitFor(() => expect(row("Host shell 1")).not.toHaveAttribute("data-pinned"));
    expect(commands("set_terminal_pinned")[0].args).toEqual({ terminalId: "a", pinned: false });
    expect(commands("close_terminal")).toHaveLength(0);
    expect(props.onOpenTerminal).not.toHaveBeenCalled();
  });

  it("TRM-FR-MANC, TRM-FR-OCSN: Close terminal on an exited row ends it with no question", async () => {
    await seed([session({ id: "a", state: "exited", exitCode: 0 })]);
    serve("close_terminal", null);
    renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Close terminal" }));
    await waitFor(() => expect(screen.queryByRole("option")).toBeNull());
    expect(commands("close_terminal").map((c) => c.args.terminalId)).toEqual(["a"]);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("TRM-FR-JJEY: Close terminal on a running row asks first, and Cancel changes nothing", async () => {
    await seed([session({ id: "a", pinned: true })]);
    serve("close_terminal", null);
    renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Close terminal" }));
    const dialog = screen.getByRole("dialog", { name: "End running terminals" });
    expect(within(dialog).getByText("End 1 running terminal?")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(commands("close_terminal")).toHaveLength(0);
    expect(row("Host shell 1")).toBeInTheDocument();
  });

  it("TRM-FR-BHYF, TRM-FR-JJEY: Close other terminals ends every unpinned row in all groups and names the running count", async () => {
    await seed([
      session({ id: "a", label: "Host shell 1" }),
      session({ id: "p", label: "Host shell 2", pinned: true }),
      session({ id: "s", label: "Host shell 3", target: { kind: "work_stream", streamId: "st" }, targetName: "auth" }),
      session({ id: "x", label: "Host shell 4", state: "exited", exitCode: 1 }),
    ]);
    serve("close_terminal", null);
    renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Close other terminals" }));
    const dialog = screen.getByRole("dialog", { name: "End running terminals" });
    expect(within(dialog).getByText("End 1 running terminal?")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "End terminals" }));
    await waitFor(() => expect(commands("close_terminal")).toHaveLength(2));
    expect(commands("close_terminal").map((c) => c.args.terminalId).sort()).toEqual(["s", "x"]);
    await waitFor(() => expect(labels().sort()).toEqual(["Host shell 1", "Host shell 2"]));
  });

  it("TRM-FR-BHYF: Close all terminals ends the clicked row too, and never a pinned one", async () => {
    await seed([
      session({ id: "a", label: "Host shell 1", state: "exited" }),
      session({ id: "p", label: "Host shell 2", pinned: true }),
    ]);
    serve("close_terminal", null);
    renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Close all terminals" }));
    await waitFor(() => expect(commands("close_terminal").map((c) => c.args.terminalId)).toEqual(["a"]));
    await waitFor(() => expect(labels()).toEqual(["Host shell 2"]));
  });

  it("TRM-FR-EZMO: the mass closes are disabled while they would end nothing", async () => {
    await seed([session({ id: "p", pinned: true }), session({ id: "q", label: "Host shell 2", pinned: true })]);
    renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    expect(screen.getByRole("menuitem", { name: "Close other terminals" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Close all terminals" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Close terminal" })).toBeEnabled();
    expect(screen.getByRole("menuitem", { name: "Unpin terminal" })).toBeEnabled();
  });

  it("TRM-FR-EZMO: with the clicked row the only unpinned one, only Close all terminals acts", async () => {
    await seed([session({ id: "a", label: "Host shell 1" }), session({ id: "p", label: "Host shell 2", pinned: true })]);
    renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    expect(screen.getByRole("menuitem", { name: "Close other terminals" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Close all terminals" })).toBeEnabled();
  });

  it("TRM-FR-JJEY: Close all terminals names every running and starting session it ends", async () => {
    await seed([
      session({ id: "a", label: "Host shell 1" }),
      session({ id: "b", label: "Host shell 2", state: "starting" }),
      session({ id: "g", label: "Host shell 3", state: "target_gone" }),
    ]);
    serve("close_terminal", null);
    renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Close all terminals" }));
    const dialog = screen.getByRole("dialog", { name: "End running terminals" });
    expect(within(dialog).getByText("End 2 running terminals?")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "End terminals" }));
    await waitFor(() => expect(commands("close_terminal")).toHaveLength(3));
  });
});
