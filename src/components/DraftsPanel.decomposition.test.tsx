/**
 * The decomposition facts on Drafts panel rows (`DRP-drafts-panel.md`
 * DRP-FR-KAWD, DRP-FR-IAWQ, DRP-FR-AOLJ, DRP-FR-RUVR, DRP-FR-IGCP,
 * DRP-FR-TCRW).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async () => () => {},
}));

import { draftRow, flat, renderPanel } from "../test/draftsPanelFixtures";
import { resetPanelReveals } from "../state/panelReveal";
import { resetLogBufferForTest } from "../logging";
import type { DraftSummary } from "../types";

let drafts: DraftSummary[];

const row = (id: string, name: string, over: Partial<DraftSummary> = {}): DraftSummary => ({
  id,
  name,
  status: "active",
  folder: "",
  updatedAt: "2026-07-31T08:00:00Z",
  ...over,
});

beforeEach(() => {
  resetPanelReveals();
  resetLogBufferForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_drafts") return flat(drafts);
    if (cmd === "append_log_records") return undefined;
    if (cmd === "search_drafts") return [];
    throw new Error(`unexpected command: ${cmd}`);
  });
});
afterEach(cleanup);

const sequence = (position: number) => ({
  setId: "s",
  position,
  total: 3,
  originalDraftId: "orig",
  originalName: "relay-old",
});

describe("Decomposition badges", () => {
  it("DRP-FR-KAWD, DRP-FR-IAWQ, DRP-FR-AOLJ, DRP-FR-RUVR, DRP-FR-IGCP: rows carry the badges and the order stays", async () => {
    drafts = [
      row("p1", "relay-auth", { sequence: sequence(1), updatedAt: "2026-07-31T09:00:00Z" }),
      row("orig", "relay", {
        status: "archived",
        updatedAt: "2026-07-31T08:30:00Z",
        superseded: { setId: "s", confirmedAt: "x", drafts: [{ id: "p1", name: "relay-auth" }] },
      }),
      row("p3", "relay-sync", { sequence: sequence(3), status: "archived" }),
    ];
    renderPanel({ filter: "all" });
    await waitFor(() => draftRow("relay-auth"));

    const auth = within(draftRow("relay-auth")).getByTestId("draft-sequence-marker");
    expect(auth).toHaveTextContent("Part 1 of 3");
    expect(auth).toHaveAccessibleName("Part 1 of 3 of “relay”");
    expect(within(draftRow("relay-sync")).getByTestId("draft-sequence-marker")).toHaveTextContent(
      "Part 3 of 3",
    );
    const original = draftRow("relay");
    expect(within(original).getByTestId("draft-superseded-marker")).toHaveAccessibleName(
      "Superseded by relay-auth",
    );
    expect(original).toHaveAccessibleName("Draft relay, archived, superseded");
    expect(draftRow("relay-auth")).toHaveAccessibleName("Draft relay-auth, part 1 of 3");
    const names = screen
      .getAllByRole("treeitem")
      .map((r) => r.getAttribute("aria-label") ?? "")
      .filter((n) => n.startsWith("Draft"));
    expect(names.map((n) => n.split(",")[0])).toEqual([
      "Draft relay-auth",
      "Draft relay",
      "Draft relay-sync",
    ]);
  });

  it("DRP-FR-IGCP: a superseded draft is admitted where an archived one is", async () => {
    drafts = [
      row("orig", "relay", {
        status: "archived",
        superseded: { setId: "s", confirmedAt: "x", drafts: [] },
      }),
      row("other", "other"),
    ];
    renderPanel({ filter: "active" });
    await waitFor(() => draftRow("other"));
    expect(screen.queryByRole("treeitem", { name: /^Draft relay/ })).toBeNull();
  });

  it("DRP-FR-TCRW: a pending new-draft proposal shows the pending-proposal marker from the row", async () => {
    drafts = [row("orig", "relay", { hasPendingProposal: true })];
    renderPanel({ filter: "all" });
    const r = await waitFor(() => draftRow("relay"));
    expect(r.querySelector(".draft-row__proposal")).not.toBeNull();
  });
});
