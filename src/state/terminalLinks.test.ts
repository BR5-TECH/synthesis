// The links of a terminal open in the system browser (`TRM-terminals.md` TRM-FR-HWKO, TRM-FR-LNOZ).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import { activateTerminalLink, isBrowserLink, openLinksInBrowser } from "./terminalLinks";
import { FakeLinkDetector, type FakeLink } from "../test/ghosttyMock";
import { logDebug, logWarn } from "../logging";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

const URL_TEXT = "https://github.com/acme/app/pull/7";

/** Lets the microtasks of an activation run, so a check that nothing opened means something. */
async function flush() {
  await Promise.resolve();
  await Promise.resolve();
}

function click(mods: { metaKey?: boolean; ctrlKey?: boolean } = {}): MouseEvent {
  return new MouseEvent("click", { metaKey: mods.metaKey ?? false, ctrlKey: mods.ctrlKey ?? false });
}

/** The library's own activation: it calls `window.open`, which the WebView ignores. */
function libraryLink(text: string): FakeLink {
  return {
    text,
    range: { start: { x: 0, y: 0 }, end: { x: text.length - 1, y: 0 } },
    activate: (event) => {
      if (event.ctrlKey || event.metaKey) window.open(text, "_blank", "noopener,noreferrer");
    },
  };
}

/** A terminal with a link detector that holds `link` at cell (0, 0), with the hook applied. */
function terminalWith(link: FakeLink) {
  const linkDetector = new FakeLinkDetector();
  linkDetector.links.set("0,0", link);
  const term = { linkDetector };
  expect(openLinksInBrowser(term, "t1")).toBe(true);
  return linkDetector;
}

const records = () => [...vi.mocked(logDebug).mock.calls, ...vi.mocked(logWarn).mock.calls];

