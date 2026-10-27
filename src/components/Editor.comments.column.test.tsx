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
import { discussionColumnState, fileColumn } from "../state/discussionColumn";
import { consumePendingFocus } from "../state/discussionFocus";
import { resetLogBufferForTest } from "../logging";
import {
  BODY,
  discussionToggle,
  human,
  makeThread,
  openColumn,
  wireBackend,
  fragment,
} from "../test/editorCommentsFixtures";
import type { Backend } from "../test/editorCommentsFixtures";
import type { Discussion } from "../types";
import type { EditorProps } from "./Editor/props";

/**
 * CMT-comments.md and DDS-draft-discussion.md, driven through the real Editor.
 *
 * The Editor tab sets the file's discussions in the discussion column of the
 * draft tab (CMT-FR-01, DDS-FR-KTVW): one grid below the toolbar, with the
 * document column, the splitter and the column. What is exercised here is that
 * layout, the toolbar controls of the column, the routes into it (Discuss, the
 * Comment affordance, a highlight, the Comments panel), and the events it
 * redraws from.
 */
const invokeMock = vi.fn();
const unlistenMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The column follows `"discussion changed"` (CMT-FR-04), so the mock keeps the
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

const scrollIntoView = vi.fn();

beforeEach(() => {
  // The column logs when it is hidden or shown, and the flush of that log must
  // not land in the next test.
  resetLogBufferForTest();
  invokeMock.mockReset();
  unlistenMock.mockReset();
  scrollIntoView.mockReset();
  Element.prototype.scrollIntoView = scrollIntoView;
});

afterEach(() => {
  cleanup();
});

const RUST = 'fn main() {\n    // greet\n    let s = "x < y & z";\n}\n';

interface MountOptions extends Partial<Backend> {
  artifactId?: string;
  artifactType?: EditorProps["artifactType"];
  toc?: boolean;
  focusThreadId?: string | null;
  onThreadFocused?: () => void;
  sessionPatch?: Parameters<EditSessionStore["update"]>[1];
  /** The file cannot be read, as when it is gone from the project. */
  loadFails?: boolean;
}

async function mount(options: MountOptions = {}) {
  const {
    artifactId = "a.md",
    artifactType = "skill",
    toc,
    focusThreadId,
    onThreadFocused,
    sessionPatch,
    loadFails,
    ...over
  } = options;
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
  if (loadFails) {
    const wired = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: unknown) =>
      cmd === "load_artifact_contents_by_id"
        ? Promise.reject("artifact_not_found")
        : wired(cmd, args),
    );
  }
  if (sessionPatch) sessions.update(artifactId, sessionPatch);
  render(
    <Editor
      artifactId={artifactId}
      artifactName={artifactId}
      artifactType={artifactType}
      sessions={sessions}
      toc={toc}
      focusThreadId={focusThreadId}
      onThreadFocused={onThreadFocused}
    />,
  );
  if (!loadFails) {
    await screen.findByLabelText(artifactId.endsWith(".md") ? "artifact body" : "Source");
  }
  return { sessions, backend };
}

/** A fragment discussion over `quote` where it sits in the body. */
function threadOn(quote: string, id: string, body = BODY): Discussion {
  return makeThread({
    id,
    fragmentTarget: fragment({ start: body.indexOf(quote), end: 0, quote }),
  });
}

/** A whole-target discussion of the file. */
function wholeThread(id: string, artifactId = "a.md"): Discussion {
  return makeThread({
    id,
    target: { kind: "artifact", artifactId },
    fragmentTarget: null,
  });
}

/**
 * Selecting text in a jsdom contenteditable does not give ProseMirror a real
 * selection, so these drive the affordance through the same DOM selection the
 * component reads (`window.getSelection().toString()`).
 */
