import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import {
  activeIn,
  awaitDiff,
  blockWords,
  diffLine,
  findDiffLine,
  makeWireBackend,
  renderDiff,
  revs,
  richBlockEl,
  richBlocks,
  richBody,
  richGlyph,
  richTag,
  target,
  toolbarGroup,
} from "../test/diffViewFixtures";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
let listeners: Record<string, (event: { payload: unknown }) => void> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: unknown }) => void) => {
    listeners[event] = cb;
    return unlistenMock;
  }),
}));

const wireBackend = makeWireBackend(invokeMock);

const callsTo = (name: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === name);

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
  resetDiffModes();
  resetAppPreferencesCache();
  wireBackend();
});

afterEach(cleanup);

/**
 * Rich marks a changed block as a whole *and* the words within it that differ.
 * The block is still rendered unbroken, but DFV-FR-19's "no marking is applied
 * inside a block" no longer holds and needs amending.
 */
describe("Rendering group (DFV-FR-16 .. DFV-FR-22, less FR-19's inner clause)", () => {
  const MD_OLD = "# Title\n\nA paragraph that stays put.\n\n- kept item\n- old item\n";
  const MD_NEW = "# Title\n\nA paragraph that stays put.\n\n- kept item\n- new item\n";

  it("shows Markdown syntax as literal text in Source and as formatting in Rich (DFV-FR-16, DFV-FR-17)", async () => {
    wireBackend({
      revisions: revs("## Was\n", "## Now\n"),
    });
    renderDiff(target("specifications/ui/DFV.md"));

    expect(await findDiffLine("## Now")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("radio", { name: "Rich" }));

    const heading = await screen.findByRole("heading", { name: "Now" });
    expect(heading.tagName).toBe("H2");
    expect(diffLine("## Now")).toBeUndefined();
  });

  it("keeps both rendering toggles present but disabled for a non-Markdown file (DFV-FR-18, DFV-FR-23)", async () => {
    wireBackend({ prefs: { diffRenderingMode: "rich" } });
    renderDiff(target("src/components/Library.tsx"));
    await awaitDiff();

    const toggles = within(toolbarGroup("Diff rendering")).getAllByRole("radio");
    expect(toggles).toHaveLength(2);
    for (const toggle of toggles) expect(toggle).toBeDisabled();
    expect(activeIn("Diff rendering")).toEqual(["Source"]);
    expect(screen.getByText(/Rich rendering doesn’t apply/)).toBeInTheDocument();
    // The disablement is display-only: nothing was written to the stored mode.
    expect(callsTo("save_app_preferences")).toHaveLength(0);
  });

  it("opens a Markdown file rich when that is the stored mode (DFV-FR-18 / DFV-FR-23)", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: { old: MD_OLD, new: MD_NEW, isBinary: false },
    });
    renderDiff(target("notes.md"));

    expect(await screen.findByRole("heading", { name: "Title" })).toBeInTheDocument();
    expect(activeIn("Diff rendering")).toEqual(["Rich"]);
  });

  it("typesets the rendered document with the Editor's own document styles", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: { old: MD_OLD, new: MD_NEW, isBinary: false },
    });
    renderDiff(target("notes.md"));

    const view = await screen.findByTestId("diff-rich-unified");
    // `doc` is the class the Editor puts on its ProseMirror node. Sharing it is
    // what makes a Spec read here and the same Spec open in the Editor one
    // document rather than two that merely resemble each other.
    for (const body of view.querySelectorAll(".diff-block__body")) {
      expect(body.classList.contains("doc")).toBe(true);
    }
    // And the elements are the ones those rules style, not a private set.
    expect(view.querySelector("h1")).toBeTruthy();
    expect(view.querySelector("ul li")).toBeTruthy();
    expect(view.querySelector("p")).toBeTruthy();
  });

  it("renders the one-column rich modes on the Editor's page and the split view beside it", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: { old: MD_OLD, new: MD_NEW, isBinary: false },
    });
    renderDiff(target("notes.md"));

    expect(await screen.findByTestId("diff-rich-unified")).toHaveClass(
      "diff-rich--page",
    );

    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));
    // DFV-FR-38: each revision occupies a page of its own here, drawn behind
    // the rows rather than by this class — two columns cannot each carry the
    // one-column page's measure and padding, and a border per row would rule
    // the file into strips. The pages themselves are asserted against the
    // stylesheet in `src/test/style-invariants.test.ts`.
    const split = await screen.findByTestId("diff-rich-side-by-side");
    expect(split).toHaveClass("diff-rich--split");
    expect(split).not.toHaveClass("diff-rich--page");

    await userEvent.click(screen.getByRole("radio", { name: "Final" }));
    expect(await screen.findByTestId("diff-rich-final")).toHaveClass(
      "diff-rich--page",
    );
  });

  it("DFV-FR-38: the body is the page's field for Rich and nothing's field for Source", async () => {
    // The field behind a Diff tab's page is declared only for the rendering
    // that HAS a page — Source takes none, because a gutter aligned to a
    // bounded measure would be reading the file as a document it is
    // deliberately not rendering. The stylesheet hangs off this attribute, so
    // dropping it leaves those rules selecting nothing.
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: { old: MD_OLD, new: MD_NEW, isBinary: false },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");

    const body = document.querySelector(".diff-view__body") as HTMLElement;
    expect(body).toHaveAttribute("data-page", "on");

    await userEvent.click(screen.getByRole("radio", { name: "Source" }));
    await waitFor(() =>
      expect(screen.queryByTestId("diff-rich-unified")).not.toBeInTheDocument(),
    );
    expect(body).toHaveAttribute("data-page", "off");
  });

  it("renders heading levels the document styles do not restate", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: { old: "#### Was\n", new: "#### Now\n", isBinary: false },
    });
    renderDiff(target("notes.md"));

    // `.doc` styles h1..h3 explicitly; an h4 inherits, which is exactly why
    // `.diff-rich` has to override the source modes' chrome metrics.
    const heading = await screen.findByRole("heading", { name: "Now" });
    expect(heading.tagName).toBe("H4");
  });

  it("marks a changed paragraph as one whole block, unbroken, and the words within it", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: {
        old: "One sentence. Two sentences. Three sentences.\n",
        new: "One sentence. Two revised sentences. Three sentences.\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));

    const text = "One sentence. Two revised sentences. Three sentences.";
    await waitFor(() => expect(richBlockEl(text)).toBeTruthy());
    const block = richBlockEl(text)!;

    // The block carries the change as a whole…
    expect(block).toHaveAttribute("data-mark", "added");
    // …and the paragraph is still rendered unbroken — one `p`, not a run of
    // separately-marked fragments, and no nested block.
    expect(block.querySelectorAll(".diff-block")).toHaveLength(0);
    expect(richTag(text)).toBe("p");
    expect(richBody(block).textContent).toBe(text);
    // Within it, the word that actually changed is what is marked.
    expect(blockWords(text)).toEqual(["revised "]);
  });

  it("marks the changed words on both sides of a replaced block in Rich + Unified", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: {
        old: "The quick brown fox jumps over the lazy dog.\n",
        new: "The quick red fox jumps over the lazy dog.\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");

    // Each side marks in the terms of its own revision.
    expect(blockWords("The quick brown fox jumps over the lazy dog.")).toEqual([
      "brown",
    ]);
    expect(blockWords("The quick red fox jumps over the lazy dog.")).toEqual([
      "red",
    ]);
  });

  it("marks the changed words in Rich + Side-by-side", async () => {
    wireBackend({
      prefs: {
        diffRenderingMode: "rich",
        diffVisualizationMode: "side_by_side",
      },
      revisions: {
        old: "- alpha beta gamma\n",
        new: "- alpha delta gamma\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-side-by-side");

    expect(blockWords("alpha beta gamma")).toEqual(["beta"]);
    expect(blockWords("alpha delta gamma")).toEqual(["delta"]);
  });

  it("marks a range that spans an emphasis span and the text after it", async () => {
    // The only case that exercises `Inline`'s cursor accumulation ACROSS a span
    // boundary: a cursor that failed to advance past `<strong>` would mark the
    // wrong characters, and every single-span test would still pass.
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: {
        old: "the **quick** brown fox\n",
        new: "the **slow** red fox\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");

    const block = richBlockEl("the slow red fox")!;
    // The emphasised word and the word after it are both inside the range, so
    // the marking has to split across the boundary.
    expect(blockWords("the slow red fox").join("")).toContain("slow");
    expect(blockWords("the slow red fox").join("")).toContain("red");
    const strong = block.querySelector("strong")!;
    expect(strong.textContent).toBe("slow");
    expect(strong.querySelectorAll("mark")).toHaveLength(1);
    // And the unchanged tail is outside every mark.
    const fox = Array.from(block.querySelectorAll("mark")).map(
      (m) => m.textContent,
    );
    expect(fox.join("")).not.toContain("fox");
  });

  it("marks no words when only a link's target changed", async () => {
    // The marking is computed over the rendered text, so a change the reader
    // cannot see produces no word marking — only the block's own. Read in
    // Final, where a replacement renders once: in Unified both halves render,
    // and here they render the same text.
    wireBackend({
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
      revisions: {
        old: "See [the docs](./a.md) for more.\n",
        new: "See [the docs](./b.md) for more.\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");

    expect(richBlockEl("See the docs for more.")).toHaveAttribute(
      "data-mark",
      "added",
    );
    expect(blockWords("See the docs for more.")).toEqual([]);
    // The target really did change — the block is marked, just not word-marked.
    // The editable rendering is the Editor's own, which renders a link as one
    // (`href`); the static rendering, which navigates nowhere, names the target
    // in a tooltip instead (DFV-FR-06).
    const link = richBlockEl("See the docs for more.")!.querySelector("a")!;
    expect(link.getAttribute("href") ?? link.getAttribute("title")).toBe(
      "./b.md",
    );
  });

  it("marks no words when only inline markup was added", async () => {
    // Same basis, from the other direction: emphasising a word changes the
    // source but not a single rendered character.
    wireBackend({
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
      revisions: {
        old: "A plain word here.\n",
        new: "A **plain** word here.\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");

    expect(blockWords("A plain word here.")).toEqual([]);
    // The emphasis really was added, and it is the block that is marked.
    const block = richBlockEl("A plain word here.")!;
    expect(block.querySelector("strong")!.textContent).toBe("plain");
    expect(block).toHaveAttribute("data-mark", "added");
  });

  it("leaves a table row to its whole-block marking", async () => {
    // A row's cells are already a fine-grained unit, and there is no single
    // string whose offsets would address them.
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: {
        old: "| a | b |\n|---|---|\n| 1 | 2 |\n",
        new: "| a | b |\n|---|---|\n| 1 | 9 |\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));
    const view = await screen.findByTestId("diff-rich-unified");

    const changedRow = view.querySelector<HTMLElement>(
      '[data-kind="table-row"][data-mark="added"]',
    )!;
    expect(changedRow.textContent).toContain("9");
    expect(changedRow.querySelectorAll("mark")).toHaveLength(0);
    // The header row did not change, and carries no marking of its own — the
    // `|---|---|` between them is structure rather than a row at all.
    const rows = richBlocks(view).filter(
      (b) => b.getAttribute("data-kind") === "table-row",
    );
    expect(rows[0]).toHaveAttribute("data-mark", "none");
  });

  it("marks a heading whose level changed without marking its words", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
      revisions: { old: "## Title\n", new: "### Title\n", isBinary: false },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");

    const block = richBlockEl("Title")!;
    expect(richTag("Title")).toBe("h3");
    expect(block).toHaveAttribute("data-mark", "added");
    // The rendered text is identical, so nothing inside it is marked.
    expect(blockWords("Title")).toEqual([]);
  });

  it("keeps an emoji whole inside a rich block", async () => {
    // `splitByRanges` slices a second time, against `Inline`'s cursor — a
    // boundary landing inside a surrogate pair would render two broken glyphs.
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: { old: "status 👍 ok\n", new: "status 👎 ok\n", isBinary: false },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");

    for (const mark of blockWords("status 👎 ok")) {
      expect(mark).not.toMatch(/[\uD800-\uDBFF]$/);
      expect(mark).not.toMatch(/^[\uDC00-\uDFFF]/);
    }
    expect(blockWords("status 👎 ok").join("")).toContain("👎");
  });

  it("distinguishes an added block from a removed one without colour", async () => {
    // Final puts added and removed blocks in one single-column document, so
    // the block marker is what separates them for a colour-blind reader.
    wireBackend({
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
      revisions: {
        old: "kept para\n\nold para\n\ndropped para\n",
        new: "kept para\n\nnew para\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");

    expect(richGlyph("new para")).toBe("+");
    expect(richGlyph("dropped para")).toBe("−");
    expect(richGlyph("kept para")).toBe("");
  });

  it("keeps DFV-FR-15's deleted state ahead of Rich + Final", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
      revisions: { old: "# Title\n\nbody\n", new: null, isBinary: false },
    });
    renderDiff(target("old-notes.md"));

    expect(
      await screen.findByText(/does not exist in the new revision/),
    ).toBeInTheDocument();
    // Not the whole old document rendered as removed blocks.
    expect(screen.queryByTestId("diff-rich-final")).toBeNull();
  });

  it("marks the right words through inline markup rather than through its syntax", async () => {
    // The marking is computed over the *rendered* text. Computing it over the
    // source would put the range at an offset that no longer exists once
    // `**bold**` has lost four characters, and the wrong word would light up.
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: {
        old: "A **bold** word and a plain tail.\n",
        new: "A **bold** word and a different tail.\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");

    const block = richBlockEl("A bold word and a different tail.")!;
    expect(blockWords("A bold word and a different tail.")).toEqual([
      "different",
    ]);
    // And the emphasis itself survives the segmentation unmarked.
    const strong = block.querySelector("strong")!;
    expect(strong.textContent).toBe("bold");
    expect(strong.querySelectorAll("mark")).toHaveLength(0);
  });

  it("marks the changed line inside a replaced code block", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: {
        old: "```ts\nconst a = 1;\nconst b = 2;\n```\n",
        new: "```ts\nconst a = 1;\nconst b = 99;\n```\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");

    // A fence is literal, so its own text is what the marking is computed over.
    expect(blockWords("const a = 1;\nconst b = 99;")).toEqual(["99"]);
  });

  it("marks no words inside a block that has no counterpart", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: {
        old: "# Title\n",
        new: "# Title\n\nA wholly new paragraph.\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");

    // The block is already marked whole; there is nothing to compare it to.
    expect(richBlockEl("A wholly new paragraph.")).toHaveAttribute(
      "data-mark",
      "added",
    );
    expect(blockWords("A wholly new paragraph.")).toEqual([]);
  });

  it("renders a replaced block immediately above its replacement in Rich + Unified (DFV-FR-20, DFV-FR-21, DFV-FR-13)", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich" },
      revisions: { old: MD_OLD, new: MD_NEW, isBinary: false },
    });
    renderDiff(target("notes.md"));

    const view = await screen.findByTestId("diff-rich-unified");
    const marked = richBlocks(view).map((b) => [
      b.getAttribute("data-mark"),
      richBody(b).textContent!.trim(),
    ]);

    const removedAt = marked.findIndex(([mark]) => mark === "removed");
    expect(marked[removedAt][1]).toContain("old item");
    expect(marked[removedAt + 1][0]).toBe("added");
    expect(marked[removedAt + 1][1]).toContain("new item");
    // The surrounding document is there and unmarked.
    expect(marked.filter(([mark]) => mark === "none").length).toBeGreaterThan(0);
  });

  it("pairs blocks across two panes with filler in Rich + Side-by-side (DFV-FR-20, DFV-FR-21, DFV-FR-13)", async () => {
    wireBackend({
      prefs: {
        diffRenderingMode: "rich",
        diffVisualizationMode: "side_by_side",
      },
      revisions: {
        old: "# Title\n\n- kept\n",
        new: "# Title\n\n- kept\n- brand new\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));

    const view = await screen.findByTestId("diff-rich-side-by-side");
    const rows = Array.from(view.querySelectorAll(".diff-sbs__row"));
    const marks = rows.map((row) =>
      // A filler is a static block on either side; the target's own cell is the
      // run's document, whose block carries the mark (DFV-FR-47).
      Array.from(
        row.querySelectorAll<HTMLElement>(
          ".diff-block[data-mark], .diff-run [data-mark]",
        ),
      ).map((b) => b.getAttribute("data-mark")),
    );
    expect(marks[0]).toEqual(["none", "none"]); // the heading
    expect(marks[1]).toEqual(["none", "none"]); // the kept item
    // A block with no counterpart faces filler.
    expect(marks[2]).toEqual(["filler", "added"]);
    // And one scroller still governs both panes (DFV-FR-13).
    expect(view.querySelectorAll(".diff-sbs__scroll")).toHaveLength(1);
  });

  it("shows the outcome with its changes marked in Rich + Final", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
      revisions: { old: MD_OLD, new: MD_NEW, isBinary: false },
    });
    renderDiff(target("notes.md"));

    const view = await screen.findByTestId("diff-rich-final");
    // The replaced block shows only what replaced it — this is what separates
    // Final from Unified, which shows the pair.
    expect(richBlockEl("new item")).toHaveAttribute("data-mark", "added");
    expect(richBlockEl("old item")).toBeUndefined();
    // And the word that changed within it is marked.
    expect(blockWords("new item")).toEqual(["new"]);
    // The rest of the document is unmarked.
    expect(richBlockEl("Title")).toHaveAttribute("data-mark", "none");
    expect(view.querySelectorAll('[data-mark="added"]')).toHaveLength(1);
  });

  it("shows a block removed outright as removed in Rich + Final", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
      revisions: {
        old: "# Title\n\nkept paragraph\n\ndropped paragraph\n",
        new: "# Title\n\nkept paragraph\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));

    await screen.findByTestId("diff-rich-final");
    // Nothing replaced it, so the finished document has no trace of it — which
    // is exactly why it is shown.
    expect(richBlockEl("dropped paragraph")).toHaveAttribute(
      "data-mark",
      "removed",
    );
    expect(richBlockEl("kept paragraph")).toHaveAttribute("data-mark", "none");
  });
});
