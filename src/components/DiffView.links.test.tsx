// The links of a Diff tab's Rich rendering (`DFV-diff-viewer.md` DFV-FR-QJRW).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { openUrl } from "@tauri-apps/plugin-opener";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(() => Promise.resolve()) }));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

import { RichSideBySide, RichUnified } from "./DiffView/rich";
import { revealDiffFragment, staticDestination } from "./DiffView/links";
import { RichLinkHostContext, type LinkHost } from "./linkFollow";
import { resetToasts, visibleToasts } from "../state/toasts";

let host: LinkHost;
let scrolled: Element[];
const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollIntoView");

beforeEach(() => {
  resetToasts();
  vi.mocked(openUrl).mockClear();
  scrolled = [];
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value(this: Element) {
      scrolled.push(this);
    },
  });
  host = {
    base: { fileDir: "docs", roots: ["/p"] },
    selfPath: "docs/a.md",
    openFile: vi.fn(),
    revealFragment: revealDiffFragment,
  };
});

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
  if (original) Object.defineProperty(HTMLElement.prototype, "scrollIntoView", original);
  else delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollIntoView;
});

const link = (text: string) => screen.getAllByText(text).find((el) => el.closest("a"))!.closest("a")!;

describe("a read-only Rich rendering", () => {
  it("DFV-FR-QJRW, EDT-FR-YHKG: a plain click on a static link follows it against the compared file", () => {
    render(
      <RichLinkHostContext.Provider value={host}>
        <RichUnified revisions={{ old: "Old [b](b.md#c).\n", new: "New [b](b.md#c).\n", isBinary: false }} />
      </RichLinkHostContext.Provider>,
    );
    const links = screen.getAllByText("b").map((el) => el.closest("a")!);
    expect(links).toHaveLength(2);
    // The original's link is followed too: it is a read-only rendering.
    fireEvent.click(links[0]);
    fireEvent.click(links[1]);
    expect(host.openFile).toHaveBeenCalledTimes(2);
    expect(host.openFile).toHaveBeenCalledWith("docs/b.md", "c");
  });

  it("DFV-FR-QJRW, EDT-FR-UXYY, EDT-FR-BBIF: an external link opens outside the app and shows its destination", async () => {
    render(
      <RichLinkHostContext.Provider value={host}>
        <RichUnified revisions={{ old: null, new: "[web](https://example.com)\n", isBinary: false }} />
      </RichLinkHostContext.Provider>,
    );
    const web = link("web");
    expect(web.getAttribute("title")).toBe("https://example.com");
    expect(web.hasAttribute("href")).toBe(false);
    fireEvent.click(web);
    await Promise.resolve();
    await Promise.resolve();
    expect(openUrl).toHaveBeenCalledWith("https://example.com");
  });

  it("DFV-FR-QJRW, EDT-FR-HMJI: a fragment scrolls to the heading of the target revision", () => {
    const text = "[go](#usage)\n\n# Usage\n\nBody.\n";
    render(
      <RichLinkHostContext.Provider value={host}>
        <RichSideBySide revisions={{ old: text, new: text, isBinary: false }} />
      </RichLinkHostContext.Provider>,
    );
    const goLinks = screen.getAllByText("go").map((el) => el.closest("a")!);
    // Each side's link goes to the target revision's heading.
    fireEvent.click(goLinks[0]);
    fireEvent.click(goLinks[1]);
    expect(scrolled).toHaveLength(2);
    for (const heading of scrolled) {
      expect(heading.tagName).toBe("H1");
      expect(heading.closest('[data-revision="new"]')).not.toBeNull();
    }
    expect(scrolled[0]).toBe(scrolled[1]);
  });

  it("EDT-FR-HYVP: a fragment with no heading in the target shows the toast", () => {
    render(
      <RichLinkHostContext.Provider value={host}>
        <RichUnified revisions={{ old: "# Gone\n", new: "[go](#gone)\n", isBinary: false }} />
      </RichLinkHostContext.Provider>,
    );
    fireEvent.click(link("go"));
    expect(scrolled).toHaveLength(0);
    expect(visibleToasts()[0]).toMatchObject({ key: "link-target-missing" });
  });

  it("EDT-FR-QBIE: with no host a file link is refused", () => {
    render(<RichUnified revisions={{ old: null, new: "[b](b.md)\n", isBinary: false }} />);
    fireEvent.click(link("b"));
    expect(visibleToasts()[0]).toMatchObject({ key: "link-refused" });
  });
});

describe("revealDiffFragment", () => {
  it("DFV-FR-QJRW: the headings of the editable runs count, in document order", () => {
    document.body.innerHTML = `
      <div class="diff-rich">
        <div class="diff-block" data-revision="old"><h2>Setup</h2></div>
        <div class="diff-block" data-revision="new"><h2>Setup</h2></div>
        <div class="diff-run"><div class="diff-run__doc"><h2>Setup</h2></div></div>
        <a id="origin">x</a>
      </div>`;
    const origin = document.getElementById("origin")!;
    expect(revealDiffFragment("setup-1", origin)).toBe(true);
    expect(scrolled[0].closest(".diff-run")).not.toBeNull();
    expect(revealDiffFragment("setup-2", origin)).toBe(false);
  });

  it("DFV-FR-QJRW: a link outside a Rich rendering jumps nowhere", () => {
    const lone = document.createElement("a");
    expect(revealDiffFragment("x", lone)).toBe(false);
  });

  it("DFV-FR-QJRW, EDT-FR-ZGKO: a static destination drops its title and its angle brackets", () => {
    expect(staticDestination('a.md "Title"')).toBe("a.md");
    expect(staticDestination("<a b.md>")).toBe("a b.md");
    expect(staticDestination("  b.md#c ")).toBe("b.md#c");
    render(
      <RichLinkHostContext.Provider value={host}>
        <RichUnified revisions={{ old: null, new: "[s](<s.md#h>)\n", isBinary: false }} />
      </RichLinkHostContext.Provider>,
    );
    fireEvent.click(link("s"));
    expect(host.openFile).toHaveBeenCalledWith("docs/s.md", "h");
  });

  it("DFV-FR-QJRW: a static link takes the focus, and Enter follows it", () => {
    render(
      <RichLinkHostContext.Provider value={host}>
        <RichUnified revisions={{ old: null, new: "[b](b.md)\n", isBinary: false }} />
      </RichLinkHostContext.Provider>,
    );
    const b = screen.getByRole("link", { name: "b" });
    expect(b.tabIndex).toBe(0);
    fireEvent.keyDown(b, { key: " " });
    expect(host.openFile).not.toHaveBeenCalled();
    fireEvent.keyDown(b, { key: "Enter" });
    expect(host.openFile).toHaveBeenCalledWith("docs/b.md", null);
  });

  it("EDT-FR-AWFH, NTF-FR-LSKJ: a static link with another scheme is refused", async () => {
    render(
      <RichLinkHostContext.Provider value={host}>
        <RichUnified revisions={{ old: null, new: "[bad](javascript:alert(1))\n", isBinary: false }} />
      </RichLinkHostContext.Provider>,
    );
    fireEvent.click(link("bad"));
    await Promise.resolve();
    expect(openUrl).not.toHaveBeenCalled();
    expect(visibleToasts()[0]).toMatchObject({ key: "link-refused" });
  });
});
