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

import { AFFORDANCE_WIDTH, Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { discussionColumnState, fileColumn } from "../state/discussionColumn";
import { GITHUB_TOKEN_ERRORS } from "../types";
import type { AttachmentInput, FragmentRange } from "../types";
import {
  BODY,
  discussionToggle,
  makeThread,
  openColumn,
  wireBackend,
  fragment,
} from "../test/editorCommentsFixtures";
import type { Backend } from "../test/editorCommentsFixtures";

/**
 * CMT-comments.md, driven through the real Editor.
 *
 * The anchor arithmetic — re-anchoring and tracking an edit — lives in
 * `../state/commentAnchors` and is covered by its own unit tests. What is
 * exercised here is the wiring of a selection: the Comment affordance, the
 * opening composer it puts in the discussion column (DDS-FR-QMBC), and the
 * command a post invokes.
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

/**
 * DDS-FR-QMBC: the column's opening composer, which the affordance opens for a
 * passage. The quote it carries is its own element.
 */
const opening = () => screen.getByTestId("draft-open-discussion");
const openingQuote = () => screen.findByTestId("discussion-opening-fragment");
const opener = () =>
  within(opening()).getByRole("textbox", { name: "Discuss this file" }) as HTMLTextAreaElement;
const openerButton = (name: "Post" | "Cancel") =>
  within(opening()).getByRole("button", { name });

describe("CMT-FR-05 / CMT-FR-06 / CMT-FR-07: starting a thread", () => {
  it("CMT-FR-05, DDS-FR-QMBC: opens a thread over the selection and anchors it in the Markdown source", async () => {
    // CMT-FR-05, CMT-FR-06, CMT-FR-07. The entry point of the whole feature.
    const { backend } = await mount({ threads: [] });
    // The column is shown for every file, so the author selects at once.
    await openColumn();

    selectInBody("the first session");
    const affordance = await screen.findByRole("button", { name: "Comment on selection" });
    fireEvent.click(affordance);

    expect(await openingQuote()).toHaveTextContent("the first session");
    expect(opening()).toHaveTextContent(
      "This begins a discussion about the passage you selected.",
    );

    fireEvent.change(opener(), { target: { value: "needs tightening" } });
    fireEvent.click(openerButton("Post"));

    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "open_discussion")).toBe(true),
    );
    const call = backend.calls.find((c) => c.cmd === "open_discussion")!;
    const args = call.args as {
      target: { artifactId: string };
      fragmentTarget: FragmentRange;
      body: string;
      attachments: AttachmentInput[];
    };
    expect(args.target.artifactId).toBe("a.md");
    expect(args.body).toBe("needs tightening");
    expect(args.fragmentTarget.quote).toBe("the first session");
    // CMT-FR-05, CMT-FR-06, CMT-FR-07 / CMT-FR-47: an empty list, which is the ordinary case.
    expect(args.attachments).toEqual([]);
    // The offsets address the Markdown source, not the rendered body.
    expect(BODY.slice(args.fragmentTarget.start, args.fragmentTarget.end)).toBe("the first session");
    // And the composer is replaced by the created discussion, in the column.
    expect(await screen.findByTestId("comment-thread-t-new")).toBeInTheDocument();
    expect(screen.queryByTestId("discussion-opening-fragment")).toBeNull();
  });

  it("anchors to the occurrence nearest the selection when the quote repeats", async () => {
    // CMT-FR-06: a spec that repeats a heading must not send every thread on the
    // later ones to the first.
    const repeated = "## Steps\n\nalpha\n\n## Steps\n\nbeta\n";
    const { backend } = await mount({ threads: [], load: { body: repeated, checksum: "ck1" } });
    await openColumn();

    selectInBody("## Steps");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();
    fireEvent.change(opener(), { target: { value: "x" } });
    fireEvent.click(openerButton("Post"));

    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "open_discussion")).toBe(true),
    );
    const args = backend.calls.find((c) => c.cmd === "open_discussion")!.args as {
      fragmentTarget: FragmentRange;
    };
    // Whichever occurrence it picked, the anchor must actually cover the quote.
    expect(repeated.slice(args.fragmentTarget.start, args.fragmentTarget.end)).toBe("## Steps");
  });

  it("reveals nothing for an empty selection and discards a cancelled draft", async () => {
    // CMT-FR-05 / CMT-FR-07 / CMT-FR-05.
    const { backend } = await mount({ threads: [makeThread()] });
    await openColumn("t1");

    // A collapsed selection reveals no affordance.
    window.getSelection()?.removeAllRanges();
    fireEvent.mouseUp(screen.getByLabelText("artifact body"));
    expect(screen.queryByRole("button", { name: "Comment on selection" })).toBeNull();

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();
    fireEvent.change(opener(), { target: { value: "typed" } });
    fireEvent.click(openerButton("Cancel"));

    await waitFor(() =>
      expect(screen.queryByTestId("discussion-opening-fragment")).toBeNull(),
    );
    // The column goes back to the discussion it was reading.
    expect(await screen.findByTestId("comment-thread-t1")).toBeInTheDocument();
    expect(backend.calls.some((c) => c.cmd === "open_discussion")).toBe(false);
  });

  it("CMT-FR-34: surfaces a refused first comment on the opening composer itself", async () => {
    // CMT-FR-34 for the one composer that has no thread id yet. Without this the
    // author sees a composer that appears to have done nothing.
    const { backend } = await mount({
      threads: [],
      onOpenThread: () => {
        throw "invalid_fragment";
      },
    });
    await openColumn();

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();
    fireEvent.change(opener(), { target: { value: "body" } });
    fireEvent.click(openerButton("Post"));

    const alert = await within(opening()).findByRole("alert");
    expect(alert.textContent).not.toBe("invalid_fragment");
    expect(alert.textContent).toMatch(/anchored/i);
    // The composer is still there with the passage and what the author typed.
    expect(opener().value).toBe("body");
    expect(screen.getByTestId("discussion-opening-fragment")).toHaveTextContent(
      "the first session",
    );
    expect(backend.calls.some((c) => c.cmd === "open_discussion")).toBe(true);
  });
});

