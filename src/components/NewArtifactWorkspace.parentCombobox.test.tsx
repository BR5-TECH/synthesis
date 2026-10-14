/**
 * The parent combobox of the publication chooser: the sub-issue option, the
 * filter, the bounded overlay, and the keyboard model
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-YLKD, NAW-FR-ASYN,
 * NAW-FR-XTGP, NAW-FR-YELA, NAW-FR-RBTE, NAW-FR-VUCK, NAW-FR-VJRW,
 * NAW-FR-PDXJ).
 *
 * Driven through the real tab, like its sibling
 * `NewArtifactWorkspace.publicationChooser`.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import {
  chooseParent,
  loadedList,
  makeStubs,
  openActions,
  publicationChooser,
  publicationMetadata,
  renderWorkspace,
} from "../test/newArtifactFixtures";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { matchesParent } from "./PublicationChooser";
import { menuPlacement } from "./FilterableSelect";
import type { PublicationParentIssue } from "../types";

const { stub } = makeStubs(invokeMock);

const calls = (name: string) =>
  invokeMock.mock.calls.filter(([cmd]) => cmd === name);

function parent(number: number, title: string): PublicationParentIssue {
  return {
    number,
    title,
    url: `https://github.com/acme/widgets/issues/${number}`,
    issueType: "Feature",
    milestone: null,
  };
}

/** Many parents, so a list of rows would overflow the window. */
const MANY = Array.from({ length: 60 }, (_, i) => parent(500 + i, `Parent ${i}`));

async function openChooser() {
  renderWorkspace();
  const menu = await openActions();
  await userEvent.click(within(menu).getByRole("menuitem", { name: /Publish to GitHub/ }));
  const dialog = await publicationChooser();
  // The read has landed once the combobox stands in place of its notice.
  await within(dialog).findByRole("combobox", { name: "Parent issue" });
  return dialog;
}

const combobox = (dialog: HTMLElement) =>
  within(dialog).getByRole("combobox", { name: "Parent issue" });
const filter = (dialog: HTMLElement) =>
  within(dialog).getByRole("combobox", { name: "Filter parent issue" });
