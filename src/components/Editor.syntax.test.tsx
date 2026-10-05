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
import { HIGHLIGHT_DELAY_MS } from "../hooks/useSourceTokens";
import { letWriteLand } from "../test/autosave";
import type { ArtifactContents, Discussion } from "../types";

/**
 * `ESH-editor-source-files.md` driven through the real Editor.
 *
 * The language map, the autodetection fallback and the tokenising are covered
 * against the module in `../state/syntaxHighlight.test.ts`; what is exercised
 * here is the surface — which page a file opens on, what the cluster and the
 * band carry over each, and that the token layer decorates the text without ever
 * becoming it.
 */
const invokeMock = vi.fn();
const unlistenMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => unlistenMock),
}));

interface Backend {
  load: ArtifactContents;
  threads?: Discussion[];
  lastSavedBody?: string;
  calls: { cmd: string; args: unknown }[];
}

function wireBackend(b: Backend) {
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "load_artifact_contents_by_id":
        return b.load;
      case "save_artifact_contents":
        b.lastSavedBody = (args as { body: string }).body;
        return { checksum: "ck-saved" };
      case "list_discussions":
        return b.threads ?? [];
      case "resolve_comment_identity":
        return { kind: "human", login: "raver119" };
      case "list_discussion_threads_for_target":
        return [];
      case "list_agents":
        return [];
      case "append_log_records":
        return undefined;
      default:
        return undefined;
    }
  });
}

const RUST = 'fn main() {\n    // greet\n    let s = "x < y & z";\n}\n';

let sessions: EditSessionStore;

function mount(
  id: string,
  body: string,
  over: Partial<Backend> = {},
): Backend {
  const backend: Backend = { load: { body, checksum: "ck1" }, calls: [], ...over };
  wireBackend(backend);
  render(<Editor artifactId={id} artifactName={id} sessions={sessions} />);
  return backend;
}

/** The aria-hidden layer the tokens and matches are drawn on (ESH-FR-YTNN). */
const layer = () => screen.queryByTestId("source-highlight");

/**
 * What the layer draws of the buffer — its text without the one trailing
 * newline it carries so that its last line box matches the textarea's.
 *
 * The parity newline itself is asserted where it belongs, in the two tests about
 * line boxes; everywhere else the question is whether the layer reproduces the
 * buffer, and the mirror's own last line is not part of that.
 */
const layerText = (): string => {
  const text = layer()?.textContent ?? "";
  return text.endsWith("\n") ? text.slice(0, -1) : text;
};

/** Wait past the rest that precedes a tokenising pass (ESH-FR-MJRH). */
async function letTokensLand() {
  await waitFor(
    () => {
      expect(layer()?.querySelector(".hl")).not.toBeNull();
    },
    { timeout: 2_000 },
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  sessions = new EditSessionStore();
});

afterEach(() => {
  cleanup();
});

