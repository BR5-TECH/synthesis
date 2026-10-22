import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { DiffView } from "./DiffView";
import { TabScrollScope } from "../hooks/useRememberedScroll";
import { resetLogBufferForTest } from "../logging";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetDiffModes, setVisualizationMode } from "../state/diffModes";
import { EditSessionStore } from "../state/editSessions";
import {
  pruneTabScroll,
  resetTabScrollForTest,
  tabScrollOf,
} from "../state/tabScroll";
import {
  awaitDiff,
  makeWireBackend,
  revs,
  target,
} from "../test/diffViewFixtures";
import {
  installFakeResizeObserver,
  installFakeScroll,
  limitScroll,
  resizeAll,
} from "../test/fakeScroll";
import type { DiffVisualizationMode } from "../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const wireBackend = makeWireBackend(invokeMock);

let sessions: EditSessionStore;
let restoreScroll: () => void;
let restoreObserver: () => void;

beforeEach(() => {
  invokeMock.mockReset();
  resetDiffModes();
  resetAppPreferencesCache();
  resetTabScrollForTest();
  resetLogBufferForTest();
  restoreScroll = installFakeScroll();
  restoreObserver = installFakeResizeObserver();
  sessions = new EditSessionStore();
});

afterEach(() => {
  cleanup();
  restoreObserver();
  restoreScroll();
  resetLogBufferForTest();
});

/** The Diff tab `tabId`, as the shell renders it. */
async function mount(tabId = "diff-a") {
  const view = render(
    <TabScrollScope tabId={tabId}>
      <DiffView target={target()} sessions={sessions} />
    </TabScrollScope>,
  );
  await awaitDiff();
  return view;
}

/** Mount, and wait until the comparison's scroller for `mode` is there. */
async function mountIn(mode: DiffVisualizationMode, tabId = "diff-a") {
  const view = await mount(tabId);
  await waitFor(() => expect(scroller(mode)).not.toBeNull());
  return view;
}

function scroll(el: HTMLElement, top: number) {
  el.scrollTop = top;
  fireEvent.scroll(el);
}

/** The element that scrolls the comparison in the given visualization. */
function scroller(mode: DiffVisualizationMode): HTMLElement {
  const body = document.querySelector(".diff-view__target .diff-view__body") as HTMLElement;
  if (mode === "side_by_side") {
    return body.querySelector(".diff-sbs__scroll") as HTMLElement;
  }
  return body;
}

describe("DiffView scroll memory", () => {
  it.each(["unified", "side_by_side"] as const)(
    "DFV-FR-YMJA, TAB-FR-09: the comparison (%s) comes back where the author left it after a tab switch",
    async (mode) => {
      wireBackend({ prefs: { diffVisualizationMode: mode } });
      const first = await mountIn(mode);
      scroll(scroller(mode), 250);
      expect(tabScrollOf("diff-a", "comparison")).toBe(250);
      first.unmount();

      await mountIn(mode);
      expect(scroller(mode).scrollTop).toBe(250);
    },
  );

  it("DFV-FR-YMJA: a Diff tab whose offsets the shell dropped on close starts at the top", async () => {
    wireBackend();
    const first = await mount("diff-a");
    scroll(scroller("unified"), 250);
    first.unmount();
    // The shell drops the offsets of a tab that left the strip.
    pruneTabScroll(new Set());

    await mount("diff-a");
    expect(scroller("unified").scrollTop).toBe(0);
  });

  it("DFV-FR-YMJA, DFV-FR-52: a mode changed while the tab is in the background keeps the tab's offset", async () => {
    wireBackend();
    const first = await mount();
    scroll(scroller("unified"), 250);
    first.unmount();

    // Another Diff tab changes the application's mode.
    act(() => setVisualizationMode("side_by_side"));

    await mountIn("side_by_side");
    expect(scroller("side_by_side").scrollTop).toBe(250);
  });

  it("DFV-FR-YMJA: the comparison of a deleted file comes back where the author left it", async () => {
    wireBackend({ revisions: revs("one\ntwo\nthree\n", null) });
    const comparison = () =>
      document.querySelector(".diff-view > .diff-view__body") as HTMLElement;
    const first = await mount();
    scroll(comparison(), 180);
    first.unmount();

    await mount();
    expect(comparison().scrollTop).toBe(180);
  });

  it("DFV-FR-52, DFV-FR-YMJA: a switch to side-by-side in the open tab keeps the offset on the new scroller", async () => {
    wireBackend();
    await mountIn("unified");
    scroll(scroller("unified"), 250);

    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));
    await waitFor(() => expect(scroller("side_by_side")).not.toBeNull());

    expect(scroller("side_by_side").scrollTop).toBe(250);
  });

  it.each([
    ["unified", ".diff-view__body"],
    ["side_by_side", ".diff-sbs__scroll"],
  ] as const)(
    "DFV-FR-YMJA: a comparison (%s) not yet tall enough gets the offset when it grows",
    async (mode, selector) => {
      wireBackend({ prefs: { diffVisualizationMode: mode } });
      const first = await mountIn(mode);
      scroll(scroller(mode), 250);
      first.unmount();

      limitScroll(selector, 40);
      await mountIn(mode);
      expect(scroller(mode).scrollTop).toBe(40);

      limitScroll(selector, null);
      act(() => resizeAll());
      expect(scroller(mode).scrollTop).toBe(250);
    },
  );

  it("DFV-FR-YMJA: the author scrolling first ends a restore that waits", async () => {
    wireBackend();
    const first = await mountIn("unified");
    scroll(scroller("unified"), 250);
    first.unmount();

    limitScroll(".diff-view__body", 40);
    await mountIn("unified");
    fireEvent.wheel(scroller("unified"));
    limitScroll(".diff-view__body", null);
    act(() => resizeAll());

    expect(scroller("unified").scrollTop).toBe(40);
  });
});
