import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { CURRENT_MATCH_CLASS, MATCH_CLASS } from "./findHighlight";
import type { ArtifactChangedPayload, ArtifactContents } from "../types";
import { ARTIFACT_CHANGED_EXTERNALLY } from "../events";
import { letWriteLand } from "../test/autosave";

/**
 * `EFR-editor-find-replace.md`: the Find and Find & Replace panels, driven through the
 * real Editor.
 *
 * The accelerators themselves are delivered by the native Edit menu (SNV-FR-43)
 * and routed to the artifact's find state by the shell, so these tests drive the
 * same store mutation the shell performs — `sessions.setFind` — rather than
 * synthesising a ⌘F the webview never receives while a native menu owns the
 * chord. The menu wiring itself is covered in `App.menu.test.tsx` and the Rust
 * suite.
 */
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

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
});

afterEach(() => {
  cleanup();
});

/** What the shell does when a find accelerator fires (SNV-FR-43). */
function setFind(
  sessions: EditSessionStore,
  patch: Parameters<EditSessionStore["setFind"]>[1],
) {
  act(() => {
    sessions.setFind("a.md", patch);
  });
}

/** Mount an Editor on `body`, already switched into raw-text mode. */
async function renderInTextMode(body: string) {
  const sessions = new EditSessionStore();
  const backend: Backend = { load: { body, checksum: "ck1" } };
  wireBackend(backend);
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      artifactType="skill"
      sessions={sessions}
    />,
  );
  await screen.findByRole("button", { name: "Edit as Markdown source" });
  fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
  const source = (await screen.findByLabelText(
    "Markdown source",
  )) as HTMLTextAreaElement;
  return { sessions, backend, source };
}

const query = () => screen.getByLabelText("Find") as HTMLInputElement;
const replacementInput = () =>
  screen.getByLabelText("Replace with") as HTMLInputElement;
const count = () => screen.getByTestId("find-count").textContent;
const marks = () =>
  Array.from(
    document.querySelectorAll<HTMLElement>(`.${MATCH_CLASS}`),
  );
const currentMarkText = () =>
  document.querySelector<HTMLElement>(`.${CURRENT_MATCH_CLASS}`)?.textContent ??
  null;
/**
 * The rich body's rendered text. Read off the container rather than through
 * `getByText`, because the match decorations split it across several elements.
 */
const bodyText = () =>
  document.querySelector<HTMLElement>(".editor__prose")?.textContent ?? "";


/**
 * The find panel over the rich surfaces — the WYSIWYG body, the frontmatter
 * region, and what survives a tab change.
 *
 * One part of the group `./Editor.find.test.tsx` heads; the mock wiring above is
 * the same, and is repeated because `vi.mock` is hoisted per file.
 */

