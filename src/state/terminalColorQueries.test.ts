// The answers to the colour queries of a program (`TRM-terminals.md` TRM-FR-XBAP, TRM-FR-LNOZ).
import { beforeEach, describe, expect, it, vi } from "vitest";
import { newColorQueryReplier, xtermColor } from "./terminalColorQueries";
import { logDebug } from "../logging";

vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

const FG = "\x1b]10;rgb:1f1f/2121/2424";
const BG = "\x1b]11;rgb:fbfb/fafa/f7f7";
const ST = "\x1b\\";
const BEL = "\x07";

const bytes = (text: string) => new TextEncoder().encode(text);

function replier(colors: { foreground?: string; background?: string } = {}) {
  const sent: string[] = [];
  const r = newColorQueryReplier({
    terminalId: "t1",
    foreground: "foreground" in colors ? colors.foreground : "#1f2124",
    background: "background" in colors ? colors.background : "#fbfaf7",
    send: (data) => {
      sent.push(data);
      return true;
    },
  });
  return { r, sent };
}

/** A replier whose attach sent no history. */
function live(colors?: { foreground?: string; background?: string }) {
  const out = replier(colors);
  out.r.historyKnown(0);
  return out;
}

beforeEach(() => {
  vi.mocked(logDebug).mockClear();
});

describe("xtermColor", () => {
  it("TRM-FR-XBAP: each channel has four hex digits", () => {
    expect(xtermColor([251, 250, 247])).toBe("rgb:fbfb/fafa/f7f7");
    expect(xtermColor([0, 9, 255])).toBe("rgb:0000/0909/ffff");
  });
});

describe("the colour queries in live output", () => {
  it("TRM-FR-XBAP: OSC 11 gets the background and OSC 10 the foreground, with the terminator of the query", () => {
    const { r, sent } = live();
    r.scan(bytes(`\x1b]11;?${ST}`));
    r.scan(bytes(`\x1b]10;?${BEL}`));
    r.scan(bytes(`\x1b]11;?${BEL}`));
    expect(sent).toEqual([BG + ST, FG + BEL, BG + BEL]);
  });

  it("TRM-FR-XBAP: queries between other output, several in one frame, are all answered in order", () => {
    const { r, sent } = live();
    r.scan(bytes(`hello\x1b[31mred\x1b]10;?${ST}mid\x1b]11;?${ST}\x1b[6nend`));
    expect(sent).toEqual([FG + ST, BG + ST]);
  });

  it("TRM-FR-XBAP: a query split across frames at each byte is answered once", () => {
    const query = `\x1b]11;?${ST}`;
    for (let cut = 1; cut < query.length; cut++) {
      const { r, sent } = live();
      r.scan(bytes(`ab${query.slice(0, cut)}`));
      expect(sent, `cut ${cut}`).toEqual([]);
      r.scan(bytes(`${query.slice(cut)}cd`));
      expect(sent, `cut ${cut}`).toEqual([BG + ST]);
    }
  });

  it("TRM-FR-XBAP: other OSC sequences get no answer", () => {
    const { r, sent } = live();
    for (const text of [
      `\x1b]11;#ffffff${BEL}`, // a set, not a query
      `\x1b]11;rgb:ff/ff/ff${ST}`,
      `\x1b]4;12;?${BEL}`, // palette queries are out of scope
      `\x1b]12;?${BEL}`, // the cursor colour is out of scope
      `\x1b]111;?${BEL}`,
      `\x1b]11;?x${BEL}`,
      `\x1b]0;11;?${BEL}`, // a title
      `\x1b]8;;https://example.com/11;?${ST}link\x1b]8;;${ST}`,
      "]11;?\x07", // no ESC
      "\x1b[11;?\x07", // CSI, not OSC
      `\x1b]10;?;11;?${BEL}`,
      `\x1b]110;?${BEL}`,
    ])
      r.scan(bytes(text));
    expect(sent).toEqual([]);
  });

  it("TRM-FR-XBAP: an OSC cut by an ESC, CAN, or SUB is not a query, and the next query is still found", () => {
    const { r, sent } = live();
    r.scan(bytes(`\x1b]11;\x1b[0m?${BEL}`));
    r.scan(bytes(`\x1b]11;\x18?${BEL}`));
    r.scan(bytes(`\x1b]11;\x1a?${BEL}`));
    expect(sent).toEqual([]);
    r.scan(bytes(`\x1b]0;title\x1b]11;?${BEL}`));
    r.scan(bytes(`\x1b\x1b]11;?${BEL}`));
    r.scan(bytes(`\x1b]0;t\x1b\x1b]11;?${BEL}`));
    expect(sent).toEqual([BG + BEL, BG + BEL, BG + BEL]);
  });

  it("TRM-FR-XBAP: a long OSC does not hide the query after it", () => {
    const { r, sent } = live();
    r.scan(bytes(`\x1b]52;c;${"A".repeat(5000)}${BEL}\x1b]11;?${ST}`));
    expect(sent).toEqual([BG + ST]);
  });

  it("TRM-FR-XBAP: colours from CSS variables in rgb() form are answered", () => {
    const { r, sent } = live({ foreground: "rgb(31, 35, 40)", background: "rgb(255, 255, 255)" });
    r.scan(bytes(`\x1b]11;?${BEL}\x1b]10;?${BEL}`));
    expect(sent).toEqual([`\x1b]11;rgb:ffff/ffff/ffff${BEL}`, `\x1b]10;rgb:1f1f/2323/2828${BEL}`]);
  });

  it("TRM-FR-XBAP: a colour that cannot be read gets no answer, and the other one still does", () => {
    const { r, sent } = live({ foreground: undefined, background: "#fbfaf7" });
    r.scan(bytes(`\x1b]10;?${BEL}\x1b]11;?${BEL}`));
    expect(sent).toEqual([BG + BEL]);
  });

  it("TRM-FR-LNOZ: each answer emits one DEBUG record with the kind and no colour or output", () => {
    const { r } = live();
    r.scan(bytes(`\x1b]11;?${BEL}\x1b]10;?${BEL}secret`));
    expect(vi.mocked(logDebug).mock.calls).toEqual([
      [["frontend"], "terminal colour query answered", { terminalId: "t1", kind: "background" }],
      [["frontend"], "terminal colour query answered", { terminalId: "t1", kind: "foreground" }],
    ]);
  });
});

