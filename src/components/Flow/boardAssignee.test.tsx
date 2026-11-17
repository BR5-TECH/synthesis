/**
 * The assignee of the bottom line of the board
 * (`../../../specifications/ui/DBR-delivery-board.md` DBR-FR-WQAN).
 */
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import type { TrackerFacts } from "../../types";
import { BoardAssignee } from "./boardItems";
import type { ItemFacts } from "./items";

function factsOf(kind: "issue" | "milestone", assignees?: string[]): ItemFacts {
  const facts: TrackerFacts = {
    itemId: "i1",
    ref: { kind, number: 7, url: "https://github.com/acme/payments/issues/7" },
    state: "open",
    closeReason: null,
    updatedAt: "2026-11-01T00:00:00Z",
    status: null,
    claimEligible: false,
    externalBlockers: [],
    ...(assignees ? { assignees } : {}),
  };
  return { blockers: [], overlap: null, pending: false, references: [], tracker: { repository: "acme/payments", facts } };
}

afterEach(cleanup);

describe("the assignee (DBR-FR-WQAN)", () => {
  it("DBR-FR-WQAN: two assignees show the first login and +1, and the name lists both", () => {
    render(<BoardAssignee facts={factsOf("issue", ["mia", "jo"])} />);
    const assignee = screen.getByTestId("flow-card-assignee");
    expect(assignee).toHaveTextContent(/^@mia\+1$/);
    expect(assignee).toHaveAccessibleName("Assigned to @mia, @jo");
  });

  it("DBR-FR-WQAN: a milestone shows no assignee, also where its facts name one", () => {
    render(<BoardAssignee facts={factsOf("milestone", ["ola"])} />);
    expect(screen.queryByTestId("flow-card-assignee")).toBeNull();
  });

  it("DBR-FR-WQAN: an issue with no assignee, facts with no assignee list, and an item with no tracker show nothing", () => {
    render(<BoardAssignee facts={factsOf("issue", [])} />);
    render(<BoardAssignee facts={factsOf("issue")} />);
    render(<BoardAssignee facts={{ blockers: [], overlap: null, pending: false, references: [] }} />);
    expect(screen.queryByTestId("flow-card-assignee")).toBeNull();
  });
});
