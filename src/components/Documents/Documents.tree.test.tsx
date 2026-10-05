// The tree of the Documents panel as a user meets it (`DPN-documents-panel.md`
// DPN-FR-NHDZ, DPN-FR-MICG, DPN-FR-UGVG, DPN-FR-TELU, DPN-FR-CDFO,
// DPN-FR-KEBH).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetDocumentsPanelStateForTest } from "../../state/documentsPanelState";
import { blocksFor, decl, sheet } from "../../test/cssRules";
import { DocumentsPanel } from ".";
import {
  doc,
  invokeMock,
  resetPanelFixtures,
  serve,
  snap,
  src,
} from "./panelFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("./panelFixtures")).invokeMock,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./panelFixtures")).listenMock,
}));
vi.mock("../../logging", async () => (await import("./panelFixtures")).loggingMock);

beforeEach(() => {
  resetPanelFixtures();
  resetDocumentsPanelStateForTest();
});
afterEach(cleanup);

const apiGuide = doc("/Users/me/reference/specs/api-guide.pdf");
const oldTxt = doc("/Users/me/reference/specs/Old.TXT", { status: "unavailable", revision: undefined });
const notes = doc("/Users/me/reference/notes.md");
const onboarding = doc("/work/docs/onboarding.markdown");

async function open(extra: Partial<Parameters<typeof DocumentsPanel>[0]> = {}) {
  serve({
    list: () =>
      snap(
        [src("folder", "/Users/me/reference"), src("folder", "/work/docs")],
        [apiGuide, oldTxt, notes, onboarding],
      ),
  });
  const props = { onOpenDocument: vi.fn(), onDocumentsRemoved: vi.fn(), ...extra };
  const view = render(<DocumentsPanel {...props} />);
  await screen.findByRole("tree", { name: "Documents" });
  return { ...view, props };
}

const tree = () => screen.getByRole("tree", { name: "Documents" });
const focusTree = () => act(async () => tree().focus());
const rowNames = () =>
  within(tree())
    .getAllByRole("treeitem")
    .map((r) => r.querySelector(".documents-row__name")!.textContent);

describe("DPN-FR-NHDZ: what the tree shows", () => {
  it("DPN-FR-NHDZ: shows the folders and the documents, folders first, with a chain of single folders joined", async () => {
    await open();
    // Two folder rows at the top: the joined chain, and /work/docs joined.
    expect(rowNames()).toEqual([
      "Users/me/reference",
      "specs",
      "api-guide.pdf",
      "Old.TXT",
      "notes.md",
      "work/docs",
      "onboarding.markdown",
    ]);
  });

  it("DPN-FR-NHDZ: shows each document path once, however many sources reached it", async () => {
    serve({
      list: () =>
        snap(
          [src("folder", "/Users/me/reference"), src("file", "/Users/me/reference/notes.md")],
          [notes],
        ),
    });
    render(<DocumentsPanel onOpenDocument={vi.fn()} onDocumentsRemoved={vi.fn()} />);
    await screen.findByRole("tree", { name: "Documents" });
    expect(screen.getAllByText("notes.md")).toHaveLength(1);
  });

  it("DPN-FR-NHDZ: renders only the rows of expanded folders, so a closed folder draws none of its documents", async () => {
    const user = userEvent.setup();
    await open();
    expect(rowNames()).toContain("api-guide.pdf");
    await user.click(screen.getByRole("treeitem", { name: /Users\/me\/reference/ }));
    expect(rowNames()).toEqual(["Users/me/reference", "work/docs", "onboarding.markdown"]);
  });

  it("DPN-FR-NHDZ: a collection of a few thousand documents mounts only the rows of the expanded folders", async () => {
    const many = Array.from({ length: 3000 }, (_, i) => doc(`/big/d${i % 30}/f${i}.md`));
    serve({ list: () => snap([src("folder", "/big")], many) });
    render(<DocumentsPanel onOpenDocument={vi.fn()} onDocumentsRemoved={vi.fn()} />);
    await screen.findByRole("tree", { name: "Documents" });
    // Folders start expanded, so every row is drawn here; the test confirms the
    // closed state draws none of them.
    // A role query calculates the accessible name of each of the 3031 rows,
    // which takes most of a second. A selector counts every mounted row, also
    // a hidden one, which is what "mounts" means. The timeout gives a loaded CI
    // runner a margin.
    const rowCount = () => tree().querySelectorAll('[role="treeitem"]').length;
    const user = userEvent.setup();
    expect(rowCount()).toBe(3031);
    await user.click(within(tree()).getByText("big").closest<HTMLElement>('[role="treeitem"]')!);
    expect(rowCount()).toBe(1);
  }, 15_000);
});

