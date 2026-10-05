import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { TabStrip } from "./TabStrip";
import type { Tab } from "../types";

afterEach(cleanup);

const noop = () => {};

function strip(tabs: Tab[]) {
  return render(
    <TabStrip
      tabs={tabs}
      activeId={tabs[0]?.id ?? ""}
      onActivate={noop}
      onClose={noop}
      onHome={noop}
    />,
  );
}

/** The tabs' own tooltips — SNV-FR-09's Home affordance is a control of the
 *  strip rather than a tab, so it is not one of them. */
const tabTitles = () =>
  Array.from(
    document.querySelectorAll(".tabstrip .tab:not([data-testid='home-affordance'])"),
  ).map((n) => n.getAttribute("title"));

describe("TabStrip tooltips (DFV-FR-03)", () => {
  it("tells two Diff tabs on one file apart by their tooltips", () => {
    // DFV-FR-02: both labels name only the file, deliberately — so the tooltip
    // is the only thing in the strip that distinguishes the comparisons.
    strip([
      {
        id: "diff:uncommitted:src/App.tsx",
        label: "Diff: App.tsx",
        tooltip: "Diff: src/App.tsx — uncommitted",
        kind: "diff",
      },
      {
        id: "diff:branch:main:src/App.tsx",
        label: "Diff: App.tsx",
        tooltip: "Diff: src/App.tsx — against main",
        kind: "diff",
      },
    ]);

    expect(screen.getAllByText("Diff: App.tsx")).toHaveLength(2);
    expect(tabTitles()).toEqual([
      "Diff: src/App.tsx — uncommitted",
      "Diff: src/App.tsx — against main",
    ]);
  });

  it("falls back to the label when a tab carries no tooltip", () => {
    strip([{ id: "art:1", label: "App.tsx", kind: "editor" }]);
    expect(tabTitles()).toEqual(["App.tsx"]);
  });
});

describe("TabStrip close control", () => {
  const renderStrip = (tabs: Tab[]) => {
    const onClose = vi.fn();
    const onActivate = vi.fn();
    render(
      <TabStrip
        tabs={tabs}
        activeId={tabs[0].id}
        onActivate={onActivate}
        onClose={onClose}
        onHome={noop}
      />,
    );
    return { onClose, onActivate };
  };

  const DASHBOARD: Tab = { id: "dashboard", label: "Dashboard" };
  const EDITOR: Tab = { id: "art:1", label: "App.tsx", kind: "editor" };

  it("closes the tab it belongs to without activating it", () => {
    const { onClose, onActivate } = renderStrip([DASHBOARD, EDITOR]);

    screen.getByTestId("close-dashboard").click();

    expect(onClose).toHaveBeenCalledWith("dashboard");
    expect(onActivate).not.toHaveBeenCalled();
  });

  // TAB-FR-16: the Dashboard alone in the strip. The control is still rendered
  // — it is not the tab's shape that changes, only its enablement — but it is
  // marked disabled and requests nothing, so the strip cannot be emptied.
  it("renders the Dashboard's close control inert while it is the only tab", () => {
    const { onClose, onActivate } = renderStrip([DASHBOARD]);

    const close = screen.getByTestId("close-dashboard");
    expect(close).toHaveAttribute("aria-disabled", "true");
    expect(close).toHaveClass("tab__close--disabled");
    expect(close).toHaveAttribute(
      "title",
      "Dashboard stays open while it is the only tab",
    );

    close.click();

    expect(onClose).not.toHaveBeenCalled();
    expect(onActivate).not.toHaveBeenCalled();
  });

  // The condition is the Dashboard's alone: any other tab standing by itself
  // closes on request, and TAB-FR-15 is what puts the Dashboard back.
  it("keeps a lone non-Dashboard tab's close control live", () => {
    const { onClose } = renderStrip([EDITOR]);

    const close = screen.getByTestId("close-art:1");
    expect(close).not.toHaveAttribute("aria-disabled");

    close.click();

    expect(onClose).toHaveBeenCalledWith("art:1");
  });
});

// ---------------------------------------------------------------------------
// The tabs sit in a scrolling region (TAB-FR-27), so the active tab has to be
// brought into view — a tab the user has just opened sitting off-screen with
// nothing pointing at it is indistinguishable from it not having opened.
// ---------------------------------------------------------------------------

