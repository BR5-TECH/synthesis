// The PDF viewer's text search with highlights, match navigation, and the
// selectable text layer (PDV-FR-XMRL, PDV-FR-BPXG, PDV-FR-TIFB, PDV-FR-KDVB).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetViewStateForTest } from "../../state/documentViewState";
import { ID, resetFixtures } from "../DocumentViewer/viewerFixtures";
import { PdfViewer } from ".";
import {
  page,
  pdfPayload,
  pdfReadReturns,
  pdfjsFake,
  resetPdfFixtures,
  state,
} from "./pdfFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../DocumentViewer/viewerFixtures")).invokeMock,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../DocumentViewer/viewerFixtures")).listenMock,
}));
vi.mock("../../logging", async () => (await import("../DocumentViewer/viewerFixtures")).loggingMock);
vi.mock("./pdfDocument", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./pdfDocument")>()),
  loadPdfjs: async () => (await import("./pdfFixtures")).pdfjsFake,
}));

beforeEach(() => {
  resetFixtures();
  resetPdfFixtures();
  resetViewStateForTest();
  pdfjsFake.getDocument.mockReset();
  // Page 1 holds two matches of "hello", page 2 one, page 3 none, page 4 one.
  state.pages = [
    page("Hello world", "say hello again"),
    page("Oh HELLO there", "nothing here"),
    page("no match on this page"),
    page("last: hello"),
  ];
  pdfReadReturns(pdfPayload());
});
afterEach(cleanup);

async function opened() {
  const view = render(<PdfViewer documentId={ID} />);
  await screen.findByRole("region", { name: "PDF page" });
  await waitFor(() => expect(screen.queryByText("Loading page…")).toBeNull());
  return view;
}

const search = () => screen.getByRole("searchbox", { name: "Search text" });
const field = () => screen.getByRole("textbox", { name: "Page number" });
const marks = () => Array.from(document.querySelectorAll<HTMLElement>("mark.pdf-match"));
const currentMarks = () => marks().filter((m) => m.dataset.current === "true");
const layerText = () => document.querySelector(".pdf-text-layer")!.textContent;
const count = () => document.querySelector(".pdf-search-count")!.textContent;