// EFR-FR-APFR, ESH-FR-FKZB, ESH-FR-DCQX, EDT-FR-16, EDT-FR-17 / ESH-FR-SSDV: a source file opens on the source surface and stays
// there; a Markdown file keeps both modes and the toggle that moves between them.
describe("EFR-FR-APFR, ESH-FR-FKZB, ESH-FR-DCQX, EDT-FR-16, EDT-FR-17: the two shapes of file", () => {
  it("opens a source file on the source surface with no mode toggle", async () => {
    mount("src/main.rs", RUST);
    const source = await screen.findByLabelText("Source");
    expect((source as HTMLTextAreaElement).value).toBe(RUST);
    // No rich rendering of it anywhere in the tab...
    expect(screen.queryByLabelText("artifact body")).toBeNull();
    // ...and nothing offering to put it on one.
    expect(
      screen.queryByRole("button", { name: "Edit as rich text" }),
    ).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Edit as Markdown source" }),
    ).toBeNull();
  });

  it("opens an untyped Markdown file in WYSIWYG with the toggle present", async () => {
    mount("README.md", "# Read me\n\nSome prose.\n");
    await screen.findByLabelText("artifact body");
    expect(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText("Source")).toBeNull();
  });

  // EFR-FR-ACMM: the formatting controls format a rich Markdown document, and a
  // source file has none to format — so the band holds the find panels alone.
  it("holds no formatting toolbar over a source file, and the band keeps its place", async () => {
    mount("src/main.rs", RUST);
    await screen.findByLabelText("Source");
    expect(screen.queryByRole("button", { name: "Bold" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Heading 1" })).toBeNull();

    const band = document.querySelector(".editor__toolbar")!;
    expect(band).not.toBeNull();
    // ⌘F is the shell's accelerator (SNV-FR-43); what it does to the artifact is
    // this store mutation, which is what the panel renders from.
    act(() => sessions.setFind("src/main.rs", { form: "find" }));
    await screen.findByPlaceholderText("Find");
    // The same band, still the tab's own chrome rather than a second one.
    expect(document.querySelectorAll(".editor__toolbar")).toHaveLength(1);
    act(() => sessions.setFind("src/main.rs", { form: null }));
    await waitFor(() =>
      expect(screen.queryByPlaceholderText("Find")).toBeNull(),
    );
    expect(document.querySelectorAll(".editor__toolbar")).toHaveLength(1);
  });
});

// ESH-FR-SSDV, ESH-FR-LKNM, ESH-FR-BABL, ESH-FR-PBFJ / ESH-FR-SNJU / ESH-FR-XNRV, ESH-FR-LMNI, ESH-FR-QFCD / ESH-FR-TVUT: the mapped grammars, through
// the surface. Which language each extension resolves to is the module's own
// test; what matters here is that the surface actually paints what it resolved.
describe("ESH-FR-SSDV, ESH-FR-LKNM, ESH-FR-BABL, ESH-FR-PBFJ: mapped extensions paint on the surface", () => {
  it("colours a mapped file whatever the case of its extension", async () => {
    mount("src/main.RS", RUST);
    await screen.findByLabelText("Source");
    await letTokensLand();
    const marked = layer()!;
    expect(marked.querySelector(".hl--keyword")?.textContent).toBe("fn");
    expect(marked.querySelector(".hl--comment")?.textContent).toBe("// greet");
    // ESH-FR-YTNN: the layer reproduces the buffer character for character, plus
    // the one trailing newline that makes its last line box match the
    // textarea's. Without that the layer is a line short: its scroll clamps
    // sooner than the text's, and at the foot of a file every glyph on it sits a
    // line above the character it decorates, so a click lands on the wrong line.
    expect(marked.textContent).toBe(`${RUST}\n`);
    expect(marked.textContent!.slice(0, RUST.length)).toBe(RUST);
  });

  // The parity above, stated as the invariant rather than as one file's text:
  // the layer must never be shorter in line boxes than the surface it mirrors.
  it("gives the layer a line box for every line the textarea has", async () => {
    for (const body of [
      'fn main() {\n    let s = "x";\n}\n', // ends with a newline
      "fn main() {}", // ends without one
      "\n\n\n", // nothing but blank lines
      "", // empty
    ]) {
      cleanup();
      sessions = new EditSessionStore();
      mount("src/main.rs", body);
      const source = (await screen.findByLabelText(
        "Source",
      )) as HTMLTextAreaElement;
      expect(source.value).toBe(body);
      const text = layer()!.textContent ?? "";
      // Same characters, and never fewer lines than the textarea will lay out.
      expect(text.startsWith(body)).toBe(true);
      expect(text.split("\n").length).toBeGreaterThanOrEqual(
        body.split("\n").length,
      );
    }
  });

  it("leaves a file no language resolves for plain and editable", async () => {
    mount("notes.txt", "just some words about a cat\n");
    const source = await screen.findByLabelText("Source");
    await act(async () => {
      await new Promise((r) => setTimeout(r, HIGHLIGHT_DELAY_MS * 3));
    });
    expect(layer()?.querySelector(".hl")).toBeNull();
    fireEvent.change(source, { target: { value: "still typing\n" } });
    expect((source as HTMLTextAreaElement).value).toBe("still typing\n");
  });
});

// ESH-FR-SSDV, ESH-FR-BPLJ, EDT-FR-66 / ESH-FR-ZCKP: a Markdown file receives no colouring in either mode,
// and its presentation is unchanged.
describe("ESH-FR-SSDV, ESH-FR-BPLJ, EDT-FR-66: Markdown is not coloured", () => {
  it("paints no token layer in either of a Markdown file's modes", async () => {
    const backend = mount("Notes.MD", "# H\n\n```rust\nfn main() {}\n```\n");
    await screen.findByLabelText("artifact body");
    expect(layer()).toBeNull();

    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    await screen.findByLabelText("Markdown source");
    await act(async () => {
      await new Promise((r) => setTimeout(r, HIGHLIGHT_DELAY_MS * 3));
    });
    // The find layer is the only thing that puts one there, and no panel is open.
    expect(layer()).toBeNull();

    // …and with the panel open, the layer exists for the matches alone: a
    // Markdown file's raw surface is where a layer drawn for three reasons could
    // most easily start colouring one it should not (ESH-FR-ZCKP).
    act(() => sessions.setFind("Notes.MD", { form: "find", query: "fn" }));
    const marked = await screen.findByTestId("source-highlight");
    await waitFor(() =>
      expect(marked.querySelectorAll("mark.find-match").length).toBeGreaterThan(0),
    );
    await act(async () => {
      await new Promise((r) => setTimeout(r, HIGHLIGHT_DELAY_MS * 3));
    });
    expect(marked.querySelector(".hl")).toBeNull();
    act(() => sessions.setFind("Notes.MD", { form: null, query: "" }));

    // ...and the round trip changed nothing (EDT-FR-66).
    await act(async () => {
      await sessions.flush("Notes.MD", { force: true });
    });
    expect(backend.lastSavedBody).toBe("# H\n\n```rust\nfn main() {}\n```\n");
  });
});

// ESH-FR-BPLJ: highlighting is presentation-only — the text, the
// history, and the bytes a save writes are what they would be without it.
describe("ESH-FR-BPLJ: editing a highlighted file", () => {
  it("types, undoes, and saves exactly as an unhighlighted surface does", async () => {
    const backend = mount("app.ts", "const a = 1;\n");
    const source = (await screen.findByLabelText(
      "Source",
    )) as HTMLTextAreaElement;
    await letTokensLand();

    fireEvent.change(source, { target: { value: "const a = 2;\n" } });
    expect(source.value).toBe("const a = 2;\n");
    // The layer follows the edited text rather than the text it was computed
    // over — stale spans never reach it (ESH-FR-VUVO).
    await waitFor(() => expect(layerText()).toBe("const a = 2;\n"));

    // One undo reverses the edit and nothing else.
    fireEvent.keyDown(source, { key: "z", metaKey: true });
    await waitFor(() => expect(source.value).toBe("const a = 1;\n"));

    // Redo puts it back, on the same single history the surface shares.
    fireEvent.keyDown(source, { key: "z", metaKey: true, shiftKey: true });
    await waitFor(() => expect(source.value).toBe("const a = 2;\n"));

    // A multi-line paste and a selection delete are ordinary edits here too.
    fireEvent.change(source, {
      target: { value: "const a = 2;\nconst b = 2;\nconst c = 2;\n" },
    });
    await waitFor(() => expect(layer()!.textContent).toContain("const c"));

    // Replace All is one step over the whole surface, and the colouring follows
    // what it wrote rather than what was there.
    act(() =>
      sessions.setFind("app.ts", {
        form: "replace",
        query: "2",
        replacement: "7",
      }),
    );
    await screen.findByPlaceholderText("Find");
    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));
    await waitFor(() => expect(source.value).not.toContain("2"));
    await waitFor(() =>
      expect(
        Array.from(layer()!.querySelectorAll(".hl--number")).map(
          (n) => n.textContent,
        ),
      ).toEqual(["7", "7", "7"]),
    );
    // One undo restores the whole sweep (EFR-FR-FYRJ), colouring and all.
    fireEvent.keyDown(source, { key: "z", metaKey: true });
    await waitFor(() => expect(source.value).toContain("const a = 2;"));

    act(() => sessions.setFind("app.ts", { form: null, query: "" }));
    fireEvent.change(source, { target: { value: "const a = 3;\n" } });
    await letWriteLand();
    expect(backend.lastSavedBody).toBe("const a = 3;\n");
  });

  // ESH-FR-GXUX, ESH-FR-UXQJ: a query matched inside a coloured token marks the match and
  // keeps the colour.
  it("marks a find match inside a coloured token without losing the token", async () => {
    mount("app.ts", "const answer = 42;\nconst other = 42;\n");
    await screen.findByLabelText("Source");
    await letTokensLand();

    act(() => sessions.setFind("app.ts", { form: "find" }));
    const query = await screen.findByPlaceholderText("Find");
    fireEvent.change(query, { target: { value: "42" } });

    await waitFor(() => {
      const marks = layer()!.querySelectorAll("mark.find-match");
      expect(marks).toHaveLength(2);
    });
    const marked = layer()!;
    // The match's marking is around the token's colour rather than instead of it.
    expect(marked.querySelector("mark.find-match .hl--number")).not.toBeNull();
    expect(marked.querySelector("mark.find-match--current")).not.toBeNull();
    expect(layerText()).toBe("const answer = 42;\nconst other = 42;\n");
  });
});