describe("DPN-FR-MICG: a document row", () => {
  it("DPN-FR-MICG: shows the file name in the case it has on disk and an icon for its format", async () => {
    await open();
    const pdf = screen.getByRole("treeitem", { name: /api-guide\.pdf/ });
    const text = screen.getByRole("treeitem", { name: /Old\.TXT/ });
    const markdown = screen.getByRole("treeitem", { name: /notes\.md/ });
    expect(text.querySelector(".documents-row__name")!.textContent).toBe("Old.TXT");
    const icon = (row: HTMLElement) => row.querySelector(".tree-row__icon svg")!.innerHTML;
    expect(icon(pdf)).not.toBe("");
    expect(new Set([icon(pdf), icon(text), icon(markdown)]).size).toBe(3);
    const { blocksFor: blocks, decl: d } = { blocksFor, decl };
    const rule = blocks(sheet("components.css"), ".documents-row__name");
    expect(rule.length).toBe(0);
    // A display treatment never case-transforms a name (SNV-FR-57).
    const css = sheet("components.css") + sheet("kit.css");
    for (const block of [...blocks(css, ".tree-row__name"), ...blocks(css, ".documents-row__name")]) {
      expect(d(block, "text-transform")).toBeNull();
    }
  });

  it("DPN-FR-MICG: shows the text Unavailable on an unavailable document, and only on that one", async () => {
    await open();
    const unavailable = screen.getByRole("treeitem", { name: /Old\.TXT/ });
    expect(within(unavailable).getByText("Unavailable")).toBeInTheDocument();
    expect(unavailable).toHaveAttribute("data-unavailable", "true");
    expect(screen.getAllByText("Unavailable")).toHaveLength(1);
  });

  it("DPN-FR-MICG: the tooltip of a document discloses its full path, and a folder row shows no path of its own", async () => {
    await open();
    expect(screen.getByRole("treeitem", { name: /api-guide\.pdf/ })).toHaveAttribute(
      "title",
      "/Users/me/reference/specs/api-guide.pdf",
    );
    expect(screen.getByRole("treeitem", { name: /^Users\/me\/reference/ })).not.toHaveAttribute("title");
    // A row never shows a path the collection did not report: the folder labels
    // are built from reported document paths only.
    const text = tree().textContent!;
    expect(text).not.toContain("/Users/me/reference/specs/api");
  });
});

