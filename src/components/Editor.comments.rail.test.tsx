import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import {
  BODY,
  commentsToggle,
  human,
  makeThread,
  openRail,
  wireBackend,
  fragment,
} from "../test/editorCommentsFixtures";
import type { Backend } from "../test/editorCommentsFixtures";

/**
 * CMT-comments.md, driven through the real Editor.
 *
 * The rail's arithmetic — re-anchoring, tracking an edit, card stacking — lives
 * in `../state/commentAnchors` and is covered by its own unit tests. What is
 * exercised here is the wiring: which command each affordance invokes, what the
 * rail renders in each of its states, and the claims the Editor spec makes about
 * hosting it (EDT-FR-61).
 */
const invokeMock = vi.fn();
const unlistenMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The rail follows `"discussion changed"` (CMT-FR-04), so the mock keeps the
 * handlers rather than discarding them — a test emits on a channel exactly as
 * the backend would.
 */
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
function emitEvent(name: string, payload: unknown) {
  [...(listeners.get(name) ?? [])].forEach((h) => h({ payload }));
}
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => {
        set.delete(cb);
        unlistenMock();
      };
    },
  ),
}));

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
});

afterEach(() => {
  cleanup();
});

async function mount(over: Partial<Backend> = {}) {
  const sessions = new EditSessionStore();
  const backend: Backend = {
    load: { body: BODY, checksum: "ck1" },
    threads: [],
    calls: [],
    ...over,
  };
  // The anchor's `end` is derived so a hand-written fixture cannot drift.
  backend.threads = backend.threads.map((t) => ({
    ...t,
    fragmentTarget:
      t.fragmentTarget === null
        ? null
        : {
            ...t.fragmentTarget,
            end: t.fragmentTarget.start + t.fragmentTarget.quote.length,
          },
  }));
  wireBackend(invokeMock, backend);
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      artifactType="skill"
      sessions={sessions}
    />,
  );
  await screen.findByLabelText("artifact body");
  return { sessions, backend };
}

/**
 * Selecting text in a jsdom contenteditable does not give ProseMirror a real
 * selection, so these drive the affordance through the same DOM selection the
 * component reads (`window.getSelection().toString()`), which is the part under
 * test — the mapping from a selection to a source anchor and the command it
 * produces.
 */
function selectInBody(text: string) {
  // Deliberately NOT inside `.editor__prose`: mutating ProseMirror's own DOM
  // makes its observer read a DOM change and call `coordsAtPos`, which jsdom
  // cannot serve. `captureSelection` reads `window.getSelection().toString()`
  // and takes its position hint from the editor's own selection state, so a
  // node parked elsewhere in the document exercises exactly the same path.
  const host = document.createElement("div");
  host.textContent = text;
  document.body.appendChild(host);
  selectionHosts.push(host);
  const range = document.createRange();
  range.selectNodeContents(host);
  const sel = window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
  fireEvent.mouseUp(screen.getByLabelText("artifact body"));
}

const selectionHosts: HTMLElement[] = [];
afterEach(() => {
  window.getSelection()?.removeAllRanges();
  while (selectionHosts.length) selectionHosts.pop()!.remove();
});

