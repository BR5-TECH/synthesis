// Links of a rich surface can be followed (`EDT-editor.md` EDT-FR-YHKG to EDT-FR-QBIE).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Editor } from "@tiptap/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { markdownExtensions } from "./markdownFidelity";
import { MODIFIER_HELD_CLASS, followsOnClick, linkTooltip } from "./linkNavigation";
import type { LinkHost } from "./linkFollow";
import { resetToasts, visibleToasts } from "../state/toasts";
import { logWarn } from "../logging";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(() => Promise.resolve()) }));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

const ROOT = "/Users/me/project";

let editor: Editor | null = null;
let host: LinkHost | null = null;

function mount(content: string, { editable = true, withHost = true } = {}): Editor {
  const element = document.createElement("div");
  document.body.appendChild(element);
  host = withHost
    ? {
        base: { fileDir: "docs", roots: [ROOT] },
        selfPath: "docs/guide.md",
        openFile: vi.fn(),
      }
    : null;
  editor = new Editor({
    element,
    extensions: markdownExtensions([], { linkHost: () => host }),
    content,
    editable,
  });
  return editor;
}

function linkNamed(text: string): HTMLAnchorElement {
  const link = Array.from(editor!.view.dom.querySelectorAll("a")).find((a) => a.textContent === text);
  if (!link) throw new Error(`no link ${text}`);
  return link;
}

/** Click the link `text` and return the event, so its default can be read. */
function click(text: string, init: MouseEventInit = {}): MouseEvent {
  const event = new MouseEvent("click", { bubbles: true, cancelable: true, button: 0, ...init });
  const target = linkNamed(text).firstChild ?? linkNamed(text);
  target.dispatchEvent(event);
  return event;
}

/** Lets the microtasks of an external open run. */
async function flush() {
  await Promise.resolve();
  await Promise.resolve();
}

function setPlatform(value: string) {
  vi.spyOn(navigator, "platform", "get").mockReturnValue(value);
}

beforeEach(() => {
  resetToasts();
  vi.mocked(openUrl).mockClear();
  vi.mocked(openUrl).mockImplementation(() => Promise.resolve());
  vi.mocked(logWarn).mockClear();
  setPlatform("Linux x86_64");
  // No surface of an earlier test still watches the modifier.
  expect(document.documentElement.classList.contains(MODIFIER_HELD_CLASS)).toBe(false);
});