function selectInBody(text: string) {
  // Deliberately NOT inside `.editor__prose`: mutating ProseMirror's own DOM
  // makes its observer read a DOM change and call `coordsAtPos`, which jsdom
  // cannot serve.
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

const split = () => screen.getByTestId("editor-discussion-split");
const column = () => screen.getByTestId("editor-discussion-column");
const markOf = (id: string) =>
  document.querySelector<HTMLElement>(`.ProseMirror [data-comment-thread="${id}"]`);
const toRaw = async () => {
  fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
  await screen.findByLabelText("Markdown source");
};
const hideColumn = async () => {
  const held = await screen.findByTestId("editor-discussion-column");
  fireEvent.click(discussionToggle());
  await waitFor(() => expect(held.hidden).toBe(true));
};
const openDiscuss = () => {
  fireEvent.click(screen.getByRole("button", { name: "Actions" }));
  return screen.getByRole("menuitem", { name: "Discuss" });
};

/** ACT-FR-19 / ACT-FR-20: no floating surface of the action control stands. */
function expectNoFloatingDiscussion() {
  expect(document.querySelector(".action-control__discussions")).toBeNull();
  expect(document.querySelector(".action-control__panel")).toBeNull();
  expect(document.querySelector(".draft-composer")).toBeNull();
  expect(screen.queryByRole("button", { name: /^(Show|Hide) discussion\b/ })).toBeNull();
}

describe("CMT-FR-01, DDS-FR-KTVW: the grid below the toolbar", () => {
  /**
   * The document column, the splitter and the discussion column, as one grid
   * that follows the toolbar, with the column shown.
   */
  function expectGrid(page: HTMLElement) {
    const grid = split();
    expect(grid).toHaveClass("dds-split", "editor__split");
    expect(grid).toHaveAttribute("data-discussion", "shown");
    expect(grid.previousElementSibling).toHaveClass("editor__toolbar");
    const [documentColumn, splitter, discussion] = Array.from(grid.children);
    expect(documentColumn).toHaveClass("editor__column");
    expect(within(documentColumn as HTMLElement).getByTestId("editor-document")).toContainElement(page);
    expect(splitter).toHaveAttribute("role", "separator");
    expect(splitter).not.toHaveAttribute("hidden");
    expect(discussion).toBe(column());
    expect(discussion).toHaveAttribute("aria-label", "Discussion");
    expect(discussion).toBeVisible();
  }

  it("CMT-FR-01, CMT-FR-02, DDS-FR-KTVW, EDT-FR-61: in WYSIWYG, with the column shown and no step from the author", async () => {
    await mount({ threads: [threadOn("the first session", "t1")] });
    expectGrid(screen.getByLabelText("artifact body"));
    expect(await within(column()).findByTestId("comment-thread-t1")).toBeInTheDocument();
  });

  it("CMT-FR-01, CMT-FR-02, DDS-FR-KTVW, EDT-FR-61: in raw Markdown, with the rich page hidden in the same region", async () => {
    // Both pages live in the one region. The page that is not showing is
    // `display: none` rather than merely empty: it is a `flex: 1` sibling of
    // the other, so leaving it displayed would let it claim half the height.
    await mount({ threads: [threadOn("the first session", "t1")] });
    const richPage = screen.getByTestId("editor-document").querySelector<HTMLElement>(".editor")!;
    expect(richPage.style.display).toBe("");
    await toRaw();
    expect(richPage.style.display).toBe("none");
    expectGrid(screen.getByLabelText("Markdown source"));
    // CMT-FR-02: the discussions are read in the column in raw text too.
    expect(within(column()).getByTestId("comment-thread-t1")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await waitFor(() => expect(richPage.style.display).toBe(""));
  });

  it("CMT-FR-01, CMT-FR-02, DDS-FR-KTVW, EDT-FR-61: over a source file", async () => {
    await mount({
      artifactId: "src/main.rs",
      load: { body: RUST, checksum: "ck1" },
      threads: [wholeThread("w1", "src/main.rs")],
    });
    expectGrid(screen.getByLabelText("Source"));
    expect(await within(column()).findByTestId("comment-thread-w1")).toBeInTheDocument();
  });

  it("DDS-FR-XQMF: hides the column and the splitter in place, and keeps the column mounted", async () => {
    await mount({ threads: [threadOn("the first session", "t1")] });
    const held = column();
    await hideColumn();
    expect(split()).toHaveAttribute("data-discussion", "hidden");
    // The document column takes the tab's full width.
    expect(split().style.gridTemplateColumns).toBe("1fr");
    expect(screen.getByRole("separator", { hidden: true })).toHaveAttribute("hidden");
    // Hidden rather than unmounted: the same element, still holding the discussion.
    expect(column()).toBe(held);
    expect(within(held).getByTestId("comment-thread-t1")).toBeInTheDocument();
    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(true);
  });

  it("DDS-FR-PNXR, CMT-FR-30: a preset sets the split of the grid, and the keyboard moves the splitter, for this file", async () => {
    await mount({ threads: [threadOn("the first session", "t1")] });
    const even = split().style.gridTemplateColumns;

    fireEvent.click(screen.getByRole("radio", { name: "Document" }));
    await waitFor(() => expect(split().style.gridTemplateColumns).not.toBe(even));
    const documentWide = split().style.gridTemplateColumns;
    expect(discussionColumnState(fileColumn("a.md")).ratio).toBe("doc70");

    fireEvent.keyDown(screen.getByRole("separator"), { key: "ArrowLeft" });
    await waitFor(() => expect(split().style.gridTemplateColumns).not.toBe(documentWide));
    expect(typeof discussionColumnState(fileColumn("a.md")).ratio).toBe("number");
    expect(discussionColumnState(fileColumn("b.md")).ratio).toBe("even");
  });
});

describe("EDT-FR-16, CMT-FR-29: the column's controls in the toolbar cluster", () => {
  /** What each element of the cluster is, from leading to trailing. */
  const clusterOrder = () =>
    Array.from(
      screen.getByTestId("editor-discussion-toggle").closest(".editor__actions")!.children,
    )
      // The column's controls stand in one group of their own.
      .flatMap((el) =>
        el.classList.contains("editor__split-controls") ? Array.from(el.children) : [el],
      )
      .map((el) => {
        if (el.classList.contains("toc")) return "toc";
        if (el.getAttribute("role") === "radiogroup") return "presets";
        if (el.tagName === "KBD") return el.textContent;
        if (el.getAttribute("data-testid") === "editor-discussion-toggle") return "toggle";
        return el.getAttribute("aria-label");
      });

  it("EDT-FR-16, CMT-FR-29: reads the table of contents, the mode toggle, the presets, ⌘\\, and the column toggle last", async () => {
    await mount({ toc: true });
    expect(clusterOrder()).toEqual([
      "toc",
      "Edit as Markdown source",
      "presets",
      "⌘\\",
      "toggle",
    ]);
    const presets = screen.getByRole("radiogroup");
    expect(within(presets).getAllByRole("radio").map((r) => r.textContent)).toEqual([
      "Document",
      "Even",
      "Discussion",
    ]);
    const toggle = discussionToggle();
    expect(toggle).toHaveAttribute("aria-label", "Hide the discussion");
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    // No count anywhere in the cluster: the chooser in the column lists them.
    expect(toggle.textContent).toBe("");
  });

  it("DDS-FR-PNXR, DDS-FR-XQMF: has no presets and no ⌘\\ while the column is hidden, and the toggle stays last", async () => {
    await mount({ toc: true });
    await hideColumn();
    expect(clusterOrder()).toEqual(["toc", "Edit as Markdown source", "toggle"]);
    expect(screen.queryByRole("radiogroup")).toBeNull();
    const toggle = discussionToggle();
    expect(toggle).toHaveAttribute("aria-label", "Show the discussion");
    expect(toggle).toHaveAttribute("aria-pressed", "false");

    fireEvent.click(toggle);
    await waitFor(() => expect(clusterOrder()).toHaveLength(5));
  });

  it("CMT-FR-29: keeps the column's controls in raw Markdown, and holds them alone over a source file", async () => {
    await mount({ toc: true });
    await toRaw();
    expect(clusterOrder()).toEqual(["toc", "Edit as rich text", "presets", "⌘\\", "toggle"]);
    cleanup();

    await mount({ artifactId: "src/main.rs", load: { body: RUST, checksum: "ck1" } });
    expect(clusterOrder()).toEqual(["presets", "⌘\\", "toggle"]);
  });
});

describe("ACT-FR-BRZK, DDS-FR-JWNC: Discuss and the column", () => {
  it("ACT-FR-BRZK: is disabled with its reason while the column is shown", async () => {
    await mount();
    await screen.findByTestId("draft-open-discussion");
    const discuss = openDiscuss();
    expect(discuss).toBeDisabled();
    expect(discuss).toHaveAttribute("title", "The discussion is already beside the file");
  });

  it("ACT-FR-BRZK, DDS-FR-JWNC: shows the hidden column and focuses its opening composer", async () => {
    await mount();
    await hideColumn();
    const discuss = openDiscuss();
    expect(discuss).toBeEnabled();
    fireEvent.click(discuss);

    await waitFor(() => expect(column().hidden).toBe(false));
    // By the route of the toolbar control, so the shown state persists.
    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(false);
    expect(discussionToggle()).toHaveAttribute("aria-pressed", "true");
    const opener = within(column()).getByRole("textbox", { name: "Discuss this file" });
    await waitFor(() => expect(opener).toHaveFocus());
    // ACT-FR-19: Discuss opens no composer of its own.
    expectNoFloatingDiscussion();
  });

  it("DDS-FR-JWNC: focuses the reply composer where the file has a discussion", async () => {
    await mount({ threads: [wholeThread("w1")] });
    await screen.findByTestId("comment-thread-w1");
    await hideColumn();
    fireEvent.click(openDiscuss());
    const reply = within(column()).getByRole("textbox", { name: "Reply to thread w1" });
    await waitFor(() => expect(reply).toHaveFocus());
  });
});

describe("CMT-FR-05, DDS-FR-QMBC: Comment on a selection", () => {
  it.each([
    ["shown", false],
    ["hidden", true],
  ] as const)(
    "CMT-FR-05, DDS-FR-QMBC: opens the opening composer of the fragment in the column, with the column %s",
    async (_state, hide) => {
      await mount();
      if (hide) await hideColumn();

      selectInBody("the first session");
      fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));

      await waitFor(() => expect(column().hidden).toBe(false));
      const opening = within(column()).getByTestId("draft-open-discussion");
      expect(within(opening).getByTestId("discussion-opening-fragment")).toHaveTextContent(
        "the first session",
      );
      expect(opening).toHaveTextContent("This begins a discussion about the passage you selected.");
      const field = within(opening).getByRole("textbox", { name: "Discuss this file" });
      await waitFor(() => expect(field).toHaveFocus());
      // No floating composer stands for the passage: it is the column's.
      expectNoFloatingDiscussion();
    },
  );
});