describe("terminal links", () => {
  let platform: ReturnType<typeof vi.spyOn>;
  let windowOpen: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    vi.mocked(openUrl).mockReset();
    vi.mocked(openUrl).mockResolvedValue(undefined);
    vi.mocked(logDebug).mockClear();
    vi.mocked(logWarn).mockClear();
    platform = vi.spyOn(navigator, "platform", "get").mockReturnValue("MacIntel");
    windowOpen = vi.spyOn(window, "open").mockReturnValue(null);
  });
  afterEach(() => {
    platform.mockRestore();
    windowOpen.mockRestore();
  });

  it("TRM-FR-HWKO: only http and https addresses are browser links", () => {
    expect(isBrowserLink("https://example.com")).toBe(true);
    expect(isBrowserLink("HTTP://example.com/a")).toBe(true);
    for (const url of ["https://", "mailto:a@b.c", "ssh://host", "file:///etc/passwd", "ftp://host/x", "javascript:alert(1)", "example.com"]) {
      expect(isBrowserLink(url)).toBe(false);
    }
  });

  it("TRM-FR-HWKO, TRM-FR-LNOZ: Cmd+click on macOS opens the link through the opener, not window.open", async () => {
    const detector = terminalWith(libraryLink(URL_TEXT));
    const link = await detector.getLinkAt(0, 0);
    link!.activate(click({ metaKey: true }));
    await vi.waitFor(() => expect(openUrl).toHaveBeenCalledWith(URL_TEXT));
    expect(windowOpen).not.toHaveBeenCalled();
    expect(logDebug).toHaveBeenCalledWith(["frontend"], "terminal link opened", { terminalId: "t1", scheme: "https" });
  });

  it("TRM-FR-HWKO: Ctrl+click on Linux opens the link, and Cmd+click there opens nothing", async () => {
    platform.mockReturnValue("Linux x86_64");
    const detector = terminalWith(libraryLink(URL_TEXT));
    const link = await detector.getLinkAt(0, 0);
    link!.activate(click({ metaKey: true }));
    await flush();
    expect(openUrl).not.toHaveBeenCalled();
    link!.activate(click({ ctrlKey: true }));
    await vi.waitFor(() => expect(openUrl).toHaveBeenCalledWith(URL_TEXT));
    expect(openUrl).toHaveBeenCalledTimes(1);
  });

  it("TRM-FR-HWKO: a plain click and Ctrl+click on macOS open nothing and log nothing", async () => {
    const detector = terminalWith(libraryLink(URL_TEXT));
    const link = await detector.getLinkAt(0, 0);
    link!.activate(click());
    link!.activate(click({ ctrlKey: true }));
    await flush();
    expect(openUrl).not.toHaveBeenCalled();
    expect(windowOpen).not.toHaveBeenCalled();
    expect(records()).toEqual([]);
  });

  it("TRM-FR-HWKO, TRM-FR-LNOZ: a link with another scheme opens nothing and logs only its scheme", async () => {
    for (const url of ["mailto:dev@example.com", "ssh://git@example.com/repo", "file:///Users/dev/secret"]) {
      activateTerminalLink(url, click({ metaKey: true }), "t1", true);
    }
    await flush();
    expect(openUrl).not.toHaveBeenCalled();
    expect(vi.mocked(logDebug).mock.calls).toEqual([
      [["frontend"], "terminal link refused", { terminalId: "t1", scheme: "mailto" }],
      [["frontend"], "terminal link refused", { terminalId: "t1", scheme: "ssh" }],
      [["frontend"], "terminal link refused", { terminalId: "t1", scheme: "file" }],
    ]);
  });

  it("TRM-FR-LNOZ: a link that the opener cannot open gives a WARN with the scheme and no address", async () => {
    vi.mocked(openUrl).mockRejectedValue(new Error(`not allowed: ${URL_TEXT}`));
    activateTerminalLink(URL_TEXT, click({ metaKey: true }), "t1", true);
    await vi.waitFor(() =>
      expect(logWarn).toHaveBeenCalledWith(["frontend"], "terminal link could not be opened", { terminalId: "t1", scheme: "https" }),
    );
    expect(JSON.stringify(records())).not.toContain("github.com");
  });

  it("TRM-FR-LNOZ: an opener that throws at once gives the same WARN and no unhandled error", async () => {
    vi.mocked(openUrl).mockImplementation(() => {
      throw new Error("no runtime");
    });
    expect(() => activateTerminalLink(URL_TEXT, click({ metaKey: true }), "t1", true)).not.toThrow();
    await vi.waitFor(() => expect(logWarn).toHaveBeenCalledTimes(1));
  });

  it("TRM-FR-HWKO: the detector gives the same link object each time, changed once", async () => {
    const original = libraryLink(URL_TEXT);
    const detector = terminalWith(original);
    const first = await detector.getLinkAt(0, 0);
    const activate = first!.activate;
    const second = await detector.getLinkAt(0, 0);
    expect(first).toBe(original);
    expect(second).toBe(original);
    expect(second!.activate).toBe(activate);
    expect(await detector.getLinkAt(5, 5)).toBeUndefined();
  });

  it("TRM-FR-HWKO: a link opens the address that the library found, also for an OSC 8 link with other text", async () => {
    const osc8 = { ...libraryLink("https://example.com/docs?a=1#b"), range: { start: { x: 0, y: 0 }, end: { x: 3, y: 1 } } };
    const detector = terminalWith(osc8);
    (await detector.getLinkAt(0, 0))!.activate(click({ metaKey: true }));
    await vi.waitFor(() => expect(openUrl).toHaveBeenCalledWith("https://example.com/docs?a=1#b"));
  });

  it("TRM-FR-HWKO, TRM-FR-LNOZ: Cmd+Ctrl+click on macOS opens, and the scheme of the record is in lower case", async () => {
    activateTerminalLink("HTTPS://EXAMPLE.COM/A", click({ metaKey: true, ctrlKey: true }), "t1", true);
    await vi.waitFor(() => expect(openUrl).toHaveBeenCalledWith("HTTPS://EXAMPLE.COM/A"));
    expect(logDebug).toHaveBeenCalledWith(["frontend"], "terminal link opened", { terminalId: "t1", scheme: "https" });
  });

  it("TRM-FR-HWKO, TRM-FR-LNOZ: an address with no host, a leading space, or no scheme is refused", async () => {
    const urls = ["http://", " https://example.com", "www.example.com", "averyveryverylongscheme://x", "javascript:alert(1)", "secret123:x"];
    for (const url of urls) activateTerminalLink(url, click({ ctrlKey: true }), "t1", false);
    await flush();
    expect(openUrl).not.toHaveBeenCalled();
    expect(vi.mocked(logDebug).mock.calls.map((c) => (c[2] as { scheme: string }).scheme)).toEqual([
      "http",
      "none",
      "none",
      "none",
      "javascript",
      "other",
    ]);
  });

  it("TRM-FR-HWKO: Cmd+click on a link of another scheme from the detector opens nothing, also no window", async () => {
    for (const url of ["mailto:dev@example.com", "file:///etc/hosts"]) {
      const detector = terminalWith(libraryLink(url));
      (await detector.getLinkAt(0, 0))!.activate(click({ metaKey: true }));
    }
    await flush();
    expect(openUrl).not.toHaveBeenCalled();
    expect(windowOpen).not.toHaveBeenCalled();
  });

  it("TRM-FR-HWKO: a click without the modifier on a refused link gives no record", async () => {
    activateTerminalLink("mailto:dev@example.com", click(), "t1", true);
    await flush();
    expect(records()).toEqual([]);
  });

  it("TRM-FR-LNOZ: no record of an opened, a refused, or a failed link carries the address", async () => {
    activateTerminalLink("https://x.test/secret-token", click({ metaKey: true }), "t1", true);
    activateTerminalLink("file:///x.test/secret-token", click({ metaKey: true }), "t1", true);
    await vi.waitFor(() => expect(openUrl).toHaveBeenCalledTimes(1));
    vi.mocked(openUrl).mockRejectedValue(new Error("https://x.test/secret-token"));
    activateTerminalLink("https://x.test/secret-token", click({ metaKey: true }), "t1", true);
    await vi.waitFor(() => expect(logWarn).toHaveBeenCalledTimes(1));
    expect(records()).toHaveLength(4);
    const text = JSON.stringify(records());
    expect(text).not.toContain("secret-token");
    expect(text).not.toContain("x.test");
  });

  it("TRM-FR-HWKO: a new link object for the same cell, after the library clears its cache, also opens", async () => {
    const detector = terminalWith(libraryLink(URL_TEXT));
    await detector.getLinkAt(0, 0);
    detector.links.set("0,0", libraryLink("https://example.com/new"));
    (await detector.getLinkAt(0, 0))!.activate(click({ metaKey: true }));
    await vi.waitFor(() => expect(openUrl).toHaveBeenCalledWith("https://example.com/new"));
    expect(windowOpen).not.toHaveBeenCalled();
  });

  it("TRM-FR-HWKO: a failed read of the library's detector goes to the library unchanged", async () => {
    const linkDetector = { getLinkAt: () => Promise.reject(new Error("no line")) };
    expect(openLinksInBrowser({ linkDetector }, "t1")).toBe(true);
    await expect(linkDetector.getLinkAt()).rejects.toThrow("no line");
  });

  it("TRM-FR-LNOZ: a terminal with no link detector changes nothing and gives one WARN where asked", () => {
    expect(openLinksInBrowser({}, "t1")).toBe(false);
    expect(openLinksInBrowser({ linkDetector: {} }, "t2", false)).toBe(false);
    expect(openLinksInBrowser({ linkDetector: { getLinkAt: "x" } }, "t3")).toBe(false);
    expect(vi.mocked(logWarn).mock.calls).toEqual([
      [["frontend"], "terminal links not applied", { terminalId: "t1", reason: "no_link_hook" }],
      [["frontend"], "terminal links not applied", { terminalId: "t3", reason: "no_link_hook" }],
    ]);
  });
});