describe("PDV-FR-XMRL: text search", () => {
  it("PDV-FR-XMRL: a search ignores case, runs across every page, and shows the count in the form 1 of 4", async () => {
    const user = userEvent.setup();
    await opened();
    await user.type(search(), "HeLLo");
    expect(await screen.findByText("1 of 4")).toBeInTheDocument();
    expect(count()).toBe("1 of 4");
  });

  it("PDV-FR-XMRL: shows No matches when the text is not in the PDF", async () => {
    const user = userEvent.setup();
    await opened();
    await user.type(search(), "zebra");
    expect(await screen.findByText("No matches")).toBeInTheDocument();
    expect(marks()).toHaveLength(0);
    expect(screen.getByRole("button", { name: "Next match" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Previous match" })).toBeDisabled();
  });

  it("PDV-FR-XMRL: the count is a polite live region", async () => {
    await opened();
    const region = document.querySelector(".pdf-search-count")!;
    expect(region).toHaveAttribute("role", "status");
    expect(region).toHaveAttribute("aria-live", "polite");
  });

  it("PDV-FR-XMRL: spaces at the ends of the search text do not count", async () => {
    const user = userEvent.setup();
    await opened();
    await user.type(search(), "  hello  ");
    expect(await screen.findByText("1 of 4")).toBeInTheDocument();
  });

  it("PDV-FR-XMRL: a search reads the text of each page once and reuses it for later searches", async () => {
    const user = userEvent.setup();
    await opened();
    await user.type(search(), "hello");
    await screen.findByText("1 of 4");
    expect([...state.textContentReads].sort()).toEqual([1, 2, 3, 4]);
    const reads = state.textContentReads.length;
    await user.clear(search());
    await user.type(search(), "world");
    await screen.findByText(/of 1$/);
    await user.clear(search());
    await user.type(search(), "last");
    await screen.findByText(/of 1$/);
    expect(state.textContentReads.length).toBe(reads);
  });
});

describe("PDV-FR-BPXG: highlights and match navigation", () => {
  it("PDV-FR-BPXG: every match on the displayed page is highlighted, and the current one distinctly", async () => {
    const user = userEvent.setup();
    await opened();
    await user.type(search(), "hello");
    await screen.findByText("1 of 4");
    await waitFor(() => expect(marks()).toHaveLength(2));
    expect(marks().map((m) => m.textContent)).toEqual(["Hello", "hello"]);
    expect(currentMarks()).toHaveLength(1);
    expect(currentMarks()[0].textContent).toBe("Hello");
    // The current match is told apart by an attribute the style frames, not by
    // colour alone.
    const { sheet, blocksFor, decl } = await import("../../test/cssRules");
    const block = blocksFor(sheet("components.css"), '.pdf-match[data-current="true"]');
    expect(block).toHaveLength(1);
    expect(decl(block[0], "outline")).not.toBeNull();
  });

  it("PDV-FR-KDVB, PDV-FR-BPXG: a highlight never changes the text of the layer", async () => {
    const user = userEvent.setup();
    await opened();
    const before = layerText();
    await user.type(search(), "hello");
    await waitFor(() => expect(marks()).toHaveLength(2));
    expect(layerText()).toBe(before);
    await user.clear(search());
    await waitFor(() => expect(marks()).toHaveLength(0));
    expect(layerText()).toBe(before);
  });

  it("PDV-FR-BPXG: Next match and Previous match move the current match and wrap at both ends", async () => {
    const user = userEvent.setup();
    await opened();
    await user.type(search(), "hello");
    await screen.findByText("1 of 4");
    const next = screen.getByRole("button", { name: "Next match" });
    const previous = screen.getByRole("button", { name: "Previous match" });
    await user.click(next);
    expect(count()).toBe("2 of 4");
    expect(field()).toHaveValue("1");
    await waitFor(() => expect(currentMarks()[0]?.textContent).toBe("hello"));
    await user.click(next);
    expect(count()).toBe("3 of 4");
    expect(field()).toHaveValue("2");
    await user.click(next);
    expect(count()).toBe("4 of 4");
    expect(field()).toHaveValue("4");
    await user.click(next);
    expect(count()).toBe("1 of 4");
    expect(field()).toHaveValue("1");
    await user.click(previous);
    expect(count()).toBe("4 of 4");
    expect(field()).toHaveValue("4");
    await user.click(previous);
    expect(count()).toBe("3 of 4");
  });

  it("PDV-FR-BPXG: Enter and Shift+Enter in the search field move the current match", async () => {
    const user = userEvent.setup();
    await opened();
    await user.type(search(), "hello");
    await screen.findByText("1 of 4");
    await user.type(search(), "{Enter}");
    expect(count()).toBe("2 of 4");
    await user.type(search(), "{Enter}");
    expect(count()).toBe("3 of 4");
    await user.type(search(), "{Shift>}{Enter}{/Shift}");
    expect(count()).toBe("2 of 4");
    await user.type(search(), "{Shift>}{Enter}{/Shift}");
    await user.type(search(), "{Shift>}{Enter}{/Shift}");
    expect(count()).toBe("4 of 4");
  });
});

describe("PDV-FR-TIFB: moving between pages", () => {
  it("PDV-FR-TIFB: moving to a match on another page shows that page and highlights the match there", async () => {
    const user = userEvent.setup();
    await opened();
    await user.type(search(), "hello");
    await screen.findByText("1 of 4");
    await user.click(screen.getByRole("button", { name: "Next match" }));
    await user.click(screen.getByRole("button", { name: "Next match" }));
    expect(field()).toHaveValue("2");
    await waitFor(() => expect(layerText()).toBe("Oh HELLO therenothing here"));
    await waitFor(() => expect(currentMarks()).toHaveLength(1));
    expect(currentMarks()[0].textContent).toBe("HELLO");
    expect(state.renders[state.renders.length - 1].page).toBe(2);
  });

  it("PDV-FR-TIFB: the current match is scrolled into view", async () => {
    const user = userEvent.setup();
    const scrolled: HTMLElement[] = [];
    const original = HTMLElement.prototype.scrollIntoView;
    HTMLElement.prototype.scrollIntoView = function scroll(this: HTMLElement) {
      scrolled.push(this);
    };
    try {
      await opened();
      await user.type(search(), "hello");
      await screen.findByText("1 of 4");
      await waitFor(() => expect(scrolled.length).toBeGreaterThan(0));
      expect(scrolled[scrolled.length - 1].classList.contains("pdf-match")).toBe(true);
      const before = scrolled.length;
      await user.click(screen.getByRole("button", { name: "Next match" }));
      await waitFor(() => expect(scrolled.length).toBeGreaterThan(before));
    } finally {
      HTMLElement.prototype.scrollIntoView = original;
    }
  });

  it("PDV-FR-TIFB: emptying the search field removes every highlight and the count", async () => {
    const user = userEvent.setup();
    await opened();
    await user.type(search(), "hello");
    await screen.findByText("1 of 4");
    await waitFor(() => expect(marks().length).toBeGreaterThan(0));
    await user.clear(search());
    await waitFor(() => expect(marks()).toHaveLength(0));
    expect(count()).toBe("");
    expect(screen.getByRole("button", { name: "Next match" })).toBeDisabled();
  });

  it("PDV-FR-TIFB: a new search text starts at the first match at or after the current page", async () => {
    const user = userEvent.setup();
    await opened();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.click(screen.getByRole("button", { name: "Next page" }));
    expect(field()).toHaveValue("3");
    await user.type(search(), "hello");
    // Page 3 has no match, so the first at or after it is the one on page 4.
    expect(await screen.findByText("4 of 4")).toBeInTheDocument();
    await waitFor(() => expect(field()).toHaveValue("4"));
  });

  it("PDV-FR-TIFB: a search with matches only before the current page wraps to the first match", async () => {
    state.pages = [page("hello"), page("x"), page("y")];
    const user = userEvent.setup();
    await opened();
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.click(screen.getByRole("button", { name: "Next page" }));
    await user.type(search(), "hello");
    expect(await screen.findByText("1 of 1")).toBeInTheDocument();
    await waitFor(() => expect(field()).toHaveValue("1"));
  });
});

describe("PDV-FR-KDVB: the text layer is selectable", () => {
  it("PDV-FR-KDVB: the text layer holds the page text, and a selection of it equals that text", async () => {
    await opened();
    const layer = document.querySelector(".pdf-text-layer") as HTMLElement;
    expect(layer.textContent).toBe("Hello worldsay hello again");
    const range = document.createRange();
    range.selectNodeContents(layer);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    expect(selection.toString()).toBe("Hello worldsay hello again");
  });

  it("PDV-FR-KDVB: a selection across highlights equals the text without them", async () => {
    const user = userEvent.setup();
    await opened();
    await user.type(search(), "hello");
    await waitFor(() => expect(marks()).toHaveLength(2));
    const layer = document.querySelector(".pdf-text-layer") as HTMLElement;
    const range = document.createRange();
    range.selectNodeContents(layer);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    expect(selection.toString()).toBe("Hello worldsay hello again");
  });

  it("PDV-FR-KDVB: nothing in the viewer blocks the pointer or the selection", async () => {
    await opened();
    const region = screen.getByRole("region", { name: "PDF page" });
    for (const element of [region, ...region.querySelectorAll<HTMLElement>("*")]) {
      expect(element.style.userSelect).not.toBe("none");
      expect(element.style.pointerEvents).not.toBe("none");
    }
    const { sheet, blocksFor, decl } = await import("../../test/cssRules");
    const css = sheet("components.css");
    const textRule = blocksFor(css, ".pdf-text-layer span");
    expect(textRule).toHaveLength(1);
    expect(decl(textRule[0], "user-select")).toBe("text");
    for (const selector of [".pdf-text-layer", ".pdf-page-sheet", ".pdf-page-canvas", ".pdf-match"]) {
      for (const block of blocksFor(css, selector)) {
        expect(decl(block, "pointer-events"), selector).toBeNull();
        expect(decl(block, "user-select"), selector).not.toBe("none");
      }
    }
  });

  it("PDV-FR-KDVB: the copy command gives the selected text, and the viewer writes no clipboard data of its own", async () => {
    await opened();
    const layer = document.querySelector(".pdf-text-layer") as HTMLElement;
    const range = document.createRange();
    range.selectNodeContents(layer.firstElementChild!);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    let intercepted = false;
    document.addEventListener("copy", () => {
      intercepted = true;
    });
    const data = new Map<string, string>();
    const event = new Event("copy", { bubbles: true, cancelable: true });
    Object.defineProperty(event, "clipboardData", {
      value: { setData: (type: string, value: string) => data.set(type, value) },
    });
    layer.dispatchEvent(event);
    expect(intercepted).toBe(true);
    // The viewer sets nothing and cancels nothing, so the platform copies the
    // selection as it is.
    expect(data.size).toBe(0);
    expect(event.defaultPrevented).toBe(false);
    expect(selection.toString()).toBe("Hello world");
  });
});
