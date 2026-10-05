import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { TabStrip } from "./TabStrip";
import { DASHBOARD_TAB, SPEC_MAP_TAB } from "../hooks/shell/tabRecords";

afterEach(cleanup);

describe("the Map tab in the strip (SMP-FR-KQTD)", () => {
  it("SMP-FR-KQTD: the Map tab is labelled `Map — specifications` with the layers icon in front", () => {
    render(
      <TabStrip
        tabs={[DASHBOARD_TAB, SPEC_MAP_TAB]}
        activeId={SPEC_MAP_TAB.id}
        onActivate={() => {}}
        onClose={() => {}}
        onHome={() => {}}
      />,
    );
    const tab = screen.getByTitle("Map — specifications");
    expect(tab.querySelector(".tab__label")).toHaveTextContent("Map — specifications");
    const icon = tab.querySelector('svg[data-icon="layers"]');
    expect(icon).not.toBeNull();
    // In front of the label.
    expect(icon!.compareDocumentPosition(tab.querySelector(".tab__label")!)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
    expect(SPEC_MAP_TAB).toMatchObject({ id: "map:specifications", kind: "map" });
  });
});