describe("CMT-FR-01 / CMT-FR-27: comments sit in the page's own margin", () => {
  it("moves the cards with the body as it scrolls", async () => {
    // The cards are placed in the body's coordinates, so without this they
    // would sit still while the passages they belong to scrolled away.
    await mount({ threads: [makeThread()] });
    await openRail();

    const layer = document.querySelector(".comment-rail__aligned") as HTMLElement;
    expect(layer.style.transform).toBe("translateY(0px)");

    const surface = document.querySelector(".editor") as HTMLElement;
    fireEvent.scroll(surface, { target: { scrollTop: 240 } });
    expect(layer.style.transform).toBe("translateY(-240px)");
  });

  it("leaves the threads with no line to sit beside where they are", async () => {
    // CMT-FR-27: an orphaned thread belongs to the artifact rather than to a
    // passage of it, so scrolling past nothing in particular must not carry it
    // off the bottom of the margin.
    await mount({ threads: [makeThread({ fragmentTarget: fragment({ start: 0, end: 0, quote: "gone" }) })] });
    await openRail();
    await screen.findByText("Orphaned", { selector: ".comment-rail__section-title" });

    const surface = document.querySelector(".editor") as HTMLElement;
    fireEvent.scroll(surface, { target: { scrollTop: 240 } });
    const pinned = document.querySelector(".comment-rail__unaligned") as HTMLElement;
    expect(pinned.style.transform).toBe("");
  });

  it("passes each card's key directly rather than through a spread", async () => {
    // React 19 warns — via `console.error` — when a `key` arrives inside a
    // spread object, and a key that arrives that way is not read as a key at
    // all. Nothing in the suite watches `console.error`, so the defect was
    // silent both times: when it landed and when it would land again. The rail
    // is rendered with all three card kinds, because the three call sites are
    // the aligned stack, the orphaned section and the resolved disclosure.
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      await mount({
        threads: [
          makeThread({ id: "aligned" }),
          makeThread({ id: "orphan", fragmentTarget: fragment({ start: 0, end: 0, quote: "gone" }) }),
          makeThread({ id: "settled", resolved: true }),
        ],
      });
      await openRail();
      await screen.findByText("Orphaned", { selector: ".comment-rail__section-title" });

      // Matched on "key" generally, not on the spread message alone: removing
      // the key entirely also silences that specific warning while leaving the
      // list unkeyed, and a guard that green-lights the second defect while
      // catching the first is worse than none.
      const keyWarnings = spy.mock.calls.filter((c) =>
        /\bkey\b/i.test(String(c[0])),
      );
      expect(keyWarnings).toEqual([]);
    } finally {
      spy.mockRestore();
    }
  });

  it("renders the rail beside the editing surface rather than inside it", async () => {
    // Out of flow and a sibling of the page, which is what lets it occupy the
    // margin without taking a column from the text. The width arithmetic that
    // finishes the job lives in the stylesheet, which jsdom does not apply.
    await mount({ threads: [makeThread()] });
    const rail = await openRail();
    expect(rail.parentElement?.className).toContain("editor__with-rail");
    expect(rail.previousElementSibling?.className).toContain("editor");
  });

  it("tells the tab whether the margin is showing and where its cards are", async () => {
    // CMT-FR-64 / EDT-FR-63: every step of the layout — the page sliding toward
    // the leading edge, the rail widening into the field it gives up, the cards
    // going below the page — is a stylesheet rule keyed on these two attributes.
    // Nothing else the DOM carries would betray their absence, and without them
    // `--page-lead` never sets: the rail then sizes itself as if the page's
    // leading edge were at the tab's own, and the cards land over the prose.
    await mount({ threads: [makeThread()] });
    await openRail();
    const tab = document.querySelector(".editor-tab") as HTMLElement;
    // The attribute the *tab* carries, which is not the one the rail carries —
    // writing the rail's name here is exactly the mistake this would ship. In
    // jsdom nothing has a width, so the arrangement is the ordinary one.
    expect(tab.dataset.rail).toBe("open");
    expect(tab.dataset.comments).toBe("beside");

    // EDT-FR-63, EDT-FR-61, CMT-FR-64's last clause: with the comments hidden the page does not slide
    // at all, because there is nothing beside it asking for the width.
    fireEvent.click(commentsToggle());
    await waitFor(() => expect(tab.dataset.rail).toBe("closed"));

    // …and raw-text mode renders no card at all (CMT-FR-02), so the page is
    // centred there whatever the rail's own remembered state is.
    fireEvent.click(commentsToggle());
    await screen.findByRole("complementary", { name: "Comments" });
    expect(tab.dataset.rail).toBe("open");
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    expect(tab.dataset.rail).toBe("closed");
  });

  it("hides the rich page and the cards with it in raw-text mode", async () => {
    // Both pages live in the one region, because the rail is beside whichever of
    // them is showing (CMT-FR-02). The page that is not showing is `display:
    // none` rather than merely empty: it is a `flex: 1` sibling of the other, so
    // leaving it displayed would let it claim half the tab's height and push the
    // visible one into the bottom of it.
    await mount({ threads: [makeThread()] });
    await openRail();
    const region = screen.getByTestId("editor-with-rail");
    const richPage = region.querySelector<HTMLElement>(".editor")!;
    expect(richPage.style.display).toBe("");

    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    expect(richPage.style.display).toBe("none");
    // CMT-FR-02: a Markdown file toggled to raw text renders no card at all.
    expect(screen.queryByRole("complementary", { name: "Comments" })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByRole("complementary", { name: "Comments" });
    expect(richPage.style.display).toBe("");
  });
});

