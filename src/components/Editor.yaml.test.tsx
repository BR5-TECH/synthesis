import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import type { ArtifactChangedPayload, ArtifactContents } from "../types";
import { ARTIFACT_CHANGED_EXTERNALLY } from "../events";

// Same invoke/listen mocking convention as the rest of the suite. The detailed
// lifecycle (load/save/external-change resolution) is covered against the hook
// in useArtifactDocument.test.tsx; here we verify the integrated Editor mounts
// the WYSIWYG surface, renders Markdown as rich content (EDT-FR-02), and shows
// the blocking modal on an external change.
const invokeMock = vi.fn();
const unlistenMock = vi.fn();
let listeners: Record<
  string,
  (event: { payload: ArtifactChangedPayload }) => void
> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (
      event: string,
      cb: (event: { payload: ArtifactChangedPayload }) => void,
    ) => {
      listeners[event] = cb;
      return unlistenMock;
    },
  ),
}));

// A mutable backend so a test can change the on-disk body before a reload and
// capture what `save` was handed.
interface Backend {
  load: ArtifactContents;
  lastSavedBody?: string;
}

function wireBackend(b: Backend) {
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "load_artifact_contents_by_id") return b.load;
    if (cmd === "save_artifact_contents") {
      b.lastSavedBody = (args as { body: string }).body;
      return { checksum: "ck-saved" };
    }
    throw new Error(`unexpected invoke ${cmd}`);
  });
}

function wireLoad(load: ArtifactContents) {
  wireBackend({ load });
}

/**
 * The store the tests that need a handle on the write path mount their Editor
 * over. Most tests leave the Editor to its own private store; one that has to
 * bring a write forward on demand (EDT-FR-34) — the only route that writes a
 * file nobody edited — passes this and calls `saveNow`.
 */
let sessions: EditSessionStore;

