import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import type {
  FileRevisions,
} from "../types";
import {
  awaitDiff,
  changedWords,
  diffLine,
  findDiffLine,
  gutters,
  lineEl,
  makeWireBackend,
  renderDiff,
  revs,
  richBlocks,
  target,
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

describe("Word-level marking inside a replacement", () => {
  const replacement = (oldLine: string, newLine: string): FileRevisions =>
    revs(`${oldLine}\n`, `${newLine}\n`);

  it("marks the words that changed within a replaced line in Unified", async () => {
    wireBackend({
      revisions: replacement(
        "  const total = compute(a, b);",
        "  const total = compute(a, b, c);",
      ),
    });
    renderDiff(target());
    await findDiffLine("  const total = compute(a, b, c);");

    // The line already carries its own tint; these are the words that are the
    // actual edit — and nothing else on the line is marked.
    expect(changedWords("  const total = compute(a, b, c);")).toEqual([", c"]);
    expect(changedWords("  const total = compute(a, b);")).toEqual([]);
  });

  it("marks the words that changed on both sides in Side-by-side", async () => {
    wireBackend({
      revisions: {
        old: "the quick brown fox\n",
        new: "the quick red fox\n",
        isBinary: false,
      },
    });
    renderDiff(target());
    await findDiffLine("the quick red fox");
    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));
    await screen.findByTestId("diff-side-by-side");

    expect(changedWords("the quick brown fox")).toEqual(["brown"]);
    expect(changedWords("the quick red fox")).toEqual(["red"]);
  });

  it("marks no words in a line that has no counterpart", async () => {
    // A wholly new line is already marked whole; repeating that at higher
    // contrast on every word says nothing more.
    wireBackend({ revisions: revs("", "a brand new line\n") });
    renderDiff(target());
    await findDiffLine("a brand new line");

    expect(changedWords("a brand new line")).toEqual([]);
  });

  it("marks the same words in Unified as in Side-by-side", async () => {
    // The hunk pairing and the side-by-side alignment are meant to be the same
    // rule, so a line must read the same way in either mode.
    wireBackend({
      revisions: revs("the quick brown fox\n", "the quick red fox\n"),
    });
    renderDiff(target());
    await findDiffLine("the quick red fox");
    const unified = changedWords("the quick red fox");

    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));
    await screen.findByTestId("diff-side-by-side");

    expect(changedWords("the quick red fox")).toEqual(unified);
  });

  it("pairs each removal with its own replacement when the runs are unequal", async () => {
    wireBackend({
      revisions: revs(
        "alpha one two\nbeta three four\ngamma five six\n",
        "alpha one ONE\n",
      ),
    });
    renderDiff(target());
    await findDiffLine("alpha one ONE");

    // Only the first removal has a counterpart; the surplus removals are
    // marked whole, not paired against a line that did not replace them.
    expect(changedWords("alpha one ONE")).toEqual(["ONE"]);
    expect(changedWords("alpha one two")).toEqual(["two"]);
    expect(changedWords("beta three four")).toEqual([]);
    expect(changedWords("gamma five six")).toEqual([]);
  });

  it("marks no words when a removal run is followed by context rather than additions", async () => {
    wireBackend({
      revisions: revs("removed outright\nkept as is\n", "kept as is\n"),
    });
    renderDiff(target());
    await findDiffLine("removed outright");

    expect(changedWords("removed outright")).toEqual([]);
    expect(changedWords("kept as is")).toEqual([]);
  });

  it("leaves a filler row inert and the alignment unchanged (DFV-FR-11)", async () => {
    wireBackend({
      revisions: {
        old: "keep\nthe quick brown fox\ntail\n",
        new: "keep\nthe quick red fox\nsurplus line\ntail\n",
        isBinary: false,
      },
    });
    renderDiff(target());
    await awaitDiff();
    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));

    const view = await screen.findByTestId("diff-side-by-side");
    const rows = Array.from(view.querySelectorAll(".diff-sbs__row"));
    // Marking adds no rows: one row per aligned pair, filler included.
    expect(rows).toHaveLength(4);

    const filler = view.querySelector('[data-kind="filler"]')!;
    expect(filler.textContent).toBe("·");
    expect(filler.querySelectorAll("mark")).toHaveLength(0);

    // And the row that does carry marking still pairs its own line numbers.
    const paired = rows[1].querySelectorAll(".diff-sbs__cell");
    expect(paired[0].querySelector(".diff-line__gutter")!.textContent).toBe("2");
    expect(paired[1].querySelector(".diff-line__gutter")!.textContent).toBe("2");
    expect(changedWords("the quick red fox")).toEqual(["red"]);
  });

  it("keeps a line's leading whitespace, which is what wrapping preserves", async () => {
    wireBackend({ revisions: revs("", "        deeply indented\n") });
    renderDiff(target());

    // `pre-wrap` is what makes indentation visible; a regression that trimmed
    // it or substituted non-breaking spaces would be invisible otherwise.
    const line = await findDiffLine("        deeply indented");
    expect(line.querySelector(".diff-line__text")!.textContent).toBe(
      "        deeply indented",
    );
    expect(line.textContent).not.toContain("\u00a0");
  });

  it("marks no words when the two lines are unrelated", async () => {
    wireBackend({
      revisions: replacement("const alpha = 1;", "throw new Error('other');"),
    });
    renderDiff(target());
    await findDiffLine("throw new Error('other');");

    expect(changedWords("throw new Error('other');")).toEqual([]);
    expect(changedWords("const alpha = 1;")).toEqual([]);
  });
});

