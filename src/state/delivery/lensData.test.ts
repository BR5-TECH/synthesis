/**
 * What the Delivery lens keeps in the map session
 * (`../../../specifications/ui/SMP-specification-map.md` SMP-FR-RNLH,
 * SMP-FR-QNUH; `MDL-map-delivery-lens.md` MDL-FR-LZIE).
 */
import { describe, expect, it, vi } from "vitest";

import { githubAnswer } from "../../test/deliveryLensHarness";
import { item, plan } from "../../test/flowPlanFixtures";
import { DeliveryLensData } from "./lensData";
import type { GithubPlanOps } from "./githubPlan";

vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

const DOC = plan([item("t1", "task")]);

function ops(): GithubPlanOps {
  return { load: vi.fn(async () => githubAnswer(DOC)), apply: vi.fn(async () => githubAnswer(DOC)) };
}

describe("the GitHub plan of the lens data (SMP-FR-RNLH)", () => {
  it("SMP-FR-RNLH: the same Project keeps its plan, so a lens switch loads nothing again", async () => {
    const o = ops();
    const data = new DeliveryLensData(o);
    const first = data.githubFor("PVT_1");
    first.start();
    await vi.waitFor(() => expect(first.snapshot().loading).toBe(false));
    const again = data.githubFor("PVT_1");
    again.start();
    expect(again).toBe(first);
    expect(o.load).toHaveBeenCalledTimes(2);
    expect(again.snapshot().shown?.items.map((i) => i.id)).toEqual(["t1"]);
  });

  it("SMP-FR-RNLH, MDL-FR-LZIE: another Project starts a new plan, with nothing of the old one, that loads again", async () => {
    const o = ops();
    const data = new DeliveryLensData(o);
    const first = data.githubFor("PVT_1");
    first.start();
    await vi.waitFor(() => expect(first.snapshot().loading).toBe(false));
    const other = data.githubFor("PVT_2");
    expect(other).not.toBe(first);
    expect(other.projectNodeId).toBe("PVT_2");
    expect(data.github).toBe(other);
    expect(other.hasStarted()).toBe(false);
    expect(other.snapshot().shown).toBeNull();
    other.start();
    await vi.waitFor(() => expect(o.load).toHaveBeenCalledTimes(4));
    // Going back to the first Project starts it afresh too.
    expect(data.githubFor("PVT_1")).not.toBe(first);
  });
});
