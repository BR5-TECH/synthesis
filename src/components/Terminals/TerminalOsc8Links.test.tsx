// The OSC 8 links, the link tooltip, and the native paste of an open terminal
// tab (`TRM-terminals.md` TRM-FR-QMVZ, TRM-FR-KIRO, TRM-FR-DCBR, TRM-FR-HWKO,
// TRM-FR-DHEM, TRM-FR-LNOZ).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { fakeNoLinkHook, fakeNoLinkProvider, fakeTerminals, type FakeLink, type FakeTerminal } from "../../test/ghosttyMock";
import { refreshTerminals, upsertTerminal } from "../../state/terminals";
import { hideTerminalView, pasteIntoTerminal, releaseTerminalView, showTerminalView, terminalViewStarted } from "../../state/terminalViews";
import { TerminalTab } from "./TerminalTab";
import { commands, FakeChannel, resetTerminalFixtures, serve, session } from "./terminalFixtures";
import { logWarn } from "../../logging";

vi.mock("@tauri-apps/api/core", async () => {
  const f = await import("./terminalFixtures");
  return { invoke: f.invokeMock, Channel: f.FakeChannel };
});
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./terminalFixtures")).listenMock,
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("../../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

const PR = "https://github.com/acme/app/pull/7";
const OSC8_FRAME = `\x1b]8;id=1;${PR}\x1b\\PR #7\x1b]8;;\x1b\\\r\n`;

async function open(id: string, theme = "paper") {
  serve("list_terminals", () => [session({ id, theme: theme as never })]);
  await act(() => refreshTerminals());
  const view = render(<TerminalTab terminalId={id} />);
  await act(() => terminalViewStarted(id));
  return view;
}

function termsOf(id: string): FakeTerminal[] {
  return fakeTerminals.filter((t) => (t.opened as HTMLElement | null)?.dataset.terminalId === id);
}

async function push(id: string, index: number, text: string) {
  const attach = commands("attach_terminal").filter((c) => c.args.terminalId === id)[index];
  const channel = attach.args.output as FakeChannel<ArrayBuffer>;
  await act(async () => channel.push(new TextEncoder().encode(text).buffer as ArrayBuffer));
}

/** The terminal shows "PR #7" with hyperlink id 1 on row 0, as ghostty-web would after `OSC8_FRAME`. */
function showLinkCells(term: FakeTerminal) {
  term.buffer.active.setRow(0, "PR #7", 20, [{ from: 0, to: 4, link: 1 }]);
}

/** The link at (2, 0) as the library gives it on hover: the detector asks the registered providers. */
async function hoveredLink(term: FakeTerminal): Promise<FakeLink> {
  const link = await term.linkDetector!.getLinkAt(2, 0);
  expect(link?.text).toBe(PR);
  return link!;
}

function pasteEvent(text: string | null): Event {
  const event = new Event("paste", { bubbles: true, cancelable: true });
  Object.defineProperty(event, "clipboardData", { value: text === null ? null : { getData: () => text } });
  return event;
}

const tooltips = () => document.querySelectorAll(".terminal-link-tooltip");
const linkWarns = () => vi.mocked(logWarn).mock.calls.filter((c) => c[1] === "terminal links not applied");

describe("the OSC 8 links of an open terminal tab", () => {
  let platform: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    resetTerminalFixtures();
    serve("attach_terminal", { state: "running", historyBytes: 0 });
    serve("acknowledge_terminal_output", null);
    serve("resize_terminal", null);
    serve("detach_terminal", null);
    serve("write_terminal_input", null);
    vi.mocked(openUrl).mockReset();
    vi.mocked(openUrl).mockResolvedValue(undefined);
    vi.mocked(logWarn).mockClear();
    platform = vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
  });
  afterEach(() => {
    cleanup();
    platform.mockRestore();
  });

  it("TRM-FR-QMVZ, TRM-FR-HWKO: a hyperlink in the output opens its address with Cmd+click", async () => {
    await open("a");
    const [term] = termsOf("a");
    await push("a", 0, OSC8_FRAME);
    showLinkCells(term);
    const links = await term.providedLinks(0);
    expect(links.map((l) => l.text)).toEqual([PR]);
    links[0].activate(new MouseEvent("click"));
    links[0].activate(new MouseEvent("click", { ctrlKey: true }));
    await act(async () => {});
    expect(openUrl).not.toHaveBeenCalled();
    links[0].activate(new MouseEvent("click", { metaKey: true }));
    await waitFor(() => expect(openUrl).toHaveBeenCalledWith(PR));
  });

  it("TRM-FR-QMVZ: cells with no hyperlink in the output give no link", async () => {
    await open("a");
    const [term] = termsOf("a");
    await push("a", 0, "PR #7\r\n");
    showLinkCells(term);
    expect(await term.providedLinks(0)).toEqual([]);
  });

  it("TRM-FR-QMVZ: a terminal made again reads its hyperlinks from the new attach", async () => {
    await open("a");
    act(() => upsertTerminal(session({ id: "a", theme: "sand" })));
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(2));
    await act(() => terminalViewStarted("a"));
    const terms = termsOf("a");
    expect(terms).toHaveLength(2);
    showLinkCells(terms[1]);
    await push("a", 0, OSC8_FRAME);
    expect(await terms[1].providedLinks(0)).toEqual([]);
    await push("a", 1, OSC8_FRAME);
    expect((await terms[1].providedLinks(0)).map((l) => l.text)).toEqual([PR]);
  });

  it("TRM-FR-KIRO, TRM-FR-DCBR: hover on a web link shows its address and the Cmd hint, and leave hides it", async () => {
    await open("a");
    const [term] = termsOf("a");
    await push("a", 0, OSC8_FRAME);
    showLinkCells(term);
    const link = await hoveredLink(term);
    const host = term.opened as HTMLElement;
    host.dispatchEvent(new MouseEvent("mousemove", { clientX: 30, clientY: 40 }));
    act(() => link.hover!(true));
    expect(tooltips()).toHaveLength(1);
    const tip = tooltips()[0] as HTMLElement;
    expect(tip.parentElement).toBe(document.body);
    expect(tip).toHaveAttribute("role", "tooltip");
    expect(tip).toHaveTextContent(PR);
    expect(tip).toHaveTextContent("⌘-click to open");
    act(() => link.hover!(false));
    expect(tooltips()).toHaveLength(0);
    act(() => link.hover!(true));
    host.dispatchEvent(new MouseEvent("mouseleave"));
    expect(tooltips()).toHaveLength(0);
  });

  it("TRM-FR-KIRO: on Linux the hint names Ctrl+click", async () => {
    platform.mockReturnValue("Linux x86_64");
    await open("a");
    const [term] = termsOf("a");
    await push("a", 0, OSC8_FRAME);
    showLinkCells(term);
    const link = await hoveredLink(term);
    (term.opened as HTMLElement).dispatchEvent(new MouseEvent("mousemove", { clientX: 1, clientY: 1 }));
    act(() => link.hover!(true));
    expect(tooltips()[0]).toHaveTextContent("Ctrl+click to open");
  });

  it("TRM-FR-KIRO: a link with another scheme shows no tooltip", async () => {
    await open("a");
    const [term] = termsOf("a");
    term.linkDetector!.links.set("0,0", { text: "file:///etc/hosts", range: { start: { x: 0, y: 0 }, end: { x: 3, y: 0 } }, activate: () => {} });
    const link = (await term.linkDetector!.getLinkAt(0, 0))!;
    (term.opened as HTMLElement).dispatchEvent(new MouseEvent("mousemove", { clientX: 1, clientY: 1 }));
    expect(link.hover).toBeUndefined();
  });

  it("TRM-FR-DCBR: the tooltip closes when the tab hides and when the tab closes", async () => {
    const view = await open("a");
    const [term] = termsOf("a");
    await push("a", 0, OSC8_FRAME);
    showLinkCells(term);
    const link = await hoveredLink(term);
    const host = term.opened as HTMLElement;
    host.dispatchEvent(new MouseEvent("mousemove", { clientX: 5, clientY: 5 }));
    act(() => link.hover!(true));
    const container = host.parentElement!;
    act(() => hideTerminalView("a"));
    expect(tooltips()).toHaveLength(0);
    act(() => showTerminalView("a", container));
    act(() => link.hover!(true));
    expect(tooltips()).toHaveLength(1);
    view.unmount();
    expect(tooltips()).toHaveLength(0);
    // A closed tab disposes the tooltip: a later hover of an old link shows none.
    act(() => releaseTerminalView("a", false));
    document.body.appendChild(host);
    host.dispatchEvent(new MouseEvent("mousemove", { clientX: 9, clientY: 9 }));
    act(() => link.hover!(true));
    expect(tooltips()).toHaveLength(0);
    host.remove();
  });

  it("TRM-FR-LNOZ: a library with no link provider hook gives one WARN for the terminal, also over remakes", async () => {
    fakeNoLinkProvider.on = true;
    await open("a");
    act(() => upsertTerminal(session({ id: "a", theme: "sand" })));
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(2));
    await act(() => terminalViewStarted("a"));
    expect(linkWarns()).toEqual([[["frontend"], "terminal links not applied", { terminalId: "a", reason: "no_link_provider" }]]);
  });

  it("TRM-FR-LNOZ: a library with neither link hook gives one WARN for the terminal", async () => {
    fakeNoLinkProvider.on = true;
    fakeNoLinkHook.on = true;
    await open("a");
    expect(linkWarns()).toHaveLength(1);
  });
});

