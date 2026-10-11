import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";

import { NotificationToasts, TOP_CHROME_HEIGHT_PX } from "./NotificationToasts";
import { readStylesheet } from "../test/readStylesheet";
import {
  INFO_TOAST_MS,
  resetToasts,
  showToast,
  visibleToasts,
  type ToastInput,
} from "../state/toasts";
import {
  indicatedAddresses,
  markIndication,
  resetTabIndications,
} from "../state/tabIndications";

/**
 * The toast stack (`NTF-notifications.md` NTF-FR-DGLS through NTF-FR-OPCD,
 * NTF-FR-20, NTF-FR-21, NTF-FR-LAWP).
 */

const ADDRESS = "synthesis://%2Fdev%2Facme/%2Fdev%2Facme/run/r1";

const input = (key: string, over: Partial<ToastInput> = {}): ToastInput => ({
  key,
  level: "Info",
  title: `Title ${key}`,
  body: `Body ${key}`,
  address: ADDRESS,
  ...over,
});

const show = (...toasts: ToastInput[]) =>
  act(() => {
    for (const t of toasts) showToast(t);
  });

const advance = (ms: number) =>
  act(() => {
    vi.advanceTimersByTime(ms);
  });

const renderStack = (onActivate = vi.fn()) => {
  render(<NotificationToasts onActivate={onActivate} />);
  return onActivate;
};

const titles = () =>
  screen.queryAllByTestId("notification-toast").map(
    (el) => within(el).getByTestId("notification-toast-target").textContent,
  );

beforeEach(() => {
  vi.useFakeTimers();
  resetToasts();
  resetTabIndications();
});

afterEach(() => {
  // `globals` is off in vitest.config.ts, so RTL registers no auto-cleanup.
  cleanup();
  resetToasts();
  vi.useRealTimers();
});

describe("what a toast shows (NTF-FR-DGLS)", () => {
  it("renders nothing while there is nothing to show", () => {
    renderStack();
    expect(screen.queryByTestId("notification-toast-stack")).toBeNull();
  });

  it("NTF-FR-DGLS: shows the level as an icon with a text label, the title, the body, and a dismissal", () => {
    renderStack();
    show(input("a", { level: "Error" }));
    const toast = screen.getByTestId("notification-toast");
    expect(within(toast).getByTestId("notification-toast-level")).toHaveTextContent(
      "Error",
    );
    expect(toast.querySelector("svg")).not.toBeNull();
    expect(within(toast).getByText("Title a")).toBeInTheDocument();
    expect(within(toast).getByText("Body a")).toBeInTheDocument();
    expect(
      within(toast).getByRole("button", { name: "Dismiss" }),
    ).toBeInTheDocument();
  });

  it("NTF-FR-DGLS: names each level in words", () => {
    renderStack();
    show(
      input("a", { level: "Info" }),
      input("b", { level: "Warn" }),
      input("c", { level: "Error" }),
    );
    const labels = screen
      .getAllByTestId("notification-toast-level")
      .map((el) => el.textContent);
    expect(labels).toEqual(["Error", "Warn", "Info"]);
  });

  it("NTF-FR-DGLS: shows no project name and no body line for an empty body", () => {
    renderStack();
    show(input("a", { body: "" }));
    const target = screen.getByTestId("notification-toast-target");
    expect(target.textContent).toBe("InfoTitle a");
  });
});

describe("the Info timer (NTF-FR-YJAE) and the persistence of Warn and Error (NTF-FR-RUBT)", () => {
  it("NTF-FR-YJAE: an Info toast goes away by itself after about 5 s", () => {
    renderStack();
    show(input("a"));
    advance(INFO_TOAST_MS - 1);
    expect(titles()).toHaveLength(1);
    advance(1);
    expect(titles()).toHaveLength(0);
  });

  it("NTF-FR-RUBT: a Warn toast and an Error toast stay however long they show", () => {
    renderStack();
    show(input("w", { level: "Warn" }), input("e", { level: "Error" }));
    advance(10 * 60 * 1000);
    expect(titles()).toHaveLength(2);
  });

  it("NTF-FR-YJAE: the timer pauses while the pointer is over the toast and resumes with the time that remained", () => {
    renderStack();
    show(input("a"));
    advance(3000);
    const toast = screen.getByTestId("notification-toast");
    fireEvent.pointerEnter(toast);
    advance(60_000);
    expect(titles()).toHaveLength(1);
    fireEvent.pointerLeave(toast);
    advance(INFO_TOAST_MS - 3000 - 1);
    expect(titles()).toHaveLength(1);
    advance(1);
    expect(titles()).toHaveLength(0);
  });

  it("NTF-FR-YJAE: the timer pauses while keyboard focus is inside the toast", () => {
    renderStack();
    show(input("a"));
    const dismiss = screen.getByRole("button", { name: "Dismiss" });
    act(() => dismiss.focus());
    advance(60_000);
    expect(titles()).toHaveLength(1);
    act(() => dismiss.blur());
    advance(INFO_TOAST_MS);
    expect(titles()).toHaveLength(0);
  });

  it("NTF-FR-YJAE: focus moving between the two controls of a toast does not resume the timer", () => {
    renderStack();
    show(input("a"));
    const target = screen.getByTestId("notification-toast-target");
    const dismiss = screen.getByRole("button", { name: "Dismiss" });
    act(() => target.focus());
    fireEvent.blur(target, { relatedTarget: dismiss });
    act(() => dismiss.focus());
    advance(60_000);
    expect(titles()).toHaveLength(1);
  });

  it("NTF-FR-HZNF: a replacement restarts the Info timer", () => {
    renderStack();
    show(input("a"));
    advance(4000);
    show(input("a", { title: "Again" }));
    advance(4000);
    expect(titles()).toHaveLength(1);
    expect(screen.getByText("Again")).toBeInTheDocument();
    advance(1000);
    expect(titles()).toHaveLength(0);
  });

  it("NTF-FR-HZNF: a Warn toast replaced by an Info toast of its key starts the timer", () => {
    renderStack();
    show(input("a", { level: "Warn" }));
    advance(60_000);
    show(input("a", { level: "Info" }));
    advance(INFO_TOAST_MS);
    expect(titles()).toHaveLength(0);
  });

  it("NTF-FR-YJAE: stops the timer when the stack unmounts", () => {
    const onActivate = vi.fn();
    const { unmount } = render(<NotificationToasts onActivate={onActivate} />);
    show(input("a"));
    unmount();
    advance(60_000);
    expect(visibleToasts()).toHaveLength(1);
  });
});

