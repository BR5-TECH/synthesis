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
])("a turn the app could not complete, in %s", (_name, TurnOutcomes) => {
  it.each(["invalid_response", "invalid_request"])(
    "CTA-FR-EXVN, CTA-FR-ZDDI, AGC-FR-15: a recoverable %s card offers Retry and names no code",
    (failure) => {
      render(<TurnOutcomes failedTurn={{ ...failed, failure } as AgentTurn} thread={thread} />);
      const card = screen.getByTestId("comment-failed");
      expect(card).toHaveTextContent("arch could not produce a response.");
      expect(card).not.toHaveTextContent(failure);
      expect(screen.queryByTestId("comment-failed-tls")).not.toBeInTheDocument();
      expect(screen.getByTestId("comment-failed-retry")).toBeInTheDocument();
    },
  );
});
