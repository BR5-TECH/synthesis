/**
 * What a container measures of itself for the automatic layout
 * (`../../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-ECPZ;
 * `FLO-flow.md` FLO-FR-PLUZ, FLO-FR-HBBF). jsdom lays nothing out, so each
 * box states its own size here.
 */
import { describe, expect, it } from "vitest";

import { HEAD_NAME_MAX, measureContainer } from "./measure";

function box(
  tag: string,
  size: Partial<Record<"offsetWidth" | "offsetHeight" | "offsetTop" | "scrollWidth" | "rectWidth", number>>,
  style = "",
  cls = "",
) {
  const el = document.createElement(tag);
  if (cls) el.className = cls;
  el.setAttribute("style", style);
  for (const [k, v] of Object.entries(size)) {
    if (k === "rectWidth") {
      el.getBoundingClientRect = () => ({ width: v }) as DOMRect;
    } else {
      Object.defineProperty(el, k, { configurable: true, value: v });
    }
  }
  return el;
}

function container(nameWidth: number, below: HTMLElement[] = []) {
  const root = box("div", {}, "position: absolute");
  const head = box("div", { offsetWidth: 120, offsetTop: 0, offsetHeight: 40 }, "column-gap: 6px; padding-left: 6px; padding-right: 10px");
  head.append(
    box("button", { offsetWidth: 16 }),
    box("span", { offsetWidth: 24, scrollWidth: nameWidth }, "", "flow-ctr__name"),
    box("span", { offsetWidth: 300 }, "min-width: 4px", "flow-ctr__spacer"),
    box("span", { offsetWidth: 60 }, "margin-left: 2px", "flow-chip"),
    box("button", { offsetWidth: 12 }, "position: absolute", "flow-port"),
  );
  root.append(head, ...below, box("span", { offsetTop: 500, offsetHeight: 8 }, "position: absolute", "flow-resize"));
  return { root, head };
}

describe("measureContainer (MDL-FR-ECPZ)", () => {
  it("MDL-FR-ECPZ: the header needs its padding, each chip in full, the full name, and the spacer's floor", () => {
    const { root, head } = container(65);
    // 6 + 10 padding, 16 + (65 + 1) + 4 + (60 + 2), and 3 gaps of 6; the port
    // floats. The name adds 1, as its scrollWidth can be rounded down.
    expect(measureContainer(root, head).headWidth).toBe(16 + 16 + 66 + 4 + 62 + 18);
  });

  it("MDL-FR-ECPZ: a long name takes at most HEAD_NAME_MAX and truncates past it", () => {
    const { root, head } = container(900);
    expect(measureContainer(root, head).headWidth).toBe(16 + 16 + HEAD_NAME_MAX + 4 + 62 + 18);
  });

  it("MDL-FR-PUCM: the content above the members ends below the error line, and a resize handle does not count", () => {
    const error = box("div", { offsetTop: 40, offsetHeight: 34 });
    const { root, head } = container(65, [error]);
    expect(measureContainer(root, head).memberTop).toBe(74);
    expect(measureContainer(...Object.values(container(65)) as [HTMLElement, HTMLElement]).memberTop).toBe(40);
  });

  it("MDL-FR-ECPZ, FLO-FR-HBBF: each part counts to the fraction of a pixel, rounded up, with the container's border, at any zoom", () => {
    // The canvas is zoomed to 150%: each rectangle on the screen is 1.5 times its width.
    const root = box("div", {}, "border-left-width: 1px; border-right-width: 1px; border-style: solid");
    const head = box("div", { offsetWidth: 120, rectWidth: 180 }, "column-gap: 6px; padding-left: 6px; padding-right: 10px");
    head.append(
      box("span", { offsetWidth: 7, scrollWidth: 42, rectWidth: 10.5 }, "", "flow-ctr__name"),
      // A chip 80.41 wide, which offsetWidth rounds down to 80.
      box("span", { offsetWidth: 80, rectWidth: 80.41 * 1.5 }, "", "flow-chip"),
    );
    root.append(head);
    // 2 border + 16 padding + (42 + 1) name + 81 chip + one gap of 6.
    expect(measureContainer(root, head).headWidth).toBe(2 + 16 + 43 + 81 + 6);
  });

  it("FLO-FR-PLUZ: a header that is not laid out measures 0, which the layout reads as not measured", () => {
    const { root, head } = container(65);
    Object.defineProperty(head, "offsetWidth", { configurable: true, value: 0 });
    expect(measureContainer(root, head).headWidth).toBe(0);
  });
});