describe("the native paste of an open terminal tab", () => {
  beforeEach(() => {
    resetTerminalFixtures();
    serve("attach_terminal", { state: "running", historyBytes: 0 });
    serve("acknowledge_terminal_output", null);
    serve("resize_terminal", null);
    serve("detach_terminal", null);
    serve("write_terminal_input", null);
  });
  afterEach(() => cleanup());

  it("TRM-FR-DHEM: a paste event on the terminal pastes its text once through the terminal", async () => {
    await open("a");
    const [term] = termsOf("a");
    const host = term.opened as HTMLElement;
    const canvas = host.querySelector("canvas")!;
    const library = vi.fn();
    canvas.addEventListener("paste", library);
    host.addEventListener("paste", library);
    const event = pasteEvent("echo hi");
    act(() => {
      canvas.dispatchEvent(event);
    });
    expect(term.pasted).toEqual(["echo hi"]);
    expect(event.defaultPrevented).toBe(true);
    expect(library).not.toHaveBeenCalled();
    await waitFor(() => expect(commands("write_terminal_input").map((c) => c.args.data)).toEqual(["echo hi"]));
  });

  it("TRM-FR-DHEM: a paste with no text pastes nothing", async () => {
    await open("a");
    const [term] = termsOf("a");
    act(() => {
      (term.opened as HTMLElement).dispatchEvent(pasteEvent(""));
      (term.opened as HTMLElement).dispatchEvent(pasteEvent(null));
    });
    expect(term.pasted).toEqual([]);
  });

  it("TRM-FR-DHEM: a paste before the terminal exists pastes nothing and keeps the text out of the page", () => {
    const event = pasteEvent("x");
    expect(pasteIntoTerminal(event as ClipboardEvent, null)).toBe(false);
    expect(event.defaultPrevented).toBe(true);
  });
});
