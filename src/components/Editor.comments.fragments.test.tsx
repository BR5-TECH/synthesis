import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import type { FragmentTarget } from "../types";
import {
  BODY,
  fragment,
  makeThread,
  openColumn,
  wireBackend,
} from "../test/editorCommentsFixtures";
import type { Backend } from "../test/editorCommentsFixtures";

/**
 * The fragment of a discussion in the Editor's text surface, and the surface in
 * the discussion column that holds the conversation (`CMT-comments.md` CMT-FR-28,
 * CMT-FR-RPLC, CMT-FR-HQNV; `CVP-conversation-presentation.md` CVP-FR-05,
 * CVP-FR-06).
 */
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const QUOTE = "the first session";
const scrollIntoView = vi.fn();

beforeEach(() => {
  invokeMock.mockReset();
  scrollIntoView.mockReset();
  Element.prototype.scrollIntoView = scrollIntoView;
});

const selectionHosts: HTMLElement[] = [];
afterEach(() => {
  cleanup();
  window.getSelection()?.removeAllRanges();
  while (selectionHosts.length) selectionHosts.pop()!.remove();
});

async function mount(over: Partial<Backend> = {}) {
  const sessions = new EditSessionStore();
  const backend: Backend = {
    load: { body: BODY, checksum: "ck1" },
    threads: [],
    calls: [],
    ...over,
  };
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

function threadAt(start: number, quote: string, id = "t1") {
  const target: FragmentTarget = fragment({
    start,
    end: start + quote.length,
    quote,
  });
  return makeThread({ id, fragmentTarget: target });
}

const marks = () =>
  Array.from(document.querySelectorAll<HTMLElement>("[data-comment-thread]"));

function selectInBody(text: string) {
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

describe("CMT-FR-28, CMT-FR-18, CMT-FR-RPLC: a fragment is restored and shown", () => {
  it("CMT-FR-28: marks the fragment with an underline style and an accessible label", async () => {
    await mount({ threads: [threadAt(BODY.indexOf(QUOTE), QUOTE)] });
    await openColumn("t1");

    await waitFor(() => expect(marks()).toHaveLength(1));
    const mark = marks()[0];
    expect(mark.textContent).toBe(QUOTE);
    expect(mark.classList.contains("comment-anchor")).toBe(true);
    expect(mark).toHaveAttribute("role", "mark");
    expect(mark).toHaveAttribute("aria-label", "Fragment of discussion t1");
    expect(
      screen.getByLabelText("Fragment of discussion t1"),
    ).toBe(mark);
  });

  it("CMT-FR-18: restores a fragment whose stored offsets are stale from its quote", async () => {
    await mount({ threads: [threadAt(0, QUOTE)] });
    await openColumn("t1");

    await waitFor(() => expect(marks()).toHaveLength(1));
    expect(marks()[0].textContent).toBe(QUOTE);
  });

  it("CMT-FR-19, CMT-FR-RPLC: shows an orphaned fragment as orphaned and deletes nothing", async () => {
    const { backend } = await mount({
      threads: [threadAt(3, "a passage nobody wrote")],
    });
    const column = await openColumn("t1");

    const card = await within(column).findByTestId("comment-thread-t1");
    expect(card).toHaveAttribute("data-availability", "orphaned");
    expect(within(card).getByText("Orphaned")).toBeInTheDocument();
    expect(card).toHaveTextContent("a passage nobody wrote");
    // Still readable and answerable, never turned into a whole-target discussion.
    expect(card).toHaveTextContent("Which session?");
    expect(within(card).getByLabelText("Reply to thread t1")).toBeEnabled();
    expect(card).toHaveAttribute("data-target", "fragment");
    expect(marks()).toHaveLength(0);
    const writes = backend.calls.filter((c) =>
      /^(add_comment|set_discussion|reanchor|open_discussion)/.test(c.cmd),
    );
    expect(writes).toEqual([]);
  });
});

describe("CMT-FR-28, CVP-FR-06: the fragment and the discussion focus each other", () => {
  it("CMT-FR-28, DDS-FR-QMBC: choosing the discussion in the column gives its fragment the focused style and scrolls it into view", async () => {
    await mount({
      threads: [
        threadAt(BODY.indexOf("Onboarding"), "Onboarding", "t1"),
        threadAt(BODY.indexOf(QUOTE), QUOTE, "t2"),
      ],
    });
    const column = await openColumn("t1");
    await waitFor(() => expect(marks()).toHaveLength(2));
    const markOf = (id: string) =>
      marks().find((m) => m.dataset.commentThread === id)!;
    // Nothing is focused before the author chooses.
    expect(markOf("t2")).toHaveAttribute("aria-label", "Fragment of discussion t2");

    fireEvent.click(within(column).getAllByRole("tab")[1]);

    await waitFor(() =>
      expect(markOf("t2")).toHaveAttribute("aria-label", "Fragment of discussion t2 (focused)"),
    );
    expect(markOf("t2").classList.contains("comment-anchor--focused")).toBe(true);
    expect(markOf("t2")).toHaveAttribute("data-comment-focused", "true");
    expect(markOf("t1")).not.toHaveAttribute("data-comment-focused", "true");
    await waitFor(() =>
      expect(scrollIntoView.mock.contexts.some((el) => el === markOf("t2"))).toBe(true),
    );
  });

  it("CMT-FR-28: activating the fragment focuses the discussion and changes no content", async () => {
    const { sessions, backend } = await mount({
      threads: [threadAt(BODY.indexOf(QUOTE), QUOTE)],
    });
    await openColumn("t1");
    await waitFor(() => expect(marks()).toHaveLength(1));
    const before = sessions.get("a.md")?.buffer;
    const calls = backend.calls.length;

    fireEvent.click(marks()[0]);

    await waitFor(() =>
      expect(screen.getByTestId("comment-thread-t1")).toHaveFocus(),
    );
    expect(screen.getByTestId("comment-thread-t1").dataset.focused).toBe("true");
    expect(sessions.get("a.md")?.buffer).toBe(before);
    expect(sessions.get("a.md")?.dirty).toBe(false);
    expect(
      backend.calls
        .slice(calls)
        .filter((c) => c.cmd.startsWith("save") || c.cmd === "add_comment"),
    ).toEqual([]);
  });

  it("CMT-FR-28: activating the card's quote scrolls the fragment into view again", async () => {
    await mount({ threads: [threadAt(BODY.indexOf(QUOTE), QUOTE)] });
    await openColumn("t1");
    await waitFor(() => expect(marks()).toHaveLength(1));
    fireEvent.click(screen.getByTestId("discussion-fragment-quote"));
    await waitFor(() => expect(scrollIntoView).toHaveBeenCalled());
    scrollIntoView.mockClear();

    fireEvent.click(screen.getByTestId("discussion-fragment-quote"));

    await waitFor(() =>
      expect(scrollIntoView.mock.contexts.some((el) => el === marks()[0])).toBe(true),
    );
  });
});

describe("CMT-FR-HQNV, CVP-FR-05: the column holds the shared surface, once", () => {
  it("CMT-FR-53: renders the chosen discussion as one surface owned by the column", async () => {
    await mount({
      threads: [
        threadAt(BODY.indexOf(QUOTE), QUOTE, "t1"),
        threadAt(BODY.indexOf("Onboarding"), "Onboarding", "t2"),
      ],
    });
    const column = await openColumn("t1");
    const tabs = within(column).getAllByRole("tab");
    expect(tabs).toHaveLength(2);

    for (const [at, id] of [[0, "t1"], [1, "t2"]] as const) {
      fireEvent.click(tabs[at]);
      const cards = await screen.findAllByTestId(`comment-thread-${id}`);
      expect(cards).toHaveLength(1);
      expect(column).toContainElement(cards[0]);
      expect(cards[0]).toHaveAttribute("data-discussion-owner", "column");
      expect(cards[0]).toHaveAttribute("data-target", "fragment");
      // One discussion at a time: the other is reached through its tab.
      const other = id === "t1" ? "t2" : "t1";
      expect(screen.queryByTestId(`comment-thread-${other}`)).toBeNull();
    }
  });
});

describe("CVP-FR-40, CMT-FR-07, DDS-FR-QMBC: opening a fragment discussion from a selection", () => {
  /** The opening composer of the column, which carries the passage. */
  const opener = () =>
    within(screen.getByTestId("draft-open-discussion")).getByRole("textbox", {
      name: "Discuss this file",
    });

  it("posts the opening composer on Ctrl+Enter with the fragment target", async () => {
    const { backend } = await mount({ threads: [] });
    await openColumn();

    selectInBody(QUOTE);
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await screen.findByTestId("discussion-opening-fragment");
    const field = opener();
    await waitFor(() => expect(field).toHaveFocus());
    fireEvent.change(field, { target: { value: "needs tightening" } });
    fireEvent.keyDown(field, { key: "Enter", ctrlKey: true });

    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "open_discussion")).toBe(true),
    );
    const args = backend.calls.find((c) => c.cmd === "open_discussion")!.args as {
      target: { kind: string; artifactId: string };
      fragmentTarget: FragmentTarget;
      body: string;
    };
    expect(args.target).toEqual({ kind: "artifact", artifactId: "a.md" });
    expect(args.fragmentTarget.quote).toBe(QUOTE);
    expect(args.fragmentTarget.path).toBe("a.md");
    expect(BODY.slice(args.fragmentTarget.start, args.fragmentTarget.end)).toBe(QUOTE);
    expect(args.body).toBe("needs tightening");
    expect(await screen.findAllByTestId("comment-thread-t-new")).toHaveLength(1);
    expect(screen.queryByTestId("discussion-opening-fragment")).toBeNull();
  });

  it("opens nothing on Ctrl+Enter while the draft is empty", async () => {
    const { backend } = await mount({ threads: [] });
    await openColumn();

    selectInBody(QUOTE);
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await screen.findByTestId("discussion-opening-fragment");
    fireEvent.keyDown(opener(), { key: "Enter", ctrlKey: true });

    expect(backend.calls.some((c) => c.cmd === "open_discussion")).toBe(false);
    expect(screen.getByTestId("discussion-opening-fragment")).toHaveTextContent(QUOTE);
  });
});
