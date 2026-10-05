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

import { LIVE_RUN_LIMIT } from "./DiffRichTarget";
import { sealBurst } from "../state/editHistory";
import { resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import {
  awaitDiff,
  diffLine,
  lineEl,
  makeWireBackend,
  renderDiff,
  revs,
  richBlockEl,
  target,
  targetCell,
  targetCells,
  typeInto,
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

// ---------------------------------------------------------------------------
// DFV-FR-41, DFV-FR-43, DFV-FR-44, DFV-FR-51 .. GTC-FR-17, EDT-FR-40 — the editable target
// ---------------------------------------------------------------------------

describe("the editable target (DFV-FR-41 .. DFV-FR-50)", () => {
  const FILE = "one\ntwo\nthree\n";

  it("DFV-FR-41, DFV-FR-43, DFV-FR-44, DFV-FR-51 edits an added row and a context row, and writes the whole file", async () => {
    wireBackend({ revisions: revs("one\nTWO\nthree\n", FILE) });
    const { sessions } = renderDiff(target("notes.md"));
    await awaitDiff();

    // The added row carries target content and takes the edit.
    typeInto(targetCell("two"), "two corrected");
    expect(sessions.get("src/components/Library.tsx")).toBeUndefined();
    expect(sessions.get("notes.md")?.buffer).toBe("one\ntwo corrected\nthree\n");
    // DFV-FR-51: the tab reports the target as pending.
    expect(screen.getByRole("status")).toHaveTextContent(/Unsaved changes/);

    // DFV-FR-44: a CONTEXT row is target content too, and editing it edits the
    // position it occupies in the whole file — including the lines the hunk
    // does not render.
    typeInto(targetCell("three"), "three corrected");
    expect(sessions.get("notes.md")?.buffer).toBe(
      "one\ntwo corrected\nthree corrected\n",
    );
  });

  it("DFV-FR-41, DFV-FR-45, DFV-FR-13 edits the right pane alone in Side-by-side", async () => {
    wireBackend({ revisions: revs("one\nTWO\nthree\n", FILE) });
    const { sessions } = renderDiff(target("notes.md"));
    await awaitDiff();
    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));
    const view = await screen.findByTestId("diff-side-by-side");

    // DFV-FR-45: every editable host is in the right pane; the left pane and the
    // filler rows accept nothing.
    const cells = Array.from(view.querySelectorAll(".diff-sbs__row"));
    for (const row of cells) {
      const [left, right] = Array.from(
        row.querySelectorAll<HTMLElement>(".diff-sbs__cell"),
      );
      expect(left.querySelector("[contenteditable]")).toBeNull();
      const editable = right.querySelector("[contenteditable]");
      if (right.dataset.kind === "filler") expect(editable).toBeNull();
      else expect(editable).not.toBeNull();
    }

    typeInto(targetCell("two"), "two edited");
    expect(sessions.get("notes.md")?.buffer).toBe("one\ntwo edited\nthree\n");
    // The left pane is byte-for-byte as it was.
    expect(lineEl("TWO")).toBeTruthy();
  });

  it("DFV-FR-46, DFV-FR-41, DFV-FR-14 edits the outcome in Final and never the removed row", async () => {
    wireBackend({
      revisions: revs("one\ngone\ntwo\n", "one\ntwo\n"),
      prefs: { diffVisualizationMode: "final" },
    });
    const { sessions } = renderDiff(target("notes.md"));
    await awaitDiff();

    // DFV-FR-46: the removed row is the original's evidence for a deletion and
    // takes no caret; the rows around it are the target and do.
    expect(diffLine("gone")).toHaveAttribute("data-kind", "del");
    expect(diffLine("gone")!.querySelector("[contenteditable]")).toBeNull();

    typeInto(targetCell("two"), "two edited");
    expect(sessions.get("notes.md")?.buffer).toBe("one\ntwo edited\n");
  });

  it("DFV-FR-52, DFV-FR-25, DFV-FR-43 keeps the edited target across all six mode combinations", async () => {
    wireBackend({ revisions: revs("one\nTWO\nthree\n", FILE) });
    const { sessions } = renderDiff(target("notes.md"));
    await awaitDiff();
    typeInto(targetCell("two"), "two edited");

    for (const visualization of ["Side-by-side", "Final", "Unified"]) {
      for (const rendering of ["Rich", "Source"]) {
        await userEvent.click(screen.getByRole("radio", { name: visualization }));
        await userEvent.click(screen.getByRole("radio", { name: rendering }));
        // DFV-FR-52: every combination shows the EDITED target ON SCREEN rather
        // than the pre-edit one. Asserted against the rendered document, not
        // against the store the test itself wrote to — a store read holds
        // whether the mode rendered the edit, rendered nothing, or fell through
        // to an inline error state.
        await waitFor(() =>
          expect(document.body.textContent).toContain("two edited"),
        );
        expect(targetCells().length).toBeGreaterThan(0);
        expect(sessions.get("notes.md")?.dirty).toBe(true);
      }
    }
    // The switch is a re-derivation from the two revisions already held;
    // neither was re-read.
    expect(callsTo("get_file_revisions")).toHaveLength(1);
    expect(callsTo("load_artifact_contents_by_id")).toHaveLength(1);
  });

  it("DFV-FR-47, DFV-FR-41, DFV-FR-19, DFV-FR-20 edits a rich target with the Editor's own surface", async () => {
    // DFV-FR-47: the requirement is about WHICH editor, so the test is about
    // whether the Editor's surface is what the target IS — one ProseMirror
    // instance over the run of blocks it renders, seeded from those very lines
    // and writing back through the same serialisation.
    const OLD = "# Title\n\nA paragraph that stays put.\n\n- old item\n";
    const NEW = "# Title\n\nA paragraph that stays put.\n\n- new item\n";
    wireBackend({ revisions: revs(OLD, NEW), prefs: { diffRenderingMode: "rich" } });
    const { sessions } = renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");

    // The removed block rendered above its replacement takes no caret and is
    // announced read-only (DFV-FR-20, DFV-FR-41).
    const removed = richBlockEl("old item")!;
    expect(removed).toHaveAttribute("data-mark", "removed");
    expect(removed.closest(".diff-run")).toBeNull();
    expect(removed).toHaveAttribute("aria-readonly", "true");

    // The target is live throughout — two runs, because the removed block cuts
    // the document in two, and nothing to activate in either.
    const runs = Array.from(
      document.querySelectorAll<HTMLElement>(".diff-run[data-editing='true']"),
    );
    expect(runs).toHaveLength(2);
    expect(document.querySelectorAll(".ProseMirror")).toHaveLength(2);
    for (const run of runs) {
      expect(
        run.querySelector(".ProseMirror")!.getAttribute("aria-label"),
      ).toContain("target revision");
    }

    // The block that replaced the removed one is a node of the second run's own
    // document, carrying its marking as a decoration rather than as a wrapper.
    const added = richBlockEl("new item")!;
    expect(added).toHaveAttribute("data-mark", "added");
    expect(added.closest(".diff-run")).toBe(runs[1]);

    // The first run is the heading and the paragraph as one document, so the
    // caret crosses from one to the other — and an edit spanning both is one
    // edit to exactly the lines they occupy, with the list item below untouched.
    const first = runs[0].querySelector<HTMLElement>(".ProseMirror")!;
    expect(first.textContent).toContain("Title");
    expect(first.textContent).toContain("A paragraph that stays put.");
    await act(async () => {
      fireEvent.focusIn(first);
      fireEvent.keyDown(first, { key: "a", ctrlKey: true });
      fireEvent.keyDown(first, { key: "Backspace" });
    });
    expect(sessions.get("notes.md")?.buffer).toBe("\n- new item\n");
    expect(sessions.get("notes.md")?.dirty).toBe(true);
    // The surface stayed the one it was rather than being rebuilt under the
    // edit — the churn that would take the caret with it. That is what the hold
    // of DFV-FR-43 buys: an edit that dropped two blocks would otherwise
    // re-group the runs mid-keystroke and unmount the document being typed in.
    expect(runs[0].querySelector(".ProseMirror")).toBe(first);
    expect(document.querySelectorAll(".ProseMirror")).toHaveLength(2);

    // And letting the caret go re-derives at once, against the same original:
    // the two blocks the edit dropped now read as removed rather than as target
    // content still standing.
    await act(async () => {
      fireEvent.focusOut(first);
    });
    await waitFor(() =>
      expect(richBlockEl("A paragraph that stays put.")).toHaveAttribute(
        "data-mark",
        "removed",
      ),
    );
    expect(richBlockEl("A paragraph that stays put.")!.closest(".diff-run")).toBeNull();
  });

  it("writes nothing to the target merely by rendering it rich", async () => {
    // The regression this exists for: Tiptap announces an `update` for
    // transactions that changed no document — turning the surface editable is
    // one, and so is the meta transaction that refreshes the marking. Treating
    // those as edits made every run write its own serialisation back over its
    // own lines the moment it mounted, and with one run per block (as
    // Side-by-side has) each write moved the lines the next run addressed: the
    // target was rewritten, block by block, before the author touched it.
    // The paragraph is hard-wrapped, which is what makes the bug visible: the
    // round trip joins it into one line, so a spurious write shortens the file
    // and every run below it then addresses the wrong lines.
    const OLD = "# Title\n\nold para that runs on\nover two lines\n\n- kept\n";
    const NEW = "# Title\n\nnew para that runs on\nover two lines\n\n- kept\n";
    wireBackend({ revisions: revs(OLD, NEW), prefs: { diffRenderingMode: "rich" } });
    const { sessions } = renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");
    await waitFor(() =>
      expect(document.querySelectorAll(".ProseMirror").length).toBeGreaterThan(0),
    );

    for (const visualization of ["Side-by-side", "Final", "Unified"]) {
      await userEvent.click(screen.getByRole("radio", { name: visualization }));
      await waitFor(() =>
        expect(document.querySelectorAll(".ProseMirror").length).toBeGreaterThan(0),
      );
      expect(sessions.get("notes.md")?.buffer).toBe(NEW);
      expect(sessions.get("notes.md")?.dirty).toBe(false);
    }
    expect(callsTo("save_artifact_contents")).toHaveLength(0);
    expect(screen.getByRole("status").textContent).toBe("Saved");
  });

  it("addresses only its own lines, over an edit that changed how many there are", async () => {
    // DFV-FR-44: a run rewrites the lines it occupies and carries the rest of
    // the file through. The second edit is the one that matters: it is made in
    // a *different* run, whose own lines moved when the first edit dropped
    // three of them. Letting the caret go re-derives (DFV-FR-43), which is what
    // refreshes the ranges — without it the second run would still address the
    // lines it held before and the edit would land nowhere.
    const OLD = "# Title\n\nfirst\n\ndropped\n\n- tail\n";
    const NEW = "# Title\n\nfirst\n\n- tail\n";
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
    });
    const { sessions } = renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");
    // The block dropped outright sits between them, so the target is two runs.
    const runs = () =>
      Array.from(
        document.querySelectorAll<HTMLElement>(".diff-run[data-editing]"),
      );
    expect(runs()).toHaveLength(2);

    const first = runs()[0].querySelector<HTMLElement>(".ProseMirror")!;
    await act(async () => {
      fireEvent.focusIn(first);
      fireEvent.keyDown(first, { key: "a", ctrlKey: true });
      fireEvent.keyDown(first, { key: "Backspace" });
    });
    expect(sessions.get("notes.md")?.buffer).toBe("\n- tail\n");

    await act(async () => {
      fireEvent.focusOut(first);
    });
    await waitFor(() => expect(runs().length).toBeGreaterThan(0));
    const second = runs()
      .map((r) => r.querySelector<HTMLElement>(".ProseMirror")!)
      .find((s) => s.textContent?.includes("tail"))!;
    await act(async () => {
      fireEvent.focusIn(second);
      fireEvent.keyDown(second, { key: "a", ctrlKey: true });
      fireEvent.keyDown(second, { key: "Backspace" });
    });

    // Addressed line 1, where the list now is — not line 4, where it was when
    // the run mounted. A stale range would have clamped past the end and
    // deleted nothing.
    expect(sessions.get("notes.md")?.buffer).toBe("");
  });

  it("re-seeds the rich surface when undo replaces the target under the caret", async () => {
    // EDT-FR-22 / DFV-FR-50: the history is the artifact's, and a traversal
    // replaces the whole target rather than editing it. With the caret in a run
    // the derivation is held, so unless the traversal adopts the document it
    // just restored — and adopts *that* document rather than the one the last
    // render saw — the author presses undo at a surface that never changes.
    const OLD = "# Title\n\nbefore\n";
    const NEW = "# Title\n\nafter\n";
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
    });
    const { sessions } = renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");
    const run = document.querySelector<HTMLElement>(".diff-run[data-editing]")!;
    const surface = run.querySelector<HTMLElement>(".ProseMirror")!;

    await act(async () => {
      fireEvent.focusIn(surface);
      fireEvent.keyDown(surface, { key: "a", ctrlKey: true });
      fireEvent.keyDown(surface, { key: "Backspace" });
    });
    expect(sessions.get("notes.md")?.buffer).toBe("");

    await act(async () => {
      sealBurst(sessions.get("notes.md")!.history);
      fireEvent.keyDown(surface, { key: "z", metaKey: true });
    });

    expect(sessions.get("notes.md")?.buffer).toBe(NEW);
    // On screen, not merely in the store: a surface holds the restored
    // document again.
    await waitFor(() =>
      expect(document.querySelector(".diff-run")?.textContent).toContain("after"),
    );
  });

  it("re-marks in place as the comparison catches up, on the surface already there", async () => {
    // DFV-FR-43: the marking follows the target. While the caret is in a run it
    // is carried by decorations that map through the author's own edits; when
    // the caret leaves, the comparison re-derives and the marking is rebuilt on
    // the same document rather than by replacing it.
    const OLD = "# Title\n\nfirst\n\nsecond\n";
    const NEW = "# Title\n\nfirst\n\nsecond edited\n";
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");
    expect(richBlockEl("second edited")).toHaveAttribute("data-mark", "added");
    expect(richBlockEl("first")).toHaveAttribute("data-mark", "none");

    const run = document.querySelector<HTMLElement>(".diff-run[data-editing]")!;
    const surface = run.querySelector<HTMLElement>(".ProseMirror")!;
    await act(async () => {
      fireEvent.focusIn(surface);
      fireEvent.keyDown(surface, { key: "a", ctrlKey: true });
      fireEvent.keyDown(surface, { key: "Backspace" });
    });
    await act(async () => {
      fireEvent.focusOut(surface);
    });

    // The whole target went, so the target has nothing marked and the original's
    // blocks read as removed — rebuilt, not stale.
    await waitFor(() =>
      expect(richBlockEl("second")).toHaveAttribute("data-mark", "removed"),
    );
    expect(document.querySelector(".diff-run [data-mark='added']")).toBeNull();
  });

  it("keeps a mark on the text it belongs to while the author types around it", async () => {
    // DFV-FR-43: the comparison is re-derived once the typing settles, and
    // between now and then the marking has to stay attached to the text rather
    // than to the position it used to occupy. It is carried as decorations for
    // exactly this reason: they map through the author's own transactions.
    const OLD = "first\n\nsecond\n";
    const NEW = "first\n\nsecond edited\n";
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");
    await waitFor(() =>
      expect(richBlockEl("second edited")).toHaveAttribute("data-mark", "added"),
    );

    const surface = document.querySelector<HTMLElement>(".ProseMirror")!;
    await act(async () => {
      fireEvent.focusIn(surface);
      // Opens a paragraph above everything, so every block below moves.
      fireEvent.keyDown(surface, { key: "Enter" });
    });

    // The mark moved with the text rather than staying at the position it used
    // to occupy — and nothing the edit created acquired one.
    expect(richBlockEl("second edited")).toHaveAttribute("data-mark", "added");
    const marked = Array.from(
      document.querySelectorAll<HTMLElement>(".diff-run [data-mark='added']"),
    );
    expect(marked).toHaveLength(1);
    expect(marked[0].textContent).toBe("second edited");
    // Including the word marking inside it, which is part of the same reading
    // and would be lost by anything that rebuilt the marking from a comparison
    // that has not caught up yet.
    expect(marked[0].querySelector("mark.diff-word")?.textContent).toBe(
      " edited",
    );
  });

  it("marks the changed words in a task item, whose checkbox is not its text", async () => {
    // The block parse reads `- [ ] ` as part of the item's text and the schema
    // reads it as an attribute, so the ranges and the node's own text are four
    // characters apart. Unshifted, every range falls past the end of the node
    // and the marking is silently dropped (DFV-FR-19).
    const OLD = "- [ ] write the old thing\n";
    const NEW = "- [ ] write the new thing\n";
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");

    await waitFor(() =>
      expect(document.querySelector(".diff-run .ProseMirror")).toBeTruthy(),
    );
    const marks = Array.from(
      document.querySelectorAll<HTMLElement>(".diff-run mark.diff-word"),
    ).map((n) => n.textContent);
    expect(marks).toEqual(["new"]);
  });

  it("mounts a deferred run reached by the keyboard, not only by the pointer", async () => {
    // Past the live-run limit a run mounts when the author reaches for it, and
    // tabbing to it is reaching for it (DFV-FR-50).
    const count = LIVE_RUN_LIMIT + 2;
    const lines = (word: string) =>
      Array.from({ length: count }, (_, i) => `para ${i} ${word}`).join("\n\n") +
      "\n";
    wireBackend({
      revisions: revs(lines("before"), lines("after")),
      prefs: { diffRenderingMode: "rich" },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");
    const run = document.querySelector<HTMLElement>(".diff-run")!;
    expect(run).toHaveAttribute("tabindex", "0");
    expect(document.querySelectorAll(".ProseMirror")).toHaveLength(0);

    await act(async () => {
      fireEvent.focus(run);
    });
    await waitFor(() =>
      expect(document.querySelectorAll(".ProseMirror")).toHaveLength(1),
    );
  });

  it("leaves the rich target inert while an external change is unresolved", async () => {
    // DFV-FR-53: the surface stops accepting input and the reading does not
    // change — the run is not replaced by something else while the author
    // decides.
    const OLD = "# Title\n\nbefore\n";
    const NEW = "# Title\n\nafter\n";
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
    });
    const { sessions } = renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");
    const surface = document.querySelector<HTMLElement>(".ProseMirror")!;
    expect(surface.getAttribute("contenteditable")).toBe("true");

    act(() => {
      sessions.update("notes.md", { pending: "other", conflict: true });
    });
    await screen.findByRole("dialog", { name: "File changed on disk" });

    // Same surface, still showing the same document, no longer editable.
    expect(document.querySelector(".ProseMirror")).toBe(surface);
    expect(surface.textContent).toContain("after");
    await waitFor(() =>
      expect(surface.getAttribute("contenteditable")).toBe("false"),
    );
  });

  it("marks what it can and stops where the two readings of the lines part company", async () => {
    // The block parse is a renderer's parse: a setext heading is a paragraph
    // followed by a rule to it, and one heading node to the schema the run is
    // edited with. From there the positions have slipped, so marking on would
    // put every mark on a block nobody touched. What it must NOT do is give up
    // on the whole run — a document with one such construct near the top would
    // then read as a diff with no changes in it at all (DFV-FR-19).
    const OLD = "opening\n\nTitle\n---\n\ntail\n";
    const NEW = "opening edited\n\nTitle\n---\n\ntail edited\n";
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-final");
    await waitFor(() =>
      expect(document.querySelector(".diff-run .ProseMirror")).toBeTruthy(),
    );

    // Everything up to the disagreement is marked…
    expect(richBlockEl("opening edited")).toHaveAttribute("data-mark", "added");
    // …and nothing past it claims a marking it cannot vouch for.
    expect(richBlockEl("tail edited")).toBeUndefined();
    expect(
      document.querySelector(".diff-run .ProseMirror")!.textContent,
    ).toContain("tail edited");
  });

  it("renders a block that is not a document of its own as the static block it is", async () => {
    // DFV-FR-47: a run is edited by parsing its own lines and serialising them
    // back, so a table row without the `|---|---|` that makes it a row — or a
    // nested list item without its parent — must not be one: on its own the
    // first is a paragraph of pipes and the second is a top-level item, and
    // writing either back would rewrite the file into something the author
    // never typed. Side-by-side pairs every block into a row of its own, which
    // is where this is reachable.
    const TABLE = "| a | b |\n| - | - |\n| 1 | 2 |\n\n- top\n  - nested\n";
    wireBackend({
      revisions: revs(TABLE, TABLE.replace("| 1 | 2 |", "| 1 | 9 |")),
      prefs: {
        diffRenderingMode: "rich",
        diffVisualizationMode: "side_by_side",
      },
    });
    renderDiff(target("notes.md"));
    const view = await screen.findByTestId("diff-rich-side-by-side");

    const rows = Array.from(view.querySelectorAll<HTMLElement>(".diff-sbs__row"));
    expect(rows.length).toBeGreaterThan(0);
    // No run on screen holds a lone table row or the nested item.
    for (const run of Array.from(view.querySelectorAll(".diff-run"))) {
      expect(run.textContent).not.toContain("|");
      expect(run.textContent?.trim()).not.toBe("nested");
    }
    // The top-level item, which is a document on its own, still is a run — so
    // the rule refused what it had to and nothing more.
    const rightCells = rows.map((row) => row.children[1] as HTMLElement);
    const top = rightCells.find((cell) => cell.textContent?.trim() === "top")!;
    expect(top).toHaveClass("diff-run");
  });

  it("DFV-FR-47, DFV-FR-45, DFV-FR-46, DFV-FR-21, DFV-FR-22 leaves the left pane's rich document inert in Side-by-side", async () => {
    const OLD = "# Title\n\nold para\n";
    const NEW = "# Title\n\nnew para\n";
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "side_by_side" },
    });
    renderDiff(target("notes.md"));
    const view = await screen.findByTestId("diff-rich-side-by-side");

    for (const row of Array.from(view.querySelectorAll(".diff-sbs__row"))) {
      const [left, right] = Array.from(row.children) as HTMLElement[];
      // DFV-FR-45: the left document accepts nothing and says so; the right is
      // the Editor's own surface over the block opposite it, one row at a time
      // so the two panes cannot drift apart.
      expect(left).toHaveClass("diff-block");
      expect(left).toHaveAttribute("aria-readonly", "true");
      expect(left.querySelector(".ProseMirror")).toBeNull();
      if (right.dataset.mark !== "filler") {
        expect(right).toHaveClass("diff-run");
        expect(right.querySelector(".ProseMirror")).not.toBeNull();
      }
    }
  });

  it("hangs the split view's pages off a wrapper as tall as the document", async () => {
    // The rendering half of the stylesheet rule above: the rows are inside the
    // wrapper the sheets are drawn behind, rather than directly in the scroller
    // whose height is the viewport's.
    wireBackend({
      revisions: revs("# Title\n\nold\n", "# Title\n\nnew\n"),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "side_by_side" },
    });
    renderDiff(target("notes.md"));
    const view = await screen.findByTestId("diff-rich-side-by-side");

    const wrapper = view.querySelector<HTMLElement>(
      ".diff-sbs__scroll > .diff-sbs__sheets",
    );
    expect(wrapper).not.toBeNull();
    const rows = view.querySelectorAll(".diff-sbs__row");
    expect(rows.length).toBeGreaterThan(0);
    for (const row of Array.from(rows)) {
      expect(row.parentElement).toBe(wrapper);
    }
  });

  it("DFV-FR-47, DFV-FR-45, DFV-FR-46, DFV-FR-21, DFV-FR-22 renders the outcome as one document in Final", async () => {
    // DFV-FR-46: Final shows the outcome, so the target is unbroken unless the
    // comparison dropped a block outright — and an unbroken target is one
    // document, which is what makes the caret cross from block to block.
    const OLD = "# Title\n\nfirst para\n\nsecond para\n";
    const NEW = "# Title\n\nfirst para\n\nsecond para, revised\n";
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
    });
    const { sessions } = renderDiff(target("notes.md"));
    const view = await screen.findByTestId("diff-rich-final");

    const runs = view.querySelectorAll(".diff-run[data-editing='true']");
    expect(runs).toHaveLength(1);
    const surface = runs[0].querySelector<HTMLElement>(".ProseMirror")!;
    expect(surface.textContent).toContain("Title");
    expect(surface.textContent).toContain("first para");
    expect(surface.textContent).toContain("second para, revised");
    // The marking is inside that one document rather than around it.
    expect(richBlockEl("second para, revised")).toHaveAttribute(
      "data-mark",
      "added",
    );
    expect(richBlockEl("first para")).toHaveAttribute("data-mark", "none");

    // And an edit through it is one edit to the whole target.
    await act(async () => {
      fireEvent.focusIn(surface);
      fireEvent.keyDown(surface, { key: "a", ctrlKey: true });
      fireEvent.keyDown(surface, { key: "Backspace" });
    });
    expect(sessions.get("notes.md")?.buffer).toBe("");
  });

  it("keeps a file's frontmatter out of the document the run edits", async () => {
    // Per `EDT-editor.md` EDT-FR-18 the WYSIWYG surface never holds the
    // frontmatter: `---` has no node in the schema, so a run that swallowed the
    // fences would serialise them back as a rule with a paragraph between them
    // and destroy the block on the first edit made anywhere in the file.
    const OLD = "---\ntitle: notes\n---\n\nold body\n";
    const NEW = "---\ntitle: notes\n---\n\nnew body\n";
    wireBackend({
      revisions: revs(OLD, NEW),
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
    });
    renderDiff(target("notes.md"));
    const view = await screen.findByTestId("diff-rich-final");

    for (const surface of Array.from(view.querySelectorAll(".ProseMirror"))) {
      expect(surface.textContent).not.toContain("title: notes");
    }
    // The frontmatter still renders, as the static blocks it is.
    expect(view.textContent).toContain("title: notes");
    // And the body below it is a run, editable as ever.
    expect(richBlockEl("new body")!.closest(".diff-run")).not.toBeNull();
  });

  it("mounts the target's runs on demand once there are too many to hold at once", async () => {
    // A wholesale rewrite pairs every block with its replacement, so Unified
    // puts a removed block between each one and the next: the target becomes
    // one run per block, and mounting a ProseMirror view for each of hundreds
    // would cost the tab its first paint. Past the limit they mount when the
    // author reaches for one, with nothing about the reading changed.
    const count = LIVE_RUN_LIMIT + 5;
    const lines = (word: string) =>
      Array.from({ length: count }, (_, i) => `para ${i} ${word}`).join("\n\n") +
      "\n";
    wireBackend({
      revisions: revs(lines("before"), lines("after")),
      prefs: { diffRenderingMode: "rich" },
    });
    renderDiff(target("notes.md"));
    await screen.findByTestId("diff-rich-unified");

    const runs = Array.from(document.querySelectorAll<HTMLElement>(".diff-run"));
    expect(runs).toHaveLength(count);
    expect(document.querySelectorAll(".ProseMirror")).toHaveLength(0);
    // Each still announces itself as the target's editing surface.
    expect(runs[0]).toHaveAttribute("role", "textbox");
    expect(runs[0].getAttribute("aria-label")).toContain("target revision");

    await act(async () => {
      fireEvent.mouseDown(runs[0]);
    });
    await waitFor(() =>
      expect(document.querySelectorAll(".ProseMirror")).toHaveLength(1),
    );
    expect(runs[0].querySelector(".ProseMirror")!.textContent).toContain(
      "para 0 after",
    );
  });
});
