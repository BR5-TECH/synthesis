// The links of an open terminal tab (`TRM-terminals.md` TRM-FR-HWKO, TRM-FR-LNOZ).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { fakeNoLinkHook, fakeTerminals, type FakeLink, type FakeTerminal } from "../../test/ghosttyMock";
import { refreshTerminals, upsertTerminal } from "../../state/terminals";
import { terminalViewStarted } from "../../state/terminalViews";
import { TerminalTab } from "./TerminalTab";
import { commands, resetTerminalFixtures, serve, session } from "./terminalFixtures";
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

const PR_URL = "https://github.com/acme/app/pull/7";

async function open(id: string, theme: string) {
  serve("list_terminals", () => [session({ id, theme: theme as never })]);
  await act(() => refreshTerminals());
  render(<TerminalTab terminalId={id} />);
  await act(() => terminalViewStarted(id));
}

function termsOf(id: string): FakeTerminal[] {
  return fakeTerminals.filter((t) => (t.opened as HTMLElement | null)?.dataset.terminalId === id);
}

function libraryLink(text: string): FakeLink {
  return {
    text,
    range: { start: { x: 0, y: 0 }, end: { x: text.length - 1, y: 0 } },
    activate: (event) => {
      if (event.ctrlKey || event.metaKey) window.open(text, "_blank", "noopener,noreferrer");
    },
  };
}

/** Cmd+click (as the library's click handler does) on the link that `term` holds at (0, 0). */
async function cmdClick(term: FakeTerminal, url: string) {
  term.linkDetector!.links.set("0,0", libraryLink(url));
  const link = await term.linkDetector!.getLinkAt(0, 0);
  link!.activate(new MouseEvent("click", { metaKey: true }));
}

const linkWarns = () => vi.mocked(logWarn).mock.calls.filter((c) => c[1] === "terminal links not applied");

describe("the links of an open terminal tab", () => {
  let platform: ReturnType<typeof vi.spyOn>;
  let windowOpen: ReturnType<typeof vi.spyOn>;

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
    windowOpen = vi.spyOn(window, "open").mockReturnValue(null);
  });
  afterEach(() => {
    cleanup();
    platform.mockRestore();
    windowOpen.mockRestore();
  });

  it("TRM-FR-HWKO: Cmd+click on a link in the terminal opens it in the system browser", async () => {
    await open("a", "paper");
    const [term] = termsOf("a");
    await cmdClick(term, PR_URL);
    await waitFor(() => expect(openUrl).toHaveBeenCalledWith(PR_URL));
    expect(windowOpen).not.toHaveBeenCalled();
  });

  it("TRM-FR-HWKO: a terminal made again for a theme change opens its links too", async () => {
    await open("a", "paper");
    act(() => upsertTerminal(session({ id: "a", theme: "sand" })));
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(2));
    await act(() => terminalViewStarted("a"));
    const terms = termsOf("a");
    expect(terms).toHaveLength(2);
    await cmdClick(terms[1], PR_URL);
    await waitFor(() => expect(openUrl).toHaveBeenCalledWith(PR_URL));
    expect(windowOpen).not.toHaveBeenCalled();
  });

  it("TRM-FR-LNOZ: a terminal library without the link hook gives one WARN for the terminal, also over remakes", async () => {
    fakeNoLinkHook.on = true;
    await open("a", "paper");
    act(() => upsertTerminal(session({ id: "a", theme: "sand" })));
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(2));
    await act(() => terminalViewStarted("a"));
    expect(linkWarns()).toEqual([[["frontend"], "terminal links not applied", { terminalId: "a", reason: "no_link_hook" }]]);
    cleanup();
    await open("b", "paper");
    expect(linkWarns().map((c) => (c[2] as { terminalId: string }).terminalId)).toEqual(["a", "b"]);
    const others = vi.mocked(logWarn).mock.calls.filter((c) => c[1] === "terminal contrast not applied" || c[1] === "terminal canvas scale not kept");
    expect(others).toEqual([]);
  });
});
