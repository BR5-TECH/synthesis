/**
 * The timeout bar (`CVP-conversation-presentation.md` CVP-FR-IJEY,
 * CVP-FR-XZTZ, CVP-FR-YBXQ, CVP-FR-UYHO).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";

import { TimeoutBar, TIMEOUT_BAR_TICK_MS } from "./TimeoutBar";

const start = Date.UTC(2026, 1, 1);

beforeEach(() => {
  vi.useFakeTimers({ now: start });
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const fill = () => screen.getByTestId("timeout-bar").firstElementChild as HTMLElement;

describe("CVP-FR-YBXQ: the progress bar's role and values", () => {
  it("is a named progress bar with its range and value, and no live region", () => {
    render(<TimeoutBar deadline={{ startedMs: start, endsMs: start + 300_000 }} />);
    const bar = screen.getByRole("progressbar", { name: "Time left for the agent turn" });
    expect(bar).toHaveAttribute("aria-valuemin", "0");
    expect(bar).toHaveAttribute("aria-valuemax", "100");
    expect(bar).toHaveAttribute("aria-valuenow", "100");
    expect(bar).not.toHaveAttribute("aria-live");
    expect(bar.closest("[aria-live]")).toBeNull();
  });
});

describe("CVP-FR-IJEY / CVP-FR-XZTZ: the bar empties on the local clock", () => {
  it("goes down from 100% at least once each second, and a new deadline fills it at once", () => {
    const { rerender } = render(
      <TimeoutBar deadline={{ startedMs: start, endsMs: start + 10_000 }} />,
    );
    expect(fill().style.width).toBe("100%");
    act(() => {
      vi.advanceTimersByTime(5_000);
    });
    expect(fill().style.width).toBe("50%");
    expect(TIMEOUT_BAR_TICK_MS).toBeLessThanOrEqual(1_000);
    act(() => {
      vi.advanceTimersByTime(20_000);
    });
    expect(fill().style.width).toBe("0%");
    const now = Date.now();
    rerender(<TimeoutBar deadline={{ startedMs: now, endsMs: now + 10_000 }} />);
    expect(fill().style.width).toBe("100%");
  });
});

describe("CVP-FR-UYHO: the bar blocks nothing", () => {
  it("takes no focus", () => {
    render(<TimeoutBar deadline={{ startedMs: start, endsMs: start + 1_000 }} />);
    const bar = screen.getByRole("progressbar");
    expect(bar).not.toHaveAttribute("tabindex");
    expect(bar.querySelector("button, a, input, [tabindex]")).toBeNull();
  });
});