describe("CMT-FR-35: a card's controls are graphical", () => {
  it("labels Post, Cancel and Quote without spending the card's width on words", async () => {
    await mount({ threads: [makeThread()] });
    await openRail();

    // Seed a quote so the fourth control the requirement names exists too.
    fireEvent.click(screen.getByRole("button", { name: "Quote comment 1 by raver119" }));

    for (const name of [
      "Post",
      "Cancel",
      "Quote comment 1 by raver119",
      "Remove quote",
    ]) {
      const button = screen.getByRole("button", { name });
      expect(button.querySelector("svg")).not.toBeNull();
      expect(button.textContent).toBe("");
      // Still reachable by name, which is what keeps it operable at all.
      expect(button.getAttribute("aria-label")).toBe(name);
    }
  });

  it("applies the same treatment to the draft card's own pair", async () => {
    await mount({ threads: [] });
    await waitFor(() => expect(commentsToggle()).toBeInTheDocument());

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    const draft = within(await screen.findByTestId("comment-draft"));

    for (const name of ["Post", "Cancel"]) {
      const button = draft.getByRole("button", { name });
      expect(button.querySelector("svg")).not.toBeNull();
      expect(button.textContent).toBe("");
    }
  });

  it("offers the selection affordance as a glyph rather than a filled block", async () => {
    await mount({ threads: [] });
    await waitFor(() => expect(commentsToggle()).toBeInTheDocument());

    selectInBody("the first session");
    const affordance = await screen.findByRole("button", { name: "Comment on selection" });
    expect(affordance.querySelector("svg")).not.toBeNull();
    expect(affordance.textContent).toBe("");
  });
});

