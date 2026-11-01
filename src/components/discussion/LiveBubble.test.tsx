import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";

import {
  noteLiveChunk,
  noteLiveChunkEnd,
  noteLiveChunkFailed,
  noteLiveText,
  openLivePart,
  resetLiveBubbleForTest,
  dropLivePart,
} from "../../state/liveBubble";
import type { AgentTurn } from "../../types";
import { splitAroundBubble, useLiveBubbleNode } from "./LiveBubble";

function Host({ id }: { id: string }) {
  return <div data-testid={`host-${id}`}>{useLiveBubbleNode(id)}</div>;
}

function turn(id: string, over: Partial<AgentTurn> = {}): AgentTurn {
  return {
    id,
    agentId: id,
    nickname: id,
    origin: { kind: "artifact", artifactId: "a.md", discussionId: "d1" } as never,
    triggerCommentId: "k1",
    state: "running",
    failure: null,
    retryPermitted: false,
    startedAt: "2026-02-01T00:00:00Z",
    endedAt: null,
    activeToolCalls: [],
    ...over,
  } as AgentTurn;
}

beforeEach(() => {
  resetLiveBubbleForTest();
});

afterEach(() => {
  cleanup();
});

describe("the live bubble", () => {
  it("CVP-FR-TGPV, CVP-FR-GHWN: it shows the chunks with a Speaking marker, in the voice discussion alone", () => {
    render(
      <>
        <Host id="d1" />
        <Host id="d2" />
      </>,
    );
    expect(screen.queryByTestId("live-bubble")).toBeNull();
    act(() => {
      openLivePart("d1", "c1");
      noteLiveChunk("c1", 1, "The scope is too wide.");
      noteLiveChunk("c1", 0, "Check the second paragraph.");
      noteLiveText("c1", "and the");
    });
    const bubble = screen.getByTestId("live-bubble");
    expect(bubble).toHaveTextContent(
      "Check the second paragraph. The scope is too wide. and the",
    );
    expect(screen.getByTestId("live-bubble-marker")).toHaveTextContent("Speaking…");
    expect(bubble).not.toHaveAttribute("aria-live");
    expect(screen.getByTestId("host-d2")).toBeEmptyDOMElement();
  });

  it("VOM-FR-MBHB: a failed chunk shows an error at its place", () => {
    render(<Host id="d1" />);
    act(() => {
      openLivePart("d1", "c1");
      noteLiveChunk("c1", 0, "One.");
      noteLiveChunkFailed("c1", 1, "provider_unreachable");
      noteLiveChunk("c1", 2, "Three.");
    });
    expect(screen.getByTestId("live-bubble")).toHaveTextContent(
      "One. One part was not heard. Three.",
    );
    expect(screen.getByTestId("live-bubble-error")).toBeInTheDocument();
  });

  it("VOM-FR-HUXY, VOM-FR-SCAM: live text shows at once, and an ended chunk keeps it at its place", () => {
    render(<Host id="d1" />);
    act(() => {
      openLivePart("d1", "c1");
      noteLiveText("c1", "Check the");
    });
    expect(screen.getByTestId("live-bubble")).toHaveTextContent("Check the");
    act(() => {
      noteLiveText("c1", "Check the second paragraph.");
      noteLiveChunkEnd("c1", 0);
      noteLiveText("c1", "The scope");
    });
    expect(screen.getByTestId("live-bubble")).toHaveTextContent(
      "Check the second paragraph. The scope",
    );
    expect(screen.getByTestId("live-bubble")).not.toHaveAttribute("aria-live");
  });

  it("CVP-FR-GHWN, VOM-FR-REJN: the bubble goes when its utterance settles", () => {
    render(<Host id="d1" />);
    act(() => {
      openLivePart("d1", "c1");
      noteLiveChunk("c1", 0, "One.");
    });
    expect(screen.getByTestId("live-bubble")).toBeInTheDocument();
    act(() => dropLivePart("c1"));
    expect(screen.queryByTestId("live-bubble")).toBeNull();
  });

  it("CVP-FR-TGPV: only a turn that the speech registered shows below the bubble", () => {
    const typed = turn("arch");
    const steered = turn("sec", { speechId: "c1" });
    const registered = turn("ux", { speechId: "c1", triggerCommentId: "" });
    expect(splitAroundBubble([typed, steered, registered], true)).toEqual({
      before: [typed, steered],
      after: [registered],
    });
    expect(splitAroundBubble([typed, registered], false)).toEqual({
      before: [typed, registered],
      after: [],
    });
  });
});