describe("the stack (NTF-FR-HZNF, NTF-FR-SXTI)", () => {
  it("NTF-FR-HZNF: shows the newest toast on top and at most three", () => {
    renderStack();
    show(
      input("a", { level: "Warn" }),
      input("b", { level: "Warn" }),
      input("c", { level: "Warn" }),
      input("d", { level: "Warn" }),
    );
    expect(titles().map((t) => t?.match(/Title (\w)/)?.[1])).toEqual([
      "c",
      "b",
      "a",
    ]);
  });

  it("NTF-FR-SXTI: a queued toast shows when a slot frees", () => {
    renderStack();
    show(
      input("a", { level: "Warn" }),
      input("b", { level: "Warn" }),
      input("c", { level: "Warn" }),
      input("d", { level: "Warn" }),
    );
    fireEvent.click(
      within(screen.getAllByTestId("notification-toast")[2]).getByRole(
        "button",
        { name: "Dismiss" },
      ),
    );
    expect(titles().map((t) => t?.match(/Title (\w)/)?.[1])).toEqual([
      "d",
      "c",
      "b",
    ]);
  });

  it("NTF-FR-HZNF: a replacement keeps its place in the stack", () => {
    renderStack();
    show(
      input("a", { level: "Warn" }),
      input("b", { level: "Warn" }),
      input("a", { level: "Warn", title: "A2" }),
    );
    const order = screen
      .getAllByTestId("notification-toast")
      .map((el) => el.getAttribute("data-key"));
    expect(order).toEqual(["b", "a"]);
    expect(screen.getByText("A2")).toBeInTheDocument();
  });
});

describe("a click and the dismissal (NTF-FR-FNXO, NTF-FR-QEHM)", () => {
  it("NTF-FR-FNXO: a click on the toast routes its address and removes it", () => {
    const onActivate = renderStack();
    show(input("a"), input("b"));
    fireEvent.click(
      within(
        screen
          .getAllByTestId("notification-toast")
          .find((el) => el.getAttribute("data-key") === "a") as HTMLElement,
      ).getByTestId("notification-toast-target"),
    );
    expect(onActivate).toHaveBeenCalledTimes(1);
    expect(onActivate).toHaveBeenCalledWith(ADDRESS, "a");
    expect(
      screen.getAllByTestId("notification-toast").map((e) => e.getAttribute("data-key")),
    ).toEqual(["b"]);
  });

  it("NTF-FR-FNXO: a click routes the address of a draft proposal and of a prompt artifact proposal as they are", () => {
    const onActivate = renderStack();
    const draft = "synthesis://%2Fp/%2Fp/draft/d1";
    const file = "synthesis://%2Fp/%2Fp/file/a.md";
    show(input("d", { address: draft }), input("f", { address: file }));
    for (const el of screen.getAllByTestId("notification-toast-target")) {
      fireEvent.click(el);
    }
    expect(onActivate.mock.calls.map((c) => c[0]).sort()).toEqual(
      [draft, file].sort(),
    );
  });

  it("NTF-FR-QEHM: the dismissal removes only its toast, routes nothing, and clears no tab indication", () => {
    const onActivate = renderStack();
    markIndication("a", ADDRESS);
    show(input("a"), input("b"));
    const a = screen
      .getAllByTestId("notification-toast")
      .find((el) => el.getAttribute("data-key") === "a") as HTMLElement;
    fireEvent.click(within(a).getByRole("button", { name: "Dismiss" }));
    expect(onActivate).not.toHaveBeenCalled();
    expect(visibleToasts().map((t) => t.key)).toEqual(["b"]);
    expect(indicatedAddresses()).toEqual([ADDRESS]);
  });

  it("NTF-FR-20: an answer shown by the activation takes the slot the click frees, ahead of queued toasts", () => {
    const onActivate = vi.fn(() =>
      showToast(input("unreachable-address", { level: "Warn", address: null }), {
        priority: true,
      }),
    );
    renderStack(onActivate);
    show(
      input("a", { level: "Warn" }),
      input("b", { level: "Warn" }),
      input("c", { level: "Warn" }),
      input("d", { level: "Warn" }),
    );
    fireEvent.click(
      screen
        .getAllByTestId("notification-toast")
        .find((el) => el.getAttribute("data-key") === "a")!
        .querySelector("[data-testid=notification-toast-target]")!,
    );
    expect(visibleToasts().map((t) => t.key)).toEqual([
      "unreachable-address",
      "c",
      "b",
    ]);
  });

  it("NTF-FR-20: a click on a toast without an address only dismisses it", () => {
    const onActivate = renderStack();
    show(
      input("unreachable-address", {
        level: "Warn",
        address: null,
        body: "",
      }),
    );
    fireEvent.click(screen.getByTestId("notification-toast-target"));
    expect(onActivate).not.toHaveBeenCalled();
    expect(titles()).toHaveLength(0);
  });
});

