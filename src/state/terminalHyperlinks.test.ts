// The addresses of OSC 8 hyperlinks in terminal output (`TRM-terminals.md` TRM-FR-QMVZ, TRM-FR-LNOZ).
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  HYPERLINK_BODY_LIMIT,
  HYPERLINK_STORE_LIMIT,
  HYPERLINK_TEXT_LIMIT,
  hyperlinkKey,
  newHyperlinkTracker,
  newOsc8LinkProvider,
  registerOsc8Links,
  type HyperlinkTerminal,
} from "./terminalHyperlinks";
import { FakeCellBuffer, type FakeLink } from "../test/ghosttyMock";
import { logWarn } from "../logging";

vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

const PR = "https://github.com/acme/app/pull/7";
const bytes = (text: string) => new TextEncoder().encode(text);
const BEL_LINK = (uri: string, label: string, params = "") => `\x1b]8;${params};${uri}\x07${label}\x1b]8;;\x07`;
const ST_LINK = (uri: string, label: string, params = "") => `\x1b]8;${params};${uri}\x1b\\${label}\x1b]8;;\x1b\\`;

function scanned(...frames: string[]) {
  const tracker = newHyperlinkTracker();
  for (const frame of frames) tracker.scan(bytes(frame));
  return tracker;
}

describe("the hyperlink tracker", () => {
  it("TRM-FR-QMVZ: keeps the address of a hyperlink with BEL and with ST terminators", () => {
    expect(scanned(BEL_LINK(PR, "PR #7")).addressOf("PR #7")).toBe(PR);
    expect(scanned(ST_LINK(PR, "PR #7")).addressOf("PR #7")).toBe(PR);
  });

  it("TRM-FR-QMVZ: reads a hyperlink split at every byte over frames", () => {
    const text = `before ${ST_LINK(PR, "PR #7", "id=42")} after`;
    const tracker = newHyperlinkTracker();
    for (const byte of bytes(text)) tracker.scan(new Uint8Array([byte]));
    expect(tracker.addressOf("PR #7")).toBe(PR);
    expect(tracker.size).toBe(1);
  });

  it("TRM-FR-QMVZ: params before the address are not part of it", () => {
    expect(scanned(BEL_LINK(PR, "PR #7", "id=abc:x=y")).addressOf("PR #7")).toBe(PR);
  });

  it("TRM-FR-QMVZ: escapes inside the text are not part of it, and whitespace is ignored", () => {
    const label = "\x1b[1;34mPR\x1b[0m \x1b[2C#7\x1b]0;title\x07\r\n";
    const tracker = scanned(BEL_LINK(PR, label));
    expect(tracker.addressOf("PR #7")).toBe(PR);
    expect(tracker.addressOf("PR#7")).toBe(PR);
    expect(tracker.addressOf("  PR\t#7 ")).toBe(PR);
    expect(tracker.addressOf("PR #8")).toBeUndefined();
  });

  it("TRM-FR-QMVZ: a DCS string and an ESC with an intermediate inside the text are skipped", () => {
    const tracker = scanned(BEL_LINK(PR, "PR\x1bPq#0;1\x1b\\ \x1b(B#7"));
    expect(tracker.addressOf("PR #7")).toBe(PR);
  });

  it("TRM-FR-QMVZ: UTF-8 text split over frames gives the same key", () => {
    const full = bytes(BEL_LINK(PR, "Überprüfung ✓"));
    const tracker = newHyperlinkTracker();
    const cut = full.indexOf(0xc3) + 1;
    tracker.scan(full.slice(0, cut));
    tracker.scan(full.slice(cut));
    expect(tracker.addressOf("Überprüfung ✓")).toBe(PR);
  });

  it("TRM-FR-QMVZ: the latest hyperlink with the same text wins", () => {
    const tracker = scanned(BEL_LINK("https://a.test/1", "PR #7"), BEL_LINK("https://b.test/2", "PR #7"));
    expect(tracker.addressOf("PR #7")).toBe("https://b.test/2");
    expect(tracker.size).toBe(1);
  });

  it("TRM-FR-QMVZ: a new OSC 8 while one is open closes the first one", () => {
    const tracker = scanned("\x1b]8;;https://a.test/\x07one\x1b]8;;https://b.test/\x07two\x1b]8;;\x07");
    expect(tracker.addressOf("one")).toBe("https://a.test/");
    expect(tracker.addressOf("two")).toBe("https://b.test/");
  });

  it("TRM-FR-QMVZ: keeps the latest 256 hyperlinks and drops the oldest", () => {
    const tracker = newHyperlinkTracker();
    for (let i = 0; i <= HYPERLINK_STORE_LIMIT; i++) tracker.scan(bytes(BEL_LINK(`https://x.test/${i}`, `L${i}`)));
    expect(tracker.size).toBe(HYPERLINK_STORE_LIMIT);
    expect(tracker.addressOf("L0")).toBeUndefined();
    expect(tracker.addressOf("L1")).toBe("https://x.test/1");
    expect(tracker.addressOf(`L${HYPERLINK_STORE_LIMIT}`)).toBe(`https://x.test/${HYPERLINK_STORE_LIMIT}`);
  });

  it("TRM-FR-QMVZ: a reused text moves to the newest end, so it is not dropped first", () => {
    const tracker = newHyperlinkTracker(2);
    tracker.scan(bytes(BEL_LINK("https://x.test/a", "A") + BEL_LINK("https://x.test/b", "B") + BEL_LINK("https://x.test/a2", "A")));
    tracker.scan(bytes(BEL_LINK("https://x.test/c", "C")));
    expect(tracker.addressOf("B")).toBeUndefined();
    expect(tracker.addressOf("A")).toBe("https://x.test/a2");
    expect(tracker.addressOf("C")).toBe("https://x.test/c");
  });

  it("TRM-FR-QMVZ: no hyperlink is kept for an empty text, an unclosed one, or an empty address", () => {
    expect(scanned(BEL_LINK(PR, "   ")).size).toBe(0);
    expect(scanned(`\x1b]8;;${PR}\x07PR #7`).size).toBe(0);
    expect(scanned("\x1b]8;;\x07PR #7\x1b]8;;\x07").size).toBe(0);
    expect(scanned(`\x1b]8;${PR}\x07PR #7\x1b]8;;\x07`).size).toBe(0);
  });

  it("TRM-FR-QMVZ: other OSC sequences, and OSC 80, are not hyperlinks", () => {
    const tracker = scanned(`\x1b]0;${PR}\x07title\x1b]8;;\x07`, `\x1b]80;;${PR}\x07x\x1b]8;;\x07`);
    expect(tracker.size).toBe(0);
  });

  it("TRM-FR-QMVZ: CAN and SUB abort an OSC 8", () => {
    expect(scanned(`\x1b]8;;${PR}\x18PR #7\x1b]8;;\x07`).size).toBe(0);
    expect(scanned(`\x1b]8;;${PR}\x1aPR #7\x1b]8;;\x07`).size).toBe(0);
  });

  it("TRM-FR-QMVZ: as in the terminal's parser, the ESC of another sequence ends an OSC 8", () => {
    const tracker = scanned(`\x1b]8;;${PR}\x1b[31mPR #7\x1b]8;;\x07`);
    expect(tracker.addressOf("PR #7")).toBe(PR);
    expect(tracker.addressOf("[31mPR #7")).toBeUndefined();
  });

  it("TRM-FR-QMVZ: a URL that holds a semicolon is kept whole", () => {
    const uri = "https://x.test/a;b?c=d;e";
    expect(scanned(BEL_LINK(uri, "semi")).addressOf("semi")).toBe(uri);
  });

  it("TRM-FR-QMVZ: an OSC 8 inside a DCS or an APC string is not a hyperlink", () => {
    expect(scanned(`\x1bP${BEL_LINK(PR, "PR #7")}\x1b\\`).size).toBe(0);
    expect(scanned(`\x1b_${BEL_LINK(PR, "PR #7")}\x1b\\`).size).toBe(0);
  });

  it("TRM-FR-QMVZ: invalid UTF-8 in the text does not throw and keeps the link", () => {
    const tracker = newHyperlinkTracker();
    tracker.scan(new Uint8Array([...bytes(`\x1b]8;;${PR}\x07PR`), 0xff, 0xfe, ...bytes("\x1b]8;;\x07")]));
    expect(tracker.size).toBe(1);
  });

  it("TRM-FR-QMVZ: the OSC body and text limits hold at their exact sizes", () => {
    const uri = `https://x.test/${"a".repeat(HYPERLINK_BODY_LIMIT - 3 - "https://x.test/".length)}`;
    expect(scanned(BEL_LINK(uri, "edge")).addressOf("edge")).toBe(uri);
    const text = "x".repeat(HYPERLINK_TEXT_LIMIT);
    expect(scanned(BEL_LINK(PR, text)).addressOf(text)).toBe(PR);
  });

  it("TRM-FR-QMVZ: an OSC body or a text over its limit is not kept, and the next one is", () => {
    const longUri = `https://x.test/${"a".repeat(HYPERLINK_BODY_LIMIT)}`;
    expect(scanned(BEL_LINK(longUri, "long")).size).toBe(0);
    expect(scanned(BEL_LINK(PR, "x".repeat(HYPERLINK_TEXT_LIMIT + 1))).size).toBe(0);
    expect(scanned(BEL_LINK(PR, "x".repeat(HYPERLINK_TEXT_LIMIT + 1)), BEL_LINK(PR, "PR #7")).addressOf("PR #7")).toBe(PR);
  });

  it("TRM-FR-QMVZ: an OSC 8 over the body limit ends the open hyperlink", () => {
    const longUri = `https://x.test/${"a".repeat(HYPERLINK_BODY_LIMIT)}`;
    const tracker = scanned(`\x1b]8;;${PR}\x07PR #7\x1b]8;;${longUri}\x07other\x1b]8;;\x07`);
    expect(tracker.addressOf("PR #7")).toBe(PR);
    expect(tracker.addressOf("other")).toBeUndefined();
    expect(tracker.addressOf("PR #7other")).toBeUndefined();
  });

  it("TRM-FR-QMVZ: a C0 control after ESC keeps the escape, so its final byte is not text", () => {
    const tracker = scanned(BEL_LINK(PR, "PR\x1b\r7 #7"));
    expect(tracker.addressOf("PR #7")).toBe(PR);
  });

  it("TRM-FR-QMVZ: the key drops whitespace and control characters", () => {
    expect(hyperlinkKey(" P\tR\u0085 #\u00077\n")).toBe("PR#7");
    expect(hyperlinkKey(" \t\n")).toBe("");
  });

  it("TRM-FR-QMVZ: a lookup with an empty text finds nothing", () => {
    expect(scanned(BEL_LINK(PR, "PR #7")).addressOf("   ")).toBeUndefined();
  });
});

