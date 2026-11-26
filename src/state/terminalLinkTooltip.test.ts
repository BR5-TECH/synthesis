// The tooltip of a terminal link (`TRM-terminals.md` TRM-FR-KIRO, TRM-FR-DCBR).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { linkHint, newLinkTooltip, type LinkTooltip } from "./terminalLinkTooltip";

const URL_TEXT = "https://github.com/acme/app/pull/7";

describe("the terminal link tooltip", () => {
  let host: HTMLDivElement;
  let tooltip: LinkTooltip | null;
  let width: ReturnType<typeof vi.spyOn>;
  let height: ReturnType<typeof vi.spyOn>;

  const tip = () => document.querySelector<HTMLElement>(".terminal-link-tooltip");
  const move = (x: number, y: number) => host.dispatchEvent(new MouseEvent("mousemove", { clientX: x, clientY: y }));
  const make = (mac = true) => (tooltip = newLinkTooltip(host, mac));

  beforeEach(() => {
    host = document.createElement("div");
    document.body.appendChild(host);
    tooltip = null;
    vi.stubGlobal("innerWidth", 800);
    vi.stubGlobal("innerHeight", 600);
    width = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(200);
    height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(40);
  });
  afterEach(() => {
    tooltip?.dispose();
    host.remove();
    width.mockRestore();
    height.mockRestore();
    vi.unstubAllGlobals();
  });

  it("TRM-FR-KIRO: the hint names the click of the platform", () => {
    expect(linkHint(true)).toBe("⌘-click to open");
    expect(linkHint(false)).toBe("Ctrl+click to open");
  });

  it("TRM-FR-KIRO, TRM-FR-DCBR: shows the address and the hint under and after the pointer, and takes no focus", () => {
    const button = document.createElement("button");
    document.body.appendChild(button);
    button.focus();
    make(true);
    move(100, 100);
    tooltip!.show(URL_TEXT);
    const element = tip()!;
    expect(element.parentElement).toBe(document.body);
    expect(element).toHaveAttribute("role", "tooltip");
    expect(element.querySelector(".terminal-link-tooltip__address")).toHaveTextContent(URL_TEXT);
    expect(element.querySelector(".terminal-link-tooltip__hint")).toHaveTextContent("⌘-click to open");
    expect(element.style.left).toBe("114px");
    expect(element.style.top).toBe("114px");
    expect(document.activeElement).toBe(button);
    button.remove();
  });

  it("TRM-FR-KIRO: the tooltip stays inside the window at the right and bottom edges", () => {
    make();
    move(780, 590);
    tooltip!.show(URL_TEXT);
    expect(tip()!.style.left).toBe(`${800 - 200 - 4}px`);
    expect(tip()!.style.top).toBe(`${590 - 14 - 40}px`);
  });

  it("TRM-FR-KIRO: the tooltip keeps a gap from the top and left edges where the window is small", () => {
    vi.stubGlobal("innerWidth", 100);
    vi.stubGlobal("innerHeight", 30);
    make();
    move(2, 2);
    tooltip!.show(URL_TEXT);
    expect(tip()!.style.left).toBe("4px");
    expect(tip()!.style.top).toBe("4px");
  });

  it("TRM-FR-KIRO: the tooltip follows the pointer while it shows, and one element is used", () => {
    make();
    move(10, 10);
    tooltip!.show(URL_TEXT);
    move(50, 60);
    expect(tip()!.style.left).toBe("64px");
    expect(tip()!.style.top).toBe("74px");
    tooltip!.show("https://x.test/other");
    expect(document.querySelectorAll(".terminal-link-tooltip")).toHaveLength(1);
    expect(tip()).toHaveTextContent("https://x.test/other");
  });

  it("TRM-FR-KIRO: a link that shows before the first pointer move shows its tooltip at that move", () => {
    make();
    tooltip!.show(URL_TEXT);
    expect(tip()).toBeNull();
    move(20, 30);
    expect(tip()).toHaveTextContent(URL_TEXT);
    expect(tip()!.style.left).toBe("34px");
  });

  it("TRM-FR-KIRO: the tooltip gets the pointer before a later listener on the host, which the library adds", () => {
    make();
    host.addEventListener("mousemove", () => tooltip!.show(URL_TEXT));
    move(20, 30);
    expect(tip()!.style.top).toBe("44px");
  });

  it("TRM-FR-DCBR: a hide before the first pointer move drops the waiting tooltip", () => {
    make();
    tooltip!.show(URL_TEXT);
    tooltip!.hide();
    move(20, 30);
    expect(tip()).toBeNull();
  });

  it("TRM-FR-KIRO: the tooltip does not show for a host out of the page", () => {
    make();
    move(1, 1);
    host.remove();
    tooltip!.show(URL_TEXT);
    expect(tip()).toBeNull();
  });

  it("TRM-FR-DCBR: the tooltip closes on hide and when the pointer leaves the terminal; hide is safe with no tooltip", () => {
    make();
    tooltip!.hide();
    move(1, 1);
    tooltip!.show(URL_TEXT);
    tooltip!.hide();
    expect(tip()).toBeNull();
    tooltip!.show(URL_TEXT);
    host.dispatchEvent(new MouseEvent("mouseleave"));
    expect(tip()).toBeNull();
  });

  it("TRM-FR-DCBR: dispose removes the tooltip and its listeners, and a later show does nothing", () => {
    const removed = vi.spyOn(host, "removeEventListener");
    make();
    move(1, 1);
    tooltip!.show(URL_TEXT);
    tooltip!.dispose();
    expect(tip()).toBeNull();
    expect(removed.mock.calls.map((c) => c[0]).sort()).toEqual(["mouseleave", "mousemove"]);
    move(300, 300);
    tooltip!.show(URL_TEXT);
    expect(tip()).toBeNull();
  });
});
