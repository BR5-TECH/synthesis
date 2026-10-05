// The shared selector row (`SNV-shell-navigation.md` SNV-FR-62, SNV-FR-63).
//
// The form half — radio-group semantics, the tag, the tooltip — is asserted on
// the real DOM. The clipping half needs geometry, which jsdom does not compute,
// so widths are stubbed below: without that the row can only ever be measured
// as zero-wide and every fit test would pass vacuously.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { useState } from "react";

import { SelectorRow, type SelectorPosition } from "./SelectorRow";
import { selectorNames, selectorValues } from "../test/selectors";
import { readStylesheet } from "../test/readStylesheet";

type P = "a" | "b" | "c" | "d" | "e" | "f";

const SIX: SelectorPosition<P>[] = [
  { value: "a", tag: "A", title: "Alpha" },
  { value: "b", tag: "B", title: "Bravo" },
  { value: "c", tag: "C", title: "Charlie" },
  { value: "d", tag: "D", title: "Delta" },
  { value: "e", tag: "E", title: "Echo" },
  { value: "f", tag: "F", title: "Foxtrot" },
];

/** Every button the same width, so the arithmetic in a failure is readable. */
const BUTTON_W = 40;
/** Mirrors `GAP_PX` in the component and `--sp-1` in the stylesheet. */
const GAP = 4;

let rowWidth = 0;
/** The observer callbacks the component registered, so a test can fire them. */
let observers: (() => void)[] = [];

const originalOffsetWidth = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  "offsetWidth",
);
const originalClientWidth = Object.getOwnPropertyDescriptor(
  Element.prototype,
  "clientWidth",
);

beforeEach(() => {
  rowWidth = 0;
  observers = [];
  Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
    configurable: true,
    get(this: HTMLElement) {
      return this.dataset.value ? BUTTON_W : 0;
    },
  });
  Object.defineProperty(Element.prototype, "clientWidth", {
    configurable: true,
    get(this: Element) {
      return this.classList.contains("selector-row") ? rowWidth : 0;
    },
  });
  vi.stubGlobal(
    "ResizeObserver",
    class {
      constructor(cb: () => void) {
        observers.push(cb);
      }
      observe() {}
      disconnect() {}
    },
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  if (originalOffsetWidth) {
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", originalOffsetWidth);
  }
  if (originalClientWidth) {
    Object.defineProperty(Element.prototype, "clientWidth", originalClientWidth);
  }
});

/** Re-measure at `width`, the way a panel resize does (SNV-FR-33). */
async function resizeTo(width: number) {
  await act(async () => {
    rowWidth = width;
    observers.forEach((cb) => cb());
  });
}

function renderRow(value: P = "a", onChange = vi.fn()) {
  const result = render(
    <SelectorRow label="Lens" positions={SIX} value={value} onChange={onChange} />,
  );
  return { ...result, onChange };
}

// ---------------------------------------------------------------------------
// SNV-FR-62: the form
// ---------------------------------------------------------------------------