afterEach(() => {
  editor?.destroy();
  editor = null;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("the gesture", () => {
  it("EDT-FR-YHKG: on an editable surface a plain click follows nothing", async () => {
    mount("See [site](https://example.com).");
    const event = click("site");
    await flush();
    expect(openUrl).not.toHaveBeenCalled();
    // The window never goes to the address itself.
    expect(event.defaultPrevented).toBe(true);
  });

  it("EDT-FR-YHKG, EDT-FR-UXYY: on an editable surface Ctrl+click opens the address outside the app", async () => {
    mount("See [site](https://example.com/a?b=1).");
    const event = click("site", { ctrlKey: true });
    await flush();
    expect(openUrl).toHaveBeenCalledWith("https://example.com/a?b=1");
    expect(event.defaultPrevented).toBe(true);
  });

  it("EDT-FR-YHKG: on macOS Cmd+click follows, and Ctrl+click follows nothing", async () => {
    setPlatform("MacIntel");
    mount("[mail](mailto:a@b.c)");
    click("mail", { ctrlKey: true });
    await flush();
    expect(openUrl).not.toHaveBeenCalled();
    click("mail", { metaKey: true });
    await flush();
    expect(openUrl).toHaveBeenCalledWith("mailto:a@b.c");
  });

  it("EDT-FR-YHKG: on a read-only surface a plain click follows", async () => {
    mount("[site](http://x.org)", { editable: false });
    const event = click("site");
    await flush();
    expect(openUrl).toHaveBeenCalledWith("http://x.org");
    expect(event.defaultPrevented).toBe(true);
  });

  it("EDT-FR-UXYY: a middle click on a link opens no window, and one on plain text is left alone", () => {
    mount("Text and [site](http://x.org).", { editable: false });
    const onLink = new MouseEvent("auxclick", { bubbles: true, cancelable: true, button: 1 });
    linkNamed("site").dispatchEvent(onLink);
    expect(onLink.defaultPrevented).toBe(true);
    const onText = new MouseEvent("auxclick", { bubbles: true, cancelable: true, button: 1 });
    editor!.view.dom.querySelector("p")!.firstChild!.dispatchEvent(onText);
    expect(onText.defaultPrevented).toBe(false);
  });

  it("EDT-FR-YHKG: on a read-only surface a drag that selected link text follows nothing", async () => {
    mount("[site](http://x.org)", { editable: false });
    // The text node sits inside the tooltip's span.
    const text = document.createTreeWalker(linkNamed("site"), NodeFilter.SHOW_TEXT).nextNode()!;
    const range = document.createRange();
    range.setStart(text, 0);
    range.setEnd(text, 2);
    document.getSelection()!.removeAllRanges();
    document.getSelection()!.addRange(range);
    click("site");
    await flush();
    expect(openUrl).not.toHaveBeenCalled();
    document.getSelection()!.removeAllRanges();
  });

  it("EDT-FR-YHKG: a button other than the main one follows nothing", async () => {
    mount("[site](http://x.org)", { editable: false });
    click("site", { button: 1 });
    await flush();
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("a click outside a link is left to the surface", () => {
    mount("Plain text and [site](http://x.org).", { editable: false });
    const event = new MouseEvent("click", { bubbles: true, cancelable: true });
    editor!.view.dom.querySelector("p")!.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });

  it("EDT-FR-ZGKO: a click that follows a link is claimed, so the surface selects no node for it", () => {
    const ed = mount("Text and [site](http://x.org) more.");
    const claims = (init: MouseEventInit) => {
      const event = click("site", init);
      return ed.view.someProp("handleClick", (f) => f(ed.view, 1, event)) === true;
    };
    // Ctrl is also the node-select modifier of the surface on this platform.
    expect(claims({ ctrlKey: true })).toBe(true);
    // A plain click on an editable surface places the caret as on other text.
    expect(claims({})).toBe(false);
    const text = new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true });
    ed.view.dom.querySelector("p")!.dispatchEvent(text);
    expect(ed.view.someProp("handleClick", (f) => f(ed.view, 1, text)) === true).toBe(false);
  });

  it("followsOnClick holds the rule for each platform", () => {
    const plain = { button: 0, metaKey: false, ctrlKey: false };
    expect(followsOnClick(plain, false, false)).toBe(true);
    expect(followsOnClick(plain, true, false)).toBe(false);
    expect(followsOnClick({ ...plain, ctrlKey: true }, true, false)).toBe(true);
    expect(followsOnClick({ ...plain, metaKey: true }, true, false)).toBe(false);
    expect(followsOnClick({ ...plain, metaKey: true }, true, true)).toBe(true);
    expect(followsOnClick({ ...plain, ctrlKey: true }, false, true)).toBe(false);
  });
});

describe("the destination", () => {
  it("EDT-FR-ZGKO: a reference-style link and an autolink are followed", async () => {
    mount("[ref link][r] and <https://auto.example>\n\n[r]: https://ref.example");
    click("ref link", { ctrlKey: true });
    click("https://auto.example", { ctrlKey: true });
    await flush();
    expect(openUrl).toHaveBeenCalledWith("https://ref.example");
    expect(openUrl).toHaveBeenCalledWith("https://auto.example");
  });

  it("EDT-FR-HMJI, EDT-FR-ZGKO: a fragment puts the caret at its heading and changes nothing", () => {
    const scroll = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, writable: true, value: scroll });
    const ed = mount("[Go](#second-part)\n\n# First\n\ntext\n\n## Second Part\n\nmore");
    const before = ed.state.doc;
    const onUpdate = vi.fn();
    ed.on("update", onUpdate);
    click("Go", { ctrlKey: true });
    expect(ed.state.doc).toBe(before);
    expect(onUpdate).not.toHaveBeenCalled();
    const { $from } = ed.state.selection;
    expect($from.parent.type.name).toBe("heading");
    expect($from.parent.textContent).toBe("Second Part");
    expect($from.parentOffset).toBe(0);
    // EDT-FR-YSJX: the heading scrolls smoothly to the top, and the surface has the focus.
    expect(scroll).toHaveBeenCalledWith({ behavior: "smooth", block: "start" });
    expect((scroll.mock.contexts[0] as Element).textContent).toBe("Second Part");
    expect(document.activeElement).toBe(ed.view.dom);
    delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollIntoView;
  });

  it("EDT-FR-BJFP: a fragment reaches the second of two headings with one slug", () => {
    const ed = mount("[Go](#usage-1)\n\n## Usage\n\none\n\n## Usage\n\ntwo");
    click("Go", { ctrlKey: true });
    const pos = ed.state.selection.from;
    const second = ed.state.doc.resolve(pos);
    expect(second.parent.textContent).toBe("Usage");
    // The caret is in the second heading, after the first paragraph.
    expect(ed.state.doc.textBetween(0, pos)).toContain("one");
  });

  it("EDT-FR-HYVP, NTF-FR-WQRN: a fragment with no heading moves nothing and shows the toast", () => {
    const ed = mount("[Go](#nowhere)\n\n# Only");
    const selection = ed.state.selection;
    click("Go", { ctrlKey: true });
    expect(ed.state.selection.eq(selection)).toBe(true);
    expect(visibleToasts()).toEqual([
      expect.objectContaining({
        key: "link-target-missing",
        level: "Warn",
        title: "Link target not found",
        body: "No file or heading matches the link.",
        address: null,
      }),
    ]);
  });

  it("EDT-FR-RFCU, EDT-FR-GEHE, EDT-FR-QBIE: a file link goes to the host with the resolved path", () => {
    mount("[a](../specs/x.md#Part) [b](/root.md) [c](/Users/me/project/docs/y.md)", { editable: false });
    click("a");
    click("b");
    click("c");
    expect(host!.openFile).toHaveBeenNthCalledWith(1, "specs/x.md", "Part");
    expect(host!.openFile).toHaveBeenNthCalledWith(2, "root.md", null);
    expect(host!.openFile).toHaveBeenNthCalledWith(3, "docs/y.md", null);
  });

  it("EDT-FR-GEHE: a link to the file of the surface opens no tab, and its fragment jumps", () => {
    const ed = mount("[me](guide.md#end) [me too](./guide.md)\n\n# End", { editable: false });
    const start = ed.state.selection;
    // With no fragment, nothing moves and nothing is shown.
    click("me too");
    expect(ed.state.selection.eq(start)).toBe(true);
    expect(visibleToasts()).toEqual([]);
    click("me");
    expect(host!.openFile).not.toHaveBeenCalled();
    expect(ed.state.selection.$from.parent.textContent).toBe("End");
  });

  it("EDT-FR-AWFH, NTF-FR-LSKJ: another scheme is refused with the toast", async () => {
    mount("[bad](ftp://x.org/file)", { editable: false });
    click("bad");
    await flush();
    expect(openUrl).not.toHaveBeenCalled();
    expect(host!.openFile).not.toHaveBeenCalled();
    expect(visibleToasts()).toEqual([
      expect.objectContaining({
        key: "link-refused",
        level: "Warn",
        title: "Link not followed",
        body: "The app does not open this kind of link.",
        address: null,
      }),
    ]);
    // The log names the scheme, never the address.
    const fields = vi.mocked(logWarn).mock.calls.map((call) => JSON.stringify(call));
    expect(fields.join()).toContain("ftp");
    expect(fields.join()).not.toContain("x.org");
  });

  it("EDT-FR-AWFH, NTF-FR-LSKJ: a path outside the project is refused with the toast", () => {
    mount("[up](../../../etc/hosts)", { editable: false });
    click("up");
    expect(host!.openFile).not.toHaveBeenCalled();
    expect(visibleToasts()[0]).toMatchObject({
      key: "link-refused",
      body: "The link goes outside the project.",
    });
  });

  it("EDT-FR-QBIE: with no host a file link is refused, and an external link still opens", async () => {
    mount("[file](other.md) [web](https://example.com)", { editable: false, withHost: false });
    click("file");
    expect(visibleToasts()[0]).toMatchObject({
      key: "link-refused",
      body: "The app does not open this kind of link.",
    });
    click("web");
    await flush();
    expect(openUrl).toHaveBeenCalledWith("https://example.com");
  });

  it("EDT-FR-QBIE: a host jump of its own takes the fragment", () => {
    mount("[Go](#x)", { editable: false });
    const reveal = vi.fn(() => true);
    host = { ...host!, revealFragment: reveal };
    click("Go");
    expect(reveal).toHaveBeenCalledWith("x", linkNamed("Go"));
    expect(visibleToasts()).toEqual([]);
  });

  it("EDT-FR-UXYY: an address the opener cannot open is logged and throws nothing", async () => {
    vi.mocked(openUrl).mockImplementation(() => Promise.reject(new Error("denied")));
    mount("[site](https://example.com)", { editable: false });
    click("site");
    await vi.waitFor(() =>
      expect(logWarn).toHaveBeenCalledWith(["frontend"], "link could not be opened", { scheme: "https" }),
    );
  });
});

