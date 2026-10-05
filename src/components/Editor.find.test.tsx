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

describe("the band (EFR-FR-ABHF)", () => {
  it("swaps the formatting toolbar for the Find panel and back", async () => {
    const sessions = new EditSessionStore();
    wireBackend({ load: { body: "# Title\n\nsome session text", checksum: "ck1" } });
    render(
      <Editor
        artifactId="a.md"
        artifactName="a.md"
        artifactType="skill"
        sessions={sessions}
      />,
    );
    await screen.findByRole("heading", { name: "Title" });

    // The formatting toolbar occupies the band while no panel is open.
    // (The formatting buttons are labelled by `title` — their visible glyph is
    // the accessible name.)
    expect(screen.getByTitle("Bold")).toBeInTheDocument();
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();

    setFind(sessions, { form: "find" });

    // EFR-FR-ABHF: exactly one of the three surfaces holds the band.
    expect(screen.getByTestId("find-panel")).toBeInTheDocument();
    expect(screen.queryByTitle("Bold")).not.toBeInTheDocument();
    // EDT-FR-16: the action cluster stays — it is tab chrome, not the band.
    expect(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    ).toBeInTheDocument();

    setFind(sessions, { form: null });
    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
    expect(screen.getByTitle("Bold")).toBeInTheDocument();
  });

  it("renders the replacement row only in the Find & Replace form (EFR-FR-EWRP)", async () => {
    const { sessions } = await renderInTextMode("alpha beta");

    setFind(sessions, { form: "find" });
    expect(screen.getByTestId("find-panel")).toHaveAttribute("data-form", "find");
    expect(screen.queryByLabelText("Replace with")).not.toBeInTheDocument();

    setFind(sessions, { form: "replace" });
    expect(screen.getByTestId("find-panel")).toHaveAttribute("data-form", "replace");
    expect(screen.getByLabelText("Replace with")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Replace" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Replace All" })).toBeInTheDocument();
  });
});

describe("switching between the two forms (EFR-FR-BSST)", () => {
  it("carries the query, mode and replacement text across a collapse and expand", async () => {
    const { sessions } = await renderInTextMode("a session and another session");

    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "session" } });
    fireEvent.click(screen.getByRole("radio", { name: /Regular expression/ }));
    // Step off the first match, so a form switch that reset the current index
    // to 0 would be visible rather than hiding behind the default.
    fireEvent.click(screen.getByRole("button", { name: "Next match" }));
    expect(count()).toBe("2/2");

    // ⌘R over an open Find expands it, keeping the query, mode and current match.
    setFind(sessions, { form: "replace" });
    expect(query().value).toBe("session");
    expect(
      screen.getByRole("radio", { name: /Regular expression/ }),
    ).toHaveAttribute("aria-checked", "true");
    expect(count()).toBe("2/2");

    fireEvent.change(replacementInput(), { target: { value: "thread" } });

    // ⌘F over an open Find & Replace collapses it, retaining the replacement.
    setFind(sessions, { form: "find" });
    expect(query().value).toBe("session");
    expect(count()).toBe("2/2");
    expect(screen.queryByLabelText("Replace with")).not.toBeInTheDocument();

    setFind(sessions, { form: "replace" });
    expect(replacementInput().value).toBe("thread");
  });

  it("focuses the query input on open and the replacement input on expand", async () => {
    const { sessions } = await renderInTextMode("alpha");

    setFind(sessions, { form: "find" });
    await waitFor(() => expect(document.activeElement).toBe(query()));

    setFind(sessions, { form: "replace" });
    await waitFor(() =>
      expect(document.activeElement).toBe(replacementInput()),
    );
  });

  it("focuses the query input when Find & Replace is opened directly (EFR-FR-BJUY)", async () => {
    const { sessions } = await renderInTextMode("alpha");
    setFind(sessions, { form: "replace" });
    await waitFor(() => expect(document.activeElement).toBe(query()));
  });
});

