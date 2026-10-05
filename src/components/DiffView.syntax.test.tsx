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

import { resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import {
  awaitDiff,
  findDiffLine,
  makeWireBackend,
  renderDiff,
  revs,
  setCaret,
  target,
  targetCell,
  toolbarGroup,
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

/**
 * DFV-FR-57 / DFV-FR-41, DFV-FR-43, ESH-FR-BABL, ESH-FR-CFEQ: a Source rendering reads its text in the language the
 * file is written in, on both revisions, with the comparison's own marking
 * standing over the colouring rather than under it.
 */
describe("syntax highlighting in Source (DFV-FR-57)", () => {
  const OLD_RS = 'fn main() {\n    let s = "old";\n}\n';
  const NEW_RS = 'fn main() {\n    let s = "new";\n}\n';

  /** Every token role painted anywhere in the tab, with the text it covers. */
  const tokens = (): Array<[string, string]> =>
    Array.from(document.querySelectorAll<HTMLElement>(".hl")).map((n) => [
      Array.from(n.classList).find((c) => c.startsWith("hl--")) ?? "",
      n.textContent ?? "",
    ]);

  const awaitTokens = async () => {
    await waitFor(() => expect(tokens().length).toBeGreaterThan(0), {
      timeout: 2_000,
    });
  };

  it("DFV-FR-57, DFV-FR-41, DFV-FR-43, ESH-FR-BABL, ESH-FR-CFEQ colours both revisions and keeps every marking legible over it", async () => {
    wireBackend({ revisions: revs(OLD_RS, NEW_RS) });
    renderDiff(target("src/lib.rs"));
    await awaitDiff();
    await awaitTokens();

    // Both revisions are read in the same language: the removed line and the
    // line that replaced it each carry their own revision's string.
    const covered = tokens();
    expect(covered).toContainEqual(["hl--keyword", "fn"]);
    expect(covered.some(([role, text]) => role === "hl--string" && text.includes("old"))).toBe(true);
    expect(covered.some(([role, text]) => role === "hl--string" && text.includes("new"))).toBe(true);

    // DFV-FR-41: the original is still read-only and the target still editable.
    const removed = await findDiffLine('    let s = "old";');
    expect(removed).toHaveAttribute("data-kind", "del");
    expect(removed.querySelector('[data-readonly="true"]')).not.toBeNull();
    expect(removed.querySelector('[data-target="true"]')).toBeNull();
    const added = await findDiffLine('    let s = "new";');
    expect(added.querySelector('[data-target="true"]')).not.toBeNull();

    // DFV-FR-32: the word-level marking is the outer element, so a token colour
    // can neither carry the marking nor defeat it.
    expect(added.querySelector("mark.diff-word .hl")).not.toBeNull();
    expect(added.querySelector(".hl mark.diff-word")).toBeNull();
    // The rows still read as the lines they are.
    expect(added.querySelector(".diff-line__text")?.textContent).toBe(
      '    let s = "new";',
    );
  });

  it("DFV-FR-57, DFV-FR-41, DFV-FR-43, ESH-FR-BABL, ESH-FR-CFEQ follows an edit to the target and colours the edited text", async () => {
    wireBackend({ revisions: revs(OLD_RS, NEW_RS) });
    renderDiff(target("src/lib.rs"));
    await awaitDiff();
    await awaitTokens();

    typeInto(targetCell('    let s = "new";'), '    let s = "edited";');
    await waitFor(() =>
      expect(
        tokens().some(([role, text]) => role === "hl--string" && text.includes("edited")),
      ).toBe(true),
    );
  });

  it("DFV-FR-57, DFV-FR-41, DFV-FR-43, ESH-FR-BABL, ESH-FR-CFEQ reads both revisions in ONE language when the name maps to none", async () => {
    // An extensionless file is resolved from its text, and the two revisions are
    // two different texts — so resolving each on its own puts the old pane in one
    // language and the new in another. `def` is the discriminator: it is a
    // keyword of the old revision's language and of nothing the new revision is
    // read as, so a keyword marking on it is the old side having been resolved
    // separately.
    const OLD_PY = [
      "import os",
      "from typing import List",
      "",
      "def add(a: int, b: int) -> int:",
      '    """Add two numbers."""',
      "    return a + b",
      "",
      "class Widget:",
      "    def render(self):",
      "        for i in range(10):",
      "            print(self.name, i)",
      "",
    ].join("\n");
    const NEW_SH = [
      "#!/usr/bin/env bash",
      "set -euo pipefail",
      "",
      'for f in "$@"; do',
      '  if [ -f "$f" ]; then',
      '    echo "found $f"',
      "  fi",
      "done",
      "",
    ].join("\n");
    wireBackend({ revisions: revs(OLD_PY, NEW_SH) });
    renderDiff(target("scripts/tool"));
    await awaitDiff();
    await awaitTokens();

    // The new revision is the file as it will land, so it is what decided.
    const marked = tokens();
    expect(marked).toContainEqual(["hl--keyword", "done"]);
    // And the old revision is read in that same language rather than in its own:
    // nothing on screen is marked as a keyword of the language it used to be.
    expect(marked.filter(([role, text]) => role === "hl--keyword" && text === "def")).toEqual([]);
  });

  it("DFV-FR-57, DFV-FR-41, DFV-FR-43, ESH-FR-BABL, ESH-FR-CFEQ colours the rows of side-by-side and final too", async () => {
    wireBackend({ revisions: revs(OLD_RS, NEW_RS) });
    renderDiff(target("src/lib.rs"));
    await awaitDiff();
    await awaitTokens();

    // Side-by-side indexes each pane by its own revision's line numbers, and
    // Final chooses between the two per row — separate paths from Unified's, so
    // an off-by-one in either is invisible from the default mode.
    fireEvent.click(
      within(toolbarGroup("Diff visualization")).getByRole("radio", {
        name: "Side-by-side",
      }),
    );
    await waitFor(() => expect(screen.queryByTestId("diff-side-by-side")).toBeTruthy());
    await awaitTokens();
    const sbs = document.querySelector('[data-testid="diff-side-by-side"]')!;
    expect(sbs.querySelectorAll(".hl--keyword").length).toBeGreaterThan(0);
    // Each pane's own revision: `old` on the left, `new` on the right. The
    // string is read in pieces, because the word-level marking cuts the token
    // where the two revisions differ — which is the composition DFV-FR-57
    // requires, so the pieces are joined rather than looked at one at a time.
    const stringText = (root: Element): string =>
      Array.from(root.querySelectorAll(".hl--string"))
        .map((n) => n.textContent ?? "")
        .join("");
    const cells = Array.from(sbs.querySelectorAll<HTMLElement>(".diff-sbs__cell"));
    const left = cells.find((c) => c.textContent?.includes("old"));
    const right = cells.find((c) => c.textContent?.includes("new"));
    expect(stringText(left!)).toContain("old");
    expect(stringText(right!)).toContain("new");

    fireEvent.click(
      within(toolbarGroup("Diff visualization")).getByRole("radio", { name: "Final" }),
    );
    await waitFor(() => expect(screen.queryByTestId("diff-final")).toBeTruthy());
    await awaitTokens();
    const final = document.querySelector('[data-testid="diff-final"]')!;
    expect(final.querySelectorAll(".hl--keyword").length).toBeGreaterThan(0);
    expect(
      Array.from(final.querySelectorAll(".hl--string"))
        .map((n) => n.textContent ?? "")
        .join(""),
    ).toContain("new");
  });

  it("DFV-FR-57, DFV-FR-41, DFV-FR-43, ESH-FR-BABL, ESH-FR-CFEQ colours nothing in a binary comparison", async () => {
    wireBackend({ revisions: { old: null, new: null, isBinary: true } });
    renderDiff(target("assets/logo.rs"));
    await awaitDiff();
    await act(async () => {
      await new Promise((r) => setTimeout(r, 250));
    });
    expect(screen.getByText(/Binary file/)).toBeInTheDocument();
    expect(tokens()).toHaveLength(0);
  });

  it("DFV-FR-57, DFV-FR-41, DFV-FR-43, ESH-FR-BABL, ESH-FR-CFEQ does not move the caret when the colouring arrives", async () => {
    // The row is repainted when its tokens land — a deeper DOM than the plain
    // text it replaces — and the author may well have the caret in it by then.
    // A repaint that lost the caret would throw them out of the word they are
    // typing, for a change that is presentation only (DFV-FR-57, ESH-FR-BPLJ).
    wireBackend({ revisions: revs(OLD_RS, NEW_RS) });
    renderDiff(target("src/lib.rs"));
    await awaitDiff();

    const cell = targetCell('    let s = "new";');
    cell.focus();
    setCaret(cell, 8);
    expect(document.activeElement).toBe(cell);

    await awaitTokens();
    expect(document.activeElement).toBe(cell);
    const selection = document.getSelection()!;
    const range = selection.getRangeAt(0).cloneRange();
    range.selectNodeContents(cell);
    range.setEnd(
      selection.getRangeAt(0).startContainer,
      selection.getRangeAt(0).startOffset,
    );
    expect(range.toString().length).toBe(8);
    // …and the row still reads as the line it is.
    expect(cell.textContent).toBe('    let s = "new";');
  });

  it("DFV-FR-57, DFV-FR-41, DFV-FR-43, ESH-FR-BABL, ESH-FR-CFEQ colours neither revision of a Markdown file", async () => {
    wireBackend({ revisions: revs("# One\n\ntext\n", "# One\n\nprose\n") });
    renderDiff(target("CHG-changes.md"));
    await awaitDiff();
    // Give a pass the same room the coloured cases get before concluding.
    await act(async () => {
      await new Promise((r) => setTimeout(r, 250));
    });
    expect(tokens()).toHaveLength(0);
  });

  it("DFV-FR-57, DFV-FR-41, DFV-FR-43, ESH-FR-BABL, ESH-FR-CFEQ leaves a file no language resolves for plain", async () => {
    wireBackend({
      revisions: revs("plain words here\n", "plain words there\n"),
    });
    renderDiff(target("notes.txt"));
    await awaitDiff();
    await act(async () => {
      await new Promise((r) => setTimeout(r, 250));
    });
    expect(tokens()).toHaveLength(0);
    // ...and it is still an ordinary editable comparison.
    expect(targetCell("plain words there")).toBeTruthy();
  });
});