describe("keeping the active tab in view", () => {
  let scrolled: HTMLElement[];

  beforeEach(() => {
    scrolled = [];
    // jsdom does not implement `scrollIntoView` AT ALL — which is why the
    // component calls it optionally. `vi.spyOn` therefore cannot be used (it
    // requires an existing property), so the stub is installed by assignment
    // and removed again below rather than left on the prototype for the next
    // file to inherit.
    (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView =
      function (this: HTMLElement) {
        scrolled.push(this);
      };
  });

  afterEach(() => {
    delete (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView;
  });

  /** The element of the most recent `scrollIntoView` call. */
  const lastScrolled = () => scrolled[scrolled.length - 1];

  const tab = (id: string): Tab => ({ id, label: `tab ${id}`, kind: "editor" });

  const renderAt = (tabs: Tab[], activeId: string) => (
    <TabStrip
      tabs={tabs}
      activeId={activeId}
      onActivate={noop}
      onClose={noop}
      onHome={noop}
    />
  );

  it("scrolls the active tab into view when the active tab changes", () => {
    const tabs = [tab("a"), tab("b"), tab("c")];
    const view = render(renderAt(tabs, "a"));
    expect(lastScrolled()).toHaveAttribute("data-active", "true");
    expect(lastScrolled()).toHaveTextContent("tab a");

    view.rerender(renderAt(tabs, "c"));
    expect(lastScrolled()).toHaveTextContent("tab c");
  });

  it("scrolls the newly opened tab into view when one is added", () => {
    const view = render(renderAt([tab("a")], "a"));
    const before = scrolled.length;

    // Opening a tab makes it active — the case where the new tab lands past the
    // strip's visible width and nothing points at it.
    view.rerender(renderAt([tab("a"), tab("b")], "b"));

    expect(scrolled.length).toBeGreaterThan(before);
    expect(lastScrolled()).toHaveTextContent("tab b");
  });

  it("does nothing when the active id names no rendered tab", () => {
    // Reachable between a close and the shell settling on the next active tab.
    // The effect must not throw, and must scroll nothing.
    expect(() => render(renderAt([tab("a")], "gone"))).not.toThrow();
    expect(scrolled).toHaveLength(0);
  });
});

/**
 * NTF-FR-34 / NTF-FR-36: how a tab needing attention is drawn, and what it says
 * to a reader who cannot see the drawing.
 *
 * The store's own lifecycle is covered in `state/tabIndications.test.ts`; what
 * is checked here is the rendering contract that lifecycle drives — that the
 * mark is present, that it is distinguishable from the dirty indicator by more
 * than colour, that it costs the strip no layout, and that the state reaches
 * assistive technology.
 */
describe("needs-attention emphasis (NTF-FR-34 / NTF-FR-36)", () => {
  const marked = (
    tabs: Tab[],
    attention: Map<string, "pulse" | "on">,
    activeId = tabs[0]?.id ?? "",
  ) =>
    render(
      <TabStrip
        tabs={tabs}
        activeId={activeId}
        onActivate={noop}
        onClose={noop}
        onHome={noop}
        attention={attention}
      />,
    );

  const TABS: Tab[] = [
    { id: "dashboard", label: "Dashboard" },
    { id: "art:a.md", label: "a.md", kind: "editor", artifactId: "a.md" },
    { id: "art:b.md", label: "b.md", kind: "editor", artifactId: "b.md" },
  ];

  const tabEl = (id: string) =>
    document.querySelector(`.tabstrip .tab [data-testid="close-${id}"]`)
      ?.parentElement as HTMLElement;

  // NTF-FR-08, NTF-FR-26, NTF-FR-37 / NTF-FR-32, at the rendering end.
  it("marks only the tabs it is told to, and never the active one", () => {
    marked(TABS, new Map([["art:b.md", "on"]]), "dashboard");

    expect(tabEl("art:b.md").getAttribute("data-attention")).toBe("on");
    expect(tabEl("art:a.md").getAttribute("data-attention")).toBeNull();
    expect(tabEl("dashboard").getAttribute("data-attention")).toBeNull();
  });

  it("refuses to mark the active tab even when told to", () => {
    // The strip is the last frame before the author sees it, and a tab becoming
    // active and its indication clearing are one frame apart (NTF-FR-29).
    marked(TABS, new Map([["art:a.md", "on"]]), "art:a.md");
    expect(tabEl("art:a.md").getAttribute("data-attention")).toBeNull();
  });

  // NTF-FR-35: the two states the emphasis has.
  it("distinguishes the arrival pulse from the settled emphasis", () => {
    marked(
      TABS,
      new Map<string, "pulse" | "on">([
        ["art:a.md", "pulse"],
        ["art:b.md", "on"],
      ]),
      "dashboard",
    );
    expect(tabEl("art:a.md").getAttribute("data-attention")).toBe("pulse");
    expect(tabEl("art:b.md").getAttribute("data-attention")).toBe("on");
  });

  // NTF-FR-34: the mark's space is reserved on every tab whether it is showing
  // or not, so a tab taking or losing an indication shifts nothing in the strip.
  it("reserves the mark's space on every tab, marked or not", () => {
    marked(TABS, new Map([["art:b.md", "on"]]), "dashboard");
    for (const t of TABS) {
      expect(tabEl(t.id).querySelectorAll(".tab__attention")).toHaveLength(1);
    }
  });

  // NTF-FR-34: the mark and the dirty indicator stand on one tab at once and
  // are separate elements, so neither is read as the other.
  it("stands beside the dirty indicator rather than replacing it", () => {
    marked(
      [
        { id: "dashboard", label: "Dashboard" },
        {
          id: "art:a.md",
          label: "a.md",
          kind: "editor",
          artifactId: "a.md",
          dirty: true,
        },
      ],
      new Map([["art:a.md", "on"]]),
      "dashboard",
    );
    const tab = tabEl("art:a.md");
    expect(tab.querySelector(".tab__attention")).not.toBeNull();
    expect(tab.querySelector(".tab__dirty")).not.toBeNull();
    // The mark leads and the dirty indicator follows the label, so they are
    // told apart by position as well as by shape.
    const marks = Array.from(tab.children);
    expect(marks.findIndex((n) => n.classList.contains("tab__attention"))).toBe(0);
    expect(
      marks.findIndex((n) => n.classList.contains("tab__label")),
    ).toBeLessThan(marks.findIndex((n) => n.classList.contains("tab__dirty")));
  });

  // NTF-FR-35, NTF-FR-34, NTF-FR-36: the state itself, not the treatment, is what reaches a screen
  // reader.
  it("exposes a needs-attention state on the marked tab alone", () => {
    marked(TABS, new Map([["art:b.md", "on"]]), "dashboard");
    expect(tabEl("art:b.md").textContent).toContain("needs attention");
    expect(tabEl("art:a.md").textContent).not.toContain("needs attention");
  });

  // NTF-FR-36: taking and losing are each announced, once per change.
  it("announces a tab taking the state and losing it, once per change", () => {
    const { rerender } = marked(TABS, new Map(), "dashboard");
    const announcer = () =>
      document.querySelector('[role="status"]')?.textContent ?? "";
    expect(announcer()).toBe("");

    const render2 = (attention: Map<string, "pulse" | "on">) =>
      rerender(
        <TabStrip
          tabs={TABS}
          activeId="dashboard"
          onActivate={noop}
          onClose={noop}
          onHome={noop}
          attention={attention}
        />,
      );

    render2(new Map([["art:b.md", "on"]]));
    expect(announcer()).toBe("b.md needs attention");

    // A re-render that changed no tab's attention must not repeat itself — the
    // pulse settling to static is exactly such a re-render.
    render2(new Map([["art:b.md", "pulse"]]));
    expect(announcer()).toBe("b.md needs attention");

    render2(new Map([["art:b.md", "on"], ["art:a.md", "on"]]));
    expect(announcer()).toBe("a.md needs attention");

    render2(new Map([["art:a.md", "on"]]));
    expect(announcer()).toBe("b.md no longer needs attention");
  });

  // Two tabs marked at once are one announcement rather than two.
  it("announces several tabs together", () => {
    const { rerender } = marked(TABS, new Map(), "dashboard");
    rerender(
      <TabStrip
        tabs={TABS}
        activeId="dashboard"
        onActivate={noop}
        onClose={noop}
        onHome={noop}
        attention={
          new Map<string, "pulse" | "on">([
            ["art:a.md", "on"],
            ["art:b.md", "on"],
          ])
        }
      />,
    );
    expect(document.querySelector('[role="status"]')?.textContent).toBe(
      "a.md, b.md need attention",
    );
  });

  // A tab that left the strip is not announced as no longer needing attention:
  // the author closed it, and a tab that is gone is not news (NTF-FR-33).
  it("says nothing when a marked tab closes", () => {
    const { rerender } = marked(TABS, new Map([["art:b.md", "on"]]), "dashboard");
    const announcer = () =>
      document.querySelector('[role="status"]')?.textContent ?? "";
    expect(announcer()).toBe("b.md needs attention");

    rerender(
      <TabStrip
        tabs={TABS.filter((t) => t.id !== "art:b.md")}
        activeId="dashboard"
        onActivate={noop}
        onClose={noop}
        onHome={noop}
        attention={new Map()}
      />,
    );
    expect(announcer()).toBe("b.md needs attention");
  });

  /**
   * NTF-FR-27, TAB-FR-06 at the rendering end: an Editor tab and a Diff tab on one file
   * both sit in the strip, and only the Editor tab carries the emphasis.
   *
   * The store keys by address, and both tabs would produce the same address —
   * so which of them is marked can only be seen here, where the mapping onto
   * tab ids has already happened.
   */
  it("marks the editing tab, not a Diff or History tab on the same file", () => {
    const tabs: Tab[] = [
      { id: "dashboard", label: "Dashboard" },
      { id: "art:a.md", label: "a.md", kind: "editor", artifactId: "a.md" },
      { id: "diff:uncommitted:a.md", label: "a.md", kind: "diff" },
      { id: "hist:a.md@abc", label: "a.md" },
    ];
    marked(tabs, new Map([["art:a.md", "on"]]), "dashboard");

    expect(tabEl("art:a.md").getAttribute("data-attention")).toBe("on");
    expect(
      tabEl("diff:uncommitted:a.md").getAttribute("data-attention"),
    ).toBeNull();
    expect(tabEl("hist:a.md@abc").getAttribute("data-attention")).toBeNull();
    // And only one tab in the whole strip is marked.
    expect(
      document.querySelectorAll(".tabstrip .tab[data-attention]"),
    ).toHaveLength(1);
  });

  // NTF-FR-28: the Home affordance is a control of the strip rather than a
  // tab, so it never carries an emphasis whatever the Dashboard's target says.
  it("never marks the Home affordance", () => {
    marked(
      [{ id: "art:a.md", label: "a.md", kind: "editor", artifactId: "a.md" }],
      new Map([["dashboard", "on"]]),
      "art:a.md",
    );
    const home = screen.getByTestId("home-affordance");
    expect(home.getAttribute("data-attention")).toBeNull();
    expect(
      document.querySelectorAll(".tabstrip .tab[data-attention]"),
    ).toHaveLength(0);
  });

  // The scrolling region's children stay tabs: every reader of it — the
  // scroll-into-view query and the `tablist` role included — is entitled to
  // assume so, and the announcer is not one of them.
  it("keeps the announcer out of the scrolling region", () => {
    marked(TABS, new Map([["art:b.md", "on"]]), "dashboard");
    const children = Array.from(
      screen.getByTestId("tabstrip-scroll").children,
    );
    expect(children).toHaveLength(TABS.length);
    expect(children.every((n) => n.classList.contains("tab"))).toBe(true);
    expect(children.every((n) => n.getAttribute("role") === "tab")).toBe(true);
  });
});

describe("a tab's parts, in order (TAB-FR-29, TAB-FR-35)", () => {
  /** The parts of a tab that take part in its flex layout, in DOM order.
   *  The two marks in the leading gutter are laid out of flow, so they are not
   *  among them — `../styles/tabTrailingEdge.test.ts` is what holds them there. */
  const inFlow = (tab: Element) =>
    Array.from(tab.children)
      .filter(
        (n) =>
          !n.classList.contains("tab__attention") &&
          !n.classList.contains("tab__pin") &&
          !n.classList.contains("sr-only"),
      )
      .map((n) =>
        n.classList.contains("tab__label")
          ? "label"
          : n.classList.contains("tab__dirty")
            ? "dirty"
            : n.classList.contains("tab__close")
              ? "close"
              : "glyph",
      );

  const tabEl = (id: string) =>
    document.querySelector(`.tabstrip .tab[data-tab-id="${id}"]`)!;

  it("puts the close control last, after the label and the dirty indicator", () => {
    // The order is what makes the trailing edge reachable: the free space in a
    // floored tab goes to the margin ahead of the close control, so everything
    // before it stays put and the control alone moves to the edge. A close
    // control that stood before the label would take the label with it.
    strip([{ id: "art:a.md", label: "a.md", kind: "editor", dirty: true }]);
    expect(inFlow(tabEl("art:a.md"))).toEqual([
      "glyph",
      "label",
      "dirty",
      "close",
    ]);
  });

  it("keeps that order on a clean tab and on the Dashboard", () => {
    strip([
      { id: "dashboard", label: "Dashboard" },
      { id: "art:b.md", label: "b.md", kind: "editor" },
    ]);
    expect(inFlow(tabEl("dashboard"))).toEqual(["glyph", "label", "close"]);
    expect(inFlow(tabEl("art:b.md"))).toEqual(["glyph", "label", "close"]);
  });

  it("renders the label even when it is empty or one character", () => {
    // The label is the element that gives width when the tab is too full
    // (TAB-FR-29). A future `{t.label && <span…>}` would drop it on an empty
    // label, leaving nothing to truncate and no test failing for it.
    strip([
      { id: "art:empty", label: "", kind: "editor" },
      { id: "art:one", label: "a", kind: "editor" },
    ]);
    expect(inFlow(tabEl("art:empty"))).toEqual(["glyph", "label", "close"]);
    expect(inFlow(tabEl("art:one"))).toEqual(["glyph", "label", "close"]);
  });

  it("keeps a type chip in the glyph slot beside the label, not around it", () => {
    strip([
      { id: "art:c.md", label: "c.md", kind: "editor", chip: "SPEC", type: "spec" },
    ]);
    const tab = tabEl("art:c.md");
    expect(inFlow(tab)).toEqual(["glyph", "label", "close"]);
    expect(tab.querySelector(".chip-type")!.querySelector(".tab__label")).toBeNull();
  });

  it("holds a chip and a dirty indicator at once", () => {
    // The widest a tab's in-flow row gets, and so the case where the label has
    // least room before it must truncate (TAB-FR-29).
    strip([
      {
        id: "art:f.md",
        label: "f.md",
        kind: "editor",
        chip: "SPEC",
        type: "spec",
        dirty: true,
      },
    ]);
    expect(inFlow(tabEl("art:f.md"))).toEqual([
      "glyph",
      "label",
      "dirty",
      "close",
    ]);
  });

  it("adds no in-flow part when a tab is pinned or needs attention", () => {
    // TAB-FR-35 / NTF-FR-34: both marks are reserved on every tab, so taking
    // one changes neither the child order nor what the flex layout holds.
    // NTF-FR-32: the active tab is never marked, so the marked tab is not the
    // active one here.
    const TABS: Tab[] = [
      { id: "dashboard", label: "Dashboard" },
      { id: "art:d.md", label: "d.md", kind: "editor" },
    ];
    strip(TABS);
    const plain = inFlow(tabEl("art:d.md"));
    cleanup();
    render(
      <TabStrip
        tabs={TABS}
        activeId="dashboard"
        attention={new Map([["art:d.md", "on"]])}
        pinned={new Set(["art:d.md"])}
        onActivate={noop}
        onClose={noop}
        onHome={noop}
      />,
    );
    const marked = tabEl("art:d.md");
    // Both marks render unconditionally, so what proves the state took is the
    // tab's own attributes rather than the marks being present.
    expect(marked.getAttribute("data-pinned")).not.toBeNull();
    expect(marked.getAttribute("data-attention")).toBe("on");
    expect(inFlow(marked)).toEqual(plain);
    const parts = inFlow(marked);
    expect(parts[parts.length - 1]).toBe("close");
  });

  it("gives the Home affordance no label and no close control", () => {
    // SNV-FR-09: Home is a control of the strip rather than a tab. It carries
    // `.tab`, so the tab layout rules reach it; it holds one icon and must not
    // be stretched by them.
    strip([{ id: "art:e.md", label: "e.md", kind: "editor" }]);
    const home = screen.getByTestId("home-affordance");
    expect(home.classList.contains("tab")).toBe(true);
    expect(home.querySelector(".tab__label")).toBeNull();
    expect(home.querySelector(".tab__close")).toBeNull();
    expect(home.children).toHaveLength(1);
  });

  it("keeps the inert Dashboard close control on the trailing edge (TAB-FR-16)", () => {
    // Greyed-out and inert, but still the last in-flow part: a disabled close
    // holds the trailing edge like any other.
    strip([{ id: "dashboard", label: "Dashboard" }]);
    const close = screen.getByTestId("close-dashboard");
    expect(close.classList.contains("tab__close--disabled")).toBe(true);
    const parts = inFlow(tabEl("dashboard"));
    expect(parts[parts.length - 1]).toBe("close");
  });
});