describe("accessibility (NTF-FR-BVCG, NTF-FR-OPCD)", () => {
  it("NTF-FR-OPCD: Info is a polite status; Warn and Error are assertive alerts", () => {
    renderStack();
    show(
      input("i", { level: "Info" }),
      input("w", { level: "Warn" }),
      input("e", { level: "Error" }),
    );
    const role = (key: string) =>
      screen
        .getAllByTestId("notification-toast")
        .find((el) => el.getAttribute("data-key") === key)
        ?.getAttribute("role");
    expect(role("i")).toBe("status");
    expect(role("w")).toBe("alert");
    expect(role("e")).toBe("alert");
  });

  it("NTF-FR-BVCG: a toast takes no focus when it appears", () => {
    renderStack();
    const field = document.createElement("textarea");
    document.body.appendChild(field);
    field.focus();
    show(input("a"), input("b", { level: "Error" }));
    expect(document.activeElement).toBe(field);
    field.remove();
  });

  it("NTF-FR-BVCG: dismissing a toast from the keyboard moves focus to a neighbouring toast", () => {
    renderStack();
    show(input("a", { level: "Warn" }), input("b", { level: "Warn" }));
    const [top, bottom] = screen.getAllByTestId("notification-toast");
    const dismiss = within(top).getByRole("button", { name: "Dismiss" });
    act(() => dismiss.focus());
    fireEvent.click(dismiss);
    expect(document.activeElement).toBe(
      within(bottom).getByTestId("notification-toast-target"),
    );
  });

  it("NTF-FR-BVCG: the click target and the dismissal are buttons in the tab order", () => {
    renderStack();
    show(input("a"));
    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(2);
    for (const b of buttons) {
      expect(b.tagName).toBe("BUTTON");
      expect(b.getAttribute("tabindex")).not.toBe("-1");
    }
  });

  it("NTF-FR-OPCD: the entry motion is off under reduced motion", () => {
    const css = readStylesheet("components.css");
    const guard = css.match(
      /@media \(prefers-reduced-motion: reduce\)\s*\{\s*\.ntf-toast\s*\{[^}]*animation:\s*none/,
    );
    expect(guard).not.toBeNull();
  });
});

describe("layout (NTF-FR-LAWP, NTF-FR-21, NTF-FR-BVCG)", () => {
  it("NTF-FR-LAWP: anchors to the viewport's top-trailing corner below the top chrome", () => {
    renderStack();
    show(input("a"));
    const stack = screen.getByTestId("notification-toast-stack");
    expect(stack.style.position).toBe("fixed");
    expect(parseInt(stack.style.top, 10)).toBeGreaterThanOrEqual(
      TOP_CHROME_HEIGHT_PX,
    );
    expect(stack.style.right).not.toBe("");
  });

  it("NTF-FR-LAWP: keeps its chrome-height constant in step with the stylesheet", () => {
    const css = readStylesheet("kit.css");
    expect(css).toContain(
      `grid-template-rows: ${TOP_CHROME_HEIGHT_PX}px 1fr auto`,
    );
  });

  it("NTF-FR-BVCG, NTF-FR-21: the region intercepts no pointer event; only the toasts do", () => {
    renderStack();
    show(input("a"));
    expect(screen.getByTestId("notification-toast-stack").style.pointerEvents).toBe(
      "none",
    );
    expect(screen.getByTestId("notification-toast").style.pointerEvents).toBe(
      "auto",
    );
  });

  it("NTF-FR-LAWP: is opaque and bounded in width so text wraps", () => {
    renderStack();
    show(input("a", { body: "word ".repeat(200) }));
    const stack = screen.getByTestId("notification-toast-stack");
    expect(stack.style.width).toBe("360px");
    expect(stack.style.maxWidth).toBe("40vw");
    expect(screen.getByTestId("notification-toast").style.background).toContain(
      "var(--bg-elevated",
    );
  });
});