describe("CMT-FR-28: the highlight and the column focus each other", () => {
  it("CMT-FR-28: clicking a highlight shows the column and chooses its discussion", async () => {
    await mount({
      threads: [threadOn("Onboarding", "t1"), threadOn("the first session", "t2")],
    });
    await waitFor(() => expect(markOf("t2")).not.toBeNull());
    await hideColumn();

    fireEvent.click(markOf("t2")!);

    await waitFor(() => expect(column().hidden).toBe(false));
    const tabs = within(column()).getAllByRole("tab");
    expect(tabs[1]).toHaveAttribute("aria-selected", "true");
    expect(within(column()).getByTestId("comment-thread-t2")).toBeInTheDocument();
    await waitFor(() => expect(markOf("t2")).toHaveAttribute("data-comment-focused", "true"));
    // The column was shown by the route of its toolbar control, so it persists.
    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(false);
  });

  it("CMT-FR-28, CVP-FR-06: clicking the highlight of a discussion the column does not show gives that discussion the focus", async () => {
    await mount({
      threads: [threadOn("Onboarding", "t1"), threadOn("the first session", "t2")],
    });
    await waitFor(() => expect(markOf("t2")).not.toBeNull());

    fireEvent.click(markOf("t2")!);

    await waitFor(() => expect(screen.getByTestId("comment-thread-t2")).toHaveFocus());
  });

  it("CMT-FR-28, CVP-FR-06: a tab that closes before the focus lands leaves no held focus behind", async () => {
    await mount({
      threads: [threadOn("Onboarding", "t1"), threadOn("the first session", "t2")],
    });
    await waitFor(() => expect(markOf("t2")).not.toBeNull());

    fireEvent.click(markOf("t2")!);
    cleanup();
    await new Promise((resolve) => requestAnimationFrame(() => resolve(undefined)));

    expect(consumePendingFocus("t2")).toBeNull();
  });

  it("CMT-FR-28, DDS-FR-XQMF: clicking a highlight while the column is hidden shows it and gives the discussion the focus", async () => {
    await mount({ threads: [threadOn("the first session", "t1")] });
    await waitFor(() => expect(markOf("t1")).not.toBeNull());
    fireEvent.click(screen.getByTestId("editor-discussion-toggle"));
    await waitFor(() => expect(column().hidden).toBe(true));

    fireEvent.click(markOf("t1")!);

    await waitFor(() => expect(column().hidden).toBe(false));
    await waitFor(() => expect(screen.getByTestId("comment-thread-t1")).toHaveFocus());
  });

  it("CMT-FR-28: the fragment quote in the column switches raw Markdown to WYSIWYG and scrolls to the fragment", async () => {
    await mount({ threads: [threadOn("the first session", "t1")] });
    await waitFor(() => expect(markOf("t1")).not.toBeNull());
    await toRaw();
    scrollIntoView.mockClear();

    fireEvent.click(within(column()).getByTestId("discussion-fragment-quote"));

    expect(
      await screen.findByRole("button", { name: "Edit as Markdown source" }),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText("Markdown source")).toBeNull();
    await waitFor(() =>
      expect(scrollIntoView.mock.contexts.some((el) => el === markOf("t1"))).toBe(true),
    );
  });
});

