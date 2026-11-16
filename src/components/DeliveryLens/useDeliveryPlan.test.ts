import { describe, expect, it } from "vitest";

import { fileSource, githubSource } from "../../test/deliveryLensHarness";
import { githubSourceOf } from "./useDeliveryPlan";

describe("the GitHub source the lens loads (MDL-FR-WAOZ)", () => {
  it("MDL-FR-WAOZ, GDP-FR-QSHX: a valid and an unchecked configuration load; an invalid one and another source do not", () => {
    expect(githubSourceOf(githubSource("valid"))).not.toBeNull();
    expect(githubSourceOf(githubSource("unchecked"))).not.toBeNull();
    expect(githubSourceOf(githubSource("invalid", "No Status field."))).toBeNull();
    expect(githubSourceOf(fileSource("plans/a.flow"))).toBeNull();
    expect(githubSourceOf({ kind: "none", reason: "no_source" })).toBeNull();
    expect(githubSourceOf(null)).toBeNull();
  });
});
