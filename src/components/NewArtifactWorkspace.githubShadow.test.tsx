/**
 * A GitHub-shadow draft in the New Artifact tab
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-16, NAW-FR-UEWC,
 * NAW-FR-AXFQ, NAW-FR-BCHZ).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import {
  draft,
  makeStubs,
  openActions,
  renderWorkspace,
} from "../test/newArtifactFixtures";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { AUTOSAVE_DELAY_MS } from "../state/writeSchedule";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import type { DraftStatus, GithubIssueLink } from "../types";

const { stub } = makeStubs(invokeMock);
const calls = (name: string) => invokeMock.mock.calls.filter(([cmd]) => cmd === name);

const ISSUE: GithubIssueLink = {
  repositoryOwner: "acme",
  repositoryName: "platform",
  issueNumber: 9,
  issueUrl: "https://github.com/acme/platform/issues/9",
  projectNodeId: "P1",
  claimState: "claimed",
};

/**
 * The base stub, with the draft record answering as a GitHub-shadow draft and,
 * where `heldBy` names a run, that run holding it.
 */
function stubShadow(
  heldBy: string | null = null,
  status: DraftStatus = "github_shadow",
) {
  stub();
  const base = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "open_draft") return { ...draft(status), githubIssue: ISSUE };
    if (cmd === "get_draft_graduation")
      return heldBy ? { id: heldBy, state: "working", commits: [] } : null;
    if (cmd === "list_work_streams") return [];
    return base(cmd, args);
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
});
afterEach(cleanup);

describe("New Artifact workspace — a GitHub-shadow draft", () => {
  it("NAW-FR-UEWC / NAW-FR-16: the prompt is read-only and a banner says the draft mirrors a GitHub issue", async () => {
    stubShadow();
    renderWorkspace();
    const banner = await screen.findByTestId("draft-github-shadow-banner");
    expect(banner).toHaveTextContent(
      "This draft mirrors the GitHub issue acme/platform#9 and cannot be edited.",
    );
    // The surface takes no keystroke, so no write is scheduled.
    await waitFor(() =>
      expect(document.querySelector(".ProseMirror")).toHaveAttribute(
        "contenteditable",
        "false",
      ),
    );
    const toggle = await screen.findByRole("button", { name: "Edit as Markdown source" });
    fireEvent.click(toggle);
    const source = (await screen.findByLabelText("Markdown source")) as HTMLTextAreaElement;
    expect(source).toBeDisabled();
    await new Promise((r) => setTimeout(r, AUTOSAVE_DELAY_MS + 100));
    expect(calls("save_draft_file_contents")).toHaveLength(0);
  });

  it("NAW-FR-UEWC: the in-place rename is inert", async () => {
    stubShadow();
    renderWorkspace();
    await screen.findByTestId("draft-github-shadow-banner");
    const name = screen.getByRole("button", { name: /artifact-window/ });
    expect(name).toHaveAttribute("title", expect.stringMatching(/cannot be renamed/));
    await userEvent.click(name);
    expect(screen.queryByLabelText("Draft name")).toBeNull();
    expect(calls("rename_draft")).toHaveLength(0);
  });

  it("NAW-FR-AXFQ: Graduate is the only enabled action; the rest state why not", async () => {
    stubShadow();
    renderWorkspace();
    await screen.findByTestId("draft-github-shadow-banner");
    await openActions();
    expect(screen.getByRole("menuitem", { name: "Graduate" })).toBeEnabled();
    const discuss = screen.getByRole("menuitem", { name: /Discuss/ });
    expect(discuss).toBeDisabled();
    expect(discuss).toHaveAttribute("title", expect.stringMatching(/mirrors a GitHub issue/));
    const publish = screen.getByRole("menuitem", { name: /Publish to GitHub/ });
    expect(publish).toBeDisabled();
    expect(publish).toHaveAttribute("title", expect.stringMatching(/already mirrors a GitHub issue/));
    const archive = screen.getByRole("menuitem", { name: /Archive/ });
    expect(archive).toBeDisabled();
    expect(archive).toHaveAttribute("title", expect.stringMatching(/cannot be archived or restored/));
  });

  it("NAW-FR-AXFQ: Graduate follows NAW-FR-44 — a run holding the draft disables it", async () => {
    stubShadow("run-1");
    renderWorkspace();
    await screen.findByTestId("draft-github-shadow-banner");
    await openActions();
    await waitFor(() =>
      expect(screen.getByRole("menuitem", { name: "Graduate" })).toBeDisabled(),
    );
  });

  it("NAW-FR-AXFQ / GSD-FR-LXAF: Graduate opens the same start dialog with the same questions", async () => {
    stubShadow();
    renderWorkspace();
    await screen.findByTestId("draft-github-shadow-banner");
    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: "Graduate" }));
    expect(
      await screen.findByRole("dialog", { name: /Graduate .artifact-window./ }),
    ).toBeInTheDocument();
  });

  it("NAW-FR-BCHZ: the tag names the repository and issue number and opens the issue", async () => {
    stubShadow();
    renderWorkspace();
    const tag = await screen.findByTestId("draft-github-shadow-tag");
    expect(tag).toHaveTextContent("acme/platform#9");
    await userEvent.click(tag);
    await waitFor(() => expect(calls("open_publication_issue")).toHaveLength(1));
    expect(calls("open_publication_issue")[0][1]).toEqual({
      draftId: "d1",
      url: ISSUE.issueUrl,
    });
  });

  it("NAW-FR-UEWC: an ordinary draft carries no shadow banner or tag", async () => {
    stub();
    renderWorkspace();
    await openActions();
    expect(screen.queryByTestId("draft-github-shadow-banner")).toBeNull();
    expect(screen.queryByTestId("draft-github-shadow-tag")).toBeNull();
    expect(screen.getByRole("menuitem", { name: /Archive/ })).toBeEnabled();
  });

  it("NAW-FR-AXFQ: Restore is disabled on a shadow draft too", async () => {
    stubShadow(null, "archived");
    renderWorkspace();
    await screen.findByTestId("draft-github-shadow-banner");
    await openActions();
    const restore = screen.getByRole("menuitem", { name: /Restore/ });
    expect(restore).toBeDisabled();
    expect(restore).toHaveAttribute("title", expect.stringMatching(/cannot be archived or restored/));
  });

  it("NAW-FR-AXFQ: Graduate is disabled on a shadow draft that reports graduated", async () => {
    stubShadow(null, "graduated");
    renderWorkspace();
    await screen.findByTestId("draft-github-shadow-banner");
    await openActions();
    const graduate = screen.getByRole("menuitem", { name: "Graduate" });
    expect(graduate).toBeDisabled();
    expect(graduate).toHaveAttribute("title", "This draft has already been graduated");
  });
});