describe("ACT-FR-19, ACT-FR-20: no floating discussion surface in the Editor", () => {
  const threads = () => [wholeThread("w1"), threadOn("the first session", "t1")];

  it("ACT-FR-19, ACT-FR-20: in WYSIWYG and raw Markdown, with the column shown or hidden", async () => {
    await mount({ threads: threads() });
    await screen.findByTestId("comment-thread-w1");
    expectNoFloatingDiscussion();
    await hideColumn();
    expectNoFloatingDiscussion();
    await toRaw();
    expectNoFloatingDiscussion();
    fireEvent.click(openDiscuss());
    await waitFor(() => expect(column().hidden).toBe(false));
    // DDS-FR-JWNC: in raw Markdown too, the caret goes to the column.
    const reply = within(column()).getByRole("textbox", { name: "Reply to thread w1" });
    await waitFor(() => expect(reply).toHaveFocus());
    expectNoFloatingDiscussion();
  });

  it("ACT-FR-19, ACT-FR-20: over a source file", async () => {
    await mount({
      artifactId: "src/main.rs",
      load: { body: RUST, checksum: "ck1" },
      threads: [wholeThread("w1", "src/main.rs")],
    });
    await screen.findByTestId("comment-thread-w1");
    expectNoFloatingDiscussion();
    // ACT-FR-BRZK: Discuss has nothing to do while the column is shown.
    expect(openDiscuss()).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Close actions" }));
    await hideColumn();
    fireEvent.click(openDiscuss());
    await waitFor(() => expect(column().hidden).toBe(false));
    const reply = within(column()).getByRole("textbox", { name: "Reply to thread w1" });
    await waitFor(() => expect(reply).toHaveFocus());
    expectNoFloatingDiscussion();
  });
});

