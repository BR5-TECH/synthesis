import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { TextSelection } from "@tiptap/pm/state";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { CURRENT_MATCH_CLASS } from "./findHighlight";
import type { ArtifactChangedPayload, ArtifactContents } from "../types";
import { ARTIFACT_CHANGED_EXTERNALLY } from "../events";
import { resetLogBufferForTest } from "../logging";

/**
 * `EFR-editor-find-replace.md`: which changes move the view to the current match.
 *
 * A find action brings the current match into view (EFR-FR-DOQR). An edit of
 * the author recomputes the matches and can make another match current, but
 * the view stays on the caret (EFR-FR-KQWT). The first next-match after such an
 * edit brings the current match into view without stepping past it
 * (EFR-FR-HZRM).
 *
 * jsdom does no layout, so a scroll is observed where the Editor asks for it:
 * `scrollIntoView` on the current match in the rich surfaces, and a write of
 * `scrollTop` on the raw-text textarea.
 */
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

let listeners: Record<string, (event: { payload: ArtifactChangedPayload }) => void> = {};

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (event: { payload: ArtifactChangedPayload }) => void) => {
      listeners[event] = cb;
      return () => {};
    },
  ),
}));

function wireBackend(load: ArtifactContents) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "load_artifact_contents_by_id") return Promise.resolve(load);
    if (cmd === "save_artifact_contents") {
      return Promise.resolve({ checksum: "ck-saved" });
    }
    if (cmd === "append_log_records") return Promise.resolve();
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
}

/** Every element the Editor asked to scroll into view, in order. */
let scrolled: Element[] = [];

beforeEach(() => {
  invokeMock.mockReset();
  listeners = {};
  resetLogBufferForTest();
  scrolled = [];
  Element.prototype.scrollIntoView = function (this: Element) {
    scrolled.push(this);
  };
});

afterEach(() => {
  cleanup();
  delete (Element.prototype as Partial<Element>).scrollIntoView;
});

/** The scrolls that brought a current find match into view. */
const matchScrolls = () =>
  scrolled.filter((el) => el.classList.contains(CURRENT_MATCH_CLASS));

/**
 * Let the frames the Editor scheduled run. The scroll is deferred to the next
 * frame, and a frame requested after it runs after it, so two of them cover a
 * scroll that a re-render scheduled.
 */
async function nextFrames() {
  for (let i = 0; i < 2; i++) {
    await act(
      () =>
        new Promise<void>((resolve) => {
          if (typeof requestAnimationFrame === "function") {
            requestAnimationFrame(() => resolve());
          } else setTimeout(resolve, 0);
        }),
    );
  }
}

function setFind(
  sessions: EditSessionStore,
  patch: Parameters<EditSessionStore["setFind"]>[1],
) {
  act(() => {
    sessions.setFind("a.md", patch);
  });
}

const query = () => screen.getByLabelText("Find") as HTMLInputElement;
const count = () => screen.getByTestId("find-count").textContent;
const currentMarkText = () =>
  document.querySelector<HTMLElement>(`.${CURRENT_MATCH_CLASS}`)?.textContent ??
  null;
const bodyText = () =>
  document.querySelector<HTMLElement>(".editor__prose")?.textContent ?? "";
const next = () =>
  fireEvent.click(screen.getByRole("button", { name: "Next match" }));
const previous = () =>
  fireEvent.click(screen.getByRole("button", { name: "Previous match" }));

async function renderEditor(body: string) {
  const sessions = new EditSessionStore();
  wireBackend({ body, checksum: "ck1" });
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      artifactType="skill"
      sessions={sessions}
    />,
  );
  await screen.findByRole("button", { name: "Edit as Markdown source" });
  return { sessions };
}

function pmView() {
  const pm = document.querySelector(".ProseMirror") as HTMLElement;
  return (pm as unknown as { editor: TiptapEditor }).editor.view;
}

/** The document position of the `nth` occurrence of `needle` in the body. */
function posOf(needle: string, nth = 0): number {
  const found: number[] = [];
  pmView().state.doc.descendants((node, pos) => {
    if (!node.isText || !node.text) return;
    let at = node.text.indexOf(needle);
    while (at !== -1) {
      found.push(pos + at);
      at = node.text.indexOf(needle, at + 1);
    }
  });
  if (found[nth] === undefined) throw new Error(`no "${needle}" #${nth}`);
  return found[nth];
}

/**
 * Type `text` over the `nth` occurrence of `needle`, as the author does with
 * the occurrence selected. The transaction goes through the surface's own
 * update handler, so it is an edit of the author.
 */
function typeOver(needle: string, text: string, nth = 0) {
  act(() => {
    const view = pmView();
    const from = posOf(needle, nth);
    const { tr } = view.state;
    tr.setSelection(TextSelection.create(tr.doc, from, from + needle.length));
    view.dispatch(tr.insertText(text));
  });
}