const shownNumbers = (dialog: HTMLElement) =>
  within(within(dialog).getByRole("listbox", { name: "Parent issue" }))
    .queryAllByRole("option")
    .map((o) => o.textContent?.match(/^#(\d+)/)?.[1]);

beforeEach(() => {
  invokeMock.mockReset();
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  stub();
});

afterEach(cleanup);

describe("Publication chooser — the parent combobox", () => {
  it("NAW-FR-YLKD, NAW-FR-XTGP: many parents are one combobox, and the chooser holds no row for each", async () => {
    stub({ metadata: publicationMetadata({ parents: loadedList(MANY) }) });
    const dialog = await openChooser();

    expect(within(dialog).getAllByRole("radio")).toHaveLength(2);
    expect(dialog).not.toHaveTextContent("Parent 59");
    await userEvent.click(combobox(dialog));
    expect(shownNumbers(dialog)).toHaveLength(60);
    // The list is an overlay fixed to the window, with its own scroll.
    const menu = within(dialog).getByTestId("publication-parent-menu");
    expect(menu).toHaveClass("fsel__menu--fit");
    expect(menu.dataset.placement).toBeDefined();
  });

  it("NAW-FR-TCQB, NAW-FR-ASYN, NAW-FR-RBTE: choosing an issue selects the sub-issue option; the root option then sends no parent", async () => {
    const dialog = await openChooser();
    const sub = within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" });

    // NAW-FR-TCQB / NAW-FR-RBTE: the sub-issue option is selected when the
    // parents load, and with no parent chosen Publish is disabled.
    expect(sub).toBeChecked();
    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeDisabled();

    await chooseParent(dialog, 412);
    expect(sub).toBeChecked();
    expect(combobox(dialog)).toHaveTextContent("#412 Window chrome");
    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeEnabled();

    await userEvent.click(within(dialog).getByRole("radio", { name: /Publish as a root issue/ }));
    expect(combobox(dialog)).toHaveTextContent("#412 Window chrome");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: null },
    });
  });

  it("NAW-FR-ASYN, NAW-FR-RBTE: a re-pick of the kept parent selects the sub-issue option again", async () => {
    const dialog = await openChooser();
    const sub = within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" });
    const root = within(dialog).getByRole("radio", { name: /Publish as a root issue/ });

    await chooseParent(dialog, 412);
    expect(sub).toBeChecked();
    await userEvent.click(root);
    expect(root).toBeChecked();
    expect(combobox(dialog)).toHaveTextContent("#412 Window chrome");

    await chooseParent(dialog, 412);
    expect(sub).toBeChecked();
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: 412 },
    });
  });

  it("NAW-FR-YELA: the filter matches the number with or without # and the title, case-insensitively, and states no match", async () => {
    const dialog = await openChooser();
    await userEvent.click(combobox(dialog));

    await userEvent.type(filter(dialog), "41");
    expect(shownNumbers(dialog)).toEqual(["412"]);
    await userEvent.clear(filter(dialog));
    await userEvent.type(filter(dialog), "#398");
    expect(shownNumbers(dialog)).toEqual(["398"]);
    await userEvent.clear(filter(dialog));
    await userEvent.type(filter(dialog), "WINDOW");
    expect(shownNumbers(dialog)).toEqual(["412"]);
    // The Type and the milestone are not matched.
    await userEvent.clear(filter(dialog));
    await userEvent.type(filter(dialog), "feature");
    expect(shownNumbers(dialog)).toEqual([]);
    expect(within(dialog).getByTestId("publication-parent-empty")).toHaveTextContent(
      "No matching issue",
    );
  });

  it("NAW-FR-VJRW, NAW-FR-VUCK: arrows and Enter choose, focus returns to the combobox, and Escape closes the dropdown only", async () => {
    const dialog = await openChooser();
    combobox(dialog).focus();
    await userEvent.keyboard("{Enter}");
    expect(filter(dialog)).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}{Enter}");
    expect(combobox(dialog)).toHaveTextContent("#398 Release pipeline");
    expect(combobox(dialog)).toHaveFocus();
    expect(within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" })).toBeChecked();

    await userEvent.keyboard("{Enter}");
    expect(filter(dialog)).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "Publish to GitHub" })).toBe(dialog);
    expect(within(dialog).queryByRole("listbox")).toBeNull();
    expect(combobox(dialog)).toHaveFocus();
    expect(combobox(dialog)).toHaveTextContent("#398 Release pipeline");

    // With the dropdown closed, Escape dismisses the chooser.
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Publish to GitHub" })).toBeNull());
  });

  it("NAW-FR-PDXJ: a polite status announces the filter text and the count of matches; the combobox exposes its state", async () => {
    const dialog = await openChooser();
    expect(combobox(dialog)).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(combobox(dialog));
    expect(combobox(dialog)).toHaveAttribute("aria-expanded", "true");
    expect(combobox(dialog)).toHaveAttribute("aria-controls", "publication-parent-listbox");
    const status = within(dialog).getByTestId("publication-parent-count");
    expect(status).toHaveAttribute("role", "status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveTextContent("2 issues available");

    await userEvent.type(filter(dialog), "chrome");
    expect(status).toHaveTextContent("1 issue match “chrome”");
    expect(filter(dialog)).toHaveAttribute(
      "aria-activedescendant",
      "publication-parent-listbox-opt-0",
    );
  });
});

describe("NAW-FR-YELA: the parent filter rule", () => {
  const parents = [parent(412, "Window chrome"), parent(98, "Release 412 notes")];
  const match = matchesParent(parents);
  const option = (n: number) => ({ id: String(n), label: `#${n}` });

  it("NAW-FR-YELA: matches a number with or without # and a title substring", () => {
    expect(match(option(412), "#41")).toBe(true);
    expect(match(option(98), "#41")).toBe(false);
    expect(match(option(98), "412")).toBe(true);
    expect(match(option(412), "CHROME")).toBe(true);
    expect(match(option(412), "  ")).toBe(true);
    expect(match(option(412), "feature")).toBe(false);
  });
});

describe("NAW-FR-XTGP: the overlay placement rule", () => {
  const viewport = { width: 1280, height: 800 };

  it("NAW-FR-XTGP: opens below where its full height fits", () => {
    const p = menuPlacement({ top: 100, bottom: 130, left: 200, width: 300 }, viewport, 320);
    expect(p.above).toBe(false);
    expect(p.edge).toBe(134);
    expect(p.maxHeight).toBe(320);
  });

  it("NAW-FR-XTGP: flips above where below has less room than above", () => {
    const p = menuPlacement({ top: 700, bottom: 730, left: 200, width: 300 }, viewport, 320);
    expect(p.above).toBe(true);
    expect(p.edge).toBe(800 - 700 + 4);
    expect(p.maxHeight).toBe(320);
  });

  it("NAW-FR-XTGP: shrinks to the room on its side and never leaves the window", () => {
    const small = { width: 400, height: 300 };
    const p = menuPlacement({ top: 120, bottom: 150, left: 350, width: 300 }, small, 500);
    expect(p.maxHeight).toBeLessThan(500);
    expect(p.maxHeight).toBe(300 - 150 - 4 - 8);
    expect(p.left + p.width).toBeLessThanOrEqual(small.width - 8);
    expect(p.left).toBeGreaterThanOrEqual(8);
  });
});