describe("the WYSIWYG surface (EFR-FR-DDUX, EFR-FR-DOQR)", () => {
  /** Mount an Editor on `body` and leave it in the default WYSIWYG mode. */
  async function renderWysiwyg(body: string) {
    const sessions = new EditSessionStore();
    const backend: Backend = { load: { body, checksum: "ck1" } };
    wireBackend(backend);
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    return { sessions, backend };
  }

  it("matches the rendered rich text, not the Markdown syntax", async () => {
    // EFR-FR-DDUX: in WYSIWYG the active surface is what the user sees, so the
    // `#` and `**` that produced it are not part of what a query searches.
    const { sessions } = await renderWysiwyg("# Heading\n\nA **bold** claim.");
    await screen.findByRole("heading", { name: "Heading" });

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "#" } });
    expect(count()).toBe("No results");

    fireEvent.change(query(), { target: { value: "bold" } });
    expect(count()).toBe("1/1");
  });

  it("decorates every match inside the rich body", async () => {
    const { sessions } = await renderWysiwyg("alpha beta alpha gamma alpha");
    await waitFor(() => expect(bodyText()).toContain("alpha beta alpha"));

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });

    await waitFor(() => expect(marks()).toHaveLength(3));
    expect(count()).toBe("1/3");
    expect(currentMarkText()).toBe("alpha");
    expect(marks().every((m) => m.textContent === "alpha")).toBe(true);
  });

  it("moves the current decoration as the user navigates", async () => {
    const { sessions } = await renderWysiwyg("one two one two one");
    await waitFor(() => expect(bodyText()).toContain("one two one"));
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "two" } });
    await waitFor(() => expect(marks()).toHaveLength(2));

    const before = marks().findIndex((m) =>
      m.classList.contains(CURRENT_MATCH_CLASS),
    );
    fireEvent.click(screen.getByRole("button", { name: "Next match" }));
    await waitFor(() => {
      const after = marks().findIndex((m) =>
        m.classList.contains(CURRENT_MATCH_CLASS),
      );
      expect(after).not.toBe(before);
    });
    expect(count()).toBe("2/2");
  });

  it("finds matches across separate blocks but never spanning them", async () => {
    const { sessions } = await renderWysiwyg("# hit\n\nanother hit here");
    await screen.findByRole("heading", { name: "hit" });

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "hit" } });
    await waitFor(() => expect(count()).toBe("1/2"));

    // The heading ends "hit" and the paragraph begins "another"; a query
    // straddling the boundary must not match.
    fireEvent.change(query(), { target: { value: "hitanother" } });
    expect(count()).toBe("No results");
  });

  it("replaces in the rich body and keeps it one undo step", async () => {
    const { sessions, backend } = await renderWysiwyg("red red red red");
    await screen.findByText("red red red red");

    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "red" } });
    fireEvent.change(replacementInput(), { target: { value: "blue" } });
    await waitFor(() => expect(count()).toBe("1/4"));

    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));

    await waitFor(() => expect(bodyText()).toBe("blue blue blue blue"));
    expect(screen.getByText("● unsaved")).toBeInTheDocument();

    // One undo restores the whole sweep (EFR-FR-FYRJ).
    fireEvent.keyDown(screen.getByLabelText("artifact body"), {
      key: "z",
      metaKey: true,
    });
    await waitFor(() => expect(bodyText()).toBe("red red red red"));

    await letWriteLand();
    await waitFor(() => expect(backend.lastSavedBody).toContain("red red red red"));
  });

  it("replaces only the current match with Replace", async () => {
    const { sessions } = await renderWysiwyg("keep swap keep swap");
    await waitFor(() => expect(bodyText()).toBe("keep swap keep swap"));

    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "swap" } });
    fireEvent.change(replacementInput(), { target: { value: "DONE" } });
    await waitFor(() => expect(count()).toBe("1/2"));

    fireEvent.click(screen.getByRole("button", { name: "Replace" }));

    await waitFor(() => expect(bodyText()).toBe("keep DONE keep swap"));
  });
});

