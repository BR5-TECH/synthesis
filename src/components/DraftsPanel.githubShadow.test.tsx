/**
 * GitHub-shadow rows of the Drafts panel (`DRP-drafts-panel.md` DRP-FR-ZRJJ,
 * DRP-FR-YVXS, DRP-FR-NPZO, DRP-FR-YYZU, DRP-FR-10).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));

import { draftRow, flat, openDraftMenu, renderPanel } from "../test/draftsPanelFixtures";
import { resetPanelReveals } from "../state/panelReveal";
import type { DraftSummary, GithubIssueLink } from "../types";

const ISSUE: GithubIssueLink = {
  repositoryOwner: "acme",
  repositoryName: "platform",
  issueNumber: 9,
  issueUrl: "https://github.com/acme/platform/issues/9",
  projectNodeId: "P1",
  claimState: "claimed",
};

const shadow = (over: Partial<DraftSummary> = {}): DraftSummary => ({
  id: "gh-1",
  name: "cache-invalidation",
  status: "github_shadow",
  folder: "",
  updatedAt: "2026-09-30T10:00:00Z",
  githubIssue: ISSUE,
  ...over,
});
const plain: DraftSummary = {
  id: "d-plain",
  name: "plain-draft",
  status: "active",
  folder: "",
  updatedAt: "2026-09-29T10:00:00Z",
};

let drafts: DraftSummary[];
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetPanelReveals();
  drafts = [shadow(), plain];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) =>
    cmd === "list_drafts" ? flat(drafts) : undefined,
  );
});
afterEach(cleanup);

const menuItems = () =>
  within(screen.getByRole("menu"))
    .getAllByRole("menuitem")
    .map((m) => m.textContent);

describe("GitHub-shadow rows", () => {
  it("DRP-FR-ZRJJ: a shadow row carries a GitHub marker naming repository and issue, and reads GitHub task", async () => {
    renderPanel();
    const row = await waitFor(() => draftRow("cache-invalidation"));
    expect(within(row).getByTestId("draft-github-marker")).toHaveTextContent(
      "acme/platform#9",
    );
    expect(within(row).getByTestId("draft-github-task-marker")).toHaveTextContent(
      "GitHub task",
    );
    expect(within(draftRow("plain-draft")).queryByTestId("draft-github-marker")).toBeNull();
  });

  it("DRP-FR-ZRJJ: a shadow row that reports graduated reads Graduated", async () => {
    drafts = [shadow({ status: "graduated" })];
    renderPanel();
    const row = await waitFor(() => draftRow("cache-invalidation"));
    expect(within(row).getByTestId("draft-graduated-marker")).toHaveTextContent("Graduated");
    expect(within(row).queryByTestId("draft-github-task-marker")).toBeNull();
    expect(within(row).getByTestId("draft-github-marker")).toBeInTheDocument();
  });

  it("DRP-FR-YVXS: the active position admits github_shadow; graduated admits a graduated shadow", async () => {
    drafts = [shadow(), shadow({ id: "gh-2", name: "done-task", status: "graduated" }), plain];
    renderPanel({ filter: "active" });
    await waitFor(() => draftRow("cache-invalidation"));
    expect(draftRow("plain-draft")).toBeInTheDocument();
    expect(screen.queryByRole("treeitem", { name: /^Draft done-task/ })).toBeNull();
    cleanup();

    renderPanel({ filter: "graduated" });
    await waitFor(() => draftRow("done-task"));
    expect(screen.queryByRole("treeitem", { name: /^Draft cache-invalidation/ })).toBeNull();
  });

  it("DRP-FR-NPZO / DRP-FR-10: the menu offers Open, Information, Graduate…, Open issue on GitHub, and disables the rest with the reason", async () => {
    renderPanel();
    await waitFor(() => draftRow("cache-invalidation"));
    await act(async () => void openDraftMenu("cache-invalidation"));
    expect(menuItems()).toEqual([
      "Open",
      "Information",
      "Graduate…",
      "Open issue on GitHub",
      "Rename…",
      "Move to Folder…",
      "Archive",
      "Delete",
    ]);
    for (const name of ["Rename…", "Move to Folder…", "Archive", "Delete"]) {
      const item = screen.getByRole("menuitem", { name });
      expect(item).toHaveAttribute("aria-disabled", "true");
      expect(item).toHaveAttribute("title", expect.stringMatching(/mirrors a GitHub issue/));
    }
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(screen.queryByRole("dialog", { name: "Delete draft" })).toBeNull();
    expect(screen.getByRole("menuitem", { name: "Graduate…" })).not.toHaveAttribute(
      "aria-disabled",
    );
  });

  it("DRP-FR-10: an ordinary row keeps its six entries", async () => {
    renderPanel();
    await waitFor(() => draftRow("plain-draft"));
    await act(async () => void openDraftMenu("plain-draft"));
    expect(menuItems()).toEqual([
      "Open",
      "Information",
      "Rename…",
      "Move to Folder…",
      "Archive",
      "Delete",
    ]);
  });

  it("DRP-FR-NPZO: a shadow row cannot be dragged", async () => {
    renderPanel();
    const row = await waitFor(() => draftRow("cache-invalidation"));
    expect(row).toHaveAttribute("draggable", "false");
    expect(draftRow("plain-draft")).toHaveAttribute("draggable", "true");
    fireEvent.dragStart(row);
    expect(row.className).not.toContain("dragging");
  });

  it("DRP-FR-YYZU: Graduate… opens the start dialog for that draft and invokes nothing itself", async () => {
    const { onGraduateDraft } = renderPanel();
    await waitFor(() => draftRow("cache-invalidation"));
    await act(async () => void openDraftMenu("cache-invalidation"));
    const before = invokeMock.mock.calls.length;
    await userEvent.click(screen.getByRole("menuitem", { name: "Graduate…" }));
    expect(onGraduateDraft).toHaveBeenCalledWith("gh-1", "cache-invalidation");
    expect(invokeMock.mock.calls.slice(before).map((c) => c[0])).not.toContain(
      "start_graduation",
    );
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("DRP-FR-YYZU: Graduate… is disabled while a run holds the draft or once it is graduated", async () => {
    drafts = [
      shadow({
        graduation: {
          runId: "r1",
          state: "working",
          locked: true,
          graduated: false,
        },
      }),
      shadow({ id: "gh-2", name: "done-task", status: "graduated" }),
    ];
    const { onGraduateDraft } = renderPanel();
    await waitFor(() => draftRow("cache-invalidation"));
    await act(async () => void openDraftMenu("cache-invalidation"));
    const held = screen.getByRole("menuitem", { name: "Graduate…" });
    expect(held).toHaveAttribute("aria-disabled", "true");
    expect(held).toHaveAttribute("title", "A graduation run already holds this draft");
    await userEvent.click(held);
    expect(onGraduateDraft).not.toHaveBeenCalled();

    await act(async () => void openDraftMenu("done-task"));
    expect(screen.getByRole("menuitem", { name: "Graduate…" })).toHaveAttribute(
      "title",
      "This draft has already been graduated",
    );
  });

  it("DRP-FR-YYZU: Open issue on GitHub invokes open publication issue with the draft's issue URL", async () => {
    renderPanel();
    await waitFor(() => draftRow("cache-invalidation"));
    await act(async () => void openDraftMenu("cache-invalidation"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Open issue on GitHub" }));
    await waitFor(() => expect(calls("open_publication_issue")).toHaveLength(1));
    expect(calls("open_publication_issue")[0][1]).toEqual({
      draftId: "gh-1",
      url: ISSUE.issueUrl,
    });
  });
});