/**
 * jsdom implements no layout, so a Range has no `getClientRects` at all — the
 * placement code treats that as "no geometry available" and falls back. These
 * tests install one for the duration of a single test so the arithmetic that
 * turns a selection's rectangle into a position can be checked.
 */
function withSelectionRects(rects: { bottom: number; left: number }[]) {
  Object.defineProperty(Range.prototype, "getClientRects", {
    value: () => rects,
    configurable: true,
  });
  installedRects = true;
}

let installedRects = false;
afterEach(() => {
  if (installedRects) {
    delete (Range.prototype as { getClientRects?: unknown }).getClientRects;
    installedRects = false;
  }
});

describe("CMT-FR-05: the affordance stays reachable and sits with the selection", () => {
  it("CMT-FR-05, DDS-FR-QMBC, DDS-FR-XQMF: shows the column when the affordance is activated with it hidden", async () => {
    // The composer lives in the column. With the column hidden, activating the
    // affordance must show it, or the author would see nothing happen.
    await mount({ threads: [] });
    const column = await screen.findByTestId("editor-discussion-column");
    fireEvent.click(discussionToggle());
    await waitFor(() => expect(column.hidden).toBe(true));

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));

    await waitFor(() => expect(column.hidden).toBe(false));
    expect(within(column).getByTestId("discussion-opening-fragment")).toHaveTextContent(
      "the first session",
    );
    // Showing it is as deliberate an act as the toggle, so it records the same
    // choice (DDS-FR-XQMF): the column does not hide again the next time this
    // file is opened.
    expect(discussionColumnState(fileColumn("a.md")).hidden).toBe(false);
    expect(discussionToggle()).toHaveAttribute("aria-pressed", "true");
  });

  it("reveals the affordance again for a fresh selection after a cancelled draft", async () => {
    await mount({ threads: [] });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();
    fireEvent.click(openerButton("Cancel"));
    await waitFor(() =>
      expect(screen.queryByTestId("discussion-opening-fragment")).toBeNull(),
    );

    // A second selection must be able to start a thread just like the first.
    selectInBody("Steps to run");
    const again = await screen.findByRole("button", { name: "Comment on selection" });
    fireEvent.click(again);
    expect(await openingQuote()).toHaveTextContent("Steps to run");
  });

  it("does not discard an open draft when the selection collapses", async () => {
    // Clicking anywhere in the body collapses the browser selection. The
    // composer belongs to the passage it was opened over, not to whatever is
    // selected at this instant, so a stray click must not throw away what the
    // author has typed.
    await mount({ threads: [] });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();
    fireEvent.change(opener(), { target: { value: "half a thought" } });

    window.getSelection()?.removeAllRanges();
    fireEvent.mouseUp(screen.getByLabelText("artifact body"));

    expect(screen.getByTestId("discussion-opening-fragment")).toHaveTextContent(
      "the first session",
    );
    expect(opener().value).toBe("half a thought");
  });

  it("DDS-FR-XQMF: keeps the opening composer of a passage while the column is hidden", async () => {
    // The column is hidden rather than unmounted, so a message part way
    // written is where the author left it when the column shows again.
    await mount({ threads: [] });
    const column = await openColumn();

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();
    fireEvent.change(opener(), { target: { value: "half a thought" } });

    fireEvent.click(discussionToggle());
    await waitFor(() => expect(column.hidden).toBe(true));
    fireEvent.click(discussionToggle());
    await waitFor(() => expect(column.hidden).toBe(false));

    expect(screen.getByTestId("discussion-opening-fragment")).toHaveTextContent(
      "the first session",
    );
    expect(opener().value).toBe("half a thought");
  });

  it("places the affordance at the end of the selection, not at a fixed corner", async () => {
    await mount({ threads: [] });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    const surface = document.querySelector(".editor") as HTMLElement;
    surface.getBoundingClientRect = () =>
      ({ top: 100, left: 20, bottom: 700, right: 620 }) as DOMRect;
    // A two-line selection: the button belongs at the end of the passage, so the
    // *last* rect is the one that decides.
    withSelectionRects([
      { bottom: 180, left: 300 },
      { bottom: 214, left: 40 },
    ]);

    selectInBody("the first session");
    const affordance = await screen.findByRole("button", { name: "Comment on selection" });
    // 214 − 100 + a small gap below the line; 40 − 20 across.
    expect(parseFloat(affordance.style.top)).toBeGreaterThan(114);
    expect(parseFloat(affordance.style.top)).toBeLessThan(126);
    expect(affordance.style.left).toBe("20px");
  });

  it("moves an open draft's anchor with the buffer", async () => {
    // CMT-FR-20 reaches the draft too. Its anchor belongs to no thread yet, so
    // nothing else tracks it — and an edit above the passage would otherwise
    // persist a thread pointing at whatever the old offsets now cover.
    const withFm = `---\nname: a\n---\n\n${BODY}`;
    const { backend } = await mount({
      threads: [],
      load: { body: withFm, checksum: "ck1" },
    });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();

    // Grow the frontmatter, which sits entirely before the anchored passage.
    const fm = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    fireEvent.change(fm, {
      target: { value: `${fm.value}\ndescription: a much longer line` },
    });

    fireEvent.change(opener(), { target: { value: "x" } });
    fireEvent.click(openerButton("Post"));
    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "open_discussion")).toBe(true),
    );

    const args = backend.calls.find((c) => c.cmd === "open_discussion")!.args as {
      fragmentTarget: FragmentRange;
    };
    // Read the document the anchor addresses rather than recomposing it here:
    // the raw source surface holds the buffer exactly (EDT-FR-17).
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    const live = (await screen.findByLabelText("Markdown source")) as HTMLTextAreaElement;
    expect(live.value).toContain("description: a much longer line");
    // The offsets address the passage in the document as it now stands, not as
    // it stood when the selection was made.
    expect(live.value.slice(args.fragmentTarget.start, args.fragmentTarget.end)).toBe(
      "the first session",
    );
  });

  it("does not let a second selection steal the open composer", async () => {
    // The composer belongs to the passage it was opened over. Re-reading the
    // selection here would silently re-point it at text the author never chose
    // while leaving what they had typed in place.
    await mount({ threads: [] });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();

    selectInBody("Steps to run");

    expect(screen.getByTestId("discussion-opening-fragment")).toHaveTextContent(
      "the first session",
    );
    // And no second affordance appears over the editing surface.
    const surface = within(document.querySelector(".editor") as HTMLElement);
    expect(surface.queryByRole("button", { name: "Comment on selection" })).toBeNull();
  });

  it("CMT-FR-02: keeps the opening composer of a passage in raw Markdown, where the column stands too", async () => {
    // The column stands in both editing modes, so the composer in it stays
    // with what the author typed.
    await mount({ threads: [] });
    const column = await openColumn();

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();
    fireEvent.change(opener(), { target: { value: "half a thought" } });

    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    expect(column).toBeVisible();
    expect(within(column).getByTestId("discussion-opening-fragment")).toHaveTextContent(
      "the first session",
    );

    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByLabelText("artifact body");
    expect(opener().value).toBe("half a thought");
  });

  it("reveals no affordance while no identity resolves", async () => {
    // CMT-FR-24: commenting is gated on an identity, so the affordance that
    // starts a thread is gated with it — offering a composer that can never
    // post would be worse than offering nothing.
    await mount({ threads: [], identityError: GITHUB_TOKEN_ERRORS.tokenMissing });
    const column = await openColumn();
    // The refusal has landed: the column states its reason.
    await waitFor(() =>
      expect(column.querySelector(".comment-rail__blocked")).not.toBeNull(),
    );

    selectInBody("the first session");
    expect(screen.queryByRole("button", { name: "Comment on selection" })).toBeNull();
  });

  it("CMT-FR-05, DDS-FR-QMBC: puts focus in the opening composer when it opens for a passage", async () => {
    // CMT-FR-05: the author is mid-thought; the affordance suppresses the
    // browser's default focus handling precisely so this can hold.
    await mount({ threads: [] });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    selectInBody("the first session");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();
    await waitFor(() => expect(opener()).toHaveFocus());
  });

  it("keeps the affordance with the passage as the body scrolls", async () => {
    // The position is in the scroll container's *content* coordinates, which is
    // the space an absolutely positioned child of it lives in — so the button
    // travels with the text rather than hovering over unrelated prose.
    await mount({ threads: [] });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    const surface = document.querySelector(".editor") as HTMLElement;
    surface.getBoundingClientRect = () => ({ top: 0, left: 0 }) as DOMRect;
    Object.defineProperty(surface, "scrollTop", { value: 200, configurable: true });
    withSelectionRects([{ bottom: 50, left: 0 }]);

    selectInBody("the first session");
    const affordance = await screen.findByRole("button", { name: "Comment on selection" });
    // 50 below the container's top, 200 already scrolled past, plus the gap.
    expect(parseFloat(affordance.style.top)).toBeGreaterThan(250);
    expect(parseFloat(affordance.style.top)).toBeLessThan(262);
  });

  it("keeps the affordance from overhanging the right edge of the surface", async () => {
    await mount({ threads: [] });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    const surface = document.querySelector(".editor") as HTMLElement;
    surface.getBoundingClientRect = () => ({ top: 0, left: 0 }) as DOMRect;
    Object.defineProperty(surface, "clientWidth", { value: 400, configurable: true });
    withSelectionRects([{ bottom: 50, left: 396 }]);

    selectInBody("the first session");
    const affordance = await screen.findByRole("button", { name: "Comment on selection" });
    // Its whole width has to fit, not merely its left edge.
    expect(parseFloat(affordance.style.left) + AFFORDANCE_WIDTH).toBeLessThanOrEqual(400);
  });
});

