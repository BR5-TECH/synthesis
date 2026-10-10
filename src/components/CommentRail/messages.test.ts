import { describe, expect, it } from "vitest";

import { AGENT_TURN_FAILURES } from "../../types";
import { loggableTurnFailure, turnFailureMessage } from "./messages";

describe("what a refused dispatch puts in the log", () => {
  it.each(Object.values(AGENT_TURN_FAILURES))(
    "CTA-FR-UUXA: the typed failure %s is logged as itself",
    (code) => {
      expect(loggableTurnFailure(code)).toBe(code);
    },
  );

  it.each([
    'invalid args `origin` for command `dispatch_agent_turn`: unknown variant "a private passage"',
    "",
    "Error: x",
    "agent_not_found ",
  ])("CTA-FR-UUXA: any other text is logged as unexpected (%j)", (raw) => {
    expect(loggableTurnFailure(raw)).toBe("unexpected");
  });
});

describe("what a typed failure says", () => {
  it.each(Object.values(AGENT_TURN_FAILURES))(
    "AGC-FR-15: the typed failure %s has a message of its own",
    (code) => {
      expect(turnFailureMessage(code)).not.toBe(code);
    },
  );

  it("AGC-FR-15: a reply the app could not read has its own message", () => {
    expect(turnFailureMessage("invalid_response")).toBe(
      "The agent's provider answered with a reply the app could not read.",
    );
  });

  it("AGC-FR-15: a request the app could not build has its own message", () => {
    expect(turnFailureMessage("invalid_request")).toBe(
      "The app could not make a request for the agent's provider.",
    );
  });
});