describe("the colour queries in the history", () => {
  it("TRM-FR-XBAP: a query in the history gets no answer, and one after it does", () => {
    const history = `old\x1b]11;?${BEL}`;
    const { r, sent } = replier();
    r.scan(bytes(history));
    r.scan(bytes(`\x1b]10;?${BEL}`));
    expect(sent).toEqual([]);
    r.historyKnown(bytes(history).length);
    expect(sent).toEqual([FG + BEL]);
    r.scan(bytes(`\x1b]11;?${BEL}`));
    expect(sent).toEqual([FG + BEL, BG + BEL]);
  });

  it("TRM-FR-XBAP: a query that ends exactly at the end of the history is history; one byte later is live", () => {
    const query = `\x1b]11;?${BEL}`;
    const a = replier();
    a.r.scan(bytes(query));
    a.r.historyKnown(query.length);
    expect(a.sent).toEqual([]);
    const b = replier();
    b.r.scan(bytes(`x${query}`));
    b.r.historyKnown(1);
    expect(b.sent).toEqual([BG + BEL]);
  });

  it("TRM-FR-XBAP: a query in history that arrives after the attach answer is still dropped", () => {
    const query = `\x1b]11;?${BEL}`;
    const { r, sent } = replier();
    r.historyKnown(query.length);
    r.scan(bytes(query));
    expect(sent).toEqual([]);
    r.scan(bytes(query));
    expect(sent).toEqual([BG + BEL]);
  });

  it("TRM-FR-XBAP: without a history size, all output read until then is history", () => {
    const { r, sent } = replier();
    r.scan(bytes(`\x1b]11;?${BEL}`));
    r.historyKnown(undefined);
    expect(sent).toEqual([]);
    r.scan(bytes(`\x1b]10;?${BEL}`));
    expect(sent).toEqual([FG + BEL]);
  });

  it("TRM-FR-XBAP: a query that starts in the history and ends in live output is live", () => {
    const query = `\x1b]11;?${BEL}`;
    const { r, sent } = replier();
    r.scan(bytes(query.slice(0, 3)));
    r.historyKnown(3);
    r.scan(bytes(query.slice(3)));
    expect(sent).toEqual([BG + BEL]);
  });

  it("TRM-FR-XBAP, TRM-FR-LNOZ: an answer that the session does not take logs no DEBUG record", () => {
    const r = newColorQueryReplier({ terminalId: "t1", foreground: "#000", background: "#fff", send: () => false });
    r.historyKnown(0);
    r.scan(bytes(`\x1b]11;?${BEL}`));
    expect(logDebug).not.toHaveBeenCalled();
  });

  it("TRM-FR-XBAP: nothing is sent before the history size is known", () => {
    const { r, sent } = replier();
    r.scan(bytes(`\x1b]11;?${BEL}`));
    expect(sent).toEqual([]);
    expect(logDebug).not.toHaveBeenCalled();
  });
});
