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
  it("NAW-FR-TFFX, EDT-FR-LGQA: one control, in the chrome row, directly before the discussion control", async () => {
    stub();
    withPromptBody();
    renderWorkspace();
    await screen.findByRole("heading", { name: "Artifact window" });

    await waitFor(() => expect(tocButtons()).toHaveLength(1));
    const toc = tocButtons()[0];
    const chrome = toc.closest(".draft-workspace__chrome") as HTMLElement;
    expect(chrome).not.toBeNull();
    const discussion = screen.getByTestId("draft-discussion-toggle");
    const buttons: HTMLElement[] = Array.from(chrome.querySelectorAll("button"));
    expect(buttons.indexOf(discussion)).toBe(buttons.indexOf(toc) + 1);
    // The embedded Editor's own cluster carries none.
    expect(document.querySelector(".editor__actions .toc__trigger")).toBeNull();
  });

  it("NAW-FR-TFFX, EDT-FR-PNGU, EDT-FR-YSJX: the panel lists the headings of the live prompt and moves the caret to one", async () => {
    stub();
    withPromptBody();
    renderWorkspace();
    await screen.findByRole("heading", { name: "Artifact window" });
    await waitFor(() => expect(tocButtons()).toHaveLength(1));

    fireEvent.click(tocButtons()[0]);
    const tree = screen.getByRole("tree", { name: "Table of contents" });
    const items = within(tree).getAllByRole("treeitem");
    expect(items.map((i) => i.textContent)).toEqual(["Artifact window", "What it must cover"]);
    fireEvent.click(items[1]);

    expect(screen.queryByRole("tree")).toBeNull();
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
