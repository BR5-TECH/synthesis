// The rename and the Theme submenu of a Terminals panel row (`TRM-terminals.md`
// TRM-FR-SHKD, TRM-FR-LIRN, TRM-FR-QXIY). The backend is mocked at `invoke`.
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
vi.mock("../../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

beforeEach(() => resetTerminalFixtures());
afterEach(cleanup);

async function seed(list: TerminalSummary[]) {
  serve("list_terminals", () => list);
  await act(() => refreshTerminals());
}

function renderPanel() {
  const props = { activeWorktreePath: "/repo", onOpenTerminal: vi.fn(), onOverlayOpening: vi.fn() };
  render(<TerminalsPanel {...props} />);
  return props;
}

const row = (label: string) => screen.getByRole("option", { name: new RegExp(`^${label},`) });

async function startRename(label = "Host shell 1") {
  fireEvent.contextMenu(row(label));
  await userEvent.click(screen.getByRole("menuitem", { name: "Rename terminal" }));
  return screen.getByRole("textbox", { name: "Terminal name" }) as HTMLInputElement;
}

describe("rename a terminal from its row", () => {
  it("TRM-FR-SHKD: Rename terminal shows the label in a field with the focus and all text selected", async () => {
    await seed([session({ id: "a" })]);
    renderPanel();
    const field = await startRename();
    expect(screen.queryByRole("menu")).toBeNull();
    expect(field).toHaveFocus();
    expect(field.value).toBe("Host shell 1");
    expect([field.selectionStart, field.selectionEnd]).toEqual([0, "Host shell 1".length]);
  });

  it("TRM-FR-LIRN, TRM-FR-SHKD: Enter sends the trimmed text, and the row shows the new label", async () => {
    const a = session({ id: "a" });
    await seed([a]);
    serve("set_terminal_label", (args) => ({ ...a, label: args.label }));
    const props = renderPanel();
    const field = await startRename();
    await userEvent.clear(field);
    await userEvent.type(field, "  build box  {Enter}");
    await waitFor(() => expect(screen.queryByRole("textbox")).toBeNull());
    expect(commands("set_terminal_label").map((c) => c.args)).toEqual([{ terminalId: "a", label: "build box" }]);
    expect(row("build box")).toBeInTheDocument();
    // A save by Enter puts the focus back on the row.
    expect(row("build box")).toHaveFocus();
    // TRM-FR-SHKD: Enter in the field opened no tab.
    expect(props.onOpenTerminal).not.toHaveBeenCalled();
  });

  it("TRM-FR-LIRN: a blur saves", async () => {
    const a = session({ id: "a" });
    await seed([a]);
    serve("set_terminal_label", (args) => ({ ...a, label: args.label }));
    renderPanel();
    const field = await startRename();
    await userEvent.clear(field);
    await userEvent.type(field, "logs");
    act(() => field.blur());
    await waitFor(() => expect(row("logs")).toBeInTheDocument());
    expect(commands("set_terminal_label")).toHaveLength(1);
  });

  it("TRM-FR-LIRN: Escape closes the field with no call and the focus goes back to the row", async () => {
    await seed([session({ id: "a" })]);
    renderPanel();
    const field = await startRename();
    await userEvent.type(field, "x{Escape}");
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(row("Host shell 1")).toHaveFocus();
    expect(commands("set_terminal_label")).toHaveLength(0);
  });

  it("TRM-FR-LIRN: empty or unchanged text closes the field with no call", async () => {
    await seed([session({ id: "a" })]);
    renderPanel();
    let field = await startRename();
    await userEvent.type(field, "{Enter}");
    expect(screen.queryByRole("textbox")).toBeNull();
    field = await startRename();
    await userEvent.clear(field);
    await userEvent.type(field, "   {Enter}");
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(commands("set_terminal_label")).toHaveLength(0);
  });

  it("TRM-FR-LIRN: a failed call keeps the field open with the error and emits a WARN record", async () => {
    const logging = await import("../../logging");
    vi.mocked(logging.logWarn).mockClear();
    await seed([session({ id: "a" })]);
    serve("set_terminal_label", () => {
      throw { code: "invalid_label" };
    });
    renderPanel();
    const field = await startRename();
    await userEvent.clear(field);
    fireEvent.change(field, { target: { value: "x".repeat(64) } });
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("A name has 1 to 64 characters.");
    expect(screen.getByRole("textbox", { name: "Terminal name" })).toBeInTheDocument();
    expect(logging.logWarn).toHaveBeenCalledWith(["frontend"], "terminal rename failed", {
      terminalId: "a",
      code: "invalid_label",
    });
    // TRM-FR-LNOZ: no record carries the name the author typed.
    expect(JSON.stringify(vi.mocked(logging.logWarn).mock.calls)).not.toContain("xxxx");
  });

  it("TRM-FR-LIRN: the field takes at most 64 characters, and a blur after a refused save closes it with no second call", async () => {
    await seed([session({ id: "a" })]);
    serve("set_terminal_label", () => {
      throw { code: "unknown_terminal" };
    });
    renderPanel();
    const field = await startRename();
    await userEvent.clear(field);
    await userEvent.paste("y".repeat(70));
    expect(field.value).toHaveLength(64);
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("This terminal is gone.");
    act(() => field.blur());
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(row("Host shell 1")).toBeInTheDocument();
    expect(commands("set_terminal_label")).toHaveLength(1);
  });

  it("TRM-FR-SHKD: a click, Enter, or an arrow key in the field opens no tab and moves no focus", async () => {
    await seed([session({ id: "a" }), session({ id: "b", label: "Host shell 2" })]);
    const props = renderPanel();
    const field = await startRename();
    await userEvent.click(field);
    fireEvent.keyDown(field, { key: "ArrowDown" });
    fireEvent.keyDown(field, { key: "ArrowUp" });
    expect(field).toHaveFocus();
    expect(props.onOpenTerminal).not.toHaveBeenCalled();
  });

  it("TRM-FR-LNOZ: a rename emits a DEBUG record with the id alone", async () => {
    const logging = await import("../../logging");
    vi.mocked(logging.logDebug).mockClear();
    const a = session({ id: "a" });
    await seed([a]);
    serve("set_terminal_label", (args) => ({ ...a, label: args.label }));
    renderPanel();
    const field = await startRename();
    await userEvent.clear(field);
    await userEvent.type(field, "private name{Enter}");
    await waitFor(() =>
      expect(logging.logDebug).toHaveBeenCalledWith(["frontend"], "terminal renamed", { terminalId: "a" }),
    );
    expect(JSON.stringify(vi.mocked(logging.logDebug).mock.calls)).not.toContain("private name");
  });
});

describe("the Theme submenu of a row", () => {
  const openTheme = async () => {
    fireEvent.contextMenu(row("Host shell 1"));
    fireEvent.mouseEnter(screen.getByRole("menuitem", { name: "Theme" }));
    return screen.getByRole("menu", { name: "Terminal theme" });
  };

  it("TRM-FR-QXIY, TRM-FR-CFXM: Theme opens Use default and the five themes, with the current one checked", async () => {
    await seed([session({ id: "a" })]);
    renderPanel();
    const submenu = await openTheme();
    const items = within(submenu).getAllByRole("menuitemradio");
    expect(items.map((i) => i.textContent)).toEqual(["Use default", "AaPaper", "AaSand", "AaMist", "AaNight", "AaEmber"]);
    expect(items.filter((i) => i.getAttribute("aria-checked") === "true").map((i) => i.textContent)).toEqual([
      "Use default",
    ]);
    expect(screen.getByRole("menuitem", { name: "Theme" })).toHaveAttribute("aria-expanded", "true");
  });

  it("TRM-FR-QXIY: the override of the terminal is checked", async () => {
    await seed([session({ id: "a", theme: "mist" })]);
    renderPanel();
    const submenu = await openTheme();
    expect(within(submenu).getByRole("menuitemradio", { name: /Mist/ })).toHaveAttribute("aria-checked", "true");
    expect(within(submenu).getByRole("menuitemradio", { name: "Use default" })).toHaveAttribute("aria-checked", "false");
  });

  it("TRM-FR-QXIY: a choice sets the theme, null for Use default, and closes both menus", async () => {
    const a = session({ id: "a", theme: "night" });
    await seed([a]);
    serve("set_terminal_theme", (args) => ({ ...a, theme: args.theme }));
    renderPanel();
    let submenu = await openTheme();
    await userEvent.click(within(submenu).getByRole("menuitemradio", { name: /Ember/ }));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(row("Host shell 1")).toHaveFocus();
    submenu = await openTheme();
    await userEvent.click(within(submenu).getByRole("menuitemradio", { name: "Use default" }));
    await waitFor(() => expect(commands("set_terminal_theme")).toHaveLength(2));
    expect(commands("set_terminal_theme").map((c) => c.args)).toEqual([
      { terminalId: "a", theme: "ember" },
      { terminalId: "a", theme: null },
    ]);
  });

  it("TRM-FR-QXIY, TRM-FR-VJXC: Right or Enter opens the submenu with the focus on its first entry; Left and Escape close it alone", async () => {
    await seed([session({ id: "a" })]);
    renderPanel();
    fireEvent.contextMenu(row("Host shell 1"));
    const menu = screen.getByRole("menu");
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    const theme = screen.getByRole("menuitem", { name: "Theme" });
    expect(theme).toHaveFocus();
    fireEvent.keyDown(theme, { key: "ArrowRight" });
    const submenu = screen.getByRole("menu", { name: "Terminal theme" });
    expect(within(submenu).getByRole("menuitemradio", { name: "Use default" })).toHaveFocus();
    fireEvent.keyDown(submenu, { key: "ArrowDown" });
    expect(within(submenu).getByRole("menuitemradio", { name: /Paper/ })).toHaveFocus();
    fireEvent.keyDown(submenu, { key: "ArrowLeft" });
    expect(screen.queryByRole("menu", { name: "Terminal theme" })).toBeNull();
    expect(theme).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    expect(screen.getByRole("menu", { name: "Terminal theme" })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu", { name: "Terminal theme" })).toBeNull();
    expect(screen.getByRole("menuitem", { name: "Theme" })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("TRM-FR-VJXC: Escape closes a submenu that a pointer opened, and the row menu stays", async () => {
    await seed([session({ id: "a" })]);
    renderPanel();
    await openTheme();
    expect(screen.getByRole("menuitem", { name: "Theme" })).toHaveFocus();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu", { name: "Terminal theme" })).toBeNull();
    expect(screen.getByRole("menuitem", { name: "Theme" })).toHaveFocus();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("TRM-FR-VJXC: Right on a submenu that a pointer opened puts the focus on its first entry", async () => {
    await seed([session({ id: "a" })]);
    renderPanel();
    await openTheme();
    const theme = screen.getByRole("menuitem", { name: "Theme" });
    expect(theme).toHaveFocus();
    fireEvent.keyDown(theme, { key: "ArrowRight" });
    expect(screen.getByRole("menuitemradio", { name: "Use default" })).toHaveFocus();
  });

  it("TRM-FR-QXIY: a pointer over another entry closes the submenu", async () => {
    await seed([session({ id: "a" })]);
    renderPanel();
    await openTheme();
    fireEvent.mouseEnter(screen.getByRole("menuitem", { name: "Rename terminal" }));
    expect(screen.queryByRole("menu", { name: "Terminal theme" })).toBeNull();
  });

  it("TRM-FR-LNOZ: a failed theme change emits a WARN record and keeps the override", async () => {
    const logging = await import("../../logging");
    vi.mocked(logging.logWarn).mockClear();
    await seed([session({ id: "a" })]);
    serve("set_terminal_theme", () => {
      throw { code: "unknown_terminal" };
    });
    renderPanel();
    const submenu = await openTheme();
    await userEvent.click(within(submenu).getByRole("menuitemradio", { name: /Sand/ }));
    await waitFor(() =>
      expect(logging.logWarn).toHaveBeenCalledWith(["frontend"], "terminal theme change failed", {
        terminalId: "a",
        code: "unknown_terminal",
      }),
    );
    const again = await openTheme();
    expect(within(again).getByRole("menuitemradio", { name: "Use default" })).toHaveAttribute("aria-checked", "true");
  });
});
