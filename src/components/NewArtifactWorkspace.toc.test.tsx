/**
 * The table of contents control of the New Artifact tab, and the formatting
 * toolbar its live prompt shares with an Editor tab
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-TFFX, NAW-FR-11).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import {
  makeStubs,
  renderWorkspace,
  showRail,
  threeVersions,
  versionRows,
} from "../test/newArtifactFixtures";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetToasts } from "../state/toasts";
import { resetLogBufferForTest } from "../logging";
import { resetMermaidMock } from "../test/mermaidMock";
import { resetMermaidForTest } from "./markdownMermaidRender";

const { stub } = makeStubs(invokeMock);

const PROMPT_BODY = "# Artifact window\n\nIntro.\n\n## What it must cover\n\nThe list.\n";

/** Serve the live prompt with headings; every other command goes to the stub. */
function withPromptBody(body = PROMPT_BODY) {
  const base = invokeMock.getMockImplementation();
  invokeMock.mockImplementation((cmd: string, args: unknown) =>
    cmd === "load_draft_file_contents"
      ? Promise.resolve({ body, checksum: "c1" })
      : base?.(cmd, args),
  );
}

/** jsdom has no `scrollIntoView`; the restore removes the spy again. */
const originalScrollIntoView = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  "scrollIntoView",
);
const tocButtons = () => screen.queryAllByRole("button", { name: "Table of contents" });

/**
 * NAW-FR-TFFX: the one control stands in the live prompt's action cluster, and
 * its wrapper is the element directly before the mode toggle — so no control,
 * the discussion-column control included, stands between them.
 */
function expectPairedWithToggle(toggleName: string) {
  expect(tocButtons()).toHaveLength(1);
  const toc = tocButtons()[0];
  const toggle = screen.getByRole("button", { name: toggleName });
  const cluster = toc.closest(".editor__actions");
  expect(cluster).not.toBeNull();
  expect(toggle.closest(".editor__actions")).toBe(cluster);
  expect(toc.closest(".toc")?.nextElementSibling).toBe(toggle);
  expect(toc.closest(".draft-workspace__chrome")).toBeNull();
  expect(cluster!.contains(screen.getByTestId("draft-discussion-toggle"))).toBe(false);
}

beforeEach(() => {
  invokeMock.mockReset();
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetDraftDiscussions();
  resetToasts();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: vi.fn(),
  });
});

afterEach(() => {
  cleanup();
  if (originalScrollIntoView) {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", originalScrollIntoView);
  } else {
    delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollIntoView;
  }
});

describe("the table of contents of a draft (NAW-FR-TFFX)", () => {
  it("NAW-FR-TFFX, EDT-FR-LGQA: one control, in the live prompt's action cluster, directly before its mode toggle", async () => {
    stub();
    withPromptBody();
    renderWorkspace();
    await screen.findByRole("heading", { name: "Artifact window" });

    await waitFor(() => expect(tocButtons()).toHaveLength(1));
    expectPairedWithToggle("Edit as Markdown source");
  });

  it("NAW-FR-TFFX, EDT-FR-LGQA: in raw-text mode the control still stands directly before the toggle", async () => {
    stub();
    withPromptBody();
    renderWorkspace();
    await screen.findByRole("heading", { name: "Artifact window" });
    await waitFor(() => expect(tocButtons()).toHaveLength(1));

    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByRole("button", { name: "Edit as rich text" });
    expectPairedWithToggle("Edit as rich text");
  });

  it("NAW-FR-TFFX, EDT-FR-PNGU, EDT-FR-YSJX: the panel lists the headings of the live prompt and moves the caret to one", async () => {
    stub();
    withPromptBody();
    renderWorkspace();
    await screen.findByRole("heading", { name: "Artifact window" });
    await waitFor(() => expect(tocButtons()).toHaveLength(1));

    fireEvent.click(tocButtons()[0]);
    const list = screen.getByRole("listbox", { name: "Table of contents" });
    const items = within(list).getAllByRole("option");
    expect(items.map((i) => i.textContent)).toEqual(["Artifact window", "What it must cover"]);
    fireEvent.click(items[1]);

    expect(screen.queryByRole("listbox")).toBeNull();
    const pm = document.querySelector(".ProseMirror") as HTMLElement;
    await waitFor(() => expect(pm.contains(document.activeElement)).toBe(true));
    const selection = document.getSelection();
    expect(selection?.anchorNode?.textContent).toBe("What it must cover");
  });

  it("NAW-FR-TFFX: a past version that shows has no control, and the return to the live prompt brings it back", async () => {
    stub({ history: threeVersions() });
    withPromptBody();
    renderWorkspace();
    await screen.findByRole("heading", { name: "Artifact window" });
    await waitFor(() => expect(tocButtons()).toHaveLength(1));

    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");
    expect(tocButtons()).toHaveLength(0);

    await userEvent.click(screen.getByRole("button", { name: "← Back to live prompt" }));
    await waitFor(() => expect(tocButtons()).toHaveLength(1));
    expectPairedWithToggle("Edit as Markdown source");
  });

  it("NAW-FR-11, EDT-FR-NTHC, EDT-FR-VUGQ: the live prompt has the same toolbar, with H4 and Insert diagram", async () => {
    stub();
    withPromptBody();
    renderWorkspace();
    await screen.findByRole("heading", { name: "Artifact window" });
    for (const n of [1, 2, 3, 4]) {
      expect(screen.getByRole("button", { name: `Heading ${n}` })).toBeInTheDocument();
    }
    expect(screen.getByRole("button", { name: "Insert diagram" })).not.toBeDisabled();
  });
});