describe("the frontmatter region (EFR-FR-DDUX, EFR-FR-DSKI)", () => {
  const FM = "---\nname: alpha\ndescription: an alpha artifact\n---\n";

  async function renderWithFrontmatter(body = "the body mentions alpha too") {
    const sessions = new EditSessionStore();
    const backend: Backend = { load: { body: FM + body, checksum: "ck1" } };
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
    return { sessions, backend };
  }

  it("counts frontmatter matches ahead of body matches, in render order", async () => {
    const { sessions } = await renderWithFrontmatter();
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });

    // Two in the frontmatter (`name:` and `description:`), one in the body.
    await waitFor(() => expect(count()).toBe("1/3"));
    const first = document.querySelector(`.${CURRENT_MATCH_CLASS}`);
    // The region renders above the body, so its matches are numbered first.
    expect(
      document
        .querySelector(".editor__frontmatter-hl")
        ?.contains(first ?? null),
    ).toBe(true);
  });

  it("marks matches on the same layer that bolds the YAML keys", async () => {
    const { sessions } = await renderWithFrontmatter();
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });
    await waitFor(() => expect(count()).toBe("1/3"));

    const layer = document.querySelector(".editor__frontmatter-hl")!;
    // EDT-FR-21 survives: the keys are still bold…
    expect(
      Array.from(layer.querySelectorAll(".editor__frontmatter-key")).map(
        (k) => k.textContent,
      ),
    ).toEqual(["name", "description"]);
    // …and the layer still reproduces the buffer byte-for-byte, so it stays
    // registered under the textarea's glyphs.
    const fm = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    expect(layer.textContent).toBe(fm.value);
  });

  // EDT-FR-60 (find half) / EDT-FR-59: the description count line is region
  // chrome, not content — the panel matches the buffer, never the reading. The
  // body carries the same string, so the query does match something: what the
  // test pins is that the one match is the body's and not the chrome's.
  it("never matches the description count line", async () => {
    const sessions = new EditSessionStore();
    const description = "an alpha artifact";
    const reading = `${description.length}/1024`;
    wireBackend({
      load: {
        body: `---\nname: alpha\ndescription: ${description}\n---\n\nthe body says ${reading} too\n`,
        checksum: "ck1",
      },
    });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByLabelText("Frontmatter");
    const countLine = document.querySelector(".editor__frontmatter-count")!;
    expect(countLine.textContent).toBe(reading);

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: reading } });

    // Exactly one match, in the body — the identical text on the chrome line is
    // not part of the buffer and so is neither counted nor marked.
    await waitFor(() => expect(count()).toBe("1/1"));
    expect(marks()).toHaveLength(1);
    expect(
      document.querySelector(".editor__prose")?.contains(marks()[0]),
    ).toBe(true);
    expect(countLine.querySelector(`.${MATCH_CLASS}`)).toBeNull();
  });

  // EDT-FR-21 + EFR-FR-DSKI: a match that starts inside a YAML key composes with
  // the key treatment instead of one overlay winning — the marked glyphs stay
  // inside the bold span, and the layer still reproduces the buffer.
  it("marks a match that starts inside a YAML key without dropping the key treatment", async () => {
    const { sessions } = await renderWithFrontmatter();
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "ame" } });

    // `name` in the frontmatter, plus nothing in the body of this fixture.
    await waitFor(() => expect(count()).toBe("1/1"));
    const layer = document.querySelector(".editor__frontmatter-hl")!;
    const mark = layer.querySelector(`.${MATCH_CLASS}`)!;
    expect(mark.textContent).toBe("ame");
    // The mark sits inside the key's bold span rather than replacing it…
    expect(mark.closest(".editor__frontmatter-key")).not.toBeNull();
    // …the key's glyphs are all still bold, across however many elements the
    // cut produced…
    expect(
      Array.from(layer.querySelectorAll(".editor__frontmatter-key"))
        .map((k) => k.textContent)
        .join(""),
    ).toBe("namedescription");
    // …and nothing was inserted or dropped.
    const fm = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    expect(layer.textContent).toBe(fm.value);
  });

  // EDT-FR-58: matching does not depend on the YAML parsing — an unparseable
  // block is unhighlighted but still searchable.
  it("still matches inside a block that does not parse", async () => {
    const sessions = new EditSessionStore();
    wireBackend({
      load: {
        body: '---\nname: alpha\ndescription: "unclosed alpha\n---\n\nbody\n',
        checksum: "ck1",
      },
    });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByLabelText("Frontmatter");
    expect(screen.getByText("not valid YAML")).toBeInTheDocument();

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });

    await waitFor(() => expect(count()).toBe("1/2"));
    const layer = document.querySelector(".editor__frontmatter-hl")!;
    expect(layer.querySelectorAll(`.${MATCH_CLASS}`).length).toBe(2);
    // Still no role treatment on the invalid block, matches or not.
    expect(layer.querySelector(".editor__frontmatter-key")).toBeNull();
    const fm = screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    expect(layer.textContent).toBe(fm.value);
  });

  it("expands a collapsed region when the current match is inside it (EFR-FR-DSKI)", async () => {
    const { sessions } = await renderWithFrontmatter();

    // Scroll the body down so the region collapses to its summary bar. jsdom
    // does no layout, so scrollTop is forced before the event is dispatched.
    const scroller = document.querySelector(".editor") as HTMLElement;
    Object.defineProperty(scroller, "scrollTop", {
      configurable: true,
      value: 400,
    });
    fireEvent.scroll(scroller);
    await waitFor(() =>
      expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument(),
    );

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });

    // The first match lies in the frontmatter, so the region peeks open.
    await waitFor(() =>
      expect(screen.getByLabelText("Frontmatter")).toBeInTheDocument(),
    );
  });

  it("replaces across the frontmatter and the body as a single undo step", async () => {
    const { sessions, backend } = await renderWithFrontmatter();
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "alpha" } });
    fireEvent.change(replacementInput(), { target: { value: "omega" } });
    await waitFor(() => expect(count()).toBe("1/3"));

    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));

    const fm = () => screen.getByLabelText("Frontmatter") as HTMLTextAreaElement;
    await waitFor(() => expect(fm().value).toContain("name: omega"));
    expect(fm().value).toContain("description: an omega artifact");
    await waitFor(() => expect(bodyText()).toBe("the body mentions omega too"));

    // One undo reverses the whole sweep — both surfaces at once (EFR-FR-FYRJ).
    fireEvent.keyDown(screen.getByLabelText("artifact body"), {
      key: "z",
      metaKey: true,
    });
    await waitFor(() => expect(fm().value).toContain("name: alpha"));
    await waitFor(() => expect(bodyText()).toBe("the body mentions alpha too"));

    await letWriteLand();
    await waitFor(() => expect(backend.lastSavedBody).toContain("name: alpha"));
  });
});