describe("DDS-FR-VHZN, CMT-FR-61: the opening composer names the file", () => {
  it.each([
    ["skill", "file"],
    ["spec", "specification"],
  ] as const)(
    "DDS-FR-VHZN, ACT-FR-BRZK: a %s is called a %s in the empty statement, the composer and the reason on Discuss",
    async (artifactType, noun) => {
      await mount({ artifactType });
      const opening = await within(column()).findByTestId("draft-open-discussion");
      expect(opening).toHaveTextContent(`Nothing has been said about this ${noun} yet.`);
      const field = within(opening).getByRole("textbox", { name: `Discuss this ${noun}` });
      expect(field).toHaveAttribute("placeholder", `Ask an agent about this ${noun}…`);
      // With no discussion there is no chooser to list one in.
      expect(within(column()).queryByRole("tablist")).toBeNull();
      expect(openDiscuss()).toHaveAttribute(
        "title",
        `The discussion is already beside the ${noun}`,
      );
    },
  );

  it("DDS-FR-VHZN, CMT-FR-53, CVP-FR-08: lists whole-target and fragment discussions in one chooser and begins another", async () => {
    await mount({
      artifactType: "spec",
      threads: [
        threadOn("the first session", "t1"),
        { ...wholeThread("w1"), createdAt: "2026-01-03T00:00:00Z" },
      ],
    });
    const chooser = await within(column()).findByRole("tablist", { name: "Discussions" });
    await waitFor(() => expect(within(chooser).getAllByRole("tab")).toHaveLength(2));
    expect(
      within(chooser).getAllByRole("tab").map((t) => t.getAttribute("data-target")),
    ).toEqual(["fragment", "whole"]);

    const another = within(chooser).getByTestId("discussion-new");
    expect(another).toHaveAttribute(
      "aria-label",
      "Begin another discussion about this specification",
    );
    fireEvent.click(another);
    expect(await within(column()).findByTestId("draft-open-discussion")).toHaveTextContent(
      "This begins another discussion about this specification.",
    );
  });
});

