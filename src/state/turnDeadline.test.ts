/**
 * The deadline the timeout bar draws (`CVP-conversation-presentation.md`
 * CVP-FR-JSHF, CVP-FR-IJEY, CVP-FR-ZRWW).
 */
import { describe, expect, it } from "vitest";

import { barDeadlineOf, percentLeft } from "./turnDeadline";
import type { AgentTurn } from "../types";

function turn(over: Partial<AgentTurn>): AgentTurn {
  return { state: "running", deadlineStartedAt: null, deadlineAt: null, ...over } as AgentTurn;
}

const iso = (ms: number) => new Date(ms).toISOString();

describe("CVP-FR-JSHF / CVP-FR-ZRWW: which deadline the bar shows", () => {
  it("none where no turn runs with a deadline, a waiting turn among them", () => {
    expect(barDeadlineOf([])).toBeNull();
    expect(barDeadlineOf([turn({})])).toBeNull();
    expect(
      barDeadlineOf([turn({ state: "delivered", deadlineStartedAt: iso(0), deadlineAt: iso(10) })]),
    ).toBeNull();
  });

  it("the running turn with the latest end, which has the most time left", () => {
    const deadline = barDeadlineOf([
      turn({ deadlineStartedAt: iso(0), deadlineAt: iso(1_000) }),
      turn({ deadlineStartedAt: iso(500), deadlineAt: iso(2_000) }),
    ]);
    expect(deadline).toEqual({ startedMs: 500, endsMs: 2_000 });
  });
});

describe("CVP-FR-IJEY: the share left, from 100 to 0", () => {
  const deadline = { startedMs: 1_000, endsMs: 11_000 };
  it("goes down as time passes and never leaves the range", () => {
    expect(percentLeft(deadline, 1_000)).toBe(100);
    expect(percentLeft(deadline, 6_000)).toBe(50);
    expect(percentLeft(deadline, 11_000)).toBe(0);
    expect(percentLeft(deadline, 20_000)).toBe(0);
    expect(percentLeft(deadline, 0)).toBe(100);
  });
});