function terminal(buffer = new FakeCellBuffer()): HyperlinkTerminal & { buffer: { active: FakeCellBuffer } } {
  return { buffer: { active: buffer } };
}

function provided(term: HyperlinkTerminal, tracker: ReturnType<typeof newHyperlinkTracker>, y: number, activate = vi.fn()) {
  const provider = newOsc8LinkProvider(term, tracker, activate);
  return new Promise<FakeLink[] | undefined>((resolve) => provider.provideLinks(y, resolve as never));
}

describe("the OSC 8 link provider", () => {
  it("TRM-FR-QMVZ: a run of cells with one hyperlink id gets the address of its text", async () => {
    const term = terminal();
    term.buffer.active.setRow(0, "see PR #7 now", 20, [{ from: 4, to: 8, link: 3 }]);
    const links = await provided(term, scanned(BEL_LINK(PR, "PR #7")), 0);
    expect(links).toHaveLength(1);
    expect(links![0].text).toBe(PR);
    expect(links![0].range).toEqual({ start: { x: 4, y: 0 }, end: { x: 8, y: 0 } });
  });

  it("TRM-FR-QMVZ, TRM-FR-HWKO: the link activation gets the address and the event", async () => {
    const term = terminal();
    term.buffer.active.setRow(0, "PR #7", 10, [{ from: 0, to: 4, link: 1 }]);
    const activate = vi.fn();
    const [link] = (await provided(term, scanned(BEL_LINK(PR, "PR #7")), 0, activate))!;
    const event = new MouseEvent("click", { metaKey: true });
    link.activate(event);
    expect(activate).toHaveBeenCalledWith(PR, event);
  });

  it("TRM-FR-QMVZ: two links on one row get their own addresses", async () => {
    const term = terminal();
    term.buffer.active.setRow(0, "PR #7 and PR #8", 20, [
      { from: 0, to: 4, link: 1 },
      { from: 10, to: 14, link: 2 },
    ]);
    const tracker = scanned(BEL_LINK(PR, "PR #7"), BEL_LINK("https://github.com/acme/app/pull/8", "PR #8"));
    const links = await provided(term, tracker, 0);
    expect(links?.map((l) => l.text)).toEqual([PR, "https://github.com/acme/app/pull/8"]);
  });

  it("TRM-FR-QMVZ: a run that goes on to the next row is one link with both rows", async () => {
    const term = terminal();
    term.buffer.active.setRow(0, "xxxxxPR #", 9, [{ from: 5, to: 8, link: 4 }]);
    term.buffer.active.setRow(1, "7 yyyy", 9, [{ from: 0, to: 0, link: 4 }]);
    const tracker = scanned(BEL_LINK(PR, "PR #7"));
    const top = await provided(term, tracker, 0);
    const bottom = await provided(term, tracker, 1);
    const range = { start: { x: 5, y: 0 }, end: { x: 0, y: 1 } };
    expect(top?.[0]).toMatchObject({ text: PR, range });
    expect(bottom?.[0]).toMatchObject({ text: PR, range });
  });

  it("TRM-FR-QMVZ: a run with no kept text, and a row with no hyperlink, give no link", async () => {
    const term = terminal();
    term.buffer.active.setRow(0, "PR #9", 10, [{ from: 0, to: 4, link: 1 }]);
    term.buffer.active.setRow(1, "PR #7", 10);
    const tracker = scanned(BEL_LINK(PR, "PR #7"));
    expect(await provided(term, tracker, 0)).toBeUndefined();
    expect(await provided(term, tracker, 1)).toBeUndefined();
    expect(await provided(term, tracker, 5)).toBeUndefined();
    expect(await provided(term, newHyperlinkTracker(), 0)).toBeUndefined();
  });

  it("TRM-FR-QMVZ: a link text with a combining mark does not match, as each cell holds one code point", async () => {
    const term = terminal();
    term.buffer.active.setRow(0, "e\u0301", 4, [{ from: 0, to: 0, link: 1 }]);
    expect(await provided(term, scanned(BEL_LINK(PR, "e\u0301")), 0)).toBeUndefined();
  });

  it("TRM-FR-QMVZ: a run stops where the next row starts with another id, and at the last row", async () => {
    const term = terminal();
    term.buffer.active.setRow(0, "abPR", 4, [{ from: 2, to: 3, link: 1 }]);
    term.buffer.active.setRow(1, "#7xx", 4, [{ from: 0, to: 1, link: 2 }]);
    const links = await provided(term, scanned(BEL_LINK(PR, "PR"), BEL_LINK("https://x.test/7", "#7")), 0);
    expect(links?.[0]).toMatchObject({ text: PR, range: { start: { x: 2, y: 0 }, end: { x: 3, y: 0 } } });
    const last = await provided(term, scanned(BEL_LINK("https://x.test/7", "#7")), 1);
    expect(last?.[0].range).toEqual({ start: { x: 0, y: 1 }, end: { x: 1, y: 1 } });
  });

  it("TRM-FR-QMVZ: a wide character and its spacer cell match the text", async () => {
    const term = terminal();
    term.buffer.active.setRow(0, "修\u0000正", 4, [{ from: 0, to: 2, link: 1 }]);
    const links = await provided(term, scanned(BEL_LINK(PR, "修正")), 0);
    expect(links?.[0].text).toBe(PR);
  });
});

describe("the registration of the OSC 8 link provider", () => {
  beforeEach(() => vi.mocked(logWarn).mockClear());

  it("TRM-FR-QMVZ: registers one provider on the terminal", () => {
    const registered: unknown[] = [];
    const term = { ...terminal(), registerLinkProvider: (p: unknown) => registered.push(p) };
    expect(registerOsc8Links(term, newHyperlinkTracker(), vi.fn(), "t1")).toBe(true);
    expect(registered).toHaveLength(1);
    expect(logWarn).not.toHaveBeenCalled();
  });

  it("TRM-FR-LNOZ: a library with no way to register gives a WARN, where asked, with no address", () => {
    expect(registerOsc8Links(terminal(), newHyperlinkTracker(), vi.fn(), "t1")).toBe(false);
    const throwing = {
      ...terminal(),
      registerLinkProvider: () => {
        throw new Error("Terminal must be opened");
      },
    };
    expect(registerOsc8Links(throwing, newHyperlinkTracker(), vi.fn(), "t2", false)).toBe(false);
    expect(vi.mocked(logWarn).mock.calls).toEqual([[["frontend"], "terminal links not applied", { terminalId: "t1", reason: "no_link_provider" }]]);
  });
});