describe("CMT-FR-36 / CMT-FR-30: landing on a thread sent from the Comments panel", () => {
  /**
   * Mount the way `Viewport` does when the panel's click-through has named a
   * thread: the shell has already written the mode and the rail's open state
   * into the session, and hands the thread id down as `focusThreadId`.
   */
  async function mountFocused(
    threadId: string | null,
    over: Partial<Backend> = {},
    sessionPatch: Parameters<EditSessionStore["update"]>[1] = {},
  ) {
    const sessions = new EditSessionStore();
    const backend: Backend = { load: { body: BODY, checksum: "ck1" }, threads: [], calls: [], ...over };
    backend.threads = backend.threads.map((t) => ({
      ...t,
      fragmentTarget:
        t.fragmentTarget === null
          ? null
          : {
              ...t.fragmentTarget,
              end: t.fragmentTarget.start + t.fragmentTarget.quote.length,
            },
    }));
    wireBackend(invokeMock, backend);
    // What `useShellSession.openCommentThread` writes before the tab is focused.
    sessions.update("a.md", { mode: "wysiwyg", railOpen: true, ...sessionPatch });
    const onThreadFocused = vi.fn();
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
        focusThreadId={threadId}
        onThreadFocused={onThreadFocused}
      />,
    );
    await screen.findByLabelText("artifact body");
    return { sessions, backend, onThreadFocused };
  }

  const focusedCards = () =>
    Array.from(document.querySelectorAll('.comment-card[data-focused="true"]'));

  it("scrolls the thread's anchor into view on arrival", async () => {
    // CMT-FR-28 via CMT-FR-36: landing on a card brings its passage into view,
    // or the reviewer arrives at a highlighted range they cannot see.
    const scrolled: Element[] = [];
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    try {
      await mountFocused("t1", { threads: [makeThread({ id: "t1" })] });
      await waitFor(() =>
        expect(
          scrolled.some(
            (el) => el.getAttribute("data-comment-thread") === "t1",
          ),
        ).toBe(true),
      );
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it("lands on an orphaned thread's card without a passage to scroll to", async () => {
    // CMP-FR-13: the panel lists a thread the rail renders as orphaned, so the
    // click-through must survive there being no anchored range in the body.
    await mountFocused("t1", {
      threads: [
        makeThread({
          id: "t1",
          fragmentTarget: fragment({ start: 0, end: 0, quote: "a passage this artifact lost" }),
        }),
      ],
    });
    await waitFor(() => expect(focusedCards()).toHaveLength(1));
    expect(focusedCards()[0].getAttribute("data-testid")).toBe(
      "comment-thread-t1",
    );
  });

  it("renders the rail with that thread's card focused", async () => {
    const { onThreadFocused } = await mountFocused("t2", {
      threads: [
        makeThread({ id: "t1" }),
        makeThread({ id: "t2", fragmentTarget: fragment({ start: BODY.indexOf("Onboarding"), end: 0, quote: "Onboarding" }) }),
      ],
    });

    // The rail is showing without the author having toggled it.
    const rail = await screen.findByRole("complementary", { name: "Comments" });
    expect(rail).toBeInTheDocument();

    await waitFor(() => expect(focusedCards()).toHaveLength(1));
    expect(focusedCards()[0].getAttribute("data-testid")).toBe("comment-thread-t2");
    // Consumed once, so a later re-render does not yank the body back.
    expect(onThreadFocused).toHaveBeenCalledTimes(1);
  });

  it("focuses nothing when the tab was not reached from the panel", async () => {
    const { onThreadFocused } = await mountFocused(null, {
      threads: [makeThread({ id: "t1" })],
    });
    await screen.findByRole("complementary", { name: "Comments" });
    expect(focusedCards()).toHaveLength(0);
    expect(onThreadFocused).not.toHaveBeenCalled();
  });

  it("reaches a resolved thread's card through the expanded disclosure", async () => {
    await mountFocused(
      "t1",
      { threads: [makeThread({ id: "t1", resolved: true })] },
      // CMP-FR-11: the shell expands the disclosure for a resolved thread.
      { resolvedOpen: true },
    );
    await waitFor(() => expect(focusedCards()).toHaveLength(1));
    expect(focusedCards()[0].getAttribute("data-testid")).toBe("comment-thread-t1");
  });
});

describe("CMS-FR-51, CMS-FR-31 / CMT-FR-04: the rail redraws from the event, never a re-read", () => {
  function reads(backend: Backend) {
    return backend.calls.filter((c) => c.cmd === "list_discussions").length;
  }

  it("renders a comment appended elsewhere in the window, without asking again", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openRail();
    await waitFor(() => expect(reads(backend)).toBe(1));

    // A comment posted from a second Editor tab on the same artifact: the
    // backend announces the thread, and this rail draws it from the payload.
    act(() =>
      emitEvent("discussion-changed", {
        ...makeThread(),
        comments: [
          ...makeThread().comments,
          {
            id: "c-remote",
            author: human("octocat"),
            body: "posted from another tab",
            quotes: [],
            attachments: [],
            createdAt: "2026-01-02T00:00:00Z",
          },
        ],
      }),
    );

    expect(
      await screen.findByText("posted from another tab"),
    ).toBeInTheDocument();
    expect(reads(backend)).toBe(1);
  });

  it("re-renders a card as locked when the lock was set anywhere", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openRail();
    expect(screen.getByLabelText("Reply to thread t1")).toBeInTheDocument();

    act(() =>
      emitEvent("discussion-changed", { ...makeThread(), locked: true }),
    );

    await waitFor(() =>
      expect(screen.queryByLabelText("Reply to thread t1")).not.toBeInTheDocument(),
    );
    expect(reads(backend)).toBe(1);
  });

  it("ignores a thread belonging to a different artifact", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openRail();

    act(() =>
      emitEvent("discussion-changed", {
        ...makeThread({ id: "t-other" }),
        target: { kind: "artifact", artifactId: "somewhere/else.md" },
        comments: [
          {
            id: "c-other",
            author: human("octocat"),
            body: "another artifact entirely",
            quotes: [],
            attachments: [],
            createdAt: "2026-01-02T00:00:00Z",
          },
        ],
      }),
    );

    await waitFor(() =>
      expect(screen.queryByText("another artifact entirely")).not.toBeInTheDocument(),
    );
    expect(reads(backend)).toBe(1);
  });

  it("issues no operation at all while nothing is written", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openRail();
    const before = backend.calls.length;
    await new Promise((r) => setTimeout(r, 50));
    // The session log's own batched flush is not part of the feature and lands
    // on its own timer (per `../../specifications/core/LGC-logging.md`).
    expect(
      backend.calls
        .slice(before)
        .filter((c: { cmd: string }) => c.cmd !== "append_log_records"),
    ).toEqual([]);
  });
});