describe("DPN-FR-UGVG: expanding and the keyboard", () => {
  it("DPN-FR-UGVG: a folder row expands and collapses on click and carries aria-expanded", async () => {
    const user = userEvent.setup();
    await open();
    const folder = screen.getByRole("treeitem", { name: /^specs/ });
    expect(folder).toHaveAttribute("aria-expanded", "true");
    await user.click(folder);
    expect(screen.getByRole("treeitem", { name: /^specs/ })).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("api-guide.pdf")).toBeNull();
    await user.click(screen.getByRole("treeitem", { name: /^specs/ }));
    expect(screen.getByRole("treeitem", { name: /^specs/ })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("api-guide.pdf")).toBeInTheDocument();
  });

  it("DPN-FR-UGVG: a document row carries no aria-expanded", async () => {
    await open();
    expect(screen.getByRole("treeitem", { name: /notes\.md/ })).not.toHaveAttribute("aria-expanded");
  });

  it("DPN-FR-UGVG: the tree exposes the tree roles with levels, and is one keyboard stop", async () => {
    const user = userEvent.setup();
    await open();
    expect(tree()).toHaveAttribute("role", "tree");
    const levels = within(tree())
      .getAllByRole("treeitem")
      .map((r) => r.getAttribute("aria-level"));
    expect(levels).toEqual(["1", "2", "3", "3", "2", "1", "2"]);
    for (const row of within(tree()).getAllByRole("treeitem")) {
      expect(row).not.toHaveAttribute("tabindex");
    }
    expect(tree()).toHaveAttribute("tabindex", "0");
    // Add documents, the filter, then the tree: one stop for all of its rows.
    await user.tab();
    await user.tab();
    await user.tab();
    expect(tree()).toHaveFocus();
  });

  it("DPN-FR-UGVG: the Up and Down arrow keys move between rows", async () => {
    const user = userEvent.setup();
    await open();
    await focusTree();
    expect(screen.getByRole("treeitem", { name: /^Users\/me\/reference/ })).toHaveAttribute("data-active", "true");
    await user.keyboard("{ArrowDown}");
    expect(tree().getAttribute("aria-activedescendant")).toBe(
      screen.getByRole("treeitem", { name: /^specs/ }).id,
    );
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(screen.getByRole("treeitem", { name: /Old\.TXT/ })).toHaveAttribute("data-active", "true");
    await user.keyboard("{ArrowUp}");
    expect(screen.getByRole("treeitem", { name: /api-guide\.pdf/ })).toHaveAttribute("data-active", "true");
    await user.keyboard("{End}");
    expect(screen.getByRole("treeitem", { name: /onboarding/ })).toHaveAttribute("data-active", "true");
    await user.keyboard("{Home}");
    expect(screen.getByRole("treeitem", { name: /^Users\/me\/reference/ })).toHaveAttribute("data-active", "true");
  });

  it("DPN-FR-UGVG: the Left and Right arrow keys close and open a folder and move between a folder and its rows", async () => {
    const user = userEvent.setup();
    await open();
    await focusTree();
    await user.keyboard("{ArrowDown}");
    const specs = () => screen.getByRole("treeitem", { name: /^specs/ });
    expect(specs()).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{ArrowLeft}");
    expect(specs()).toHaveAttribute("aria-expanded", "false");
    await user.keyboard("{ArrowRight}");
    expect(specs()).toHaveAttribute("aria-expanded", "true");
    // Right on an open folder moves into it.
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("treeitem", { name: /api-guide\.pdf/ })).toHaveAttribute("data-active", "true");
    // Left on a document moves out to its folder.
    await user.keyboard("{ArrowLeft}");
    expect(specs()).toHaveAttribute("data-active", "true");
  });

  it("DPN-FR-UGVG: Enter and Space open and close a folder", async () => {
    const user = userEvent.setup();
    await open();
    await focusTree();
    await user.keyboard("{ArrowDown}");
    const specs = () => screen.getByRole("treeitem", { name: /^specs/ });
    await user.keyboard("{Enter}");
    expect(specs()).toHaveAttribute("aria-expanded", "false");
    await user.keyboard(" ");
    expect(specs()).toHaveAttribute("aria-expanded", "true");
  });

  it("DPN-FR-UGVG, DPN-FR-AREM: the tree holds its keyboard position at the row, and a folder that was never toggled starts expanded", async () => {
    await open();
    for (const folder of screen.getAllByRole("treeitem").filter((r) => r.hasAttribute("aria-expanded"))) {
      expect(folder).toHaveAttribute("aria-expanded", "true");
    }
  });
});

describe("DPN-FR-TELU: the filter in the tree", () => {
  it("DPN-FR-TELU: narrows the tree to the rows whose name contains the text, ignoring case", async () => {
    const user = userEvent.setup();
    await open();
    await user.type(screen.getByRole("textbox", { name: "Filter documents" }), "NOTES");
    expect(rowNames()).toEqual(["Users/me/reference", "notes.md"]);
  });

  it("DPN-FR-TELU: a folder stays when a row below it matches, and then it shows expanded even if it was closed", async () => {
    const user = userEvent.setup();
    await open();
    await user.click(screen.getByRole("treeitem", { name: /^specs/ }));
    expect(screen.queryByText("api-guide.pdf")).toBeNull();
    await user.type(screen.getByRole("textbox", { name: "Filter documents" }), "api");
    expect(rowNames()).toEqual(["Users/me/reference", "specs", "api-guide.pdf"]);
    expect(screen.getByRole("treeitem", { name: /^specs/ })).toHaveAttribute("aria-expanded", "true");
    // Clearing the filter returns to the state the user left: specs is closed.
    await user.clear(screen.getByRole("textbox", { name: "Filter documents" }));
    expect(screen.getByRole("treeitem", { name: /^specs/ })).toHaveAttribute("aria-expanded", "false");
  });

  it("DPN-FR-TELU: a matching folder shows everything below it", async () => {
    const user = userEvent.setup();
    await open();
    await user.type(screen.getByRole("textbox", { name: "Filter documents" }), "specs");
    expect(rowNames()).toEqual(["Users/me/reference", "specs", "api-guide.pdf", "Old.TXT"]);
  });

  it("DPN-FR-TELU: a folder the user closes while the filter shows it open stays closed for that filter text", async () => {
    const user = userEvent.setup();
    await open();
    await user.type(screen.getByRole("textbox", { name: "Filter documents" }), "a");
    await user.click(screen.getByRole("treeitem", { name: /^specs/ }));
    expect(screen.getByRole("treeitem", { name: /^specs/ })).toHaveAttribute("aria-expanded", "false");
    await user.type(screen.getByRole("textbox", { name: "Filter documents" }), "p");
    expect(screen.getByRole("treeitem", { name: /^specs/ })).toHaveAttribute("aria-expanded", "true");
  });
});