/** Type `text` at the start of the body. */
function typeAtStart(text: string) {
  act(() => {
    const view = pmView();
    view.dispatch(view.state.tr.insertText(text, 1));
  });
}

async function findInBody(body: string, q: string, expected: string) {
  const { sessions } = await renderEditor(body);
  await waitFor(() => expect(bodyText()).toBe(body));
  setFind(sessions, { form: "find" });
  fireEvent.change(query(), { target: { value: q } });
  await waitFor(() => expect(count()).toBe(expected));
  await nextFrames();
  return { sessions };
}

describe("an edit of the author in the rich body (EFR-FR-KQWT)", () => {
  it("EFR-FR-KQWT, EFR-FR-EVHJ: typing over the current match does not scroll to the next one", async () => {
    await findInBody("a two b two c two", "two", "1/3");
    scrolled = [];

    typeOver("two", "XX");

    await waitFor(() => expect(count()).toBe("1/2"));
    await nextFrames();
    expect(currentMarkText()).toBe("two");
    expect(matchScrolls()).toEqual([]);
  });

  it("EFR-FR-KQWT: each keystroke of a longer replacement leaves the view alone", async () => {
    await findInBody("a two b two c two", "two", "1/3");
    scrolled = [];

    typeOver("two", "t");
    await waitFor(() => expect(count()).toBe("1/2"));
    for (const ch of ["h", "r", "e", "e"]) {
      act(() => {
        const view = pmView();
        view.dispatch(view.state.tr.insertText(ch));
      });
    }

    await waitFor(() => expect(bodyText()).toContain("three"));
    await nextFrames();
    expect(matchScrolls()).toEqual([]);
  });

  it("EFR-FR-KQWT: typing away from a current match that survives does not scroll to it", async () => {
    await findInBody("lead two tail two", "two", "1/2");
    scrolled = [];

    typeAtStart("more ");

    await waitFor(() => expect(bodyText()).toBe("more lead two tail two"));
    await nextFrames();
    expect(count()).toBe("1/2");
    expect(matchScrolls()).toEqual([]);
  });

  it("EFR-FR-KQWT: an undo with the panel open does not scroll to the current match", async () => {
    await findInBody("a two b two", "two", "1/2");
    typeOver("two", "XX");
    await waitFor(() => expect(count()).toBe("1/1"));
    scrolled = [];

    fireEvent.keyDown(screen.getByLabelText("artifact body"), {
      key: "z",
      metaKey: true,
    });

    await waitFor(() => expect(bodyText()).toBe("a two b two"));
    // EFR-FR-EVHJ: the nearest match after the one that stayed current.
    await waitFor(() => expect(count()).toBe("2/2"));
    await nextFrames();
    expect(matchScrolls()).toEqual([]);
  });
});

describe("edits of the author that are not typing (EFR-FR-KQWT)", () => {
  it("EFR-FR-KQWT: an undo that switches the editing mode back does not scroll", async () => {
    await findInBody("a two b two c two", "two", "1/3");
    typeOver("two", "XX");
    await waitFor(() => expect(count()).toBe("1/2"));
    // The toggle is a find action (EFR-FR-DOQR); the undo after it is not.
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    const source = await screen.findByLabelText("Markdown source");
    await nextFrames();
    scrolled = [];

    fireEvent.keyDown(source, { key: "z", metaKey: true });

    await waitFor(() => expect(bodyText()).toBe("a two b two c two"));
    await waitFor(() => expect(count()).toBe("1/3"));
    await nextFrames();
    expect(matchScrolls()).toEqual([]);
  });

  it("EFR-FR-KQWT, EFR-FR-HVVO: Load from filesystem does not scroll to the current match", async () => {
    await findInBody("a two b two", "two", "1/2");
    act(() => {
      listeners[ARTIFACT_CHANGED_EXTERNALLY]?.({
        payload: { artifactId: "a.md", checksum: "ck-external" },
      });
    });
    await screen.findByRole("dialog");
    wireBackend({ body: "x two y two z two", checksum: "ck-external" });
    scrolled = [];

    fireEvent.click(screen.getByRole("button", { name: "Load from filesystem" }));

    await waitFor(() => expect(bodyText()).toBe("x two y two z two"));
    // EFR-FR-EVHJ: the current match survives at the same offset.
    await waitFor(() => expect(count()).toBe("1/3"));
    await nextFrames();
    expect(matchScrolls()).toEqual([]);
  });
});