describe("React keys of the column's lists", () => {
  it("CMT-FR-53: passes each chooser entry's key directly rather than through a spread", async () => {
    // React 19 warns — via `console.error` — when a `key` arrives inside a
    // spread object, and a key that arrives that way is not read as a key at
    // all. The chooser is rendered with all three kinds of entry: anchored,
    // orphaned, and resolved behind the disclosure.
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      await mount({
        threads: [
          threadOn("the first session", "aligned"),
          makeThread({ id: "orphan", fragmentTarget: fragment({ start: 0, end: 0, quote: "gone" }) }),
          makeThread({ id: "settled", resolved: true }),
        ],
      });
      fireEvent.click(await within(column()).findByRole("button", { name: "1 resolved" }));
      await waitFor(() => expect(within(column()).getAllByRole("tab")).toHaveLength(3));

      // Matched on "key" generally, not on the spread message alone: removing
      // the key entirely also silences that specific warning.
      const keyWarnings = spy.mock.calls.filter((c) => /\bkey\b/i.test(String(c[0])));
      expect(keyWarnings).toEqual([]);
    } finally {
      spy.mockRestore();
    }
  });
});

describe("CMT-FR-35: the column's controls are graphical", () => {
  it("CMT-FR-35: labels Post, Cancel and Quote without spending the width on words", async () => {
    await mount({ threads: [makeThread()] });
    await openColumn("t1");

    // Seed a quote so the fourth control the requirement names exists too.
    fireEvent.click(screen.getByRole("button", { name: "Quote comment 1 by raver119" }));

    for (const name of ["Post", "Cancel", "Quote comment 1 by raver119", "Remove quote"]) {
      const button = within(column()).getByRole("button", { name });
      expect(button.querySelector("svg")).not.toBeNull();
      expect(button.textContent).toBe("");
      // Still reachable by name, which is what keeps it operable at all.
      expect(button.getAttribute("aria-label")).toBe(name);
    }
  });

  it("CMT-FR-35, DDS-FR-QMBC: applies the same treatment to the opening composer of a passage", async () => {
    await mount({ threads: [] });
    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    const opening = within(await within(column()).findByTestId("draft-open-discussion"));
    await opening.findByTestId("discussion-opening-fragment");

    for (const name of ["Post", "Cancel"]) {
      const button = opening.getByRole("button", { name });
      expect(button.querySelector("svg")).not.toBeNull();
      expect(button.textContent).toBe("");
    }
  });

  it("CMT-FR-35, CMT-FR-05: offers the selection affordance as a glyph rather than a filled block", async () => {
    await mount({ threads: [] });
    selectInBody("the first session");
    const affordance = await screen.findByRole("button", { name: "Comment on selection" });
    expect(affordance.querySelector("svg")).not.toBeNull();
    expect(affordance.textContent).toBe("");
  });
});

