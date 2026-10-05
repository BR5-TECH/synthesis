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
import { letWriteLand } from "../test/autosave";

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

describe("Editor (WYSIWYG integration)", () => {
  // EDT-FR-18: leading YAML frontmatter is a visually distinct,
  // editable region — never rendered as body content — and is preserved on save.
  it("renders frontmatter as a distinct region and preserves it on save", async () => {
    const original = "---\ntitle: Doc\ntags: [a, b]\n---\n\n# Heading\n\nbody";
    const backend: Backend = { load: { body: original, checksum: "ck1" } };
    wireBackend(backend);
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    const heading = await screen.findByRole("heading", { name: "Heading" });
    expect(heading.tagName).toBe("H1");

    // Frontmatter lives in its own editable region with the YAML inner text.
    const fmRegion = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    expect(fmRegion.value).toContain("title: Doc");

    // It is NOT interpreted as body content: no <hr> and no YAML in the prose.
    const prose = container.querySelector(".editor__prose")!;
    expect(prose.querySelector("hr")).toBeNull();
    expect(prose.textContent).not.toContain("title: Doc");

    // Saving without touching the frontmatter preserves the block verbatim.
    await saveNow();
    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    const saved = backend.lastSavedBody as string;
    expect(saved.startsWith("---\ntitle: Doc\ntags: [a, b]\n---\n")).toBe(true);
    expect(saved).toContain("# Heading");
  });

  // EDT-FR-18 / EDT-FR-21: in the WYSIWYG frontmatter region the YAML keys render
  // bold and the values render normal weight, via an aria-hidden highlight layer
  // behind the editable textarea. The styling is presentation-only — it changes no
  // bytes (round-trip on save is preserved) and the overlay reproduces the buffer
  // verbatim. Layout/weight pixels are a browser concern jsdom can't measure, so
  // we assert the structural contract: keys (and only keys) are wrapped in the
  // bold element, including nested keys, and never the values.
  it("bolds YAML keys and leaves values normal in the WYSIWYG frontmatter", async () => {
    // Exercise the key/value clauses of EDT-FR-21 in one fixture:
    //   name/description           — top-level keys
    //   description: "a: b"        — colon inside a quoted value: only the key is a key
    //   meta → author → nested → deep — keys at every nesting depth (bold)
    //   list: \n   - foo: bar      — list-of-maps: `foo` is a key; the `- ` marker is not
    //   nospace: a:b               — `a:b` is a plain scalar value, not a nested key
    //   items: \n  - item          — bare list scalar → no key
    const fmBlock =
      'name: Foo\ndescription: "a: b"\nmeta:\n  author: bob\n  nested:\n    deep: v\nlist:\n  - foo: bar\nnospace: a:b\nitems:\n  - item';
    const original = `---\n${fmBlock}\n---\n\n# H\n`;
    const backend: Backend = { load: { body: original, checksum: "ck1" } };
    wireBackend(backend);
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByRole("heading", { name: "H" });

    // The highlight layer exists, is hidden from assistive tech (the editable
    // textarea is the accessible surface), and sits inside the region.
    const hl = container.querySelector(".editor__frontmatter-hl")!;
    expect(hl).toBeTruthy();
    expect(hl.getAttribute("aria-hidden")).toBe("true");

    // The bold elements are exactly the mapping keys, at every nesting depth and
    // in document order — including the `- foo:` list-of-maps key.
    const keys = Array.from(
      hl.querySelectorAll(".editor__frontmatter-key"),
    ) as HTMLElement[];
    const keyTexts = keys.map((k) => k.textContent);
    expect(keyTexts).toEqual([
      "name",
      "description",
      "meta",
      "author",
      "nested",
      "deep",
      "list",
      "foo",
      "nospace",
      "items",
    ]);
    // Each key is a <strong> (semantically bold, independent of CSS — jsdom loads
    // no stylesheet, so a computed font-weight assertion would be vacuous), and
    // the bold text is the key glyphs ALONE: the `:` separator and the `- ` list
    // marker stay out of the bold span (EDT-FR-21).
    keys.forEach((k) => {
      expect(k.tagName).toBe("STRONG");
      expect(k.textContent).not.toMatch(/[:\s-]/);
    });
    // Values are never keys: the quoted `a: b`, the plain scalar `a:b`, and the
    // bare list scalar `item` are all unbolded.
    ["a", "b", "a:b", "v", "bar", "bob", "item"].forEach((t) =>
      expect(keyTexts).not.toContain(t),
    );

    // Presentation-only: the overlay reproduces the textarea buffer byte-for-byte
    // (the <strong> wraps only key glyphs, inserting nothing).
    const fm = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    expect(hl.textContent).toBe(fm.value);
    expect(fm.value).toBe(fmBlock);

    // And an untouched save preserves the frontmatter block verbatim (EDT-FR-18):
    // the highlight is presentation-only and touches no bytes. (Only the block is
    // asserted byte-exact — the body round-trips through WYSIWYG serialization,
    // same as EDT-FR-18.)
    await saveNow();
    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    expect((backend.lastSavedBody as string).startsWith(`---\n${fmBlock}\n---\n`)).toBe(
      true,
    );
  });

  // EDT-FR-18 / EDT-FR-21 (negative — named exclusion): the collapsed single-line
  // summary bar is NOT key-styled. Scrolling the body down collapses the region
  // (EDT-FR-20); the highlight layer and its bold key spans must disappear.
  it("does not key-style the collapsed frontmatter summary bar", async () => {
    wireLoad({ body: "---\nname: Foo\nflavor: 2\n---\n\n# H", checksum: "ck1" });
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    // Expanded at the top: the highlight layer is present.
    expect(container.querySelector(".editor__frontmatter-hl")).toBeTruthy();

    // Collapse by scrolling down (jsdom needs scrollTop forced before the event).
    const scroller = container.querySelector(".editor") as HTMLElement;
    Object.defineProperty(scroller, "scrollTop", {
      configurable: true,
      value: 120,
    });
    fireEvent.scroll(scroller);

    await screen.findByRole("button", { name: "Expand frontmatter" });
    // No highlight layer and no bold key spans on the summary bar.
    expect(container.querySelector(".editor__frontmatter-hl")).toBeNull();
    expect(container.querySelector(".editor__frontmatter-key")).toBeNull();
  });

  // EDT-FR-18 / EDT-FR-21 (negative — named exclusion): raw-text (source) mode is
  // NOT key-styled. The frontmatter is the literal leading lines of the source
  // surface (EDT-FR-17); there is no highlight layer and no bold key spans.
  it("does not key-style frontmatter in raw-text mode", async () => {
    wireLoad({ body: "---\nname: Foo\n---\n\n# H", checksum: "ck1" });
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    expect(container.querySelector(".editor__frontmatter-hl")).toBeTruthy();

    // Toggle to raw Markdown source.
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );

    // No highlight layer / key spans, and the frontmatter is inline in the source.
    expect(container.querySelector(".editor__frontmatter-hl")).toBeNull();
    expect(container.querySelector(".editor__frontmatter-key")).toBeNull();
    const source = screen.getByLabelText("Markdown source") as HTMLTextAreaElement;
    expect(source.value).toContain("---\nname: Foo\n---");
  });

  // EDT-FR-16 / ESH-FR-ROWK: the cluster holds the same two controls
  // whatever type the open file resolved as — and no artifact-scoped action of
  // any kind. Driven across every type so a reintroduced per-type gate is caught.
  it.each([
    "skill",
    "agent",
    "prompt",
    "instructions",
    "scenario",
    "spec",
    "scratchpad",
    // ESH-FR-ATDS: a real filesystem node with NO artifact type — a plain text
    // file — is on exactly the same terms.
    undefined,
  ] as const)("the cluster is the same for type %s", async (type) => {
    wireLoad({ body: "# H", checksum: "ck1" });
    render(
      <Editor artifactId="t.md" artifactName="t.md" artifactType={type} />,
    );
    await screen.findByRole("heading", { name: "H" });

    expect(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Inject" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
  });

  // EDT-FR-18 (negative): a file with no frontmatter shows no region, and a
  // mid-body `---` is treated as body content (a thematic break), not mistaken
  // for frontmatter.
  it("shows no frontmatter region for a file without frontmatter", async () => {
    wireLoad({ body: "# Heading\n\nbody only", checksum: "ck1" });
    render(
      <Editor artifactId="a.md" artifactName="a.md" />,
    );
    await screen.findByRole("heading", { name: "Heading" });
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();
  });

  it("treats a mid-body --- as a thematic break, not frontmatter", async () => {
    wireLoad({ body: "intro\n\n---\n\nmore", checksum: "ck1" });
    const { container } = render(
      <Editor artifactId="a.md" artifactName="a.md" />,
    );
    await screen.findByText("intro");
    // No frontmatter region; the `---` rendered as an <hr> inside the body.
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();
    expect(container.querySelector(".editor__prose hr")).not.toBeNull();
  });

  // EDT-FR-18 / EDT-FR-04: editing the frontmatter region marks the doc dirty and
  // saves the reconstructed (edited) block — the join's reconstruct branch.
  it("saves the edited frontmatter and marks the doc dirty", async () => {
    const backend: Backend = {
      load: { body: "---\ntitle: Old\n---\n\n# H\n\nbody", checksum: "ck1" },
    };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });

    const fm = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    fireEvent.change(fm, { target: { value: "title: New" } });
    expect(screen.getByText("● unsaved")).toBeInTheDocument(); // EDT-FR-04

    await letWriteLand();
    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    const saved = backend.lastSavedBody as string;
    // Reconstructed block carries the new value, in canonical fence shape.
    expect(saved.startsWith("---\ntitle: New\n---\n")).toBe(true);
    expect(saved).not.toContain("title: Old");
    expect(saved).toContain("# H");
  });

  // EDT-FR-17 (lossless): toggling modes on a freshly loaded doc must NOT mark it
  // dirty — the explicit guarantee the emitUpdate:false plumbing exists to keep.
  it("toggling modes on a clean doc does not mark it dirty", async () => {
    wireLoad({ body: "# Title\n\nhello", checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "Title" });
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    await screen.findByLabelText("Markdown source");
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
  });

  // EDT-FR-18: the WYSIWYG frontmatter region does not scroll — it
  // grows to fit every line (no vertical scrollbar) and soft-wraps long lines (no
  // horizontal scrollbar), and is not user-resizable. All content stays present.
  it("renders the frontmatter region as non-scrolling and soft-wrapping", async () => {
    const longLine = "description: " + "x".repeat(200);
    wireLoad({
      body: `---\ntitle: A\n${longLine}\nmore: 1\n---\n\n# H`,
      checksum: "ck1",
    });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    const fm = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    // No scrollbars and not manually resizable — the box is exactly content-tall.
    expect(fm.style.overflow).toBe("hidden");
    expect(fm.style.resize).toBe("none");
    // The autosize effect ran: it starts from rows={1} and writes an explicit
    // height (jsdom reports scrollHeight 0, so we assert the height was *set*,
    // not its pixel value — real growth is a layout concern jsdom can't measure).
    expect(fm.getAttribute("rows")).toBe("1");
    expect(fm.style.height).not.toBe("");
    // Nothing is clipped out of the value — every line is present.
    expect(fm.value).toContain(longLine);
    expect(fm.value).toContain("more: 1");
  });

  // EDT-FR-18 (Critical regression guard): the formatting toolbar's active state
  // must stay in sync with the editor. Tiptap v3 only re-renders on transactions
  // when asked, so this pins shouldRerenderOnTransaction.
  it("keeps the formatting toolbar active-state in sync with edits", async () => {
    wireLoad({ body: "hello world", checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByText("hello world");
    const bold = screen.getByTitle("Bold");
    expect(bold.getAttribute("data-active")).toBe("false");
    // Toolbar buttons fire on mouseDown (preventing focus theft).
    fireEvent.mouseDown(bold);
    await waitFor(() => expect(bold.getAttribute("data-active")).toBe("true"));
  });

  // EDT-FR-17 (byte fidelity through a toggle): entering text mode reproduces the
  // file verbatim — no Tiptap re-serialization drift — and an untouched save from
  // text mode writes the original bytes back unchanged, frontmatter included.
  it("keeps frontmatter-file bytes exact through a toggle and an untouched source save", async () => {
    const original = "---\ntitle: Doc\n---\n\n# H\n\n- a\n- b\n";
    const backend: Backend = { load: { body: original, checksum: "ck1" } };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    expect(source.value).toBe(original); // no drift toggling into text mode

    await saveNow();
    await waitFor(() => expect(backend.lastSavedBody).toBe(original));
  });

  // EDT-FR-17 (reload while in text mode): a Load-from-filesystem resolution that
  // lands while the user is in source mode refreshes the source surface to the
  // new on-disk file.
  it("refreshes the source surface on Load-from-filesystem while in text mode", async () => {
    const backend: Backend = {
      load: { body: "---\ntitle: One\n---\n\n# One", checksum: "ck1" },
    };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "One" });
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("---\ntitle: One\n---\n\n# One");

    // External change arrives while in text mode -> resolve by loading from disk.
    backend.load = { body: "---\ntitle: Two\n---\n\n# Two", checksum: "ck2" };
    listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
      payload: { artifactId: "a.md", checksum: "ck2" },
    });
    await screen.findByRole("dialog");
    fireEvent.click(
      screen.getByRole("button", { name: "Load from filesystem" }),
    );

    await waitFor(() =>
      expect(
        (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
      ).toBe("---\ntitle: Two\n---\n\n# Two"),
    );
  });

  // EDT-FR-18 / EDT-FR-17+18: in raw-text mode the source surface holds the whole
  // file (frontmatter inline, no separate region); editing the frontmatter there
  // and saving writes the edited frontmatter as the leading lines of the file.
  it("edits frontmatter inline in source mode and saves the whole file", async () => {
    const original = "---\ntitle: Old\n---\n\n# H\n\nbody";
    const backend: Backend = { load: { body: original, checksum: "ck1" } };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });

    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    // The full file (frontmatter + body) is shown; no separate region in text mode.
    expect(source.value).toBe(original);
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();

    // Edit the frontmatter line right in the source and save -> byte-exact write.
    const edited = original.replace("title: Old", "title: New");
    fireEvent.change(source, { target: { value: edited } });
    await letWriteLand();
    await waitFor(() => expect(backend.lastSavedBody).toBe(edited));
  });

  // EDT-FR-17/18: an inline frontmatter edit made in text mode is reflected in the
  // WYSIWYG region after toggling back (the source is re-split into the region).
  it("carries an inline frontmatter edit back into the WYSIWYG region", async () => {
    wireLoad({ body: "---\ntitle: Old\n---\n\n# H", checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });

    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    fireEvent.change(source, {
      target: { value: "---\ntitle: New\n---\n\n# H" },
    });

    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    const fm = (await screen.findByLabelText(
      "Frontmatter",
    )) as HTMLTextAreaElement;
    expect(fm.value).toBe("title: New");
  });

  // EDT-FR-16/17: the mode toggle's accessible label flips with the active mode.
  it("flips the mode-toggle label with the active mode", async () => {
    wireLoad({ body: "# T", checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "T" });
    expect(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Edit as rich text" }),
    ).not.toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    expect(
      screen.getByRole("button", { name: "Edit as rich text" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Edit as Markdown source" }),
    ).not.toBeInTheDocument();
  });

  // jsdom does no layout, so scrollTop must be forced before dispatching scroll.
  function scrollTo(el: HTMLElement, top: number) {
    Object.defineProperty(el, "scrollTop", { configurable: true, value: top });
    fireEvent.scroll(el);
  }
  // The editable textarea (aria "Frontmatter") renders only when expanded; the
  // collapsed bar is the "Expand frontmatter" button. Their presence is the mode.
  const fmFile = "---\ntitle: A\nflavor: 2\n---\n\n# H";

  // EDT-FR-20: at the top the region is expanded; scrolling the body
  // down collapses it to the single line; scrolling back to the top re-expands.
  it("collapses the frontmatter on scroll-down and expands at the top", async () => {
    wireLoad({ body: fmFile, checksum: "ck1" });
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    const scroller = container.querySelector(".editor") as HTMLElement;
    // At top -> expanded (editable region present).
    expect(screen.getByLabelText("Frontmatter")).toBeInTheDocument();

    scrollTo(scroller, 120);
    await waitFor(() =>
      expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument(),
    );
    expect(
      screen.getByRole("button", { name: "Expand frontmatter" }),
    ).toBeInTheDocument();

    scrollTo(scroller, 0);
    await waitFor(() =>
      expect(screen.getByLabelText("Frontmatter")).toBeInTheDocument(),
    );
  });

  // EDT-FR-20: clicking the collapsed line expands a transient peek;
  // a further scroll-down dismisses it (scroll position is authoritative).
  it("expands a transient peek on click, dismissed by further scroll", async () => {
    wireLoad({ body: fmFile, checksum: "ck1" });
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    const scroller = container.querySelector(".editor") as HTMLElement;

    scrollTo(scroller, 120); // collapse
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Expand frontmatter" }),
      ).toBeInTheDocument(),
    );

    fireEvent.click(screen.getByRole("button", { name: "Expand frontmatter" }));
    expect(await screen.findByLabelText("Frontmatter")).toBeInTheDocument(); // peek

    scrollTo(scroller, 260); // keep scrolling -> peek dismissed
    await waitFor(() =>
      expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument(),
    );
  });

  // EDT-FR-20: the minimize button collapses the expanded region.
  it("collapses the expanded region via the minimize button", async () => {
    wireLoad({ body: fmFile, checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    // At top -> expanded, with the minimize button present.
    expect(screen.getByLabelText("Frontmatter")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Minimize frontmatter" }));
    await waitFor(() =>
      expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument(),
    );
    expect(
      screen.getByRole("button", { name: "Expand frontmatter" }),
    ).toBeInTheDocument();
  });

  // EDT-FR-20 (scroll wins over minimize): a minimized region re-expands once the
  // body is scrolled away and back to the top.
  it("re-expands on returning to the top after a manual minimize", async () => {
    wireLoad({ body: fmFile, checksum: "ck1" });
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    const scroller = container.querySelector(".editor") as HTMLElement;

    fireEvent.click(screen.getByRole("button", { name: "Minimize frontmatter" }));
    await waitFor(() =>
      expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument(),
    );
    // Scroll down (dismisses the manual state) then back to the top -> expanded.
    scrollTo(scroller, 150);
    scrollTo(scroller, 0);
    await waitFor(() =>
      expect(screen.getByLabelText("Frontmatter")).toBeInTheDocument(),
    );
  });

  // EDT-FR-20: the collapsed bar stays identifiable as frontmatter and previews
  // the first non-blank line; the minimize button is not on the collapsed bar.
  it("the collapsed bar identifies itself as frontmatter and previews content", async () => {
    wireLoad({ body: fmFile, checksum: "ck1" }); // inner: title: A / flavor: 2
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    scrollTo(container.querySelector(".editor") as HTMLElement, 120);
    const bar = await screen.findByRole("button", { name: "Expand frontmatter" });
    expect(bar.textContent).toContain("frontmatter");
    expect(bar.textContent).toContain("title: A"); // first non-blank line
    expect(
      screen.queryByRole("button", { name: "Minimize frontmatter" }),
    ).not.toBeInTheDocument();
  });

  // EDT-FR-20: the preview skips leading blank lines in the frontmatter.
  it("the collapsed preview skips a leading blank frontmatter line", async () => {
    wireLoad({ body: "---\n\ntitle: B\n---\n\n# H", checksum: "ck1" });
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    scrollTo(container.querySelector(".editor") as HTMLElement, 120);
    const bar = await screen.findByRole("button", { name: "Expand frontmatter" });
    expect(bar.textContent).toContain("title: B");
  });

  // EDT-FR-20 (presentation-only): an unsaved frontmatter edit survives the
  // collapse/expand remount and saves uncorrupted.
  it("preserves an unsaved frontmatter edit across minimize and expand, and saves it", async () => {
    const backend: Backend = {
      load: { body: "---\ntitle: Old\n---\n\n# H\n\nbody", checksum: "ck1" },
    };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });

    const fm = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    fireEvent.change(fm, { target: { value: "title: New" } });
    expect(screen.getByText("● unsaved")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Minimize frontmatter" }));
    await waitFor(() =>
      expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Expand frontmatter" }));
    const fm2 = (await screen.findByLabelText(
      "Frontmatter",
    )) as HTMLTextAreaElement;
    expect(fm2.value).toBe("title: New"); // edit survived the remount

    await letWriteLand();
    await waitFor(() => expect(backend.lastSavedBody).toBeTruthy());
    expect(
      (backend.lastSavedBody as string).startsWith("---\ntitle: New\n---\n"),
    ).toBe(true);
  });

  // EDT-FR-20: the at/just-past-threshold boundary (scrollTop <= 8 is "top").
  it("treats scrollTop at the threshold as top and just past it as scrolled", async () => {
    wireLoad({ body: fmFile, checksum: "ck1" });
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    const scroller = container.querySelector(".editor") as HTMLElement;
    scrollTo(scroller, 8); // <= 8 -> still top -> expanded
    expect(screen.getByLabelText("Frontmatter")).toBeInTheDocument();
    scrollTo(scroller, 9); // > 8 -> collapsed
    await waitFor(() =>
      expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument(),
    );
  });

  // EDT-FR-20: a file without frontmatter has no region/bar at any scroll position.
  it("a file without frontmatter shows no region even when scrolled down", async () => {
    wireLoad({ body: "# Heading\n\nbody only", checksum: "ck1" });
    const { container } = render(
      <Editor artifactId="a.md" artifactName="a.md" />,
    );
    await screen.findByRole("heading", { name: "Heading" });
    scrollTo(container.querySelector(".editor") as HTMLElement, 200);
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Expand frontmatter" }),
    ).not.toBeInTheDocument();
  });

  // EDT-FR-20 (WYSIWYG-only): raw-text mode has no collapse affordance at all.
  it("shows no collapse affordance in raw-text mode", async () => {
    wireLoad({ body: fmFile, checksum: "ck1" });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    await screen.findByLabelText("Markdown source");
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Expand frontmatter" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Minimize frontmatter" }),
    ).not.toBeInTheDocument();
  });

  // EDT-FR-20: a mode round-trip re-baselines the collapse state to expanded
  // (the body scroll container's scrollTop is reset by display:none).
  it("re-baselines the frontmatter to expanded after a mode round-trip", async () => {
    wireLoad({ body: fmFile, checksum: "ck1" });
    const { container } = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
      />,
    );
    await screen.findByRole("heading", { name: "H" });
    scrollTo(container.querySelector(".editor") as HTMLElement, 150);
    await waitFor(() =>
      expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument(),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    await screen.findByLabelText("Markdown source");
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    expect(await screen.findByLabelText("Frontmatter")).toBeInTheDocument();
  });
});