describe("closing the panel (EFR-FR-CELO)", () => {
  it("closes outright on Escape rather than collapsing to Find", async () => {
    const { sessions } = await renderInTextMode("alpha beta");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "beta" } });

    fireEvent.keyDown(replacementInput(), { key: "Escape" });

    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
    expect(sessions.get("a.md")?.find.form).toBeNull();
  });

  it("closes on the panel's close control", async () => {
    const { sessions } = await renderInTextMode("alpha beta");
    setFind(sessions, { form: "find" });

    fireEvent.click(screen.getByRole("button", { name: "Close find" }));

    expect(screen.queryByTestId("find-panel")).not.toBeInTheDocument();
  });

  it("returns focus to the editing surface at the current match", async () => {
    const { sessions, source } = await renderInTextMode("one two three two");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "two" } });
    expect(count()).toBe("1/2");

    fireEvent.click(screen.getByRole("button", { name: "Close find" }));

    await waitFor(() => expect(document.activeElement).toBe(source));
    expect(source.selectionStart).toBe(4);
    expect(source.selectionEnd).toBe(7);
  });
});

describe("query modes (EFR-FR-CLZF)", () => {
  it("offers the same three modes as the universal search input, one active", async () => {
    const { sessions } = await renderInTextMode("text");
    setFind(sessions, { form: "find" });

    const toggles = screen.getAllByRole("radio");
    expect(toggles.map((t) => t.textContent?.slice(0, 2))).toEqual([
      "Aa",
      "aA",
      ".*",
    ]);
    // Exactly one active, with no state in which none is.
    expect(
      toggles.filter((t) => t.getAttribute("aria-checked") === "true"),
    ).toHaveLength(1);
  });

  it("starts in case-insensitive literal and never writes app preferences", async () => {
    const { sessions } = await renderInTextMode("Widget widget");
    setFind(sessions, { form: "find" });
    expect(
      screen.getByRole("radio", { name: /Case-insensitive/ }),
    ).toHaveAttribute("aria-checked", "true");

    fireEvent.change(query(), { target: { value: "widget" } });
    expect(count()).toBe("1/2");

    // EFR-FR-CWUR: the mode is the artifact's own value, stored on its edit
    // session. (That it does not touch the user-global preference the universal
    // search input persists is asserted at the App level, where both surfaces
    // exist — see App.menu.test.tsx.)
    fireEvent.click(screen.getByRole("radio", { name: /Smart case/ }));
    expect(sessions.get("a.md")?.find.mode).toBe("smart_case");
  });

  it("re-matches immediately when the mode changes (EFR-FR-ESDZ)", async () => {
    const { sessions } = await renderInTextMode("Widget widget");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "Widget" } });
    expect(count()).toBe("1/2");

    fireEvent.click(screen.getByRole("radio", { name: /Smart case/ }));
    expect(count()).toBe("1/1");
  });
});

describe("matching the in-memory buffer (EFR-FR-DKQT)", () => {
  it("finds text the user typed but has not saved", async () => {
    const { sessions, source } = await renderInTextMode("published text");

    fireEvent.change(source, { target: { value: "published text with draft" } });
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "draft" } });

    // The word exists only in the buffer; the file on disk has none of it, and
    // the disk-walking project search could not have found it.
    expect(count()).toBe("1/1");
  });

  it("does not switch the editing mode when the panel opens", async () => {
    const { sessions } = await renderInTextMode("alpha");
    setFind(sessions, { form: "find" });
    expect(screen.getByLabelText("Markdown source")).toBeInTheDocument();
    expect(sessions.get("a.md")?.mode).toBe("text");
  });
});

