import { describe, expect, it } from "vitest";

import { AGENT_TURN_FAILURES } from "../../types";
import { loggableTurnFailure } from "./messages";

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
