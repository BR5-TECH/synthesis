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
import { TabScrollScope } from "../hooks/useRememberedScroll";
import { resetLogBufferForTest } from "../logging";
import {
  documentSurface,
  holdTabScroll,
  resetTabScrollForTest,
  tabScrollOf,
} from "../state/tabScroll";
import {
  installFakeResizeObserver,
  installFakeScroll,
  limitScroll,
  resizeAll,
} from "../test/fakeScroll";
import {
  BODY,
  makeThread,
  wireBackend as wireCommentsBackend,
} from "../test/editorCommentsFixtures";
import { otherCommand } from "../test/invokeMocks";
import type { ArtifactContents } from "../types";

let load: ArtifactContents;
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const WITH_FRONTMATTER = "---\nname: Foo\n---\n\n# Title\n\nhello";

let sessions: EditSessionStore;
let restoreScroll: () => void;
let restoreObserver: () => void;

beforeEach(() => {
  resetTabScrollForTest();
  resetLogBufferForTest();
  restoreScroll = installFakeScroll();
  restoreObserver = installFakeResizeObserver();
  sessions = new EditSessionStore();
  load = { body: WITH_FRONTMATTER, checksum: "ck1" };
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "load_artifact_contents_by_id"
      ? Promise.resolve(load)
      : otherCommand(cmd),
  );
});

afterEach(() => {
  cleanup();
  restoreObserver();
  restoreScroll();
  resetLogBufferForTest();
});

/** The Editor of the tab `tabId`, as the shell renders it. */
const mount = (tabId = "tab-a", artifactId = "a.md") =>
  render(
    <TabScrollScope tabId={tabId}>
      <Editor artifactId={artifactId} artifactName={artifactId} sessions={sessions} />
    </TabScrollScope>,
  );

const body = () => document.querySelector(".editor") as HTMLElement;
const source = () => screen.getByLabelText("Markdown source") as HTMLTextAreaElement;
const sourceLayer = () =>
  document.querySelector(".editor__source-hl") as HTMLElement;

function scroll(el: HTMLElement, top: number) {
  el.scrollTop = top;
  fireEvent.scroll(el);
}

async function toText() {
  fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
  await screen.findByLabelText("Markdown source");
}

