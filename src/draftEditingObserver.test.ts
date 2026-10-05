import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { DraftEditingObserver, IDLE_THRESHOLD_MS } from "./draftEditingObserver";

/** A clock the test moves by hand, so the five minutes cost nothing to reach. */
function harness() {
  let now = Date.parse("2026-03-14T12:00:00.000Z");
  const reported: Array<[string, string, string]> = [];
  const observer = new DraftEditingObserver({
    now: () => now,
    report: (draftId, startedAt, endedAt) => reported.push([draftId, startedAt, endedAt]),
  });
  return {
    observer,
    reported,
    at: (offsetMs: number) => new Date(now + offsetMs).toISOString(),
    advance: (ms: number) => {
      now += ms;
    },
    /** The clock as it stands, for an expectation naming an absolute instant. */
    stamp: () => new Date(now).toISOString(),
  };
}

const minutes = (n: number) => n * 60_000;

describe("the editing-activity observer (DFI-FR-XKRM)", () => {
  it("DFI-FR-XKRM: losing focus settles the interval at that moment", () => {
    const h = harness();
    h.observer.setFocused(true);
    h.observer.setVisibleDrafts(["d-1"]);
    const opened = h.stamp();
    h.observer.interacted();
    expect(h.observer.isOpen("d-1")).toBe(true);

    h.advance(minutes(2));
    h.observer.setFocused(false);
    expect(h.reported).toEqual([["d-1", opened, h.stamp()]]);
    expect(h.observer.isOpen("d-1")).toBe(false);
    h.observer.dispose();
  });

  it("DFI-FR-XKRM: an idle interval settles at the threshold, not at the last interaction", () => {
    const h = harness();
    h.observer.setVisibleDrafts(["d-1"]);
    const opened = h.stamp();
    h.observer.interacted();

    // Six minutes with focus and the surface both still present.
    h.advance(minutes(6));
    h.observer.sweep();

    // The five minutes before the threshold are counted and the sixth is not.
    expect(h.reported).toHaveLength(1);
    const [, startedAt, endedAt] = h.reported[0];
    expect(startedAt).toBe(opened);
    expect(Date.parse(endedAt) - Date.parse(startedAt)).toBe(IDLE_THRESHOLD_MS);
    // And no further interval opens until the author interacts again.
    expect(h.observer.isOpen("d-1")).toBe(false);
    h.observer.dispose();
  });

  it("DFI-FR-XKRM: a later interaction opens a second interval rather than extending the first", () => {
    const h = harness();
    h.observer.setVisibleDrafts(["d-1"]);
    h.observer.interacted();
    h.advance(minutes(6));
    h.observer.sweep();

    h.advance(minutes(14));
    const reopened = h.stamp();
    h.observer.interacted();
    h.advance(minutes(1));
    h.observer.setFocused(false);

    expect(h.reported).toHaveLength(2);
    expect(h.reported[1][1]).toBe(reopened);
    h.observer.dispose();
  });

  it("DFI-FR-XKRM: an interaction before the threshold settles nothing and restarts the five minutes", () => {
    const h = harness();
    h.observer.setVisibleDrafts(["d-1"]);
    const opened = h.stamp();
    h.observer.interacted();

    h.advance(minutes(4));
    h.observer.interacted();
    h.observer.sweep();
    expect(h.reported).toEqual([]);
    expect(h.observer.isOpen("d-1")).toBe(true);

    // Four minutes further on is still inside the restarted window.
    h.advance(minutes(4));
    h.observer.sweep();
    expect(h.reported).toEqual([]);

    h.advance(minutes(1));
    h.observer.sweep();
    expect(h.reported).toHaveLength(1);
    expect(h.reported[0][1]).toBe(opened);
    h.observer.dispose();
  });

  it("DFI-FR-JDWS, DFI-FR-XKRM: two surfaces of one draft hold one interval, two drafts hold one each", () => {
    const h = harness();
    // The caller has already reduced its surfaces to the drafts they are about,
    // so a draft's tab beside its conversation tab arrives as one id.
    h.observer.setVisibleDrafts(["d-1", "d-2"]);
    h.observer.interacted();
    expect(h.observer.isOpen("d-1")).toBe(true);
    expect(h.observer.isOpen("d-2")).toBe(true);

    h.advance(minutes(3));
    h.observer.setFocused(false);
    expect(h.reported.map(([id]) => id).sort()).toEqual(["d-1", "d-2"]);
    // One interval each rather than one per surface.
    expect(h.reported).toHaveLength(2);
    h.observer.dispose();
  });

  it("DFI-FR-YAOM, DFI-FR-XKRM: a draft with no related surface visible opens no interval", () => {
    const h = harness();
    h.observer.setVisibleDrafts([]);
    h.observer.interacted();
    expect(h.observer.isOpen("d-1")).toBe(false);
    expect(h.reported).toEqual([]);
    h.observer.dispose();
  });

  it("DFI-FR-YAOM, DFI-FR-XKRM: a surface leaving view settles that draft's interval and no other's", () => {
    const h = harness();
    h.observer.setVisibleDrafts(["d-1", "d-2"]);
    h.observer.interacted();
    h.advance(minutes(2));

    h.observer.setVisibleDrafts(["d-2"]);
    expect(h.reported.map(([id]) => id)).toEqual(["d-1"]);
    expect(h.observer.isOpen("d-2")).toBe(true);
    h.observer.dispose();
  });

  it("DFI-FR-UKFR, DFI-FR-GQTZ: an interval the observer never settled is reported not at all", () => {
    const h = harness();
    h.observer.setVisibleDrafts(["d-1"]);
    h.observer.interacted();
    h.advance(minutes(3));

    // The application closing mid-interval.
    h.observer.dispose();
    expect(h.reported).toEqual([]);
  });

  it("DFI-FR-UKFR: the whole of a settled interval is reported and no boundary is applied", () => {
    const h = harness();
    h.observer.setVisibleDrafts(["d-1"]);
    const opened = h.stamp();
    h.observer.interacted();
    h.advance(minutes(9));
    h.observer.setFocused(false);

    const [, startedAt, endedAt] = h.reported[0];
    expect(startedAt).toBe(opened);
    expect(Date.parse(endedAt) - Date.parse(startedAt)).toBe(minutes(9));
    h.observer.dispose();
  });

  it("DFI-FR-XKRM: an interaction while the application is unfocused opens nothing", () => {
    const h = harness();
    h.observer.setVisibleDrafts(["d-1"]);
    h.observer.setFocused(false);
    h.observer.interacted();
    expect(h.observer.isOpen("d-1")).toBe(false);
    h.observer.dispose();
  });
});
