import { describe, expect, it } from "vitest";

import cases from "./spokenPart.cases.json";
import { spokenPart, visibleBody, withoutSpokenTags } from "./spokenPart";

describe("the parser", () => {
  it("CVL-FR-AGNN, CVL-FR-JPHT: the cases that both parsers share hold", () => {
    expect(cases.length).toBeGreaterThanOrEqual(20);
    for (const { body, spoken, written } of cases) {
      expect(spokenPart(body), JSON.stringify(body)).toBe(spoken);
      expect(withoutSpokenTags(body), JSON.stringify(body)).toBe(written);
    }
  });
});

describe("visibleBody", () => {
  it("CMT-FR-BMCI: every complete tag goes with its text", () => {
    expect(visibleBody("<spoken>Short.</spoken> Written.")).toBe("Written.");
    expect(visibleBody("Written.\n\n<spoken>Short.</spoken>")).toBe("Written.");
    expect(visibleBody("One <spoken>a</spoken>two<spoken>b</spoken> three")).toBe("One two three");
  });

  it("CMT-FR-BMCI: a tag inside code stays as code", () => {
    const body = "Write `<spoken>x</spoken>` here.";
    expect(visibleBody(body)).toBe(body);
    const fenced = "```\n<spoken>x</spoken>\n```";
    expect(visibleBody(fenced)).toBe(fenced);
  });

  it("CMT-FR-BMCI: a tag in code stays while the real one goes", () => {
    expect(visibleBody("Use `<spoken>x</spoken>`.\n\n<spoken>Real.</spoken>")).toBe(
      "Use `<spoken>x</spoken>`.",
    );
  });

  it("CMT-FR-XWVW: a body with nothing else shows the spoken part", () => {
    expect(visibleBody("<spoken>Yes.</spoken>")).toBe("Yes.");
    expect(visibleBody("\n  <spoken> Yes. </spoken>\n")).toBe("Yes.");
  });

  it("CMT-FR-BMCI: a body with no complete tag stays exactly as it is", () => {
    expect(visibleBody("  Written.\n")).toBe("  Written.\n");
    expect(visibleBody("<spoken>It never closes.")).toBe("<spoken>It never closes.");
  });

  it("CMT-FR-XWVW: a blank tag alone leaves nothing to read", () => {
    expect(visibleBody("<spoken>  </spoken>")).toBe("");
  });
});
