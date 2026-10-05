import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { expect, vi } from "vitest";

import type { Mock } from "vitest";

import { DiffView } from "../components/DiffView";
import { EditSessionStore } from "../state/editSessions";
import { comparisonLabel, diffScopeFor } from "../comparison";
import type {
  AppPreferences,
  Comparison,
  DiffPayload,
  DiffTarget,
  FileRevisions,
} from "../types";

export const uncommitted: Comparison = { kind: "uncommitted" };
export const againstMain: Comparison = {
  kind: "branch",
  targetBranch: "main",
  mergeBase: "abc123",
};

export function target(
  path = "src/components/Library.tsx",
  comparison: Comparison = uncommitted,
  artifactType?: DiffTarget["artifactType"],
): DiffTarget {
  return {
    path,
    name: path.split("/").pop()!,
    scope: diffScopeFor(comparison, path),
    comparisonLabel: comparisonLabel(comparison),
    ...(artifactType ? { artifactType } : {}),
  };
}

export const TEXT_DIFF: DiffPayload = {
  isBinary: false,
  hunks: [
    {
      header: "@@ -12,4 +12,5 @@",
      lines: [
        { kind: "context", oldLineno: 12, newLineno: 12, content: "unchanged" },
        { kind: "del", oldLineno: 13, content: "was this" },
        { kind: "add", newLineno: 13, content: "is now this" },
      ],
    },
  ],
};

/**
 * The comparison every test that does not care about the content renders: one
 * context line, one replaced line, at the top of a short file.
 */
export const DEFAULT_REVISIONS: FileRevisions = {
  old: "unchanged\nwas this\n",
  new: "unchanged\nis now this\n",
  isBinary: false,
};

/** Two revisions the tab compares, spelled as the tests read them. */
export const revs = (oldText: string | null, newText: string | null): FileRevisions => ({
  old: oldText,
  new: newText,
  isBinary: false,
});

/**
 * A Diff tab over a fresh editing-session store.
 *
 * The store is the target (DFV-FR-42), so every test gets its own — a store
 * shared between tests would carry one test's buffer into the next.
 */
export function renderDiff(t: DiffTarget = target()) {
  const sessions = new EditSessionStore();
  const result = render(<DiffView target={t} sessions={sessions} />);
  return { ...result, sessions };
}

/**
 * The backend behind a tab: the **original** revision and the facts the new side
 * carries, the artifact's own contents (which are the target, DFV-FR-42), and
 * the stored modes. Anything not given falls back to something innocuous, so
 * each test states only what it is about.
 *
 * There is no `get_diff` here, and that is the point: the tab derives the whole
 * comparison itself from the original it fetched and the target as it currently
 * stands (DFV-FR-43), because an edited target is not on disk for anything else
 * to diff.
 */
export function makeWireBackend(invokeMock: Mock) {
  return function wireBackend(options: {
    revisions?: FileRevisions;
    prefs?: Partial<AppPreferences>;
    /** A load of the target that fails, or one that answers something else. */
    contents?: () => Promise<{ body: string; checksum: string }>;
  } = {}) {
    invokeMock.mockImplementation(async (cmd: string) => {
      const revisions = options.revisions ?? DEFAULT_REVISIONS;
      switch (cmd) {
        case "get_file_revisions":
          return revisions;
        case "load_artifact_contents_by_id":
          return options.contents
            ? options.contents()
            : { body: revisions.new ?? "", checksum: "sum" };
        case "save_artifact_contents":
          return { checksum: "sum2" };
        case "create_file":
          return { path: "restored.md" };
        case "load_app_preferences":
          return {
            theme: "system",
            mainWindowFullscreen: false,
            searchQueryMode: "literal_insensitive",
            diffVisualizationMode: "unified",
            diffRenderingMode: "source",
            ...options.prefs,
          };
        case "save_app_preferences":
          return undefined;
        default:
          return undefined;
      }
    });
  };
}

export const toolbarGroup = (label: string) =>
  screen.getByRole("radiogroup", { name: label });

export const activeIn = (label: string) =>
  within(toolbarGroup(label))
    .getAllByRole("radio")
    .filter((b) => b.getAttribute("aria-checked") === "true")
    .map((b) => b.getAttribute("aria-label"));

export const gutters = (selector: string) =>
  Array.from(document.querySelectorAll(selector)).map((n) => n.textContent);

/**
 * A rendered source line, found by its complete text.
 *
 * `getByText` cannot do this any more: a replaced line is segmented so the
 * words that changed can carry the stronger mark, which splits the line across
 * several elements. The line's own text element is what holds the whole text.
 */
export const lineEl = (text: string): HTMLElement | undefined =>
  Array.from(document.querySelectorAll<HTMLElement>(".diff-line__text")).find(
    (n) => n.textContent === text,
  );

export const diffLine = (text: string): HTMLElement | undefined =>
  lineEl(text)?.closest(".diff-line") as HTMLElement | undefined;

export const findDiffLine = async (text: string): Promise<HTMLElement> => {
  await waitFor(() => expect(diffLine(text)).toBeTruthy());
  return diffLine(text)!;
};

/**
 * Wait until the tab has both revisions and has rendered the comparison.
 *
 * Two reads now stand between mounting and a rendered diff — the original
 * (`get_file_revisions`) and the target, which is the artifact's own contents
 * (DFV-FR-25) — so a test that waited on one of them would assert against a
 * half-rendered tab.
 */
