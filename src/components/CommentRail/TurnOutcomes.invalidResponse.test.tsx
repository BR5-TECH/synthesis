import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { TurnOutcomes as RailTurnOutcomes } from "./ThreadCardParts";
import { TurnOutcomes as DiscussionTurnOutcomes } from "../discussion/DiscussionParts";
import type { AgentTurn, Discussion } from "../../types";

afterEach(cleanup);

const thread = { id: "d1", locked: false, resolved: false } as unknown as Discussion;

const failed: AgentTurn = {
  id: "turn-1",
  agentId: "a",
  nickname: "arch",
  origin: { kind: "artifact" },
  triggerCommentId: "c",
  state: "failed",
  failure: "invalid_response",
  tlsFailure: null,
  retryPermitted: true,
  startedAt: "t",
  endedAt: "t",
  activeToolCalls: [],
  imagesOmitted: false,
} as unknown as AgentTurn;

describe.each([
  ["the comment rail", RailTurnOutcomes],
  ["the discussion surface", DiscussionTurnOutcomes],
])("a reply the app could not read, in %s", (_name, TurnOutcomes) => {
  it("CTA-FR-EXVN, AGC-FR-15: a recoverable invalid_response card offers Retry and names no code", () => {
    render(<TurnOutcomes failedTurn={failed} thread={thread} />);
    const card = screen.getByTestId("comment-failed");
    expect(card).toHaveTextContent("arch could not produce a response.");
    expect(card).not.toHaveTextContent("invalid_response");
    expect(screen.queryByTestId("comment-failed-tls")).not.toBeInTheDocument();
    expect(screen.getByTestId("comment-failed-retry")).toBeInTheDocument();
  });
});