describe("retained across tabs (EFR-FR-GBJT, EFR-FR-GIPZ, EFR-FR-GNBZ, EFR-FR-GMCE)", () => {
  it("resumes the panel when the artifact is reopened in the same session", async () => {
    const sessions = new EditSessionStore();
    wireBackend({ load: { body: "needle in a haystack", checksum: "ck1" } });

    const first = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    act(() => sessions.openTab("a.md"));
    setFind(sessions, { form: "find", query: "needle" });
    expect(count()).toBe("1/1");

    // Close the tab without ever editing the artifact.
    act(() => sessions.closeTab("a.md"));
    first.unmount();

    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );

    // EDT-FR-28: the panel alone kept the record alive, so it comes back with
    // its query and its matches recomputed against the restored buffer.
    expect(await screen.findByTestId("find-panel")).toBeInTheDocument();
    expect(query().value).toBe("needle");
    await waitFor(() => expect(count()).toBe("1/1"));
  });

  it("gives each artifact its own panel", async () => {
    const sessions = new EditSessionStore();
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "load_artifact_contents_by_id") {
        const { id } = args as { id: string };
        return { body: `${id} alpha beta`, checksum: `ck-${id}` };
      }
      throw new Error(`unexpected invoke ${cmd}`);
    });

    const view = render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    act(() => sessions.setFind("a.md", { form: "find", query: "alpha" }));
    expect(query().value).toBe("alpha");

    view.rerender(
      <Editor
        key="b"
        artifactId="b.md"
        artifactName="b.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );

    // b.md has never had a panel opened, so its band shows the formatting
    // toolbar and a.md's query is nowhere in sight.
    await waitFor(() =>
      expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument(),
    );
    expect(sessions.get("a.md")?.find.query).toBe("alpha");
    expect(sessions.get("b.md")?.find.query ?? "").toBe("");
  });
});

/**
 * Regressions found in review. Each of these failed before the fix it names, so
 * they are detectors rather than descriptions.
 */
describe("current-match stability under edits (EFR-FR-EVHJ)", () => {
  it("keeps the current match when text is inserted ABOVE it", async () => {
    // The anchor is an offset, so an edit above the current match shifts every
    // offset below it. Without the ordinal fallback the current match walks
    // backwards onto whichever occurrence moved into the vacated position.
    const { sessions, source } = await renderInTextMode("aa bb aa cc aa");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "aa" } });
    fireEvent.click(screen.getByRole("button", { name: "Next match" }));
    expect(count()).toBe("2/3");

    // Twenty characters of padding at the top; the match set is unchanged.
    fireEvent.change(source, {
      target: { value: "PADDING PADDING XXXX aa bb aa cc aa" },
    });

    expect(count()).toBe("2/3");
  });

  it("keeps the current match when text is deleted above it", async () => {
    const { sessions, source } = await renderInTextMode("lead text aa bb aa");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "aa" } });
    fireEvent.click(screen.getByRole("button", { name: "Next match" }));
    expect(count()).toBe("2/2");

    fireEvent.change(source, { target: { value: "aa bb aa" } });

    expect(count()).toBe("2/2");
  });

  it("falls to the first match when the current match's surface loses all of them", async () => {
    // The two surfaces number their matches in their own coordinate spaces, so
    // an offset from one is meaningless against the other's. When the anchor's
    // surface empties, the first remaining match is the only sane answer.
    const sessions = new EditSessionStore();
    wireBackend({
      load: {
        body: "---\nname: alpha\n---\nbody alpha here and alpha again",
        checksum: "ck1",
      },
    });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    const fm = (await screen.findByLabelText(
      "Frontmatter",
    )) as HTMLTextAreaElement;

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });
    await waitFor(() => expect(count()).toBe("1/3"));

    // Edit the frontmatter so it no longer matches; only body matches remain.
    fireEvent.change(fm, { target: { value: "name: omega" } });

    await waitFor(() => expect(count()).toBe("1/2"));
  });

  it("recomputes against the new surface when the editing mode is toggled", async () => {
    // The two modes present genuinely different text — raw source carries the
    // `#` and the `---` fences that WYSIWYG renders away — so a stale match set
    // (or a stale anchor) is directly observable in the counter.
    const sessions = new EditSessionStore();
    wireBackend({ load: { body: "# hash\n\nhash in the body", checksum: "ck1" } });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByRole("heading", { name: "hash" });

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "#" } });
    // WYSIWYG shows no `#` — the heading is rendered, not marked up.
    await waitFor(() => expect(count()).toBe("No results"));

    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    await screen.findByLabelText("Markdown source");

    // The raw source does carry it.
    await waitFor(() => expect(count()).toBe("1/1"));
  });
});