describe("find actions bring the current match into view (EFR-FR-DOQR)", () => {
  it("EFR-FR-DOQR: a query, next-match and previous-match each scroll to the current match", async () => {
    await findInBody("a two b two", "two", "1/2");
    expect(matchScrolls().length).toBeGreaterThan(0);

    scrolled = [];
    next();
    await waitFor(() => expect(count()).toBe("2/2"));
    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));

    scrolled = [];
    previous();
    await waitFor(() => expect(count()).toBe("1/2"));
    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
  });

  it("EFR-FR-DOQR: a query change after an edit scrolls again", async () => {
    await findInBody("a two b two c three", "two", "1/2");
    typeOver("two", "XX");
    await waitFor(() => expect(count()).toBe("1/1"));
    scrolled = [];

    fireEvent.change(query(), { target: { value: "three" } });

    await waitFor(() => expect(count()).toBe("1/1"));
    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
    const last = matchScrolls();
    expect(last[last.length - 1]?.textContent).toBe("three");
  });

  it("EFR-FR-DOQR: a match-mode change after an edit scrolls again", async () => {
    await findInBody("a two b two c two", "two", "1/3");
    typeOver("two", "XX");
    await waitFor(() => expect(count()).toBe("1/2"));
    await nextFrames();
    scrolled = [];

    const inactive = screen
      .getAllByRole("radio")
      .find((r) => r.getAttribute("aria-checked") === "false")!;
    fireEvent.click(inactive);

    await waitFor(() => expect(inactive).toHaveAttribute("aria-checked", "true"));
    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
  });

  it("EFR-FR-DOQR: opening the panel again after an edit scrolls again", async () => {
    const { sessions } = await findInBody("a two b two c two", "two", "1/3");
    setFind(sessions, { form: null });
    typeAtStart("x ");
    await waitFor(() => expect(bodyText()).toBe("x a two b two c two"));
    await nextFrames();
    scrolled = [];

    setFind(sessions, { form: "find" });

    await waitFor(() => expect(count()).toBe("1/3"));
    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
  });

  it("EFR-FR-DOQR: switching to the rich surface after an edit in the source scrolls again", async () => {
    const { sessions } = await renderEditor("a two b two c two");
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    const source = await screen.findByLabelText("Markdown source");
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "two" } });
    await waitFor(() => expect(count()).toBe("1/3"));
    fireEvent.change(source, { target: { value: "a XX b two c two" } });
    await waitFor(() => expect(count()).toBe("1/2"));
    await nextFrames();
    scrolled = [];

    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));

    await waitFor(() => expect(bodyText()).toBe("a XX b two c two"));
    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
  });

  it("EFR-FR-DOQR: a Replace that changes nothing after an edit still scrolls", async () => {
    const { sessions } = await findInBody("a two b two c two", "two", "1/3");
    setFind(sessions, { form: "replace", replacement: "two" });
    typeOver("two", "XX");
    await waitFor(() => expect(count()).toBe("1/2"));
    await nextFrames();
    scrolled = [];

    fireEvent.click(screen.getByRole("button", { name: "Replace" }));

    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
    expect(bodyText()).toBe("a XX b two c two");
  });

  it("EFR-FR-DOQR, EFR-FR-EXHA: Replace after an edit scrolls to the match it advances to", async () => {
    const { sessions } = await findInBody("a two b two c two", "two", "1/3");
    setFind(sessions, { form: "replace", replacement: "2" });
    typeOver("two", "XX");
    await waitFor(() => expect(count()).toBe("1/2"));
    scrolled = [];

    fireEvent.click(screen.getByRole("button", { name: "Replace" }));

    await waitFor(() => expect(bodyText()).toBe("a XX b 2 c two"));
    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
    expect(count()).toBe("1/1");
  });

  it("EFR-FR-DOQR, EFR-FR-EYLX: Replace All after an edit scrolls, as a find action", async () => {
    const { sessions } = await findInBody("a two b two", "two", "1/2");
    setFind(sessions, { form: "replace", replacement: "two two" });
    typeAtStart("x ");
    await waitFor(() => expect(bodyText()).toBe("x a two b two"));
    scrolled = [];

    fireEvent.click(screen.getByRole("button", { name: "Replace All" }));

    await waitFor(() => expect(bodyText()).toBe("x a two two b two two"));
    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
  });
});

