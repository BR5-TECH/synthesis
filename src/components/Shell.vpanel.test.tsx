// `VPanel` routing a selected surface to its component (SNV-FR-04), with the
// Comments surface among them (`CMP-comments-panel.md` CMP-FR-01 / CMP-FR-02).
//
// The activity bar's toggles are covered in `Shell.activityBar.test.tsx`; what
// is exercised here is the other half — that choosing a surface actually mounts
// it, and that mounting Comments issues the project-wide read.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";

import { VPanel } from "./Shell";
import type { PanelSurface } from "../types";
import {
  selectorNames,
  selectorValue,
  selectorValues,
} from "../test/selectors";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

function renderPanel(surface: PanelSurface) {
  render(
    <VPanel
      surface={surface}
      onOpenArtifact={vi.fn()}
      onShowNotes={vi.fn()}
      onRevealArtifact={vi.fn()}
      onOpenRun={vi.fn()}
      onNewFile={vi.fn()}
      onNewArtifact={vi.fn()}
      onNewFolder={vi.fn()}
      onOpenDraft={vi.fn()}
      onCreateDraft={vi.fn()}
      onDraftDeleted={vi.fn()}
      onDraftRenamed={vi.fn()}
      onDraftChanged={vi.fn()}
      draftsRevision={0}
      panelReveal={null}
      activeEntity={null}
      onRevealDiscussion={vi.fn()}
      onOpenDiff={vi.fn()}
    />,
  );
}

const commands = () => invokeMock.mock.calls.map((c) => c[0]);

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue([]);
});
afterEach(cleanup);

describe("CMP-FR-01 / CMP-FR-02: the Comments surface", () => {
  it("mounts the Comments panel and issues the project-wide read", async () => {
    renderPanel("comments");
    expect(screen.getByText("Comments")).toBeInTheDocument();
    expect(screen.getByLabelText("Filter comments")).toBeInTheDocument();
    await waitFor(() =>
      expect(commands()).toContain("list_all_discussions"),
    );
  });

  it("is not what any other surface renders", async () => {
    // Guards the routing itself: a fallthrough that rendered Comments for an
    // unrelated surface, or Notes for "comments", would fail here.
    for (const surface of ["notes", "changes", "library"] as const) {
      cleanup();
      invokeMock.mockClear();
      renderPanel(surface);
      expect(screen.queryByLabelText("Filter comments")).not.toBeInTheDocument();
      await waitFor(() => expect(invokeMock).toHaveBeenCalled());
      expect(commands()).not.toContain("list_all_discussions");
    }
  });
});

describe("DRP-FR-07: the Drafts surface opens in the active position", () => {
  // The default is set here rather than in `DraftsPanel`, whose own tests pass
  // a position explicitly and so can never see it. Active is the default
  // because a retired draft is not what the author came to the panel for — and
  // it is the Given every archive scenario in DRP is written against.
  it("mounts with the status filter on Active, offering the four positions", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_drafts")
        return {
          folders: [],
          drafts: [
            {
              id: "d1",
              name: "artifact-window",
              status: "active",
              folder: "",
              fileCount: 1,
              updatedAt: "2026-07-31T08:00:00Z",
            },
          ],
        };
      // PSS-FR-20: nothing persisted for this worktree, so the panel mounts on
      // its defaults — which is what makes `active` observable here.
      if (cmd === "load_drafts_panel_state")
        return { statusFilter: "active", textFilter: "", expandedFolders: [] };
      return [];
    });
    renderPanel("drafts");
    await screen.findByText("artifact-window");

    // DRP-FR-07 / SNV-FR-62: three toggle buttons acting as one radio group,
    // active leading and marked, with no dropdown to open.
    expect(selectorValue("Draft status")).toBe("active");
    expect(selectorValues("Draft status")).toEqual([
      "active",
      "archived",
      // DRP-FR-07: `graduated` is a position of its own, not a kind of archive.
      "graduated",
      "all",
    ]);
    expect(selectorNames("Draft status")).toEqual([
      "Active",
      "Archived",
      // DRP-FR-07: the position admits `graduated` and `published` alike, and
      // its title says so.
      "Graduated and published",
      "All drafts",
    ]);
    expect(
      screen.queryByRole("combobox", { name: "Draft status" }),
    ).not.toBeInTheDocument();
  });
});