describe("body matches that cannot be resolved (EFR-FR-DOQR, EFR-FR-FWOU)", () => {
  it("does not count a WYSIWYG match that spans a block boundary", async () => {
    // `docText` joins textblocks with a newline so a query cannot match across
    // them, but a regex can still match the newline itself. Such a match has no
    // single document range: it can be neither highlighted nor rewritten, so
    // counting it would report a match nothing else in the panel can act on.
    const sessions = new EditSessionStore();
    wireBackend({ load: { body: "# hit\n\nanother hit here", checksum: "ck1" } });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByRole("heading", { name: "hit" });

    setFind(sessions, { form: "find" });
    fireEvent.click(screen.getByRole("radio", { name: /Regular expression/ }));
    fireEvent.change(query(), { target: { value: "hit\\s+another" } });

    await waitFor(() => expect(count()).toBe("No results"));
    expect(marks()).toHaveLength(0);
  });

  it("keeps the counter, the highlights and the current match in agreement", async () => {
    // A genuinely MIXED set: the projection of `# xx` + `xx tail` is
    // "xx\nxx tail", so `xx\n?` matches "xx\n" (spanning the block separator —
    // unresolvable) and then "xx" (resolvable). Counting the first while only
    // decorating the second left the counter and the highlights disagreeing,
    // and the current-match index indexing a list one shorter than it thought.
    const sessions = new EditSessionStore();
    wireBackend({ load: { body: "# xx\n\nxx tail", checksum: "ck1" } });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByRole("heading", { name: "xx" });

    setFind(sessions, { form: "find" });
    fireEvent.click(screen.getByRole("radio", { name: /Regular expression/ }));
    fireEvent.change(query(), { target: { value: "xx\\n?" } });

    // Only the resolvable one is offered, and it is the one highlighted.
    await waitFor(() => expect(count()).toBe("1/1"));
    expect(marks()).toHaveLength(1);
    expect(
      document.querySelectorAll(`.${CURRENT_MATCH_CLASS}`),
    ).toHaveLength(1);
  });
});

describe("operating the panel is not an edit (EDT-FR-23)", () => {
  it("does not dirty the artifact or push a history step", async () => {
    const { sessions, source } = await renderInTextMode("alpha beta alpha");
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });
    fireEvent.click(screen.getByRole("button", { name: "Next match" }));
    fireEvent.click(screen.getByRole("radio", { name: /Smart case/ }));
    setFind(sessions, { form: "replace" });
    fireEvent.change(replacementInput(), { target: { value: "omega" } });
    setFind(sessions, { form: null });

    // Nothing the panel did touched a byte.
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
    expect(source.value).toBe("alpha beta alpha");
  });

  it("leaves an earlier edit as the next thing undo reverses", async () => {
    const { sessions, source } = await renderInTextMode("start");
    fireEvent.change(source, { target: { value: "start EDITED" } });

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "start" } });
    fireEvent.click(screen.getByRole("radio", { name: /Regular expression/ }));
    setFind(sessions, { form: null });

    // One undo reverses the EDIT, not any of the panel operations.
    fireEvent.keyDown(source, { key: "z", metaKey: true });
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("start");
  });

  it("does not dirty the artifact for a replacement that changes nothing", async () => {
    const { sessions, source } = await renderInTextMode("same same");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "same" } });
    fireEvent.change(replacementInput(), { target: { value: "same" } });

    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));

    expect(source.value).toBe("same same");
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
  });
});

describe("history sealing on both sides of a replacement (EFR-FR-FYRJ)", () => {
  it("keeps typing that follows a Replace All out of the sweep's step", async () => {
    // Without the trailing seal, a keystroke inside the coalescing window folds
    // into the sweep's step and one undo would reverse both.
    const { sessions, source } = await renderInTextMode("p p");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "p" } });
    fireEvent.change(replacementInput(), { target: { value: "q" } });
    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));
    expect(source.value).toBe("q q");

    // Immediately after — well inside the coalescing window.
    fireEvent.change(source, { target: { value: "q q TYPED" } });

    fireEvent.keyDown(source, { key: "z", metaKey: true });
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("q q");
  });

  it("redoes a replacement as one step (EFR-FR-FYRJ)", async () => {
    const { sessions, source } = await renderInTextMode("m m m");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "m" } });
    fireEvent.change(replacementInput(), { target: { value: "n" } });
    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));

    fireEvent.keyDown(source, { key: "z", metaKey: true });
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("m m m");

    fireEvent.keyDown(source, { key: "z", metaKey: true, shiftKey: true });
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("n n n");
  });
});