describe("Unified visualization (DFV-FR-09 / DFV-FR-10)", () => {
  it("numbers every row in two columns and places a placeholder in the other (DFV-FR-09, DFV-FR-10)", async () => {
    renderDiff(target());
    await awaitDiff();

    // The header the tab DERIVED (DFV-FR-43) over the two revisions it holds,
    // rather than one the backend handed it.
    expect(lineEl("@@ -1,2 +1,2 @@")).toBeTruthy();

    const rowFor = (text: string) => diffLine(text)!;
    const numbers = (row: HTMLElement) => [
      row.querySelector(".diff-line__gutter--old")!.textContent,
      row.querySelector(".diff-line__gutter--new")!.textContent,
    ];

    // A context line carries a number in both columns.
    expect(numbers(rowFor("unchanged"))).toEqual(["1", "1"]);
    // A removed line: the old column only, and a NON-numeric placeholder in the
    // new — never blank of layout, or the rows would shift.
    expect(numbers(rowFor("was this"))).toEqual(["2", "·"]);
    expect(numbers(rowFor("is now this"))).toEqual(["·", "2"]);
    // And the marking is readable without colour.
    expect(rowFor("is now this").querySelector(".diff-line__sign")!.textContent).toBe("+");
    expect(rowFor("was this").querySelector(".diff-line__sign")!.textContent).toBe("−");
  });

  it("renders no part of the file outside a hunk (DFV-FR-09, DFV-FR-10)", async () => {
    renderDiff(target());
    await awaitDiff();

    // The hunk header plus its three lines, and nothing else: Unified renders
    // only the changed regions, so the rest of the file never appears.
    //
    // DFV-FR-44: what it does not render is still part of the target the tab
    // holds and writes, so the windowing is a choice about reading rather than
    // a truncation — which is why both revisions are read here, exactly once
    // each (DFV-FR-25), where the read-only tab used to need neither.
    expect(document.querySelectorAll(".diff-line")).toHaveLength(4);
    expect(callsTo("get_file_revisions")).toHaveLength(1);
    expect(callsTo("load_artifact_contents_by_id")).toHaveLength(1);
  });
});

