/**
 * The decomposition badges of a Drafts panel row (`DRP-drafts-panel.md`) and
 * of the New Artifact tab's chrome (`NAW-new-artifact.md`).
 */
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import {
  decompositionSuffix,
  originalNameOf,
  SequenceBadge,
  SupersededBadge,
} from "./draftsRowBadges";
import { DecompositionTags } from "./NewArtifactWorkspace/DecompositionTags";
import { configureDecompositionRoutes } from "../state/decompositionRoutes";
import type { DraftSummary } from "../types";

const sequence = { setId: "s", position: 2, total: 3, originalDraftId: "orig", originalName: "relay" };
const superseded = {
  setId: "s",
  confirmedAt: "x",
  drafts: [
    { id: "a", name: "relay-auth" },
    { id: "b", name: "relay-sync" },
  ],
};

function draft(over: Partial<DraftSummary> = {}): DraftSummary {
  return { id: "d", name: "relay-auth", status: "active", updatedAt: "", ...over };
}

afterEach(() => {
  cleanup();
  configureDecompositionRoutes(null);
});

describe("the decomposition badges", () => {
  it("DRP-FR-KAWD, DRP-FR-IAWQ: the sequence badge reads Part N of M and names the original on hover, on any status", () => {
    for (const status of ["active", "archived", "graduated"] as const) {
      render(<SequenceBadge draft={draft({ status, sequence })} originalName="relay" />);
      const badge = screen.getByTestId("draft-sequence-marker");
      expect(badge).toHaveTextContent("Part 2 of 3");
      expect(badge).toHaveAttribute("title", "Part 2 of 3 of “relay”");
      expect(badge).toHaveAccessibleName("Part 2 of 3 of “relay”");
      expect(badge.tagName).toBe("SPAN");
      cleanup();
    }
  });

  it("DRP-FR-IAWQ: the original's current name where the tree holds it, else the stored name", () => {
    const part = draft({ sequence });
    const tree = new Map([["", [part, draft({ id: "orig", name: "relay v2" })]]]);
    expect(originalNameOf(part, tree)).toBe("relay v2");
    expect(originalNameOf(part, new Map([["", [part]]]))).toBe("relay");
  });

  it("DRP-FR-AOLJ, DRP-FR-NOXB, DRP-FR-PDAA: the superseded marker names the new drafts, on an active row too", () => {
    render(<SupersededBadge draft={draft({ status: "active", superseded })} />);
    const marker = screen.getByTestId("draft-superseded-marker");
    expect(marker).toHaveTextContent("Superseded");
    expect(marker).toHaveAttribute("title", "Superseded by relay-auth, relay-sync");
    expect(marker).toHaveAccessibleName("Superseded by relay-auth, relay-sync");
  });

  it("DRP-FR-RUVR: the row's accessible name says both facts", () => {
    expect(decompositionSuffix(draft({ sequence, superseded }))).toBe(", part 2 of 3, superseded");
    expect(decompositionSuffix(draft())).toBe("");
  });

  it("NAW-FR-MONI, NAW-FR-IDIJ: the chrome row carries both tags with the same hover text", () => {
    configureDecompositionRoutes({
      openDraft: () => {},
      draftOf: (id) => (id === "orig" ? draft({ id: "orig", name: "relay now" }) : undefined),
      openPreview: () => {},
      closePreview: () => {},
    });
    render(<DecompositionTags sequence={sequence} superseded={superseded} />);
    expect(screen.getByTestId("draft-workspace-sequence")).toHaveAccessibleName("Part 2 of 3 of “relay now”");
    expect(screen.getByTestId("draft-workspace-superseded")).toHaveAccessibleName(
      "Superseded by relay-auth, relay-sync",
    );
  });
});