describe("CMT-FR-36 / CMT-FR-30: landing on a thread sent from the Comments panel", () => {
  /**
   * Mount the way `Viewport` does when the panel's click-through has named a
   * thread: the shell has already written the mode into the session, and hands
   * the thread id down as `focusThreadId`.
   */
  async function mountFocused(
    threadId: string | null,
    threads: Discussion[],
    sessionPatch: MountOptions["sessionPatch"] = {},
  ) {
    const onThreadFocused = vi.fn();
    const mounted = await mount({
      threads,
      focusThreadId: threadId,
      onThreadFocused,
      sessionPatch: { mode: "wysiwyg", ...sessionPatch },
    });
    return { ...mounted, onThreadFocused };
  }

  it("CMT-FR-36, CMT-FR-28: scrolls the thread's anchor into view on arrival", async () => {
    // CMT-FR-28 via CMT-FR-36: landing on a discussion brings its passage into
    // view, or the reviewer arrives at a highlighted range they cannot see.
    await mountFocused("t1", [threadOn("the first session", "t1")]);
    await waitFor(() =>
      expect(
        scrollIntoView.mock.contexts.some(
          (el) => (el as Element).getAttribute("data-comment-thread") === "t1",
        ),
      ).toBe(true),
    );
  });

  it("CMT-FR-36, CMT-FR-19: lands on an orphaned thread without a passage to scroll to", async () => {
    // CMP-FR-13: the panel lists a thread the column renders as orphaned, so
    // the click-through must survive there being no anchored range in the body.
    await mountFocused("t1", [
      makeThread({
        id: "t1",
        fragmentTarget: fragment({ start: 0, end: 0, quote: "a passage this artifact lost" }),
      }),
    ]);
    const card = await within(column()).findByTestId("comment-thread-t1");
    expect(card).toHaveAttribute("data-availability", "orphaned");
  });

  it("CMT-FR-36, CMT-FR-30: renders the column with that thread chosen", async () => {
    const { onThreadFocused } = await mountFocused("t2", [
      threadOn("the first session", "t1"),
      threadOn("Onboarding", "t2"),
    ]);

    // The column is showing without the author having toggled it.
    expect(column()).toBeVisible();
    await waitFor(() =>
      expect(within(column()).getAllByRole("tab")[1]).toHaveAttribute("aria-selected", "true"),
    );
    expect(within(column()).getByTestId("comment-thread-t2")).toBeInTheDocument();
    expect(screen.queryByTestId("comment-thread-t1")).toBeNull();
    await waitFor(() => expect(markOf("t2")).toHaveAttribute("data-comment-focused", "true"));
    // Consumed once, so a later re-render does not yank the body back.
    expect(onThreadFocused).toHaveBeenCalledTimes(1);
  });

  it("CMT-FR-36: focuses nothing when the tab was not reached from the panel", async () => {
    const { onThreadFocused } = await mountFocused(null, [threadOn("the first session", "t1")]);
    await waitFor(() => expect(markOf("t1")).not.toBeNull());
    // The column reads the first discussion, and no passage carries the focus.
    expect(within(column()).getByTestId("comment-thread-t1")).toBeInTheDocument();
    expect(markOf("t1")).not.toHaveAttribute("data-comment-focused", "true");
    expect(onThreadFocused).not.toHaveBeenCalled();
  });

  it("CMT-FR-36, CMT-FR-17: reaches a resolved thread through the expanded disclosure", async () => {
    await mountFocused(
      "t1",
      [makeThread({ id: "t1", resolved: true })],
      // CMP-FR-11: the shell expands the disclosure for a resolved thread.
      { resolvedOpen: true },
    );
    const card = await within(column()).findByTestId("comment-thread-t1");
    expect(card).toHaveAttribute("data-resolved", "true");
    expect(within(column()).getByRole("button", { name: "1 resolved" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });
});

describe("CMS-FR-51, CMS-FR-31 / CMT-FR-04: the column redraws from the event, never a re-read", () => {
  function reads(backend: Backend) {
    return backend.calls.filter((c) => c.cmd === "list_discussions").length;
  }
  const comment = (id: string, body: string) => ({
    id,
    author: human("octocat"),
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-01-02T00:00:00Z",
  });

  it("CMS-FR-51, CMT-FR-04: renders a comment appended elsewhere in the window, without asking again", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openColumn("t1");
    const before = reads(backend);
    expect(before).toBeGreaterThan(0);

    // A comment posted from a second Editor tab on the same artifact: the
    // backend announces the thread, and this column draws it from the payload.
    act(() =>
      emitEvent("discussion-changed", {
        ...makeThread(),
        comments: [...makeThread().comments, comment("c-remote", "posted from another tab")],
      }),
    );

    expect(await screen.findByText("posted from another tab")).toBeInTheDocument();
    expect(reads(backend)).toBe(before);
  });

  it("CMT-FR-04, CMT-FR-16: moves a discussion behind the resolved disclosure when the resolution was set anywhere", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openColumn("t1");
    expect(screen.getByLabelText("Reply to thread t1")).toBeInTheDocument();
    const before = reads(backend);

    act(() => emitEvent("discussion-changed", { ...makeThread(), resolved: true }));

    await waitFor(() =>
      expect(screen.queryByLabelText("Reply to thread t1")).not.toBeInTheDocument(),
    );
    expect(within(column()).getByRole("button", { name: "1 resolved" })).toBeInTheDocument();
    expect(within(column()).getByTestId("draft-open-discussion")).toHaveTextContent(
      "The discussion about this file is resolved.",
    );
    expect(reads(backend)).toBe(before);
  });

  it("CMS-FR-51, CMT-FR-04: ignores a thread belonging to a different artifact", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openColumn("t1");
    const before = reads(backend);

    act(() =>
      emitEvent("discussion-changed", {
        ...makeThread({ id: "t-other" }),
        target: { kind: "artifact", artifactId: "somewhere/else.md" },
        comments: [comment("c-other", "another artifact entirely")],
      }),
    );

    expect(screen.queryByText("another artifact entirely")).not.toBeInTheDocument();
    expect(within(column()).getAllByRole("tab")).toHaveLength(1);
    expect(reads(backend)).toBe(before);
  });

  it("CMS-FR-31, DDS-FR-XQMF: reads nothing again and writes nothing while the column is hidden and shown", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openColumn("t1");
    const before = reads(backend);

    await hideColumn();
    fireEvent.click(discussionToggle());
    await waitFor(() => expect(column().hidden).toBe(false));
    expect(within(column()).getByTestId("comment-thread-t1")).toBeInTheDocument();

    expect(reads(backend)).toBe(before);
    const writes = backend.calls.filter((c) =>
      /^(add_comment|open_discussion|set_discussion|reanchor|save_artifact)/.test(c.cmd),
    );
    expect(writes).toEqual([]);
  });
});

