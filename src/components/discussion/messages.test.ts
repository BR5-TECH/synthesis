import { describe, expect, it } from "vitest";

import { AGENT_TURN_FAILURES } from "../../types";
import { commentErrorMessage, loggableTurnFailure, turnFailureMessage } from "./messages";

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

describe("what a refused change discussion says", () => {
  it.each(["unknown_stream", "change_not_found"])(
    "CMS-FR-FACZ: the refusal %s has a message of its own",
    (code) => {
      expect(commentErrorMessage(code)).not.toBe(code);
      expect(commentErrorMessage(code)).toMatch(/cannot be discussed/);
    },
  );
});

describe("what an attachment refusal says", () => {
  it.each([
    ["no_text_extracted: scan.pdf", "scan.pdf has no text to read. Scanned documents are not supported."],
    ["document_unreadable: spec.docx", "spec.docx could not be read. It can be damaged or encrypted."],
    ["extracted_text_too_large: big.pdf", "The text of big.pdf is above the Extracted text limit."],
  ])("CMT-FR-LMMQ: %j names the file", (raw, sentence) => {
    expect(commentErrorMessage(raw)).toBe(sentence);
  });

  it.each([
    ["unsupported_media_type", "This file type cannot be attached."],
    ["attachment_too_large", "This file is above the attachment size limit."],
    ["malformed_attachment", "This attachment could not be read."],
    ["unsupported_media_type: a.zip", "a.zip: This file type cannot be attached."],
  ])("CMT-FR-QZXV: %j reads as a sentence", (raw, sentence) => {
    expect(commentErrorMessage(raw)).toBe(sentence);
  });

  it("CMT-FR-LMMQ: a filename that holds a colon and a space is kept whole", () => {
    expect(commentErrorMessage("document_unreadable: draft: v2.docx")).toBe(
      "draft: v2.docx could not be read. It can be damaged or encrypted.",
    );
  });

  it("CMT-FR-LMMQ: a code without a filename still reads as a sentence", () => {
    expect(commentErrorMessage("no_text_extracted")).toBe(
      "This file has no text to read. Scanned documents are not supported.",
    );
    expect(commentErrorMessage("extracted_text_too_large")).toBe(
      "The text of this file is above the Extracted text limit.",
    );
  });

  it.each([
    ["attachment_too_large: big.png", "big.png: This file is above the attachment size limit."],
    ["malformed_attachment: odd.png", "odd.png: This attachment could not be read."],
  ])("CMT-FR-QZXV: %j names the file", (raw, sentence) => {
    expect(commentErrorMessage(raw)).toBe(sentence);
  });

  it("CMT-FR-34: an unknown refusal is shown as before", () => {
    expect(commentErrorMessage("something_else: x")).toBe("something_else: x");
  });
});
