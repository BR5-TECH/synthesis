import { describe, expect, it } from "vitest";

import { moveTabInStrip } from "./tabStripActions";
import type { Tab } from "../../types";

const tab = (id: string): Tab => ({ id, label: id, kind: "editor" });
const DASHBOARD: Tab = { id: "dashboard", label: "Dashboard" };
const ids = (tabs: Tab[]) => tabs.map((t) => t.id);

describe("moveTabInStrip", () => {
  const strip = [DASHBOARD, tab("a"), tab("b"), tab("c")];

  it("TAB-FR-ERUM: moves a tab forward, and the other tabs keep their order", () => {
    expect(ids(moveTabInStrip(strip, "a", 3))).toEqual(["dashboard", "b", "c", "a"]);
  });

  it("TAB-FR-ERUM: moves a tab back", () => {
    expect(ids(moveTabInStrip(strip, "c", 1))).toEqual(["dashboard", "c", "a", "b"]);
  });

  it("TAB-FR-ERUM: does not change the input array", () => {
    const before = ids(strip);
    moveTabInStrip(strip, "a", 3);
    expect(ids(strip)).toEqual(before);
  });

  it("TAB-FR-ZMZS: a move to the same index returns the same array", () => {
    expect(moveTabInStrip(strip, "b", 2)).toBe(strip);
  });

  it("TAB-FR-ZMZS: an unknown id returns the same array", () => {
    expect(moveTabInStrip(strip, "zzz", 1)).toBe(strip);
  });

  it("TAB-FR-XNID: the Dashboard does not move", () => {
    expect(moveTabInStrip(strip, "dashboard", 2)).toBe(strip);
  });

  it("TAB-FR-XNID: an index in front of the Dashboard puts the tab directly after it", () => {
    expect(ids(moveTabInStrip(strip, "c", 0))).toEqual(["dashboard", "c", "a", "b"]);
    expect(ids(moveTabInStrip(strip, "c", -5))).toEqual(["dashboard", "c", "a", "b"]);
  });

  it("TAB-FR-XNID: a tab directly after the Dashboard moved to index 0 stays where it is", () => {
    expect(moveTabInStrip(strip, "a", 0)).toBe(strip);
  });

  it("TAB-FR-ERUM: an index past the end puts the tab last", () => {
    expect(ids(moveTabInStrip(strip, "a", 99))).toEqual(["dashboard", "b", "c", "a"]);
  });

  it("TAB-FR-ERUM: with no Dashboard open, a tab can take index 0", () => {
    const noDashboard = [tab("a"), tab("b"), tab("c")];
    expect(ids(moveTabInStrip(noDashboard, "c", 0))).toEqual(["c", "a", "b"]);
  });

  it("TAB-FR-XNID: a NaN index moves nothing, so no tab goes before the Dashboard", () => {
    expect(moveTabInStrip(strip, "c", Number.NaN)).toBe(strip);
  });

  it("TAB-FR-XNID, TAB-FR-ERUM: infinite indexes reach the ends, and a fraction is truncated", () => {
    expect(ids(moveTabInStrip(strip, "c", -Infinity))).toEqual(["dashboard", "c", "a", "b"]);
    expect(ids(moveTabInStrip(strip, "a", Infinity))).toEqual(["dashboard", "b", "c", "a"]);
    expect(ids(moveTabInStrip(strip, "a", 2.7))).toEqual(["dashboard", "b", "a", "c"]);
  });

  it("TAB-FR-ERUM: a Terminal tab moves like any other", () => {
    const term: Tab = { id: "terminal:t1", label: "zsh", kind: "terminal", terminalId: "t1" };
    const mixed = [DASHBOARD, tab("a"), term];
    expect(ids(moveTabInStrip(mixed, term.id, 1))).toEqual(["dashboard", term.id, "a"]);
  });

  it("TAB-FR-ERUM: the moved tab is the same object", () => {
    expect(moveTabInStrip(strip, "a", 3)[3]).toBe(strip[1]);
  });

  it("TAB-FR-ERUM: a strip of one tab has nowhere to move it", () => {
    const one = [tab("a")];
    expect(moveTabInStrip(one, "a", 1)).toBe(one);
  });
});