/** EDT-FR-34: File → Save, brought forward for an artifact mounted on `sessions`. */
async function saveNow(id = "a.md") {
  await act(async () => {
    await sessions.flush(id, { force: true });
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
  sessions = new EditSessionStore();
});

afterEach(() => {
  cleanup();
});

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
/** The surface an accelerator would be delivered to (source when in text mode). */
const editSurface = (): HTMLElement =>
  (screen.queryByLabelText("Markdown source") as HTMLElement | null) ?? pmEl();

const pressUndo = () =>
  fireEvent.keyDown(editSurface(), { key: "z", metaKey: true });

// EDT-FR-21 / EDT-FR-58 / EDT-FR-59 / EDT-FR-60: the frontmatter region reads
// the block as the YAML it is — rendering it by parsed role, saying so when it
// does not parse, and reporting the description against its 1024-character
// budget. All of it is presentation over the same bytes.
describe("Editor (YAML-aware frontmatter)", () => {
  /** Mount on a file whose frontmatter inner text is `fm`. */
  async function mountWithFm(fm: string, body = "# H\n") {
    const backend: Backend = {
      load: { body: `---\n${fm}\n---\n\n${body}`, checksum: "ck1" },
    };
    wireBackend(backend);
    const view = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByLabelText("Frontmatter");
    return { backend, container: view.container };
  }

  const hlLayer = () =>
    document.querySelector(".editor__frontmatter-hl") as HTMLElement;
  const textOf = (sel: string) =>
    Array.from(hlLayer().querySelectorAll(sel)).map((e) => e.textContent);
  const countLine = () =>
    document.querySelector(".editor__frontmatter-count") as HTMLElement | null;

  // EDT-FR-21: a nested mapping, a list whose first item ends in a
  // comment, a double-quoted value and a `>` block scalar — every piece rendered
  // by the role the parse gives it, and the block written back byte-for-byte.
  it("renders every YAML role from the parse and preserves the block on save", async () => {
    const fm = [
      "name: onboarding",
      "description: >",
      "  Steps to run before",
      "  the first session",
      "metadata:",
      "  type: user",
      'quoted: "a: b"',
      "tags:",
      "  - setup      # sequenced first",
      "  - review",
    ].join("\n");
    const { backend } = await mountWithFm(fm);

    // Keys, at every depth, and nothing else.
    expect(textOf(".editor__frontmatter-key")).toEqual([
      "name",
      "description",
      "metadata",
      "type",
      "quoted",
      "tags",
    ]);
    // The receding role covers the separators, the list markers, the quotes and
    // the block-scalar indicator; the comment carries its own modifier.
    const punct = textOf(".editor__frontmatter-punct");
    expect(punct.filter((t) => t === ":")).toHaveLength(6);
    expect(punct.filter((t) => t === "-")).toHaveLength(2);
    expect(punct.filter((t) => t === '"')).toHaveLength(2);
    expect(punct).toContain(">");
    expect(textOf(".editor__frontmatter-punct--comment")).toEqual([
      "# sequenced first",
    ]);
    // The folded block-scalar lines and the quoted value's text are the value
    // weight — bare text nodes on the layer, covered by no wrapper at all.
    const wrapped = Array.from(hlLayer().querySelectorAll("*")).map(
      (e) => e.textContent ?? "",
    );
    expect(wrapped.some((t) => t.includes("Steps to run before"))).toBe(false);
    expect(wrapped.some((t) => t.includes("a: b"))).toBe(false);

    // Presentation-only: the layer reproduces the buffer, and an untouched save
    // writes the block back with its indentation, quoting and order unchanged.
    const fmInput = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    expect(hlLayer().textContent).toBe(fmInput.value);
    expect(fmInput.value).toBe(fm);
    await saveNow();
    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    expect(
      (backend.lastSavedBody as string).startsWith(`---\n${fm}\n---\n`),
    ).toBe(true);
  });

  // EDT-FR-59 / EDT-FR-58: a block that does not parse says so, renders
  // unhighlighted, counts nothing, and is neither repaired nor marked dirty —
  // and recovers the moment the user closes the quote.
  it("indicates an unparseable block and recovers when it parses again", async () => {
    const broken = 'name: onboarding\ndescription: "unclosed';
    await mountWithFm(broken);

    expect(screen.getByText("not valid YAML")).toBeInTheDocument();
    // No role treatment at all — the content renders as plain text…
    expect(hlLayer().querySelector(".editor__frontmatter-key")).toBeNull();
    expect(hlLayer().querySelector(".editor__frontmatter-punct")).toBeNull();
    // …but it is all still there, and still editable.
    const fm = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    expect(hlLayer().textContent).toBe(fm.value);
    expect(fm.value).toBe(broken);
    expect(fm.readOnly).toBe(false);
    expect(countLine()).toBeNull();
    // Rendering an invalid block is not an edit.
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();

    // Close the quote: the indication clears and the roles come back with it —
    // over exactly the text the user typed, repaired in no way.
    const repaired = 'name: onboarding\ndescription: "closed now"';
    fireEvent.change(fm, { target: { value: repaired } });
    expect(screen.queryByText("not valid YAML")).not.toBeInTheDocument();
    expect(textOf(".editor__frontmatter-key")).toEqual(["name", "description"]);
    expect(countLine()?.textContent).toBe("10/1024"); // "closed now"
    expect(
      (screen.getByLabelText("Frontmatter") as HTMLTextAreaElement).value,
    ).toBe(repaired);
    expect(hlLayer().textContent).toBe(repaired);
  });

  // EDT-FR-58: an unparseable block is as collapsible and expandable as any
  // other — the indication belongs to the expanded region, not to the summary.
  it("keeps an unparseable block collapsible and expandable", async () => {
    const broken = 'name: onboarding\ndescription: "unclosed';
    const { container } = await mountWithFm(broken);
    expect(screen.getByText("not valid YAML")).toBeInTheDocument();

    const scroller = container.querySelector(".editor") as HTMLElement;
    Object.defineProperty(scroller, "scrollTop", {
      configurable: true,
      value: 240,
    });
    fireEvent.scroll(scroller);

    const bar = await screen.findByRole("button", { name: "Expand frontmatter" });
    expect(bar.textContent).not.toContain("not valid YAML");
    expect(countLine()).toBeNull();

    fireEvent.click(bar);
    const fm = (await screen.findByLabelText(
      "Frontmatter",
    )) as HTMLTextAreaElement;
    expect(fm.value).toBe(broken);
    expect(screen.getByText("not valid YAML")).toBeInTheDocument();
  });

  // EDT-FR-21 / EDT-FR-60: a CRLF block is read at the same offsets — the roles
  // land on the right glyphs, the description counts, and the bytes survive.
  it("renders roles and counts the description over a CRLF block", async () => {
    const fm = "name: Foo\r\ndescription: hello";
    const backend: Backend = {
      load: { body: `---\r\n${fm}\r\n---\r\n\r\n# H\r\n`, checksum: "ck1" },
    };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByLabelText("Frontmatter");

    expect(textOf(".editor__frontmatter-key")).toEqual(["name", "description"]);
    // No offset drift: exactly the two separators, and neither swallows a glyph.
    expect(
      textOf(".editor__frontmatter-punct").filter((t) => t === ":"),
    ).toHaveLength(2);
    expect(countLine()?.textContent).toBe("5/1024"); // "hello"

    // A textarea normalises CRLF to LF when its value is read back, and the
    // layer drops the CR for rendering too (a bare \r is its own segment break
    // under pre-wrap), so the two still register glyph-for-glyph. The CRs live
    // on in the buffer behind them — which is what the save below proves.
    const fmInput = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    expect(fmInput.value).toBe(fm.replace(/\r/g, ""));
    expect(hlLayer().textContent).toBe(fmInput.value);
    await saveNow();
    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    expect(
      (backend.lastSavedBody as string).startsWith(`---\r\n${fm}\r\n---\r\n`),
    ).toBe(true);
  });

  // EDT-FR-58 / EDT-FR-59 (boundary): an empty block is valid and counts nothing.
  it("renders an empty frontmatter block as valid with no count", async () => {
    await mountWithFm("");
    expect(screen.queryByText("not valid YAML")).not.toBeInTheDocument();
    expect(countLine()).toBeNull();
    expect(
      (screen.getByLabelText("Frontmatter") as HTMLTextAreaElement).value,
    ).toBe("");
  });

  // EDT-FR-20 / EDT-FR-59: the reading is present in both of the region's
  // displays — on the bottom edge while expanded, at the trailing edge of the
  // summary bar while collapsed — and in neither case in raw-text mode.
  it("reports the description length in both region displays, never in raw text", async () => {
    const description = "x".repeat(412);
    const { container } = await mountWithFm(
      `name: onboarding\ndescription: ${description}`,
    );

    expect(countLine()?.textContent).toBe("412/1024");
    expect(countLine()?.getAttribute("data-over")).toBe("false");
    // It is chrome, not content: outside the editable textarea and outside the
    // highlight layer that mirrors it, on the last row of the region — the
    // trailing alignment within that row is CSS the jsdom render cannot see.
    const region = container.querySelector(".editor__frontmatter")!;
    expect(region.lastElementChild).toHaveClass("editor__frontmatter-foot");
    expect(region.lastElementChild?.contains(countLine())).toBe(true);
    expect(
      Array.from(region.children).indexOf(
        region.querySelector(".editor__frontmatter-edit")!,
      ),
    ).toBeLessThan(Array.from(region.children).length - 1);
    expect(hlLayer().contains(countLine())).toBe(false);
    expect(
      (screen.getByLabelText("Frontmatter") as HTMLTextAreaElement).value,
    ).not.toContain("1024");

    // Collapsed (scrolled down): the same reading rides at the trailing edge of
    // the summary bar, and clicking it expands the region like the rest of it.
    const scroller = container.querySelector(".editor") as HTMLElement;
    Object.defineProperty(scroller, "scrollTop", {
      configurable: true,
      value: 240,
    });
    fireEvent.scroll(scroller);
    const bar = await screen.findByRole("button", { name: "Expand frontmatter" });
    expect(countLine()?.textContent).toBe("412/1024");
    expect(countLine()?.getAttribute("data-over")).toBe("false");
    // Exactly one reading on screen, and it is the bar's trailing child.
    expect(container.querySelectorAll(".editor__frontmatter-count")).toHaveLength(1);
    expect(bar.lastElementChild).toBe(countLine());
    // The bar's own name is unchanged; the reading reaches assistive tech as its
    // description, since an aria-label would otherwise swallow the bar's content.
    expect(bar).toHaveAccessibleName("Expand frontmatter");
    expect(bar).toHaveAccessibleDescription("412/1024");

    // Clicking the reading expands the region like clicking anywhere else on it.
    fireEvent.click(countLine()!);
    await screen.findByLabelText("Frontmatter");
    expect(countLine()?.textContent).toBe("412/1024");

    // Raw-text mode: no region, so no count.
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    await screen.findByLabelText("Markdown source");
    expect(countLine()).toBeNull();
  });

  /** Scroll the artifact down far enough to collapse the region (EDT-FR-20). */
  async function collapse(container: HTMLElement, to = 240) {
    const scroller = container.querySelector(".editor") as HTMLElement;
    Object.defineProperty(scroller, "scrollTop", { configurable: true, value: to });
    fireEvent.scroll(scroller);
    return screen.findByRole("button", { name: "Expand frontmatter" });
  }

  // EDT-FR-60: the warning treatment is the same on both displays.
  it("carries the over-limit warning onto the collapsed bar", async () => {
    const { container } = await mountWithFm(`description: ${"y".repeat(1180)}`);
    expect(countLine()?.getAttribute("data-over")).toBe("true");

    const bar = await collapse(container);
    expect(countLine()?.textContent).toBe("1180/1024");
    expect(countLine()?.getAttribute("data-over")).toBe("true");
    expect(bar.lastElementChild).toBe(countLine());
    expect(container.querySelectorAll(".editor__frontmatter-count")).toHaveLength(1);

    // A warning-treated reading is as much a click target as a normal one.
    fireEvent.click(countLine()!);
    await screen.findByLabelText("Frontmatter");
  });

  // EDT-FR-59: no top-level scalar `description`, nothing to read — on either
  // display, and without the bar losing anything else it carries.
  it("shows no reading on the bar when the block has no description to count", async () => {
    const { container } = await mountWithFm("name: onboarding\ntags:\n  - setup");
    // Positive control: the region did render, it simply has nothing to count.
    expect(screen.getByLabelText("Frontmatter")).toBeInTheDocument();
    expect(countLine()).toBeNull();

    const bar = await collapse(container);
    expect(countLine()).toBeNull();
    expect(bar.textContent).toContain("name: onboarding"); // the summary survives
    expect(bar).not.toHaveAttribute("aria-describedby");
  });

  // EDT-FR-20 + EDT-FR-60: an edit made during a transient peek is on the bar
  // when the peek re-collapses — the reading is recomputed, not captured.
  it("carries an edit made during a peek back onto the re-collapsed bar", async () => {
    const { container } = await mountWithFm(
      `description: ${"x".repeat(412)}`,
      "# H\n\n" + "body\n\n".repeat(40),
    );
    const bar = await collapse(container);
    expect(countLine()?.textContent).toBe("412/1024");

    fireEvent.click(bar); // peek open
    const fm = (await screen.findByLabelText(
      "Frontmatter",
    )) as HTMLTextAreaElement;
    fireEvent.change(fm, { target: { value: `description: ${"x".repeat(500)}` } });

    // Scrolling further re-collapses the peek (scroll is authoritative).
    await collapse(container, 400);
    expect(countLine()?.textContent).toBe("500/1024");
    expect(countLine()?.getAttribute("data-over")).toBe("false");
  });

  // EDT-FR-59: the reading follows the buffer even when the region is never
  // expanded in between — a reload that replaces the description while the bar
  // is showing updates the bar.
  it("refreshes the collapsed reading when a reload replaces the description", async () => {
    const backend: Backend = {
      load: {
        body: `---\ndescription: ${"x".repeat(412)}\n---\n\n# H\n`,
        checksum: "ck1",
      },
    };
    wireBackend(backend);
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByLabelText("Frontmatter");
    await collapse(container);
    expect(countLine()?.textContent).toBe("412/1024");

    // The file changes on disk; the user takes the new version.
    backend.load = {
      body: `---\ndescription: ${"z".repeat(900)}\n---\n\n# H\n`,
      checksum: "ck2",
    };
    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });
    fireEvent.click(
      await screen.findByRole("button", { name: "Load from filesystem" }),
    );

    // Still collapsed — so the reading can only have come from the new buffer.
    await waitFor(() => expect(countLine()?.textContent).toBe("900/1024"));
    expect(
      screen.getByRole("button", { name: "Expand frontmatter" }),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();
  });

  // EDT-FR-59: the summary text is what gives way when the bar runs out of
  // room. jsdom applies no CSS, so this pins the DOM the `flex` rule needs —
  // the reading is the trailing sibling of the flexible preview, and complete.
  it("keeps the full reading on the bar behind a summary long enough to truncate", async () => {
    const { container } = await mountWithFm(
      `name: ${"n".repeat(600)}\ndescription: ${"x".repeat(412)}`,
    );
    const bar = await collapse(container);

    expect(bar.textContent?.endsWith("412/1024")).toBe(true);
    expect(Array.from(bar.children).map((c) => c.className)).toEqual([
      "editor__frontmatter-label",
      "editor__frontmatter-preview",
      "editor__frontmatter-count",
    ]);
  });

  // EDT-FR-21, ESH-FR-ATDS, ESH-FR-BLTT / EDT-FR-59: the line follows the `description` key, not the
  // artifact's type — absent when the key is missing, present on a type other
  // than Skill.
  it("shows the count for any frontmatter carrying a description, and none without", async () => {
    await mountWithFm("name: onboarding\ntags:\n  - setup");
    expect(countLine()).toBeNull();
    expect(textOf(".editor__frontmatter-key")).toEqual(["name", "tags"]);
    cleanup();

    // An untyped Markdown file and an Agent both get the line: the reading
    // follows the key rather than the type (EDT-FR-59, ESH-FR-FKZB).
    wireBackend({
      load: { body: "---\ndescription: note\n---\n\n# H\n", checksum: "ck1" },
    });
    render(<Editor artifactId="notes.md" artifactName="notes.md" />);
    await screen.findByLabelText("Frontmatter");
    expect(countLine()?.textContent).toBe("4/1024");
    cleanup();

    wireBackend({
      load: { body: "---\ndescription: agent one\n---\n\n# H\n", checksum: "ck1" },
    });
    render(
      <Editor
        artifactId="b.md"
        artifactName="b.md"
        artifactType="agent"
      />,
    );
    await screen.findByLabelText("Frontmatter");
    expect(countLine()?.textContent).toBe("9/1024");
  });

  // EDT-FR-60: the count measures the parsed value — a `>` block
  // scalar folded, its indentation and indicator excluded — and tracks edits.
  it("counts the folded block-scalar value, not the source characters", async () => {
    // Four indented lines; folding joins them with single spaces and clips one
    // trailing newline, so the value is 267 + 1 = 268 characters.
    const lines = ["a".repeat(66), "b".repeat(66), "c".repeat(66), "d".repeat(66)];
    const folded = `${lines.join(" ")}\n`;
    expect(folded).toHaveLength(268);
    const fm = `description: >\n${lines.map((l) => `  ${l}`).join("\n")}`;
    expect(fm.length).toBeGreaterThan(folded.length); // indicator + indentation

    await mountWithFm(fm);
    expect(countLine()?.textContent).toBe("268/1024");

    // Twelve more characters typed into the value move the count by twelve.
    const fmInput = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    fireEvent.change(fmInput, {
      target: { value: `${fm}${"e".repeat(12)}` },
    });
    expect(countLine()?.textContent).toBe("280/1024");
  });

  // EDT-FR-60: the count is recomputed from the buffer whatever surface changed
  // it — an edit made in raw-text mode is reflected the moment the region shows.
  it("recomputes the count from a description edited in raw-text mode", async () => {
    await mountWithFm(`name: onboarding\ndescription: ${"x".repeat(412)}`);
    expect(countLine()?.textContent).toBe("412/1024");

    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;

    // Rewrite the whole file in the source surface, past the limit this time.
    const long = "z".repeat(1100);
    fireEvent.change(source, {
      target: { value: `---\nname: onboarding\ndescription: ${long}\n---\n\n# H\n` },
    });
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));

    await screen.findByLabelText("Frontmatter");
    expect(countLine()?.textContent).toBe("1100/1024");
    expect(countLine()?.getAttribute("data-over")).toBe("true");
  });

  // EDT-FR-60 (boundary): the warning treatment starts strictly above the limit.
  it("warns above the limit, not at it", async () => {
    await mountWithFm(`description: ${"x".repeat(1024)}`);
    expect(countLine()?.textContent).toBe("1024/1024");
    expect(countLine()?.getAttribute("data-over")).toBe("false");

    fireEvent.change(screen.getByLabelText("Frontmatter"), {
      target: { value: `description: ${"x".repeat(1025)}` },
    });
    expect(countLine()?.textContent).toBe("1025/1024");
    expect(countLine()?.getAttribute("data-over")).toBe("true");
  });

  // EDT-FR-60: over the budget the line warns and keeps reading the
  // true count — it gates nothing, so Save writes the long description as it is.
  it("warns over the limit without blocking the save", async () => {
    const description = "y".repeat(1180);
    const fm = `name: onboarding\ndescription: ${description}`;
    const { backend } = await mountWithFm(fm);

    expect(countLine()?.textContent).toBe("1180/1024");
    expect(countLine()?.getAttribute("data-over")).toBe("true");

    await saveNow();
    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    // No confirmation stood between the click and the write, and the
    // over-length description reached disk unchanged.
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(backend.lastSavedBody as string).toContain(description);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_artifact_contents"),
    ).toHaveLength(1);
  });

  // EDT-FR-59 (undo half) / EDT-FR-60: the count line is chrome — shortening the
  // description is one undoable edit, and undoing it restores the reading.
  it("follows the description through an edit and its undo", async () => {
    await mountWithFm("name: onboarding\ndescription: " + "x".repeat(412));
    expect(countLine()?.textContent).toBe("412/1024");

    const fm = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    fireEvent.change(fm, {
      target: { value: "name: onboarding\ndescription: short" },
    });
    expect(countLine()?.textContent).toBe("5/1024");

    pressUndo();
    const restored = "name: onboarding\ndescription: " + "x".repeat(412);
    expect(
      (screen.getByLabelText("Frontmatter") as HTMLTextAreaElement).value,
    ).toBe(restored);
    expect(countLine()?.textContent).toBe("412/1024");

    // The reading itself took no position in the history: with the one edit
    // reversed, the next undo is a no-op at the floor (EDT-FR-23, EDT-FR-24).
    pressUndo();
    expect(
      (screen.getByLabelText("Frontmatter") as HTMLTextAreaElement).value,
    ).toBe(restored);
    expect(countLine()?.textContent).toBe("412/1024");
  });
});