describe("the toggle-row form (SNV-FR-62)", () => {
  it("is a radio group of buttons with exactly one active, and no dropdown", async () => {
    rowWidth = 1000;
    renderRow("c");

    const row = screen.getByRole("radiogroup", { name: "Lens" });
    expect(row.tagName).not.toBe("SELECT");
    expect(row.querySelector("select")).toBeNull();

    const buttons = screen.getAllByRole("radio");
    expect(buttons).toHaveLength(6);
    expect(buttons.every((b) => b.tagName === "BUTTON")).toBe(true);
    expect(
      buttons.filter((b) => b.getAttribute("aria-checked") === "true"),
    ).toHaveLength(1);
    expect(screen.getByRole("radio", { name: "Charlie" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("reports the activated position, and does not re-report the active one", async () => {
    rowWidth = 1000;
    const { onChange } = renderRow("a");

    await userEvent.click(screen.getByRole("radio", { name: "Delta" }));
    expect(onChange).toHaveBeenCalledExactlyOnceWith("d");

    // Activating what is already active changes nothing, so a panel that
    // persists on every change (LIB-FR-17) does not write for a no-op.
    onChange.mockClear();
    await userEvent.click(screen.getByRole("radio", { name: "Alpha" }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("renders each position's short tag, with the full name as its name and tooltip", async () => {
    rowWidth = 1000;
    renderRow("a");

    // The visible tag is the short one, in the artifact-tag treatment.
    const bravo = screen.getByRole("radio", { name: "Bravo" });
    const chip = bravo.querySelector(".chip-type")!;
    expect(chip).toHaveTextContent("B");

    // …and the full name is reachable: as the accessible name, and as a
    // tooltip the stylesheet reveals on hover and on keyboard focus.
    const tip = bravo.querySelector(".selector-row__tip")!;
    expect(tip).toHaveTextContent("Bravo");
    expect(selectorNames("Lens")).toEqual([
      "Alpha",
      "Bravo",
      "Charlie",
      "Delta",
      "Echo",
      "Foxtrot",
    ]);
  });

  it("carries the artifact type on the tag so a lens reads as its own content", async () => {
    rowWidth = 1000;
    render(
      <SelectorRow
        label="Lens"
        positions={[
          { value: "artifacts", tag: "All", title: "All Artifacts" },
          { value: "skill", tag: "SKL", title: "Skill", dataType: "skill" },
        ]}
        value="artifacts"
        onChange={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("radio", { name: "Skill" }).querySelector(".chip-type"),
    ).toHaveAttribute("data-type", "skill");
    // A position naming no type carries none rather than a placeholder.
    expect(
      screen
        .getByRole("radio", { name: "All Artifacts" })
        .querySelector(".chip-type"),
    ).not.toHaveAttribute("data-type");
  });

  it("moves between positions with the arrow keys, as a radio group does", async () => {
    rowWidth = 1000;
    const { onChange } = renderRow("b");

    screen.getByRole("radio", { name: "Bravo" }).focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(onChange).toHaveBeenCalledWith("c");

    onChange.mockClear();
    await userEvent.keyboard("{ArrowLeft}");
    expect(onChange).toHaveBeenCalledWith("a");
  });

  it("skips a disabled position rather than landing on it", async () => {
    rowWidth = 1000;
    const onChange = vi.fn();
    render(
      <SelectorRow
        label="Lens"
        positions={[
          { value: "a", tag: "A", title: "Alpha", disabled: true },
          { value: "b", tag: "B", title: "Bravo" },
          { value: "c", tag: "C", title: "Charlie" },
        ]}
        value="b"
        onChange={onChange}
      />,
    );

    expect(screen.getByRole("radio", { name: "Alpha" })).toBeDisabled();
    await userEvent.click(screen.getByRole("radio", { name: "Alpha" }));
    expect(onChange).not.toHaveBeenCalled();

    // …and the arrows wrap past it rather than through it.
    screen.getByRole("radio", { name: "Bravo" }).focus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(onChange).toHaveBeenCalledExactlyOnceWith("c");
  });
});

// ---------------------------------------------------------------------------
// SNV-FR-63 / SNV-FR-33: one line, clipped at the trailing edge, with the
// leading, trailing, and active buttons exempt.
// ---------------------------------------------------------------------------

describe("clipping at narrow widths (SNV-FR-63)", () => {
  it("clips from the trailing edge and widening reaches the rest", async () => {
    // Everything fits, so the first layout can measure every button.
    rowWidth = 1000;
    renderRow("a");
    expect(selectorValues("Lens")).toEqual(["a", "b", "c", "d", "e", "f"]);

    // Room for the two exempt ends plus two more: 4 * 40 + 3 * 4 = 172.
    await resizeTo(4 * BUTTON_W + 3 * GAP);
    expect(selectorValues("Lens")).toEqual(["a", "b", "c", "f"]);

    // Narrower still: only the ends survive.
    await resizeTo(2 * BUTTON_W + GAP);
    expect(selectorValues("Lens")).toEqual(["a", "f"]);

    // Widening is what reaches the rest (SNV-FR-33).
    await resizeTo(1000);
    expect(selectorValues("Lens")).toEqual(["a", "b", "c", "d", "e", "f"]);
  });

  it("keeps the leading, trailing, and active buttons at every width", async () => {
    rowWidth = 1000;
    const { rerender } = renderRow("a");

    // The active position is in the middle, where a plain overflow clip would
    // take it — the exemption is the whole point of the requirement.
    rerender(
      <SelectorRow label="Lens" positions={SIX} value="d" onChange={vi.fn()} />,
    );
    await resizeTo(2 * BUTTON_W + GAP);
    expect(selectorValues("Lens")).toEqual(["a", "d", "f"]);

    // Narrowing further does not start taking them.
    await resizeTo(10);
    expect(selectorValues("Lens")).toEqual(["a", "d", "f"]);
    expect(screen.getByRole("radio", { name: "Delta" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("recovers when a button cannot be measured yet", async () => {
    // A button can report zero while the row does not — first paint before
    // webfont metrics land, a transient `visibility`. Deciding the fit from a
    // width that is not there would clip a button the exemption protects, so
    // the row renders whole and re-decides once the measurement arrives.
    const unmeasurable = new Set(["c"]);
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
      configurable: true,
      get(this: HTMLElement) {
        const v = this.dataset.value;
        if (!v) return 0;
        return unmeasurable.has(v) ? 0 : BUTTON_W;
      },
    });

    rowWidth = 2 * BUTTON_W + GAP;
    renderRow("d");
    expect(selectorValues("Lens")).toEqual(["a", "b", "c", "d", "e", "f"]);

    unmeasurable.clear();
    await resizeTo(2 * BUTTON_W + GAP);
    expect(selectorValues("Lens")).toEqual(["a", "d", "f"]);
  });

  it("renders the row whole while nothing has been laid out", async () => {
    // A detached tree, a hidden ancestor, or a test environment: measuring zero
    // must not be read as "nothing fits", or a panel would mount with its row
    // stripped to two buttons and only recover on the first resize.
    rowWidth = 0;
    renderRow("a");
    expect(selectorValues("Lens")).toEqual(["a", "b", "c", "d", "e", "f"]);
  });
});

describe("keyboard reach (SNV-FR-62)", () => {
  it("is one tab stop, not one per position", async () => {
    // Eight lens buttons as eight tab stops would put the Project panel's tree
    // seven tabs further away than it was behind the dropdown.
    rowWidth = 1000;
    renderRow("c");

    const stops = screen
      .getAllByRole("radio")
      .filter((b) => b.getAttribute("tabindex") !== "-1");
    expect(stops).toHaveLength(1);
    expect(stops[0]).toHaveAttribute("aria-checked", "true");
  });

  it("takes focus with it when an arrow moves the active position", async () => {
    // The component drives selection from a prop, so the button that had focus
    // is re-rendered as inactive with `tabIndex={-1}`; without the imperative
    // move, focus would land on `<body>` and the next arrow key do nothing.
    rowWidth = 1000;
    function Controlled() {
      const [value, setValue] = useState<P>("b");
      return (
        <SelectorRow
          label="Lens"
          positions={SIX}
          value={value}
          onChange={setValue}
        />
      );
    }
    render(<Controlled />);

    screen.getByRole("radio", { name: "Bravo" }).focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(screen.getByRole("radio", { name: "Charlie" })).toHaveFocus();

    // …and wraps at the trailing edge rather than stopping there.
    await userEvent.keyboard("{ArrowRight}{ArrowRight}{ArrowRight}{ArrowRight}");
    expect(screen.getByRole("radio", { name: "Alpha" })).toHaveFocus();
  });
});

// ---------------------------------------------------------------------------
// The half of SNV-FR-62 and SNV-FR-63 that lives in the stylesheet.
//
// `vitest.config.ts` sets `css: false`, so jsdom loads no stylesheet: a class
// name in the DOM proves nothing about what it renders as. These read the real
// file, the way `src/styles/fontRoles.test.ts` does — without them the tooltip
// could be deleted, or the row given a scrollbar, with the suite still green.
// ---------------------------------------------------------------------------

describe("the stylesheet's half of the contract", () => {
  const css = readStylesheet("kit.css");

  /** The body of one CSS rule, read off the real stylesheet. */
  function cssRule(selector: string): string {
    const match = css.match(
      new RegExp(
        `(^|\\n)\\s*${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`,
      ),
    );
    expect(match, `expected \`${selector} { … }\` in the stylesheet`).not.toBeNull();
    return match![2];
  }

  it("keeps the row to one line with no scrollbar (SNV-FR-63)", () => {
    const row = cssRule(".selector-row");
    // A wrapped row is a second line; an `auto`/`scroll` overflow is a
    // scrollbar. SNV-FR-63, SNV-FR-33 forbids both by name.
    expect(row).toMatch(/flex-wrap\s*:\s*nowrap/);
    expect(row).toMatch(/overflow-x\s*:\s*clip/);
    // Without this the row pushes the panel wider instead of clipping, so
    // nothing is ever clipped and the measurement above never bites.
    expect(row).toMatch(/min-width\s*:\s*0/);
  });

  it("centres the row without letting its leading edge out of the clip", () => {
    const row = cssRule(".selector-row");
    // Plain `center` would move the overflow to *both* edges, so a row too wide
    // for the panel would lose its leading button — the one SNV-FR-63 exempts.
    // `safe` reverts to `start` in that case, keeping the clip at the trailing
    // edge where the requirement puts it.
    expect(row).toMatch(/justify-content\s*:\s*safe\s+center/);
  });

  it("lets the tooltip out of the row it clips (SNV-FR-62)", () => {
    const row = cssRule(".selector-row");
    // The tooltip hangs below its button, so it is *outside* the row's box.
    // `overflow: hidden` clips both axes and would swallow it whole — leaving a
    // shortened tag with no way to read its full name. `hidden` on one axis
    // also forces `auto` on the other, which is the scrollbar SNV-FR-63
    // forbids, so `clip` is the only value that satisfies both requirements.
    expect(row).not.toMatch(/overflow\s*:\s*hidden/);
    expect(row).not.toMatch(/overflow-y\s*:\s*(hidden|auto|scroll)/);
    expect(row).toMatch(/overflow-y\s*:\s*visible/);
    expect(cssRule(".selector-row__tip")).toMatch(/top\s*:\s*calc\(100%/);
  });

  it("hides the tooltip until hover or keyboard focus (SNV-FR-62)", () => {
    // Always in the DOM, so its absence from the resting page is the
    // stylesheet's doing and nothing else's.
    expect(cssRule(".selector-row__tip")).toMatch(/display\s*:\s*none/);

    // …and exactly the two states SNV-FR-62 names bring it back. Asserted on
    // one rule carrying both selectors or on two rules, either way.
    const revealing = [...css.matchAll(/([^{}]*)\{([^}]*)\}/g)].filter(
      ([, sel, body]) =>
        sel.includes(".selector-row__tip") && /display\s*:\s*block/.test(body),
    );
    const selectors = revealing.map(([, sel]) => sel).join(" ");
    expect(selectors).toMatch(/\.selector-row__btn:hover\s+\.selector-row__tip/);
    expect(selectors).toMatch(
      /\.selector-row__btn:focus-visible\s+\.selector-row__tip/,
    );
  });

  it("gives the active button a treatment of its own (SNV-FR-62)", () => {
    // `aria-checked` is what a screen reader reads; `data-active` is the only
    // thing that makes "exactly one active" visible to everyone else.
    expect(css).toMatch(/\.selector-row__btn\[data-active="true"\]\s*\{/);
  });
});