describe("DPN-FR-CDFO: opening a document", () => {
  it("DPN-FR-CDFO: a click on a document row opens its viewer tab with the entry as the collection reported it", async () => {
    const user = userEvent.setup();
    const { props } = await open();
    await user.click(screen.getByRole("treeitem", { name: /notes\.md/ }));
    expect(props.onOpenDocument).toHaveBeenCalledTimes(1);
    expect(props.onOpenDocument).toHaveBeenCalledWith(notes);
    await user.click(screen.getByRole("treeitem", { name: /api-guide\.pdf/ }));
    expect(props.onOpenDocument).toHaveBeenLastCalledWith(apiGuide);
  });

  it("DPN-FR-CDFO: Enter on a document row opens it, and a folder row opens nothing", async () => {
    const user = userEvent.setup();
    const { props } = await open();
    await focusTree();
    // The first row is a folder: Enter opens or closes it and opens no tab.
    await user.keyboard("{Enter}");
    expect(props.onOpenDocument).not.toHaveBeenCalled();
    await user.keyboard("{Enter}");
    await user.keyboard("{ArrowDown}{ArrowDown}{Enter}");
    expect(props.onOpenDocument).toHaveBeenCalledTimes(1);
    expect(props.onOpenDocument).toHaveBeenCalledWith(apiGuide);
  });

  it("DPN-FR-CDFO: an unavailable document opens on the same terms", async () => {
    const user = userEvent.setup();
    const { props } = await open();
    await user.click(screen.getByRole("treeitem", { name: /Old\.TXT/ }));
    expect(props.onOpenDocument).toHaveBeenCalledWith(oldTxt);
  });

  it("DPN-FR-CDFO: opening a document calls no backend command of its own", async () => {
    const user = userEvent.setup();
    await open();
    await user.click(screen.getByRole("treeitem", { name: /notes\.md/ }));
    expect(invokeMock.mock.calls.map((c) => c[0])).toEqual(["list_documents"]);
  });
});

describe("DPN-FR-KEBH: keyboard, names, and theme", () => {
  it("DPN-FR-KEBH: every control of the panel has an accessible name", async () => {
    await open();
    expect(screen.getByRole("button", { name: "Add documents" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Filter documents" })).toBeInTheDocument();
    expect(screen.getByRole("tree", { name: "Documents" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove /Users/me/reference" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove /work/docs" })).toBeInTheDocument();
  });

  it("DPN-FR-KEBH: the tab order is Add documents, the filter, the tree, and then each Remove button", async () => {
    const user = userEvent.setup();
    await open();
    await user.tab();
    expect(screen.getByRole("button", { name: "Add documents" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("textbox", { name: "Filter documents" })).toHaveFocus();
    await user.tab();
    expect(tree()).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Remove /Users/me/reference" })).toHaveFocus();
    await user.tab();
    expect(screen.getByRole("button", { name: "Remove /work/docs" })).toHaveFocus();
  });

  it("DPN-FR-KEBH: the controls show a visible focus indicator and use the colours and type roles of the theme", async () => {
    const css = sheet("components.css") + "\n" + sheet("kit.css");
    for (const selector of [
      ".documents-tree:focus-visible",
      ".icon-btn:focus-visible",
      // The Add documents entries remove the outline, so the ring is their indicator.
      ".documents-menu .documents-menu__item:focus-visible",
    ]) {
      const block = blocksFor(css, selector);
      expect(block.length, selector).toBeGreaterThan(0);
      expect(decl(block[block.length - 1], "box-shadow"), selector).toBe("var(--shadow-focus)");
    }
    const { readFileSync } = await import("node:fs");
    const own = readFileSync("src/styles/components/documents.css", "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
    // No colour literal and no font size of its own in the panel's rules.
    expect(own).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(own).not.toMatch(/rgba?\(/);
    expect(own).not.toMatch(/font-size:\s*\d/);
  });
});
