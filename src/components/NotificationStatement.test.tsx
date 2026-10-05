import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";

import {
  NotificationStatement,
  TOP_CHROME_HEIGHT_PX,
} from "./NotificationStatement";
import { readFileSync } from "node:fs";
import { readStylesheet } from "../test/readStylesheet";

/**
 * The statement an activation leaves when its address cannot be reached
 * (`NTF-notifications.md` NTF-FR-20 / NTF-FR-21), covering NTF-FR-20 and the
 * pass-through half of NTF-FR-21, SNV-FR-56.
 */

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  // `globals` is off in vitest.config.ts, so RTL registers no auto-cleanup —
  // without this the previous render's DOM stays mounted and every
  // `getByTestId` finds two.
  cleanup();
  vi.useRealTimers();
});

describe("the statement (NTF-FR-20)", () => {
  it("renders nothing at all when there is nothing to say", () => {
    render(<NotificationStatement message={null} onDismiss={() => {}} />);
    expect(screen.queryByTestId("notification-statement")).toBeNull();
  });

  it("shows the message with a dismissal and nothing else", () => {
    render(
      <NotificationStatement
        message="That artifact is no longer in the project."
        onDismiss={() => {}}
      />,
    );
    expect(
      screen.getByText("That artifact is no longer in the project."),
    ).toBeInTheDocument();
    // "offering nothing but a dismissal": exactly one control.
    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(1);
    expect(buttons[0]).toHaveAttribute("aria-label", "Dismiss");
  });

  it("is dismissed by its own control", () => {
    const onDismiss = vi.fn();
    render(<NotificationStatement message="Nope." onDismiss={onDismiss} />);
    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("is dismissed by Escape, even from a surface that stops propagation", () => {
    // The listener is registered in the capture phase deliberately: the
    // statement takes no focus, so a bubble-phase listener would be beaten by
    // any surface that calls stopPropagation on Escape — and the author would
    // be left with a statement they cannot dismiss with the key that dismisses
    // everything else in the window.
    const onDismiss = vi.fn();
    render(<NotificationStatement message="Nope." onDismiss={onDismiss} />);

    const greedy = document.createElement("div");
    document.body.appendChild(greedy);
    greedy.addEventListener("keydown", (e) => e.stopPropagation());

    fireEvent.keyDown(greedy, { key: "Escape" });
    expect(onDismiss).toHaveBeenCalledTimes(1);
    greedy.remove();
  });

  it("ignores keys that are not Escape", () => {
    const onDismiss = vi.fn();
    render(<NotificationStatement message="Nope." onDismiss={onDismiss} />);
    fireEvent.keyDown(window, { key: "Enter" });
    fireEvent.keyDown(window, { key: "a" });
    expect(onDismiss).not.toHaveBeenCalled();
  });

  it("goes on its own after a short interval", () => {
    const onDismiss = vi.fn();
    render(<NotificationStatement message="Nope." onDismiss={onDismiss} />);
    expect(onDismiss).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(6000);
    });
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("restarts its clock when a second statement replaces the first", () => {
    // NTF-FR-20: a second statement replaces the one showing rather than
    // stacking beside it — and must not inherit the remainder of the first
    // one's timer, which would flash it away almost immediately.
    const onDismiss = vi.fn();
    const { rerender } = render(
      <NotificationStatement message="First." onDismiss={onDismiss} />,
    );
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(onDismiss).not.toHaveBeenCalled();

    rerender(<NotificationStatement message="Second." onDismiss={onDismiss} />);
    expect(screen.getByText("Second.")).toBeInTheDocument();
    expect(screen.queryByText("First.")).toBeNull();

    // The first one's remaining 1s must not fire the second one away.
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(onDismiss).not.toHaveBeenCalled();
    act(() => {
      vi.advanceTimersByTime(4500);
    });
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });

  it("stops its timer when it unmounts", () => {
    // A dismissed statement whose timer still ran would call back into a shell
    // that has moved on.
    const onDismiss = vi.fn();
    const { unmount } = render(
      <NotificationStatement message="Nope." onDismiss={onDismiss} />,
    );
    unmount();
    act(() => {
      vi.advanceTimersByTime(10000);
    });
    expect(onDismiss).not.toHaveBeenCalled();
  });
});

describe("the statement is not an overlay (NTF-FR-21)", () => {
  it("never intercepts a pointer event except on its dismissal", () => {
    // NTF-FR-21, SNV-FR-56: the container is pointer-transparent so a click reaches
    // whatever lies beneath it, and only the dismissal takes one back. This is
    // what makes "never blocks a click" a property of the layout rather than a
    // promise, and what keeps the statement outside the single-overlay rule
    // (SNV-FR-56).
    render(<NotificationStatement message="Nope." onDismiss={() => {}} />);
    const container = screen.getByTestId("notification-statement");
    expect(container.style.pointerEvents).toBe("none");

    const dismiss = screen.getByRole("button", { name: "Dismiss" });
    expect(dismiss.style.pointerEvents).toBe("auto");
  });

  it("announces politely rather than interrupting a screen reader", () => {
    // It is a consequence of the author's own click, not an emergency, so it
    // is `status`/polite rather than `alert`/assertive.
    render(<NotificationStatement message="Nope." onDismiss={() => {}} />);
    const container = screen.getByTestId("notification-statement");
    expect(container).toHaveAttribute("role", "status");
    expect(container).toHaveAttribute("aria-live", "polite");
  });

  it("anchors to the viewport below the top chrome, not over it", () => {
    // The defect this pins: the statement is rendered at the App root, which
    // has no positioned ancestor, so `position: absolute` resolved against the
    // BODY and put the box on top of the top chrome — opaquely covering the
    // theme selector and the agents control. Only `fixed` anchors it to the
    // viewport the way "below the top chrome" (NTF-FR-20) describes.
    render(<NotificationStatement message="Nope." onDismiss={() => {}} />);
    const container = screen.getByTestId("notification-statement");
    expect(container.style.position).toBe("fixed");
    expect(parseInt(container.style.top, 10)).toBeGreaterThanOrEqual(
      TOP_CHROME_HEIGHT_PX,
    );
  });

  it("keeps its chrome-height constant in step with the stylesheet", () => {
    // The constant is duplicated from CSS because the statement must be
    // positioned before first paint. If the shell's grid row changes and this
    // does not, the statement drifts back over the chrome.
    const css = readStylesheet("kit.css");
    expect(css).toContain(`grid-template-rows: ${TOP_CHROME_HEIGHT_PX}px 1fr auto`);
  });

  it("bounds itself so its sentence wraps rather than running the full width", () => {
    render(
      <NotificationStatement
        message={"A ".repeat(200)}
        onDismiss={() => {}}
      />,
    );
    const container = screen.getByTestId("notification-statement");
    expect(container.style.maxWidth).toBeTruthy();
    expect(container.style.maxWidth).not.toBe("100%");
  });
});