describe("highlighting and the counter (EFR-FR-DXTV, EFR-FR-DDUX, EFR-FR-DOQR)", () => {
  it("marks every match, distinguishes the current one, and counts them", async () => {
    const { sessions } = await renderInTextMode("one two three two five two");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "two" } });

    expect(marks()).toHaveLength(3);
    expect(count()).toBe("1/3");
    expect(currentMarkText()).toBe("two");
    expect(
      document.querySelectorAll(`.${CURRENT_MATCH_CLASS}`),
    ).toHaveLength(1);
  });

  it("reproduces the buffer byte-for-byte on the highlight layer", async () => {
    // Presentation-only: the layer marks matches without inserting or dropping
    // a character, which is what keeps it registered under the textarea.
    const body = "alpha\nbeta alpha\n  indented alpha";
    const { sessions, source } = await renderInTextMode(body);
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });

    const drawn = screen.getByTestId("source-highlight").textContent ?? "";
    // The buffer, and then one trailing newline: a `textarea` reserves a line
    // box for a file's last line and a `pre` generates none after a final break,
    // so without it the layer is a line short and its scroll clamps that much
    // sooner — which puts every glyph at the foot of the file a line above the
    // character it is decorating.
    expect(drawn).toBe(`${source.value}\n`);
  });

  it("shows a zero state for an empty query and for no results", async () => {
    const { sessions } = await renderInTextMode("alpha");
    setFind(sessions, { form: "find" });
    expect(count()).toBe("");
    expect(marks()).toHaveLength(0);

    fireEvent.change(query(), { target: { value: "zzz" } });
    expect(count()).toBe("No results");
    expect(marks()).toHaveLength(0);
  });

  it("shows no highlight layer while the panel is closed", async () => {
    const { sessions } = await renderInTextMode("alpha alpha");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });
    expect(marks()).toHaveLength(2);

    setFind(sessions, { form: null });
    expect(screen.queryByTestId("source-highlight")).not.toBeInTheDocument();
    expect(marks()).toHaveLength(0);
  });
});

describe("navigation wraps at both ends (EFR-FR-EGQB, EFR-FR-DYMA)", () => {
  it("wraps forward past the last match and backward past the first", async () => {
    const { sessions } = await renderInTextMode("x1 x2 x3");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "x" } });
    expect(count()).toBe("1/3");

    const next = screen.getByRole("button", { name: "Next match" });
    fireEvent.click(next);
    expect(count()).toBe("2/3");
    fireEvent.click(next);
    expect(count()).toBe("3/3");
    fireEvent.click(next);
    expect(count()).toBe("1/3");

    fireEvent.click(screen.getByRole("button", { name: "Previous match" }));
    expect(count()).toBe("3/3");
  });

  it("steps with Enter and ⇧Enter from the query input", async () => {
    const { sessions } = await renderInTextMode("x1 x2 x3");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "x" } });

    fireEvent.keyDown(query(), { key: "Enter" });
    expect(count()).toBe("2/3");
    fireEvent.keyDown(query(), { key: "Enter", shiftKey: true });
    expect(count()).toBe("1/3");
  });

  it("disables both navigation controls when nothing matches", async () => {
    const { sessions } = await renderInTextMode("alpha");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "zzz" } });

    expect(screen.getByRole("button", { name: "Next match" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Previous match" })).toBeDisabled();
  });
});

describe("invalid patterns (EFR-FR-EPYP, EFR-FR-ENKY)", () => {
  it("indicates an uncompilable regex, highlights nothing, and keeps the query", async () => {
    const { sessions } = await renderInTextMode("a [unclosed b");
    setFind(sessions, { form: "replace" });
    fireEvent.click(screen.getByRole("radio", { name: /Regular expression/ }));
    fireEvent.change(query(), { target: { value: "[unclosed" } });

    expect(count()).toBe("Invalid pattern");
    expect(screen.getByTestId("find-count")).toHaveAttribute("data-invalid", "true");
    expect(marks()).toHaveLength(0);
    // The query and mode are left exactly as typed so the user can fix them.
    expect(query().value).toBe("[unclosed");
    expect(
      screen.getByRole("radio", { name: /Regular expression/ }),
    ).toHaveAttribute("aria-checked", "true");
    // EFR-FR-EPYP: neither replace action runs while the pattern is invalid.
    expect(screen.getByRole("button", { name: "Replace" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Replace All" })).toBeDisabled();

    // Escaping the bracket makes it compile, and it now matches literally.
    fireEvent.change(query(), { target: { value: "\\[unclosed" } });
    expect(count()).toBe("1/1");
    expect(screen.getByTestId("find-count")).toHaveAttribute(
      "data-invalid",
      "false",
    );
  });

  it("never reports invalid in a literal mode", async () => {
    const { sessions } = await renderInTextMode("a [unclosed b");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "[unclosed" } });

    expect(count()).toBe("1/1");
  });
});