export const awaitDiff = async (): Promise<void> => {
  await waitFor(() =>
    expect(
      document.querySelector(
        '.diff-line, .diff-block, .diff-flow__entry, .changes-state:not([data-state="loading"])',
      ),
    ).toBeTruthy(),
  );
};

/**
 * The words inside a line that carry the stronger "this is the edit" mark.
 * Throws rather than returning `[]` for a line that is not rendered — every
 * "no words are marked" assertion would otherwise pass when the component
 * rendered nothing at all.
 */
export const changedWords = (text: string) => {
  const el = lineEl(text);
  if (!el) throw new Error(`no rendered line with text ${JSON.stringify(text)}`);
  return Array.from(el.querySelectorAll("mark")).map((n) => n.textContent);
};

/**
 * A rendered rich block, found by the text its body renders to.
 *
 * Two renderings answer to this, and a test asserting a block's marking should
 * not have to know which it got: the **static** one (`.diff-block`), which is
 * what every rendering of the original is and what the target is wherever it is
 * not being edited, and the **editable** one, where the target's blocks are
 * nodes of the run's own document carrying their mark as a decoration
 * (DFV-FR-47).
 */
export const richBlockEl = (
  text: string,
  /**
   * Where to look. Worth naming in Side-by-side, where the two panes can render
   * the same text and the left one — which is the original — would otherwise
   * answer for a question about the target.
   */
  root: ParentNode = document,
): HTMLElement | undefined => {
  // An empty needle would match any block that renders no text — a table
  // divider, a rule — and quietly stand in for the block being looked for.
  if (!text) throw new Error("richBlockEl needs the text a block renders to");
  const stat = Array.from(root.querySelectorAll<HTMLElement>(".diff-block")).find(
    (n) => n.querySelector(".diff-block__body")?.textContent?.trim() === text,
  );
  if (stat) return stat;
  return Array.from(
    root.querySelectorAll<HTMLElement>(".diff-run [data-mark]"),
  ).find((n) => n.textContent?.trim() === text);
};

/**
 * Every marked rich block on screen, in document order, whichever rendering
 * produced it.
 */
export const richBlocks = (root: ParentNode = document) =>
  Array.from(
    root.querySelectorAll<HTMLElement>(
      ".diff-block[data-mark], .diff-run [data-mark]",
    ),
  );

/**
 * The element a block's rendered content hangs off: the body of a static block,
 * and the decorated node itself in an editable run — which *is* the `p`, `h3` or
 * `li` the block renders as rather than a wrapper around it.
 */
export const richBody = (el: HTMLElement): HTMLElement =>
  el.querySelector<HTMLElement>(".diff-block__body") ?? el;

/** The tag a block renders as, in either rendering. */
export const richTag = (text: string): string => {
  const el = richBlockEl(text);
  if (!el) throw new Error(`no rendered block with text ${JSON.stringify(text)}`);
  const body = richBody(el);
  return (body === el ? el : (body.firstElementChild ?? body)).tagName.toLowerCase();
};

/**
 * The sign character a marked block carries, in either rendering.
 *
 * Throws rather than answering `""` for a block that is not on screen: an
 * unmarked block and a block that failed to render both carry no glyph, and a
 * helper that cannot tell them apart turns every "carries no sign" assertion
 * into one that passes when nothing rendered.
 */
export const richGlyph = (text: string): string => {
  const el = richBlockEl(text);
  if (!el) throw new Error(`no rendered block with text ${JSON.stringify(text)}`);
  const marker = el.querySelector(".diff-block__marker");
  if (marker) return marker.textContent ?? "";
  if (!el.hasAttribute("data-mark")) {
    throw new Error(`block ${JSON.stringify(text)} carries no marking at all`);
  }
  return el.getAttribute("data-glyph") ?? "";
};

/** The words inside a rich block that carry the stronger mark. */
export const blockWords = (text: string) => {
  const el = richBlockEl(text);
  if (!el) throw new Error(`no rendered block with text ${JSON.stringify(text)}`);
  return Array.from(el.querySelectorAll("mark")).map((n) => n.textContent);
};

/**
 * The tab's editing surface, as a test drives it.
 *
 * `contenteditable` has no `value` and jsdom implements none of its editing
 * behaviour, so a test types by writing the element's text and firing the input
 * event the browser would have fired. What is under test is what the component
 * does with that event — which line of the whole target it splices it into
 * (DFV-FR-44) — and that is exactly what jsdom can answer.
 */
export const targetCells = (): HTMLElement[] =>
  Array.from(document.querySelectorAll<HTMLElement>('[data-target="true"]'));

export const targetCell = (text: string): HTMLElement => {
  const found = targetCells().find((n) => n.textContent === text);
  if (!found) {
    throw new Error(
      `no editable target row reading ${JSON.stringify(text)}; the editable rows are ${JSON.stringify(
        targetCells().map((n) => n.textContent),
      )}`,
    );
  }
  return found;
};

/** Put the caret at `offset` characters into `cell`, as a click would. */
export const setCaret = (cell: HTMLElement, offset: number) => {
  const node = cell.firstChild ?? cell;
  const range = document.createRange();
  range.setStart(node, Math.min(offset, node.textContent?.length ?? 0));
  range.collapse(true);
  const selection = window.getSelection()!;
  selection.removeAllRanges();
  selection.addRange(range);
};

export const typeInto = (cell: HTMLElement, text: string) => {
  cell.textContent = text;
  fireEvent.input(cell);
};