describe("the hover of terminal links", () => {
  const tooltipSpy = () => ({ show: vi.fn(), hide: vi.fn() });

  function hovered(link: FakeLink, tooltip?: ReturnType<typeof tooltipSpy>) {
    const linkDetector = new FakeLinkDetector();
    linkDetector.links.set("0,0", link);
    expect(openLinksInBrowser({ linkDetector }, "t1", true, tooltip)).toBe(true);
    return linkDetector.getLinkAt(0, 0);
  }

  it("TRM-FR-KIRO: a web link calls its own hover, then shows the tooltip of its address; leave hides it", async () => {
    const order: string[] = [];
    const tooltip = { show: vi.fn(() => order.push("show")), hide: vi.fn(() => order.push("hide")) };
    const own = vi.fn((on: boolean) => order.push(`own ${on}`));
    const link = (await hovered({ ...libraryLink(URL_TEXT), hover: own }, tooltip))!;
    link.hover!(true);
    expect(tooltip.show).toHaveBeenCalledWith(URL_TEXT);
    link.hover!(false);
    expect(tooltip.hide).toHaveBeenCalledTimes(1);
    expect(order).toEqual(["own true", "show", "own false", "hide"]);
  });

  it("TRM-FR-KIRO: an http link shows the tooltip too", async () => {
    const tooltip = tooltipSpy();
    const link = (await hovered(libraryLink("http://localhost:1420/x"), tooltip))!;
    link.hover!(true);
    expect(tooltip.show).toHaveBeenCalledWith("http://localhost:1420/x");
  });

  it("TRM-FR-KIRO: a link with another scheme keeps its hover and shows no tooltip", async () => {
    for (const url of ["file:///etc/hosts", "mailto:a@b.test", "ssh://host"]) {
      const tooltip = tooltipSpy();
      const own = vi.fn();
      const link = (await hovered({ ...libraryLink(url), hover: own }, tooltip))!;
      expect(link.hover).toBe(own);
      link.hover!(true);
      expect(tooltip.show).not.toHaveBeenCalled();
    }
  });

  it("TRM-FR-KIRO: with no tooltip the hover of a link does not change, and a link changes only once", async () => {
    const link = (await hovered(libraryLink(URL_TEXT)))!;
    expect(link.hover).toBeUndefined();
    const tooltip = tooltipSpy();
    const linkDetector = new FakeLinkDetector();
    linkDetector.links.set("0,0", libraryLink(URL_TEXT));
    openLinksInBrowser({ linkDetector }, "t1", true, tooltip);
    const first = (await linkDetector.getLinkAt(0, 0))!;
    const hover = first.hover;
    const again = (await linkDetector.getLinkAt(0, 0))!;
    expect(again).toBe(first);
    expect(again.hover).toBe(hover);
    again.hover!(true);
    expect(tooltip.show).toHaveBeenCalledTimes(1);
  });
});