describe("Editor scroll memory", () => {
  it("EDT-FR-UERT, TAB-FR-09: the rich body comes back where the author left it after a tab switch", async () => {
    const first = mount();
    await screen.findByRole("heading", { name: "Title" });
    scroll(body(), 600);
    first.unmount();

    mount();
    await screen.findByRole("heading", { name: "Title" });
    expect(body().scrollTop).toBe(600);
  });

  it("EDT-FR-NYCP: a restored offset below the top collapses the frontmatter region", async () => {
    holdTabScroll("tab-a", documentSurface("a.md", "wysiwyg"), 600);
    mount();
    await screen.findByRole("heading", { name: "Title" });

    expect(
      await screen.findByRole("button", { name: "Expand frontmatter" }),
    ).toBeInTheDocument();
  });

  it("EDT-FR-NYCP: a restored offset inside the top zone keeps the frontmatter region expanded", async () => {
    holdTabScroll("tab-a", documentSurface("a.md", "wysiwyg"), 5);
    mount();
    await screen.findByRole("heading", { name: "Title" });

    expect(body().scrollTop).toBe(5);
    expect(screen.queryByRole("button", { name: "Expand frontmatter" })).toBeNull();
  });

  it("EDT-FR-UERT: a Markdown file in raw-text mode comes back where the author left it", async () => {
    const first = mount();
    await screen.findByRole("heading", { name: "Title" });
    await toText();
    scroll(source(), 300);
    first.unmount();

    mount();
    expect(source().scrollTop).toBe(300);
  });

  it("EDT-FR-UERT, EDT-FR-NYCP: a source file and the highlight layer behind it come back where the author left them", async () => {
    load = { body: "fn main() {}\n".repeat(40), checksum: "ck1" };
    const first = mount("tab-a", "main.rs");
    const surface = (await screen.findByLabelText("Source")) as HTMLTextAreaElement;
    scroll(surface, 300);
    first.unmount();

    mount("tab-a", "main.rs");
    expect(((await screen.findByLabelText("Source")) as HTMLTextAreaElement).scrollTop).toBe(300);
    expect(sourceLayer().scrollTop).toBe(300);
  });

  it("EDT-FR-UERT: a mode switch starts the other mode at the top, and the tab comes back there", async () => {
    const first = mount();
    await screen.findByRole("heading", { name: "Title" });
    await toText();
    scroll(source(), 300);
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByRole("heading", { name: "Title" });
    scroll(body(), 200);

    await toText();
    expect(source().scrollTop).toBe(0);
    expect(tabScrollOf("tab-a", documentSurface("a.md", "text"))).toBe(0);
    first.unmount();

    mount();
    expect(source().scrollTop).toBe(0);
  });

  it("NAW-FR-IHDU: each file shown in one tab gets its own offset back when it is selected again", async () => {
    load = { body: "# Title\n\nhello", checksum: "ck1" };
    // The draft tab keys its Editor on the selected file, so a selection is
    // an unmount and a mount in the same tab.
    const a = mount("draft:d1", "d1/a.md");
    await screen.findByRole("heading", { name: "Title" });
    scroll(body(), 640);
    a.unmount();

    const b = mount("draft:d1", "d1/b.md");
    await screen.findByRole("heading", { name: "Title" });
    expect(body().scrollTop).toBe(0);
    scroll(body(), 50);
    b.unmount();

    mount("draft:d1", "d1/a.md");
    await screen.findByRole("heading", { name: "Title" });
    expect(body().scrollTop).toBe(640);
  });

  it("EDT-FR-UERT: two tabs each get their own offset back", async () => {
    load = { body: "# Title\n\nhello", checksum: "ck1" };
    const a = mount("tab-a", "a.md");
    await screen.findByRole("heading", { name: "Title" });
    scroll(body(), 120);
    a.unmount();

    const b = mount("tab-b", "b.md");
    await screen.findByRole("heading", { name: "Title" });
    scroll(body(), 480);
    b.unmount();

    const again = mount("tab-a", "a.md");
    await screen.findByRole("heading", { name: "Title" });
    expect(body().scrollTop).toBe(120);
    again.unmount();

    mount("tab-b", "b.md");
    await screen.findByRole("heading", { name: "Title" });
    expect(body().scrollTop).toBe(480);
  });

  it("EDT-FR-UERT: a switch back to rich text starts the rich body at the top", async () => {
    mount();
    await screen.findByRole("heading", { name: "Title" });
    scroll(body(), 600);
    await toText();

    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    await screen.findByRole("heading", { name: "Title" });

    expect(body().scrollTop).toBe(0);
    expect(tabScrollOf("tab-a", documentSurface("a.md", "wysiwyg"))).toBe(0);
  });

  it("EDT-FR-NYCP: the rich body restore waits until the body is tall enough, with the frontmatter in step", async () => {
    holdTabScroll("tab-a", documentSurface("a.md", "wysiwyg"), 600);
    limitScroll(".editor", 100);
    mount();
    await screen.findByRole("heading", { name: "Title" });
    expect(body().scrollTop).toBe(100);
    // 100 is already past the top zone.
    expect(
      await screen.findByRole("button", { name: "Expand frontmatter" }),
    ).toBeInTheDocument();

    limitScroll(".editor", null);
    act(() => resizeAll());

    expect(body().scrollTop).toBe(600);
  });

  it("EDT-FR-NYCP: the author scrolling with the wheel ends a rich body restore that waits", async () => {
    holdTabScroll("tab-a", documentSurface("a.md", "wysiwyg"), 600);
    limitScroll(".editor", 100);
    mount();
    await screen.findByRole("heading", { name: "Title" });

    fireEvent.wheel(body());
    limitScroll(".editor", null);
    act(() => resizeAll());

    expect(body().scrollTop).toBe(100);
  });

  it("EDT-FR-NYCP, CMT-FR-27: the comment rail follows a restore that finishes later, with no scroll event", async () => {
    wireCommentsBackend(invokeMock, {
      load: { body: BODY, checksum: "ck1" },
      threads: [makeThread()],
      calls: [],
    });
    holdTabScroll("tab-a", documentSurface("a.md", "wysiwyg"), 600);
    limitScroll(".editor", 100);
    mount();
    await screen.findByLabelText("artifact body");
    const layer = await waitFor(() => {
      const el = document.querySelector(".comment-rail__aligned") as HTMLElement | null;
      if (!el) throw new Error("the rail is not open");
      return el;
    });

    limitScroll(".editor", null);
    act(() => resizeAll());

    expect(body().scrollTop).toBe(600);
    expect(layer.style.transform).toBe("translateY(-600px)");
  });

  describe("with the find panel open", () => {
    let scrolledIntoView: ReturnType<typeof vi.fn>;
    let original: PropertyDescriptor | undefined;

    beforeEach(() => {
      scrolledIntoView = vi.fn();
      original = Object.getOwnPropertyDescriptor(Element.prototype, "scrollIntoView");
      Object.defineProperty(Element.prototype, "scrollIntoView", {
        configurable: true,
        value: scrolledIntoView,
      });
      load = { body: "# Title\n\nhello one\n\nhello two\n", checksum: "ck1" };
      // The current match is brought into view on the next frame. Running the
      // frame at once lets a test see whether that happened.
      vi.stubGlobal("requestAnimationFrame", (cb: FrameRequestCallback) => {
        cb(0);
        return 0;
      });
    });

    afterEach(() => {
      vi.unstubAllGlobals();
      if (original) Object.defineProperty(Element.prototype, "scrollIntoView", original);
      else delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
    });

    it("EDT-FR-UERT, EFR-FR-DOQR: a tab that comes back shows where the author left it, and the next match is still brought into view", async () => {
      const first = mount();
      await screen.findByRole("heading", { name: "Title" });
      act(() => sessions.setFind("a.md", { form: "find", query: "hello" }));
      // The match that became current when the panel opened was brought into view.
      await waitFor(() => expect(scrolledIntoView).toHaveBeenCalled());
      scroll(body(), 600);
      first.unmount();
      scrolledIntoView.mockClear();

      mount();
      await screen.findByRole("heading", { name: "Title" });
      await screen.findByLabelText("Find");
      await waitFor(() =>
        expect(document.querySelector(".find-match--current")).not.toBeNull(),
      );
      expect(body().scrollTop).toBe(600);
      expect(scrolledIntoView).not.toHaveBeenCalled();

      fireEvent.click(screen.getByRole("button", { name: "Next match" }));
      await waitFor(() => expect(scrolledIntoView).toHaveBeenCalled());
    });
  });
});