describe("focus on close, in every surface (EFR-FR-CELO, EFR-FR-AYNZ, EFR-FR-ABHF, EFR-FR-ABVQ)", () => {
  it("returns focus to the editing surface even with no current match", async () => {
    // EFR-FR-AYNZ, EFR-FR-ABHF, EFR-FR-ABVQ closes the panel without ever typing a query, so there is no
    // match to place the caret at — focus must still leave the panel.
    const { sessions, source } = await renderInTextMode("nothing to find here");
    setFind(sessions, { form: "find" });
    await waitFor(() => expect(document.activeElement).toBe(query()));

    setFind(sessions, { form: null });

    await waitFor(() => expect(document.activeElement).toBe(source));
  });

  it("places the caret in the frontmatter region when the match is there", async () => {
    const sessions = new EditSessionStore();
    wireBackend({
      load: { body: "---\nname: alpha\n---\nbody text", checksum: "ck1" },
    });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByLabelText("Frontmatter");

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });
    await waitFor(() => expect(count()).toBe("1/1"));

    setFind(sessions, { form: null });

    // The region mounts no textarea while collapsed, so the caret has to be
    // placed once it is actually on screen rather than against a null ref.
    const fm = (await screen.findByLabelText(
      "Frontmatter",
    )) as HTMLTextAreaElement;
    await waitFor(() => expect(document.activeElement).toBe(fm));
    expect(fm.value.slice(fm.selectionStart, fm.selectionEnd)).toBe("alpha");
  });

  it("closes on Escape from the query input too", async () => {
    // EFR-FR-CELO says "anywhere within the panel", not just the replacement row.
    const { sessions } = await renderInTextMode("alpha");
    setFind(sessions, { form: "find" });
    fireEvent.keyDown(query(), { key: "Escape" });
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
  });

  it("selects an existing query when the panel reopens (EFR-FR-AYNZ)", async () => {
    const { sessions } = await renderInTextMode("alpha beta");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });
    setFind(sessions, { form: null });

    setFind(sessions, { form: "find" });

    // "with any existing query selected, so typing replaces it".
    await waitFor(() => {
      const input = query();
      expect(input.selectionStart).toBe(0);
      expect(input.selectionEnd).toBe("alpha".length);
    });
  });
});

describe("adjacent matches render distinctly", () => {
  it("marks back-to-back matches separately with one current", async () => {
    const { sessions } = await renderInTextMode("aaaa");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "aa" } });

    expect(count()).toBe("1/2");
    expect(marks()).toHaveLength(2);
    expect(marks().map((m) => m.textContent)).toEqual(["aa", "aa"]);
    expect(document.querySelectorAll(`.${CURRENT_MATCH_CLASS}`)).toHaveLength(1);
  });
});

describe("the panel is tab chrome, not a floating overlay (EFR-FR-AVML)", () => {
  it("survives a pointer-down elsewhere in the tab", async () => {
    // The main window's overlays dismiss on an outside pointer-down; the band
    // is part of the tab's own chrome and must not, or a click into the
    // document to reposition the caret would close the search.
    const { sessions, source } = await renderInTextMode("alpha beta alpha");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });
    expect(count()).toBe("1/2");

    fireEvent.pointerDown(source);
    fireEvent.mouseDown(source);
    fireEvent.click(source);

    expect(screen.getByTestId("find-panel")).toBeInTheDocument();
    expect(query().value).toBe("alpha");
    expect(count()).toBe("1/2");
  });

  it("keeps its query while the action cluster is used", async () => {
    // Toggling the editing mode is the everyday interaction that takes focus out
    // of the panel — the cluster's own control, and the one an open panel could
    // most easily be dismissed by if the band were an overlay (EFR-FR-AVML).
    const { sessions } = await renderInTextMode("gamma gamma");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "gamma" } });
    expect(count()).toBe("1/2");

    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));

    expect(screen.getByTestId("find-panel")).toBeInTheDocument();
    expect(query().value).toBe("gamma");
    expect(count()).toBe("1/2");
  });
});