describe("the first next-match after an edit (EFR-FR-HZRM)", () => {
  it("EFR-FR-HZRM: reveals the current match without advancing, then steps", async () => {
    await findInBody("a two b two c two", "two", "1/3");
    typeOver("two", "XX");
    await waitFor(() => expect(count()).toBe("1/2"));
    await nextFrames();
    scrolled = [];

    next();

    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
    expect(count()).toBe("1/2");

    next();
    await waitFor(() => expect(count()).toBe("2/2"));
  });

  it("EFR-FR-HZRM: Enter in the query input behaves as next-match", async () => {
    await findInBody("a two b two c two", "two", "1/3");
    typeOver("two", "XX");
    await waitFor(() => expect(count()).toBe("1/2"));
    await nextFrames();
    scrolled = [];

    fireEvent.keyDown(query(), { key: "Enter" });

    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
    expect(count()).toBe("1/2");
  });

  it("EFR-FR-HZRM, EFR-FR-DYMA: previous-match after an edit steps backward", async () => {
    await findInBody("a two b two c two", "two", "1/3");
    next();
    await waitFor(() => expect(count()).toBe("2/3"));
    typeOver("two", "XX", 1);
    await waitFor(() => expect(count()).toBe("2/2"));
    scrolled = [];

    previous();

    await waitFor(() => expect(count()).toBe("1/2"));
    await waitFor(() => expect(matchScrolls().length).toBeGreaterThan(0));
  });
});

describe("an edit of the author in the raw-text source (EFR-FR-KQWT)", () => {
  async function renderSource(body: string) {
    const { sessions } = await renderEditor(body);
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    // jsdom does no layout: hold the textarea scrolled down and record every
    // scroll the Editor asks for.
    const writes: number[] = [];
    let top = 500;
    Object.defineProperty(source, "scrollTop", {
      configurable: true,
      get: () => top,
      set: (v: number) => {
        writes.push(v);
        top = v;
      },
    });
    return { sessions, source, writes, resetTop: () => (top = 500) };
  }

  it("EFR-FR-KQWT, EFR-FR-HZRM: typing does not scroll the textarea; next-match does", async () => {
    const { sessions, source, writes, resetTop } = await renderSource(
      "a two b two c two",
    );
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "two" } });
    await waitFor(() => expect(count()).toBe("1/3"));
    await nextFrames();
    resetTop();
    writes.length = 0;

    fireEvent.change(source, { target: { value: "a XX b two c two" } });

    await waitFor(() => expect(count()).toBe("1/2"));
    await nextFrames();
    expect(writes).toEqual([]);

    next();
    await waitFor(() => expect(writes.length).toBeGreaterThan(0));
    expect(count()).toBe("1/2");
  });
});

describe("the frontmatter peek after an edit (EFR-FR-KQWT, EFR-FR-DSKI)", () => {
  const FM = "---\nname: alpha\n---\n";

  it("EFR-FR-KQWT, EFR-FR-HZRM: an edit that makes a frontmatter match current does not expand the region; next-match does", async () => {
    const { sessions } = await renderEditor(FM + "the body has alpha");
    await waitFor(() => expect(bodyText()).toBe("the body has alpha"));
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });
    await waitFor(() => expect(count()).toBe("1/2"));
    next();
    await waitFor(() => expect(count()).toBe("2/2"));

    // Collapse the region by scrolling the body down (EDT-FR-20).
    const scroller = document.querySelector(".editor") as HTMLElement;
    Object.defineProperty(scroller, "scrollTop", {
      configurable: true,
      value: 400,
    });
    fireEvent.scroll(scroller);
    await waitFor(() =>
      expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument(),
    );

    // The body loses its only match, so the frontmatter match is current.
    typeOver("alpha", "beta");
    await waitFor(() => expect(count()).toBe("1/1"));
    await nextFrames();
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();

    next();
    await waitFor(() =>
      expect(screen.getByLabelText("Frontmatter")).toBeInTheDocument(),
    );
    expect(count()).toBe("1/1");
  });

  it("EFR-FR-DOQR, EFR-FR-DSKI: a query change that keeps the same current match expands the region", async () => {
    const { sessions } = await renderEditor(FM + "the body has alpha");
    await waitFor(() => expect(bodyText()).toBe("the body has alpha"));
    setFind(sessions, { form: "find" });
    fireEvent.change(query(), { target: { value: "alpha" } });
    await waitFor(() => expect(count()).toBe("1/2"));
    next();
    await waitFor(() => expect(count()).toBe("2/2"));
    const scroller = document.querySelector(".editor") as HTMLElement;
    Object.defineProperty(scroller, "scrollTop", {
      configurable: true,
      value: 400,
    });
    fireEvent.scroll(scroller);
    await waitFor(() =>
      expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument(),
    );
    typeOver("alpha", "beta");
    await waitFor(() => expect(count()).toBe("1/1"));
    await nextFrames();
    expect(screen.queryByLabelText("Frontmatter")).not.toBeInTheDocument();

    // The same match stays current, so only the find action can expand it.
    fireEvent.change(query(), { target: { value: "alph" } });

    await waitFor(() =>
      expect(screen.getByLabelText("Frontmatter")).toBeInTheDocument(),
    );
    expect(count()).toBe("1/1");
  });
});
