import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { TurnOutcomes as RailTurnOutcomes } from "./ThreadCardParts";
import { TurnOutcomes as DiscussionTurnOutcomes } from "../discussion/DiscussionParts";
import type { AgentTurn, Discussion } from "../../types";

afterEach(cleanup);

const thread = { id: "d1", locked: false, resolved: false } as unknown as Discussion;

const failed = (over: Partial<AgentTurn> = {}): AgentTurn =>
  ({
    id: "turn-1",
    agentId: "a",
    nickname: "arch",
    origin: { kind: "artifact" },
    triggerCommentId: "c",
    state: "failed",
    failure: "tls_untrusted",
    tlsFailure: { host: "gateway.corp.example", cause: "unknown_issuer" },
    retryPermitted: true,
    startedAt: "t",
    endedAt: "t",
    activeToolCalls: [],
    imagesOmitted: false,
    ...over,
  }) as unknown as AgentTurn;

describe.each([
  ["the comment rail", RailTurnOutcomes],
  ["the discussion surface", DiscussionTurnOutcomes],
])("a failed contribution in %s", (_name, TurnOutcomes) => {
  it("CTA-FR-ZDDI, CTA-FR-EXVN: a refused certificate names the host and the cause and offers Retry", () => {
    render(<TurnOutcomes failedTurn={failed()} thread={thread} />);
    const detail = screen.getByTestId("comment-failed-tls");
    expect(detail).toHaveTextContent("gateway.corp.example");
    expect(detail).toHaveTextContent(/issuer.*unknown/);
    expect(screen.getByTestId("comment-failed")).toHaveTextContent(
      "arch could not produce a response.",
    );
    expect(screen.getByTestId("comment-failed-retry")).toBeInTheDocument();
  });

  it("CTA-FR-EXVN: any other failure names nothing beyond the Retry", () => {
    render(
      <TurnOutcomes
        failedTurn={failed({ failure: "unreachable", tlsFailure: null })}
        thread={thread}
      />,
    );
    expect(screen.queryByTestId("comment-failed-tls")).not.toBeInTheDocument();
    expect(screen.getByTestId("comment-failed-retry")).toBeInTheDocument();
  });
});