describe("live re-matching (EFR-FR-ESDZ)", () => {
  it("recomputes as the user edits the buffer with the panel open", async () => {
    const { sessions, source } = await renderInTextMode("hit hit");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "hit" } });
    expect(count()).toBe("1/2");

    fireEvent.change(source, { target: { value: "hit hit hit" } });

    expect(count()).toBe("1/3");
    expect(marks()).toHaveLength(3);
    // The panel stays open throughout.
    expect(screen.getByTestId("find-panel")).toBeInTheDocument();
  });

  it("moves the current match to the nearest following one when it is deleted", async () => {
    const { sessions, source } = await renderInTextMode("aa bb aa cc aa");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "aa" } });
    fireEvent.click(screen.getByRole("button", { name: "Next match" }));
    expect(count()).toBe("2/3"); // the middle occurrence

    // Delete that middle occurrence; the one after it takes over.
    fireEvent.change(source, { target: { value: "aa bb XX cc aa" } });

    expect(count()).toBe("2/2");
  });
});

describe("Replace and Replace All (EFR-FR-EXHA, EFR-FR-EWRP, EFR-FR-FHSQ, EFR-FR-FYNZ, EFR-FR-FYRJ)", () => {
  it("rewrites only the current match and advances, writing nothing to disk", async () => {
    const { sessions, source } = await renderInTextMode("one one one");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "one" } });
    fireEvent.change(replacementInput(), { target: { value: "1" } });
    expect(count()).toBe("1/3");

    fireEvent.click(screen.getByRole("button", { name: "Replace" }));

    expect(source.value).toBe("1 one one");
    // Two matches remain and the current one advanced past the replacement.
    expect(count()).toBe("1/2");
    expect(screen.getByText("● unsaved")).toBeInTheDocument();
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "save_artifact_contents"),
    ).toHaveLength(0);
  });

  it("advances past a replacement that itself contains the query", async () => {
    // Naively, replacing `cat` with `cats` re-matches what it just wrote and
    // Replace would never move on.
    const { sessions, source } = await renderInTextMode("cat cat");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "cat" } });
    fireEvent.change(replacementInput(), { target: { value: "cats" } });

    fireEvent.click(screen.getByRole("button", { name: "Replace" }));
    expect(source.value).toBe("cats cat");
    fireEvent.click(screen.getByRole("button", { name: "Replace" }));
    expect(source.value).toBe("cats cats");
  });

  it("rewrites every match in one Replace All", async () => {
    const { sessions, source } = await renderInTextMode(
      "x x x x x x x x x x x x",
    );
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "x" } });
    fireEvent.change(replacementInput(), { target: { value: "y" } });
    expect(count()).toBe("1/12");

    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));

    expect(source.value).toBe("y y y y y y y y y y y y");
    expect(count()).toBe("No results");
  });

  it("inserts the replacement literally in regex mode (EFR-FR-FWOU)", async () => {
    const { sessions, source } = await renderInTextMode(
      "smart-case and upper-case",
    );
    setFind(sessions, { form: "replace" });
    fireEvent.click(screen.getByRole("radio", { name: /Regular expression/ }));
    fireEvent.change(query(), { target: { value: "(\\w+)-case" } });
    fireEvent.change(replacementInput(), { target: { value: "$1" } });

    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));

    // The characters `$1`, not the captured group — the deliberate v1 boundary.
    expect(source.value).toBe("$1 and $1");
  });

  it("does nothing when the match set is empty", async () => {
    const { sessions, source } = await renderInTextMode("alpha");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "zzz" } });
    fireEvent.change(replacementInput(), { target: { value: "beta" } });

    expect(screen.getByRole("button", { name: "Replace" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Replace All" })).toBeDisabled();
    expect(source.value).toBe("alpha");
  });

  it("saves the replaced buffer when the user saves (EDT-FR-34)", async () => {
    const { sessions, backend, source } = await renderInTextMode("old old");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "old" } });
    fireEvent.change(replacementInput(), { target: { value: "new" } });
    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));
    expect(source.value).toBe("new new");

    await letWriteLand();
    await waitFor(() => expect(backend.lastSavedBody).toBe("new new"));
  });
});