// ESH-FR-YTNN / ESH-FR-VNPW: the layer takes no part in editing.
describe("ESH-FR-YTNN: the layer is not the editing surface", () => {
  it("keeps the textarea the sole editable target and the layer inert", async () => {
    mount("src/main.rs", RUST);
    const source = await screen.findByLabelText("Source");
    await letTokensLand();
    const marked = layer()!;

    // ESH-FR-LXQR: out of the accessibility tree, and carrying no editable host.
    expect(marked).toHaveAttribute("aria-hidden", "true");
    expect(marked.querySelector("[contenteditable]")).toBeNull();
    expect(marked.querySelector("textarea")).toBeNull();
    // The one thing that accepts text is the textarea, and it holds the file
    // rather than any rendering of it.
    expect((source as HTMLTextAreaElement).value).toBe(RUST);
    expect(source.tagName).toBe("TEXTAREA");
    // A composition run through the surface reaches it like any other input.
    fireEvent.compositionStart(source);
    fireEvent.change(source, { target: { value: `${RUST}こんにちは` } });
    fireEvent.compositionEnd(source);
    expect((source as HTMLTextAreaElement).value).toBe(`${RUST}こんにちは`);
  });
});

// ESH-FR-MJRH: a file is editable before its tokens arrive, and a
// result that no longer describes what is on screen never reaches the layer.
describe("ESH-FR-MJRH: nothing waits on the colouring", () => {
  it("shows plain editable text until the pass lands", async () => {
    mount("app.ts", "const a = 1;\n");
    const source = (await screen.findByLabelText(
      "Source",
    )) as HTMLTextAreaElement;
    // Before the rest elapses there is text and no colour — and it takes input.
    expect(layer()!.querySelector(".hl")).toBeNull();
    fireEvent.change(source, { target: { value: "const a = 1; // x\n" } });
    expect(source.value).toBe("const a = 1; // x\n");
    await letTokensLand();
    expect(layerText()).toBe("const a = 1; // x\n");
  });

  it("drops a result for a file the tab no longer shows", async () => {
    mount("app.ts", "const a = 1;\n");
    await screen.findByLabelText("Source");
    await letTokensLand();
    cleanup();

    // A second file, opened before the first one's pass could have landed.
    sessions = new EditSessionStore();
    mount("notes.txt", "plain words with nothing to detect\n");
    const source = await screen.findByLabelText("Source");
    await act(async () => {
      await new Promise((r) => setTimeout(r, HIGHLIGHT_DELAY_MS * 4));
    });
    expect(layerText()).toBe("plain words with nothing to detect\n");
    expect(layer()!.querySelector(".hl")).toBeNull();
    expect((source as HTMLTextAreaElement).value).toBe(
      "plain words with nothing to detect\n",
    );
  });
});