describe("the tooltip and the pointer", () => {
  it("EDT-FR-BBIF: an editable link shows its destination and the gesture hint", () => {
    mount("[site](https://example.com)");
    expect(linkNamed("site").querySelector("[title]")?.getAttribute("title")).toBe(
      "https://example.com\nCtrl+click to follow",
    );
  });

  it("EDT-FR-BBIF: on macOS the hint names ⌘", () => {
    setPlatform("MacIntel");
    mount("[site](https://example.com)");
    expect(linkNamed("site").querySelector("[title]")?.getAttribute("title")).toBe(
      "https://example.com\n⌘-click to follow",
    );
  });

  it("EDT-FR-BBIF: a read-only link shows the destination only, also after a change of state", () => {
    const ed = mount("[site](https://example.com)");
    ed.setEditable(false);
    expect(linkNamed("site").querySelector("[title]")?.getAttribute("title")).toBe("https://example.com");
  });

  it("EDT-FR-BBIF: a long destination is cut in the tooltip", () => {
    expect(linkTooltip(`https://x.org/${"a".repeat(400)}`, false, false)).toHaveLength(200);
  });

  it("EDT-FR-BBIF: the document root is marked while the modifier is held", () => {
    mount("[site](https://example.com)");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Control", ctrlKey: true }));
    expect(document.documentElement.classList.contains(MODIFIER_HELD_CLASS)).toBe(true);
    window.dispatchEvent(new KeyboardEvent("keyup", { key: "Control", ctrlKey: false }));
    expect(document.documentElement.classList.contains(MODIFIER_HELD_CLASS)).toBe(false);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Control", ctrlKey: true }));
    editor!.destroy();
    editor = null;
    // The last surface to go takes the mark and its listeners away.
    expect(document.documentElement.classList.contains(MODIFIER_HELD_CLASS)).toBe(false);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Control", ctrlKey: true }));
    expect(document.documentElement.classList.contains(MODIFIER_HELD_CLASS)).toBe(false);
  });

  it("EDT-FR-ZGKO: the tooltip is not content, so the Markdown is unchanged", () => {
    const ed = mount("A [link](https://example.com) here.");
    const storage = ed.storage as unknown as Record<string, { getMarkdown?: () => string }>;
    expect(storage.markdown.getMarkdown?.()).toBe("A [link](https://example.com) here.");
  });
});