describe("CMT-FR-28: anchors are marked in the body", () => {
  it("marks each anchored passage and distinguishes the focused one", async () => {
    await mount({ threads: [makeThread()] });
    await openColumn("t1");

    await waitFor(() =>
      expect(document.querySelectorAll(".comment-anchor").length).toBeGreaterThan(0),
    );
    const marked = document.querySelector(".comment-anchor")!;
    expect(marked.getAttribute("data-comment-thread")).toBe("t1");
    expect(marked.classList.contains("comment-anchor--focused")).toBe(false);

    // Activating the highlight focuses its discussion in the column.
    fireEvent.click(marked);
    await waitFor(() => expect(screen.getByTestId("comment-thread-t1")).toHaveFocus());
    await waitFor(() =>
      expect(
        document.querySelector(".comment-anchor")?.classList.contains("comment-anchor--focused"),
      ).toBe(true),
    );
  });

  it("does not mark a resolved thread's passage", async () => {
    // CMT-FR-17: a resolved thread is no longer under discussion; the disclosure
    // is where it lives.
    await mount({ threads: [makeThread({ resolved: true })] });
    await openColumn();
    await waitFor(() =>
      expect(document.querySelectorAll(".comment-anchor")).toHaveLength(0),
    );
  });
});

describe("CMT-FR-06: a selection that spans blocks", () => {
  const STRUCTURED =
    "## User stories\n\n- As an author, I want a WYSIWYG editor.\n- As a reviewer, I want comments.\n";

  it("anchors a selection running from a heading into the text under it", async () => {
    // The rendered body carries neither the `##` of the heading nor the `- ` of
    // the list item, so the selection appears in the source nowhere at all.
    // Refusing it would turn away most of the selections anyone makes in a
    // structured document — which is what the artifacts here are.
    const { backend } = await mount({
      threads: [],
      load: { body: STRUCTURED, checksum: "ck1" },
    });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    selectInBody("User stories\nAs an author, I want a WYSIWYG editor.");
    fireEvent.click(await screen.findByRole("button", { name: "Comment on selection" }));
    await openingQuote();
    fireEvent.change(opener(), { target: { value: "x" } });
    fireEvent.click(openerButton("Post"));

    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "open_discussion")).toBe(true),
    );
    const { fragmentTarget: anchor } = backend.calls.find(
      (c) => c.cmd === "open_discussion",
    )!.args as { fragmentTarget: FragmentRange };
    // Pinned exactly: it runs from the selected heading text to the end of the
    // list item, carrying the syntax between them, and stops there.
    expect(anchor.quote).toBe(
      "User stories\n\n- As an author, I want a WYSIWYG editor.",
    );
    expect(anchor.start).toBe(STRUCTURED.indexOf("User stories"));
    expect(STRUCTURED.slice(anchor.start, anchor.end)).toBe(anchor.quote);
  });

  it("still refuses a selection whose text is nowhere in the source", async () => {
    // CMT-FR-06's floor: anchoring to the wrong passage is worse than not
    // anchoring at all.
    await mount({ threads: [], load: { body: STRUCTURED, checksum: "ck1" } });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    selectInBody("nothing in this document says this");
    expect(screen.queryByRole("button", { name: "Comment on selection" })).toBeNull();
  });

  it("refuses a selection whose last line lies nowhere after its first", async () => {
    // Falling back to the first line alone would hand the author a thread on a
    // strictly smaller, different passage than the one they selected, with
    // nothing to tell them so.
    const source = "## Alpha\n\nshared line\n\n## Beta\n\n- tail item\n";
    await mount({ threads: [], load: { body: source, checksum: "ck1" } });
    await waitFor(() => expect(discussionToggle()).toBeInTheDocument());

    // `shared line` occurs only above `Beta`, so there is no range from one to
    // the other.
    selectInBody("Beta\nshared line");
    expect(screen.queryByRole("button", { name: "Comment on selection" })).toBeNull();
  });
});