// EDT-FR-61, ESH-FR-CXAI, CMT-FR-03 / CMT-FR-05, ESH-FR-ATDS, ESH-FR-BLTT / CMT-FR-02: the rail renders beside a source file's
// page, because that is where the Editor sets its text.
describe("EDT-FR-61, ESH-FR-CXAI, CMT-FR-02, CMT-FR-03: comments beside the source page", () => {
  const thread = (): Discussion => ({
    id: "t1",
    target: { kind: "artifact", artifactId: "src/main.rs" },
    fragmentTarget: {
      owner: { kind: "artifact", artifactId: "src/main.rs" },
      path: "src/main.rs",
      start: RUST.indexOf("// greet"), end: 0, quote: "// greet",
    },
    comments: [
      {
        id: "c1",
        author: { kind: "human", login: "raver119" },
        body: "Why greet here?",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  });

  it("renders the rail and marks the anchored passage on the source layer", async () => {
    mount("src/main.rs", RUST, { threads: [thread()] });
    await screen.findByLabelText("Source");
    // CMT-FR-30: the rail opens for a file carrying an unresolved thread.
    const rail = await screen.findByRole("complementary", { name: "Comments" });
    expect(rail).toBeInTheDocument();
    expect(rail.textContent).toContain("Why greet here?");
    // CMT-FR-28: the passage itself is marked, on the same layer the tokens are.
    await waitFor(() => {
      const anchor = layer()!.querySelector("[data-comment-thread='t1']");
      expect(anchor?.textContent).toBe("// greet");
    });
    // ...and once the tokens land, the marking rides over the token's colour
    // rather than replacing it.
    await letTokensLand();
    expect(
      layer()!.querySelector("[data-comment-thread='t1'] .hl--comment"),
    ).not.toBeNull();
  });

  it("offers the affordance over a selection in the source surface", async () => {
    mount("src/main.rs", RUST, { threads: [] });
    const source = (await screen.findByLabelText(
      "Source",
    )) as HTMLTextAreaElement;
    await letTokensLand();

    // CMT-FR-05 / CMT-FR-06: the textarea's own offsets are the anchor.
    const start = RUST.indexOf('"x < y & z"');
    source.setSelectionRange(start, start + '"x < y & z"'.length);
    fireEvent.mouseUp(source);

    const button = await screen.findByRole("button", {
      name: "Comment on selection",
    });
    fireEvent.click(button);
    // The composer opens in the rail, quoting exactly the passage selected.
    const rail = await screen.findByRole("complementary", { name: "Comments" });
    await waitFor(() => expect(rail.textContent).toContain('"x < y & z"'));
  });

  it("renders no rail on a Markdown file's raw-text surface", async () => {
    mount("a.md", "# H\n\nSome prose about greeting.\n", {
      threads: [
        {
          ...thread(),
          target: { kind: "artifact", artifactId: "a.md" },
          fragmentTarget: {
            owner: { kind: "artifact", artifactId: "a.md" },
            path: "a.md",
            start: 6,
            end: 0,
            quote: "Some prose",
          },
        },
      ],
    });
    await screen.findByRole("complementary", { name: "Comments" });
    fireEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    await screen.findByLabelText("Markdown source");
    expect(screen.queryByRole("complementary", { name: "Comments" })).toBeNull();
  });
});

// ESH-FR-YNIV / ESH-FR-CAKG: a theme change recolours the layer in place.
//
// The colours themselves are pinned in `../styles/codeTokenContrast.test.ts` —
// `css: false` means no rendering test can compute one. What is testable here is
// the other half of the requirement: that changing the theme disturbs neither
// the text, the caret, nor the scroll position, and that the roles the layer
// carries are the same before and after (they resolve through theme variables,
// so re-colouring costs no re-render of the layer's markup).
describe("ESH-FR-CAKG, ESH-FR-YNIV: a theme change over an open file", () => {
  it("leaves the text, the caret and the token roles exactly as they were", async () => {
    mount("app.ts", "const answer = 42; // why\n");
    const source = (await screen.findByLabelText(
      "Source",
    )) as HTMLTextAreaElement;
    await letTokensLand();

    const before = Array.from(layer()!.querySelectorAll(".hl")).map(
      (n) => `${n.className}:${n.textContent}`,
    );
    source.focus();
    source.setSelectionRange(6, 12);
    layer()!.scrollTop = 0;

    act(() => {
      document.documentElement.setAttribute("data-theme", "dark");
    });
    await act(async () => {
      await new Promise((r) => setTimeout(r, HIGHLIGHT_DELAY_MS * 2));
    });

    expect(source.value).toBe("const answer = 42; // why\n");
    expect(source.selectionStart).toBe(6);
    expect(source.selectionEnd).toBe(12);
    expect(document.activeElement).toBe(source);
    expect(
      Array.from(layer()!.querySelectorAll(".hl")).map(
        (n) => `${n.className}:${n.textContent}`,
      ),
    ).toEqual(before);
    document.documentElement.removeAttribute("data-theme");
  });
});

// ACT-FR-19: a source file lends the rail a margin, so its discussions are read
// there rather than in the floating panel.
describe("ACT-FR-19: discussions over a source file", () => {
  it("reads them in the rail and carries no floating panel", async () => {
    mount("src/main.rs", RUST, { threads: [] });
    await screen.findByLabelText("Source");
    // The control is the tab's, in the corner of the field beside the page.
    expect(
      await screen.findByRole("button", { name: "Actions" }),
    ).toBeInTheDocument();
    // ACT-FR-20's discussions control belongs to the surfaces that lend no
    // margin; a source file is not one of them.
    expect(screen.queryByRole("button", { name: /discussion/i })).toBeNull();
  });
});

// ESH-FR-XXYW / ESH-FR-YTPJ: an empty file opens, renders plain, and takes input.
describe("ESH-FR-YTPJ, ESH-FR-XXYW: an empty source file", () => {
  it("renders plain and accepts typing", async () => {
    mount("empty.txt", "");
    const source = (await screen.findByLabelText(
      "Source",
    )) as HTMLTextAreaElement;
    expect(source.value).toBe("");
    expect(layer()?.querySelector(".hl")).toBeNull();
    fireEvent.change(source, { target: { value: "a" } });
    expect(source.value).toBe("a");
  });
});