describe("replacements and the undo history (EFR-FR-FYRJ)", () => {
  /** ⌘Z, the way the Editor claims it (EDT-FR-15). */
  function undo(el: HTMLElement) {
    fireEvent.keyDown(el, { key: "z", metaKey: true });
  }

  it("undoes a whole Replace All in one step", async () => {
    const { sessions, source } = await renderInTextMode("x x x x x x");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "x" } });
    fireEvent.change(replacementInput(), { target: { value: "y" } });
    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));
    expect(source.value).toBe("y y y y y y");

    undo(source);

    // One undo restores the document exactly as it stood before the sweep —
    // not one occurrence at a time.
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("x x x x x x");
  });

  it("keeps a Replace All separate from the typing that preceded it", async () => {
    // Without sealing the coalescing burst, a Replace All landing within the
    // coalescing window would fold into the previous keystrokes and one undo
    // would reverse both.
    const { sessions, source } = await renderInTextMode("seed");
    fireEvent.change(source, { target: { value: "seed x x" } });
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "x" } });
    fireEvent.change(replacementInput(), { target: { value: "y" } });
    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));
    expect(source.value).toBe("seed y y");

    undo(source);
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("seed x x");
  });

  it("keeps consecutive Replaces as separate steps", async () => {
    const { sessions, source } = await renderInTextMode("a a a");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "a" } });
    fireEvent.change(replacementInput(), { target: { value: "b" } });
    const replace = screen.getByRole("button", { name: "Replace" });
    fireEvent.click(replace);
    fireEvent.click(replace);
    expect(source.value).toBe("b b a");

    undo(source);
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("b a a");
  });

  it("leaves the panel open with its query after an undo", async () => {
    const { sessions, source } = await renderInTextMode("q q");
    setFind(sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "q" } });
    fireEvent.change(replacementInput(), { target: { value: "z" } });
    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));

    undo(source);

    expect(screen.getByTestId("find-panel")).toBeInTheDocument();
    expect(query().value).toBe("q");
    expect(replacementInput().value).toBe("z");
    expect(count()).toBe("1/2");
  });
});

describe("the external-change modal (EFR-FR-HLGY)", () => {
  async function withConflict() {
    const ctx = await renderInTextMode("keep keep");
    setFind(ctx.sessions, { form: "replace" });
    fireEvent.change(query(), { target: { value: "keep" } });
    fireEvent.change(replacementInput(), { target: { value: "drop" } });
    expect(count()).toBe("1/2");

    act(() => {
      listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
        payload: { artifactId: "a.md", checksum: "ck-external" },
      });
    });
    await screen.findByRole("dialog");
    return ctx;
  }

  it("makes both replace actions inert while the modal blocks the tab", async () => {
    const { source } = await withConflict();

    // EFR-FR-HLGY: nothing is matched while the tab is blocked, so neither action
    // has anything to act on and the buffer is untouched.
    expect(screen.getByRole("button", { name: "Replace All" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));
    expect(source.value).toBe("keep keep");
  });

  it("keeps the panel and recomputes against the loaded content on Load from filesystem", async () => {
    const { sessions } = await withConflict();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_artifact_contents_by_id")
        return { body: "keep keep keep", checksum: "ck-external" };
      throw new Error(`unexpected invoke ${cmd}`);
    });

    fireEvent.click(screen.getByRole("button", { name: "Load from filesystem" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    // The panel survives with its query, replacement and mode intact, and its
    // matches now describe the newly loaded content.
    expect(screen.getByTestId("find-panel")).toBeInTheDocument();
    expect(query().value).toBe("keep");
    expect(replacementInput().value).toBe("drop");
    await waitFor(() => expect(count()).toBe("1/3"));
    expect(sessions.get("a.md")?.find.form).toBe("replace");
  });

  it("leaves the panel exactly as it was on Keep my version", async () => {
    await withConflict();

    fireEvent.click(screen.getByRole("button", { name: "Keep my version" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );

    expect(query().value).toBe("keep");
    expect(count()).toBe("1/2");
  });
});