describe("CMT-FR-RPLC: a file that cannot be read", () => {
  it("CMT-FR-RPLC, CMT-FR-19: shows its discussions in the column as unavailable", async () => {
    await mount({ threads: [threadOn("the first session", "t1")], loadFails: true });

    const card = await within(await screen.findByTestId("editor-discussion-column")).findByTestId(
      "comment-thread-t1",
    );
    await waitFor(() => expect(card).toHaveAttribute("data-availability", "unavailable"));
  });
});

describe("DDS-FR-MXHD: the Editor's column carries no draft review", () => {
  it("DDS-FR-MXHD: renders no subject strip, anchor bar, review bar, chip or gutter map", async () => {
    await mount({ threads: [wholeThread("w1"), threadOn("the first session", "t1")] });
    const shown = column();
    await waitFor(() => expect(within(shown).getByRole("tablist")).toBeInTheDocument());

    for (const tab of within(shown).getAllByRole("tab")) {
      fireEvent.click(tab);
      expect(within(shown).queryByTestId("discussion-subject")).toBeNull();
      expect(within(shown).queryByTestId("discussion-anchor-bar")).toBeNull();
    }
    for (const id of ["review-counter", "review-standing", "hunk-chip", "hunk-gutter-map"]) {
      expect(screen.queryByTestId(id)).toBeNull();
    }
  });
});
