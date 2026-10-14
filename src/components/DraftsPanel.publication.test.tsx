/**
 * The publication marker of a Drafts panel row, and the statuses each filter
 * position admits now that publication changes no status
 * (`DRP-drafts-panel.md` DRP-FR-07, DRP-FR-08, DRP-FR-TQUB, DRP-FR-MZEA).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));

import { draftRow, flat, renderPanel } from "../test/draftsPanelFixtures";
import { resetPanelReveals } from "../state/panelReveal";
import type { DraftListPublication, DraftStatus, DraftSummary } from "../types";

/** Four hours and one minute ago, so the relative text is `4h ago`. */
const FOUR_HOURS_AGO = () => new Date(Date.now() - (4 * 60 + 1) * 60_000).toISOString();

const publication = (issueNumber: number): DraftListPublication => ({
  repositoryHost: "github.com",
  repositoryOwner: "acme",
  repositoryName: "platform",
  issueNumber,
  issueUrl: `https://github.com/acme/platform/issues/${issueNumber}`,
  publishedAt: FOUR_HOURS_AGO(),
});

const row = (
  id: string,
  status: DraftStatus,
  over: Partial<DraftSummary> = {},
): DraftSummary => ({
  id,
  name: `${status}-draft`,
  status,
  folder: "",
  updatedAt: "2026-07-31T08:00:00Z",
  ...over,
});

let drafts: DraftSummary[];
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetPanelReveals();
  drafts = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_drafts") return flat(drafts);
    if (cmd === "open_publication_issue" || cmd === "append_log_records") return undefined;
    if (cmd === "search_drafts") return [];
    throw new Error(`unexpected command: ${cmd}`);
  });
});
afterEach(cleanup);

describe("Publication marker (DRP-FR-TQUB, DRP-FR-MZEA)", () => {
  it("DRP-FR-TQUB: a row with a publication record shows the marker on every status", async () => {
    drafts = [
      row("d-active", "active", { publication: publication(1) }),
      row("d-archived", "archived", { publication: publication(2) }),
      row("d-graduated", "graduated", { publication: publication(3) }),
      row("d-published", "published", { publication: publication(4) }),
      row("d-plain", "active", { name: "plain-draft" }),
    ];
    renderPanel({ filter: "all" });
    await waitFor(() => draftRow("active-draft"));

    for (const name of ["active-draft", "archived-draft", "graduated-draft", "published-draft"]) {
      const marker = within(draftRow(name)).getByTestId("draft-publication-marker");
      expect(marker, name).toHaveTextContent("⤴");
      expect(marker, name).toHaveAttribute("title", "Published · 4h ago");
      expect(marker, name).toHaveAccessibleName("Published · 4h ago");
    }
    // A row with no record shows no marker.
    expect(within(draftRow("plain-draft")).queryByTestId("draft-publication-marker")).toBeNull();
  });

  it("DRP-FR-TQUB: a legacy published row with no record keeps its label and shows no marker", async () => {
    drafts = [row("d-published", "published")];
    renderPanel({ filter: "all" });
    const published = await waitFor(() => draftRow("published-draft"));
    expect(within(published).getByTestId("draft-published-marker")).toHaveTextContent("Published");
    expect(within(published).queryByTestId("draft-publication-marker")).toBeNull();
  });

  it("DRP-FR-MZEA: activating the marker opens the newest record's issue and not the draft", async () => {
    drafts = [row("d-active", "active", { publication: publication(7) })];
    const { onOpenDraft } = renderPanel({ filter: "all" });
    await waitFor(() => draftRow("active-draft"));

    await userEvent.click(screen.getByTestId("draft-publication-marker"));
    await waitFor(() => expect(calls("open_publication_issue")).toHaveLength(1));
    expect(calls("open_publication_issue")[0][1]).toEqual({
      draftId: "d-active",
      url: "https://github.com/acme/platform/issues/7",
    });
    expect(onOpenDraft).not.toHaveBeenCalled();

    // The keyboard reaches the same route, and Enter on the marker does not
    // open the draft either.
    screen.getByTestId("draft-publication-marker").focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(calls("open_publication_issue")).toHaveLength(2));
    expect(onOpenDraft).not.toHaveBeenCalled();
  });

  it("DRP-FR-MZEA: a refused open is reported on the row", async () => {
    drafts = [row("d-active", "active", { publication: publication(7) })];
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_drafts") return flat(drafts);
      if (cmd === "append_log_records") return undefined;
      if (cmd === "open_publication_issue") throw new Error("draft_not_found: gone");
      throw new Error(`unexpected command: ${cmd}`);
    });
    renderPanel({ filter: "all" });
    await waitFor(() => draftRow("active-draft"));
    await userEvent.click(screen.getByTestId("draft-publication-marker"));
    expect(await within(draftRow("active-draft")).findByRole("alert")).toBeInTheDocument();
  });

  it("DRP-FR-MZEA: the marker leaves the second line as it was", async () => {
    drafts = [row("d-active", "active", { publication: publication(7) })];
    renderPanel({ filter: "all" });
    const active = await waitFor(() => draftRow("active-draft"));
    const meta = active.querySelector(".draft-row__meta");
    expect(meta).not.toBeNull();
    // The timestamp of the draft's last activity, and no publication text.
    expect(within(meta as HTMLElement).getByTitle("2026-07-31T08:00:00Z")).toBeInTheDocument();
    expect(meta).not.toHaveTextContent(/Published/);
  });
});

describe("Filter positions with a legacy published draft (DRP-FR-07, DRP-FR-08)", () => {
  const mixed = () => [
    row("d-grad", "graduated", { publication: publication(1) }),
    row("d-pub", "published"),
    row("d-active", "active", { publication: publication(2) }),
    row("d-arch", "archived"),
  ];

  it("DRP-FR-07: the graduated position admits graduated drafts alone", async () => {
    drafts = mixed();
    renderPanel({ filter: "graduated" });
    await waitFor(() => draftRow("graduated-draft"));
    expect(screen.queryByText("published-draft")).toBeNull();
    // A publication record does not admit a draft to the graduated position.
    expect(screen.queryByText("active-draft")).toBeNull();
    expect(screen.queryByText("archived-draft")).toBeNull();
  });

  it("DRP-FR-07, DRP-FR-08: the active position admits a legacy published draft, which keeps its label", async () => {
    drafts = mixed();
    renderPanel({ filter: "active" });
    await waitFor(() => draftRow("active-draft"));
    const published = draftRow("published-draft");
    expect(within(published).getByTestId("draft-published-marker")).toHaveTextContent("Published");
    expect(screen.queryByText("graduated-draft")).toBeNull();
    expect(screen.queryByText("archived-draft")).toBeNull();
  });

  it("DRP-FR-08: the graduated position keeps the Graduated label", async () => {
    drafts = mixed();
    renderPanel({ filter: "graduated" });
    const graduated = await waitFor(() => draftRow("graduated-draft"));
    expect(within(graduated).getByTestId("draft-graduated-marker")).toHaveTextContent("Graduated");
  });

  it("DRP-FR-07: the all-drafts position admits every status", async () => {
    drafts = mixed();
    renderPanel({ filter: "all" });
    await waitFor(() => draftRow("graduated-draft"));
    for (const name of ["published-draft", "active-draft", "archived-draft"]) {
      expect(draftRow(name)).toBeInTheDocument();
    }
  });
});