describe("Side-by-side visualization (DFV-FR-11 .. DFV-FR-13)", () => {
  const wholeFile = (lines: number, changedAt: number) => {
    const old = Array.from({ length: lines }, (_, i) => `line ${i + 1}`);
    const next = [...old];
    next[changedAt - 1] = `line ${changedAt} edited`;
    return { old: old.join("\n") + "\n", new: next.join("\n") + "\n", isBinary: false };
  };

  it("shows both revisions in full rather than only the changed region (DFV-FR-11)", async () => {
    wireBackend({ revisions: wholeFile(40, 20) });
    renderDiff(target());
    await awaitDiff();
    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));

    const view = await screen.findByTestId("diff-side-by-side");
    const rows = view.querySelectorAll(".diff-sbs__row");
    expect(rows).toHaveLength(40);

    // Line 12 of the left pane sits opposite line 12 of the right.
    const row12 = rows[11];
    const cells = row12.querySelectorAll(".diff-sbs__cell");
    expect(cells[0].querySelector(".diff-line__gutter")!.textContent).toBe("12");
    expect(cells[1].querySelector(".diff-line__gutter")!.textContent).toBe("12");
  });

  it("marks each side in its own terms, fills the surplus, and numbers its own revision (DFV-FR-11, DFV-FR-12)", async () => {
    wireBackend({
      revisions: {
        old: "keep\nremoved line\ntail\n",
        new: "keep\nfirst added\nsecond added\ntail\n",
        isBinary: false,
      },
    });
    renderDiff(target());
    await awaitDiff();
    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));

    const view = await screen.findByTestId("diff-side-by-side");
    const rows = Array.from(view.querySelectorAll(".diff-sbs__row"));
    const kinds = rows.map((row) =>
      Array.from(row.querySelectorAll(".diff-sbs__cell")).map((c) =>
        c.getAttribute("data-kind"),
      ),
    );

    // keep | keep — unchanged, marked on neither side.
    expect(kinds[0]).toEqual(["context", "context"]);
    // the removed line faces the first addition: left marks what the change
    // acted on, right marks the outcome.
    expect(kinds[1]).toEqual(["del", "add"]);
    // the surplus addition faces an inert filler.
    expect(kinds[2]).toEqual(["filler", "add"]);
    expect(kinds[3]).toEqual(["context", "context"]);

    // Each gutter numbers its own revision: the tail is line 3 on the left and
    // line 4 on the right.
    const tail = rows[3].querySelectorAll(".diff-sbs__cell");
    expect(tail[0].querySelector(".diff-line__gutter")!.textContent).toBe("3");
    expect(tail[1].querySelector(".diff-line__gutter")!.textContent).toBe("4");
    // The filler carries no number and no content.
    const filler = rows[2].querySelectorAll(".diff-sbs__cell")[0];
    expect(filler.textContent).toBe("·");
  });

  it("gives the two panes one scroll position rather than two (DFV-FR-13)", async () => {
    wireBackend({ revisions: wholeFile(60, 5) });
    renderDiff(target());
    await awaitDiff();
    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));

    // The panes cannot be seen at different offsets because both halves of
    // every aligned row live inside ONE scroller. A regression to two
    // scrollers, or to a per-pane one, breaks this identity.
    const view = await screen.findByTestId("diff-side-by-side");
    const row = view.querySelectorAll(".diff-sbs__row")[30];
    const [left, right] = row.querySelectorAll(".diff-sbs__cell");
    const scroller = left.closest(".diff-sbs__scroll");
    expect(scroller).not.toBeNull();
    expect(right.closest(".diff-sbs__scroll")).toBe(scroller);
    expect(view.querySelectorAll(".diff-sbs__scroll")).toHaveLength(1);
  });
});

/**
 * Final shows the file as it will land *with its changes marked* — additions
 * and replacements marked and word-segmented, and a line removed outright shown
 * as removed, because a deletion is otherwise the one edit a finished file
 * cannot show. That is a deliberate departure from DFV-FR-14 and DFV-FR-10,
 * which say Final carries "no change marking of any kind"; the spec text needs
 * amending to match. DFV-FR-15's deleted state is unaffected and still wins.
 */
