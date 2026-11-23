// The menu of a Terminal tab: Rename terminal and Theme before the five entries
// (`TAB-tabs.md` TAB-FR-YUOZ, TAB-FR-33; `TRM-terminals.md` TRM-FR-MOVS).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { refreshTerminals } from "../state/terminals";
import type { Tab, TerminalSummary } from "../types";
import { TabStrip, type TabMenuState } from "./TabStrip";
import { commands, resetTerminalFixtures, serve, session } from "./Terminals/terminalFixtures";

vi.mock("@tauri-apps/api/core", async () => {
  const f = await import("./Terminals/terminalFixtures");
  return { invoke: f.invokeMock, Channel: f.FakeChannel };
});
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./Terminals/terminalFixtures")).listenMock,
}));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

beforeEach(() => resetTerminalFixtures());
afterEach(cleanup);

const TABS: Tab[] = [
  { id: "dashboard", label: "Dashboard" },
  { id: "terminal:a", label: "Host shell 1", kind: "terminal", terminalId: "a" },
];

async function seed(list: TerminalSummary[]) {
  serve("list_terminals", () => list);
  await act(() => refreshTerminals());
}

function Strip({ onActivate }: { onActivate: (id: string) => void }) {
  const [menu, setMenu] = useState<TabMenuState | null>(null);
  return (
    <TabStrip
      tabs={TABS}
      activeId="dashboard"
      onActivate={onActivate}
      onClose={() => {}}
      onHome={() => {}}
      menu={menu}
      onMenuChange={setMenu}
      massCloseTargets={() => []}
    />
  );
}

const tab = (id: string) => document.querySelector<HTMLElement>(`.tab[data-tab-id="${id}"]`)!;
const entries = () =>
  within(screen.getByTestId("tab-context-menu"))
    .getAllByRole("menuitem")
    .map((m) => m.textContent);

describe("the menu of a Terminal tab", () => {
  it("TAB-FR-YUOZ: Rename terminal and Theme, and a separator, come before the five entries", async () => {
    await seed([session({ id: "a" })]);
    render(<Strip onActivate={vi.fn()} />);
    fireEvent.contextMenu(tab("terminal:a"));
    expect(entries()).toEqual([
      "Rename terminal",
      "Theme",
      "Pin tab",
      "Close tabs to the left",
      "Close tabs to the right",
      "Close other tabs",
      "Close tab",
    ]);
    const menu = screen.getByTestId("tab-context-menu");
    expect(within(menu).getAllByRole("separator")).toHaveLength(3);
    expect(menu.firstElementChild?.nextElementSibling?.nextElementSibling).toHaveAttribute("role", "separator");
    for (const item of within(menu).getAllByRole("menuitem")) expect(item.firstElementChild?.tagName).toBe("svg");
  });

  it("TAB-FR-33: a tab that is no Terminal tab keeps its five entries", async () => {
    await seed([session({ id: "a" })]);
    render(<Strip onActivate={vi.fn()} />);
    fireEvent.contextMenu(tab("dashboard"));
    expect(entries()).toEqual(["Pin tab", "Close tabs to the left", "Close tabs to the right", "Close other tabs", "Close tab"]);
  });

  it("TRM-FR-MOVS, TRM-FR-LIRN: Rename terminal turns the tab label into the field, and Enter renames", async () => {
    const a = session({ id: "a" });
    await seed([a]);
    serve("set_terminal_label", (args) => ({ ...a, label: args.label }));
    const onActivate = vi.fn();
    render(<Strip onActivate={onActivate} />);
    fireEvent.contextMenu(tab("terminal:a"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename terminal" }));
    const field = screen.getByRole("textbox", { name: "Terminal name" }) as HTMLInputElement;
    expect(field).toHaveFocus();
    expect([field.selectionStart, field.selectionEnd]).toEqual([0, field.value.length]);
    await userEvent.click(field);
    // TRM-FR-MOVS: a click in the field does not activate the tab.
    expect(onActivate).not.toHaveBeenCalled();
    // TRM-FR-SHKD: an arrow key in the field moves no focus along the strip.
    fireEvent.keyDown(field, { key: "ArrowLeft" });
    fireEvent.keyDown(field, { key: "Home" });
    expect(field).toHaveFocus();
    await userEvent.clear(field);
    await userEvent.type(field, "deploy{Enter}");
    await waitFor(() => expect(screen.queryByRole("textbox")).toBeNull());
    expect(onActivate).not.toHaveBeenCalled();
    expect(commands("set_terminal_label").map((c) => c.args)).toEqual([{ terminalId: "a", label: "deploy" }]);
  });

  it("TRM-FR-MOVS: Escape closes the field with no call and the focus goes back to the tab", async () => {
    await seed([session({ id: "a" })]);
    render(<Strip onActivate={vi.fn()} />);
    fireEvent.contextMenu(tab("terminal:a"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename terminal" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(tab("terminal:a")).toHaveFocus();
    expect(commands("set_terminal_label")).toHaveLength(0);
  });

  it("TRM-FR-MOVS, TRM-FR-QXIY: Theme opens the submenu with the override checked, and a choice sets it and closes both menus", async () => {
    const a = session({ id: "a", theme: "sand" });
    await seed([a]);
    serve("set_terminal_theme", (args) => ({ ...a, theme: args.theme }));
    render(<Strip onActivate={vi.fn()} />);
    fireEvent.contextMenu(tab("terminal:a"));
    fireEvent.mouseEnter(screen.getByRole("menuitem", { name: "Theme" }));
    const submenu = screen.getByRole("menu", { name: "Terminal theme" });
    expect(within(submenu).getByRole("menuitemradio", { name: /Sand/ })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(within(submenu).getByRole("menuitemradio", { name: "Use default" }));
    expect(screen.queryByTestId("tab-context-menu")).toBeNull();
    expect(tab("terminal:a")).toHaveFocus();
    await waitFor(() => expect(commands("set_terminal_theme").map((c) => c.args)).toEqual([{ terminalId: "a", theme: null }]));
  });

  it("TRM-FR-VJXC: Escape closes a Theme submenu that a pointer opened, and the tab menu stays", async () => {
    await seed([session({ id: "a" })]);
    render(<Strip onActivate={vi.fn()} />);
    fireEvent.contextMenu(tab("terminal:a"));
    fireEvent.mouseEnter(screen.getByRole("menuitem", { name: "Theme" }));
    expect(screen.getByRole("menu", { name: "Terminal theme" })).toBeInTheDocument();
    // A pointer over Theme puts the focus on it, so Enter acts on Theme.
    expect(screen.getByRole("menuitem", { name: "Theme" })).toHaveFocus();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu", { name: "Terminal theme" })).toBeNull();
    expect(screen.getByTestId("tab-context-menu")).toBeInTheDocument();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByTestId("tab-context-menu")).toBeNull();
  });

  it("TRM-FR-VJXC: Escape in the Theme submenu closes the submenu alone, and Escape again closes the menu", async () => {
    await seed([session({ id: "a" })]);
    render(<Strip onActivate={vi.fn()} />);
    fireEvent.contextMenu(tab("terminal:a"));
    const theme = screen.getByRole("menuitem", { name: "Theme" });
    act(() => theme.focus());
    fireEvent.keyDown(theme, { key: "Enter" });
    const submenu = screen.getByRole("menu", { name: "Terminal theme" });
    expect(within(submenu).getByRole("menuitemradio", { name: "Use default" })).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu", { name: "Terminal theme" })).toBeNull();
    expect(screen.getByTestId("tab-context-menu")).toBeInTheDocument();
    expect(theme).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByTestId("tab-context-menu")).toBeNull();
  });
});
