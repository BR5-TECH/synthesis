/**
 * The draft's document keeps its scroll position while its tab is open
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-IHDU).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => set.delete(cb);
    },
  ),
}));

import { NewArtifactWorkspace } from "./NewArtifactWorkspace";
import { TabScrollScope } from "../hooks/useRememberedScroll";
import { resetLogBufferForTest } from "../logging";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { DraftSessionStore } from "../state/draftSessions";
import { pruneTabScroll, resetTabScrollForTest } from "../state/tabScroll";
import { installFakeResizeObserver, installFakeScroll } from "../test/fakeScroll";
import {
  liveRow,
  makeStubs,
  showRail,
  threeVersions,
  versionRows,
} from "../test/newArtifactFixtures";

const { stub } = makeStubs(invokeMock);

let drafts: DraftSessionStore;
let restoreScroll: () => void;
let restoreObserver: () => void;

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetTabScrollForTest();
  resetLogBufferForTest();
  restoreScroll = installFakeScroll();
  restoreObserver = installFakeResizeObserver();
  drafts = new DraftSessionStore();
  stub({ history: threeVersions() });
});

afterEach(() => {
  cleanup();
  restoreObserver();
  restoreScroll();
  resetLogBufferForTest();
});

/** The New Artifact tab `tabId` over the draft `draftId`, as the shell renders it. */
async function mount(tabId: string, draftId = "d1") {
  const view = render(
    <TabScrollScope tabId={tabId}>
      <NewArtifactWorkspace
        draftId={draftId}
        drafts={drafts}
        name="artifact-window"
        onDraftChanged={vi.fn()}
        draftsRevision={0}
        onGraduationStarted={vi.fn()}
        onOpenRun={vi.fn()}
        onArchived={vi.fn()}
        onNameChanged={vi.fn()}
      />
    </TabScrollScope>,
  );
  await waitFor(() => expect(document.querySelector(".editor")).not.toBeNull());
  return view;
}

const documentScroller = () => document.querySelector(".editor") as HTMLElement;

function scroll(el: HTMLElement, top: number) {
  el.scrollTop = top;
  fireEvent.scroll(el);
}

describe("draft document scroll memory (NAW-FR-IHDU)", () => {
  it("NAW-FR-IHDU, TAB-FR-09: the draft's document comes back where the author left it after a tab switch", async () => {
    const first = await mount("draft-tab-1");
    scroll(documentScroller(), 640);
    first.unmount();

    await mount("draft-tab-1");
    expect(documentScroller().scrollTop).toBe(640);
  });

  it("NAW-FR-IHDU: a draft tab whose offsets the shell dropped on close starts at the top", async () => {
    const first = await mount("draft-tab-1");
    scroll(documentScroller(), 640);
    first.unmount();
    // The shell drops the offsets of a tab that left the strip.
    pruneTabScroll(new Set());

    await mount("draft-tab-1");
    expect(documentScroller().scrollTop).toBe(0);
  });

  it("NAW-FR-IHDU, NAW-FR-10: returning from a version shows the live prompt where the author left it", async () => {
    await mount("draft-tab-1");
    scroll(documentScroller(), 640);

    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");

    await userEvent.click(liveRow());
    await waitFor(() =>
      expect(screen.queryByTestId("draft-version-reading")).toBeNull(),
    );
    await waitFor(() => expect(document.querySelector(".editor")).not.toBeNull());
    expect(documentScroller().scrollTop).toBe(640);
  });
});