describe("Final visualization (DFV-FR-15; supersedes DFV-FR-14)", () => {
  it("renders the whole new revision with its changes marked, in one gutter column", async () => {
    wireBackend({
      revisions: { old: "gone\nkept\n", new: "kept\nfresh\n", isBinary: false },
    });
    renderDiff(target());
    await awaitDiff();
    await userEvent.click(screen.getByRole("radio", { name: "Final" }));

    // The file as it will land — added content marked as added…
    expect(await findDiffLine("fresh")).toHaveAttribute("data-kind", "add");
    expect(diffLine("kept")).toHaveAttribute("data-kind", "context");
    // …and a line that was removed with nothing in its place shown as removed,
    // because a deletion is the one edit a finished file cannot show by itself.
    expect(diffLine("gone")).toHaveAttribute("data-kind", "del");
    // Still no hunk headers, and still one gutter column of new-revision
    // numbers — a removed line has none, so it carries the placeholder.
    expect(lineEl("@@ -12,4 +12,5 @@")).toBeUndefined();
    expect(gutters(".diff-line__gutter")).toEqual(["·", "1", "2"]);
    expect(document.querySelectorAll(".diff-line__gutter--old")).toHaveLength(0);
  });

  it("shows only the outcome of a replacement, word-marked", async () => {
    wireBackend({
      revisions: {
        old: "the quick brown fox\n",
        new: "the quick red fox\n",
        isBinary: false,
      },
    });
    renderDiff(target());
    await awaitDiff();
    await userEvent.click(screen.getByRole("radio", { name: "Final" }));

    // The reader is looking at the finished file, not at the pair of revisions
    // that produced it — so the old line is not shown, and the word that
    // changed carries the marking instead.
    expect(await findDiffLine("the quick red fox")).toHaveAttribute(
      "data-kind",
      "add",
    );
    expect(diffLine("the quick brown fox")).toBeUndefined();
    expect(changedWords("the quick red fox")).toEqual(["red"]);
  });

  it("numbers the new revision continuously across a removed line", async () => {
    // A removed line sits between two kept ones: its gutter carries the
    // placeholder and the numbering on either side of it is unbroken.
    wireBackend({
      revisions: {
        old: "alpha\ngone\nbeta\n",
        new: "alpha\nbeta\n",
        isBinary: false,
      },
      prefs: { diffVisualizationMode: "final" },
    });
    renderDiff(target());
    await findDiffLine("alpha");

    expect(gutters(".diff-line__gutter")).toEqual(["1", "·", "2"]);
    expect(diffLine("gone")).toHaveAttribute("data-kind", "del");
    expect(
      diffLine("gone")!.querySelector(".diff-line__sign")!.textContent,
    ).toBe("−");
  });

  it("shows every surplus removal when more lines went than arrived", async () => {
    wireBackend({
      revisions: {
        old: "one alpha\ntwo beta\nthree gamma\n",
        new: "one omega\n",
        isBinary: false,
      },
      prefs: { diffVisualizationMode: "final" },
    });
    renderDiff(target());
    await findDiffLine("one omega");

    // One line replaced the first; the other two have no outcome to show, so
    // they are shown as what they are.
    expect(diffLine("one omega")).toHaveAttribute("data-kind", "add");
    expect(diffLine("two beta")).toHaveAttribute("data-kind", "del");
    expect(diffLine("three gamma")).toHaveAttribute("data-kind", "del");
    expect(diffLine("one alpha")).toBeUndefined();
  });

  it("reads a file the comparison adds as the document it is, unmarked", async () => {
    // Every line of it is new, so marking every line says nothing the tab's
    // own header does not already say.
    wireBackend({
      revisions: { old: null, new: "first\nsecond\n", isBinary: false },
      prefs: { diffVisualizationMode: "final" },
    });
    renderDiff(target());

    expect(await findDiffLine("first")).toHaveAttribute("data-kind", "context");
    expect(diffLine("second")).toHaveAttribute("data-kind", "context");
    expect(gutters(".diff-line__gutter")).toEqual(["1", "2"]);
    expect(document.querySelectorAll("mark")).toHaveLength(0);
    // The other modes still show it as the addition it is.
    await userEvent.click(screen.getByRole("radio", { name: "Unified" }));
    expect(await findDiffLine("first")).toHaveAttribute("data-kind", "add");
  });

  it("reads a wholly new Markdown file as the document it is, unmarked", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
      revisions: {
        old: null,
        new: "# Title\n\nA paragraph.\n\n- an item\n",
        isBinary: false,
      },
    });
    renderDiff(target("notes.md"));

    const view = await screen.findByTestId("diff-rich-final");
    const marks = richBlocks(view).map((b) => b.getAttribute("data-mark"));
    expect(marks.length).toBeGreaterThan(0);
    expect(marks.every((m) => m === "none")).toBe(true);
    expect(view.querySelectorAll("mark")).toHaveLength(0);
  });

  it("does not treat a file that was empty as a file that is new", async () => {
    // `""` is an old revision that exists and holds nothing; `null` is no old
    // revision at all. Only the second makes the file new, and the whole rule
    // hangs on that distinction.
    wireBackend({
      revisions: { old: "", new: "first line\n", isBinary: false },
      prefs: { diffVisualizationMode: "final" },
    });
    renderDiff(target());

    expect(await findDiffLine("first line")).toHaveAttribute("data-kind", "add");
  });

  it("still marks a file that was rewritten rather than added", async () => {
    // "Wholly new" is a file the comparison ADDS. A file that existed and was
    // rewritten still has an old revision to show the reader.
    wireBackend({
      revisions: { old: "was this\n", new: "is that\n", isBinary: false },
      prefs: { diffVisualizationMode: "final" },
    });
    renderDiff(target());

    expect(await findDiffLine("is that")).toHaveAttribute("data-kind", "add");
  });

  it("renders a file emptied but not deleted as all removals", async () => {
    // `""` is a file that exists and is empty; `null` is a file the comparison
    // deleted. Only the second gets the deleted state.
    wireBackend({
      revisions: { old: "was here\n", new: "", isBinary: false },
      prefs: { diffVisualizationMode: "final" },
    });
    renderDiff(target());

    expect(await findDiffLine("was here")).toHaveAttribute("data-kind", "del");
    expect(
      screen.queryByText(/does not exist in the new revision/),
    ).not.toBeInTheDocument();
  });

  it("says the file does not exist in the new revision, and still shows removals in Unified (DFV-FR-15)", async () => {
    wireBackend({ revisions: revs("first\nsecond\n", null) });
    renderDiff(target("old-notes.md"));
    await findDiffLine("first");
    await userEvent.click(screen.getByRole("radio", { name: "Final" }));

    expect(
      await screen.findByText(/does not exist in the new revision/),
    ).toBeInTheDocument();
    expect(diffLine("first")).toBeUndefined();

    await userEvent.click(screen.getByRole("radio", { name: "Unified" }));
    expect(await findDiffLine("first")).toHaveAttribute("data-kind", "del");
  });
});
