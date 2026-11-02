import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { NotificationToasts } from "./NotificationToasts";
import { resetLogBufferForTest } from "../logging";
import { downgradeRaise, SUPPRESS_FAILED_KEY } from "../state/modelDowngrades";
import { resetToasts, showToast, visibleToasts } from "../state/toasts";
import type { ModelDowngrade } from "../types";

/**
 * The model downgrade toast and its Suppress menu (`NTF-notifications.md`
 * NTF-FR-JXQL, NTF-FR-PHXY, NTF-FR-LHVU, NTF-FR-RPZS, NTF-FR-BVCG).
 */

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const KEY = "model-downgrade:claude-opus-5-5→claude-opus-4-8";

const event: ModelDowngrade = {
  source: "agent",
  requestedModel: "claude-opus-5-5",
  servedModel: "claude-opus-4-8",
  provider: "anthropic",
  projectKey: "/dev/acme",
  worktree: "/dev/acme",
  origin: { discussionId: "d1", target: { kind: "draft", draftId: "dr1" } },
  runId: null,
};

let saveFails = false;

function showDowngrade() {
  const raise = downgradeRaise(event, { projectKey: "/dev/acme", worktree: "/dev/acme" })!;
  act(() =>
    showToast({
      key: raise.key,
      level: raise.level,
      title: raise.title,
      body: raise.body,
      address: raise.address,
      menu: raise.menu,
      dismissLabel: raise.dismissLabel,
      keepOnArrival: true,
    }),
  );
}

const renderStack = () => {
  const onActivate = vi.fn();
  render(<NotificationToasts onActivate={onActivate} />);
  return onActivate;
};

const control = () => screen.getByTestId("notification-toast-menu-control");
const saves = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_model_downgrade_suppression")
    .map((c) => (c[1] as { duration: string }).duration);

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  saveFails = false;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "save_model_downgrade_suppression") {
      if (saveFails) throw new Error("store unavailable");
      return { ...args, expiresAt: null };
    }
    throw new Error(`unexpected command ${cmd}`);
  });
});

afterEach(() => {
  cleanup();
});

describe("the model downgrade toast", () => {
  it("NTF-FR-AGTB, NTF-FR-DGLS, NTF-FR-OPCD: a Warn toast with the level label, the alert role, and the Suppress control after the body", () => {
    renderStack();
    showDowngrade();
    const toast = screen.getByTestId("notification-toast");
    expect(toast).toHaveAttribute("data-level", "Warn");
    expect(toast).toHaveAttribute("role", "alert");
    expect(within(toast).getByTestId("notification-toast-level")).toHaveTextContent("Warn");
    expect(toast).toHaveTextContent("Requested claude-opus-5-5, served claude-opus-4-8.");
    expect(control()).toHaveAttribute("aria-haspopup", "menu");
    expect(control()).toHaveAttribute("aria-expanded", "false");
    expect(control()).toHaveTextContent("Suppress");
  });

  it("NTF-FR-BVCG: the toast takes no focus when it appears", () => {
    renderStack();
    const before = document.activeElement;
    showDowngrade();
    expect(document.activeElement).toBe(before);
  });

  it("NTF-FR-RPZS, NTF-FR-QEHM: Acknowledge removes only that toast and saves nothing", async () => {
    const onActivate = renderStack();
    showDowngrade();
    act(() =>
      showToast({ key: "other", level: "Warn", title: "Other", body: "", address: null }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Acknowledge" }));
    expect(visibleToasts().map((t) => t.key)).toEqual(["other"]);
    expect(saves()).toEqual([]);
    expect(onActivate).not.toHaveBeenCalled();
  });

  it("NTF-FR-RUBT: the Warn toast does not go away by itself", () => {
    vi.useFakeTimers();
    try {
      renderStack();
      showDowngrade();
      act(() => {
        vi.advanceTimersByTime(60_000);
      });
      expect(screen.getByTestId("notification-toast")).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("NTF-FR-FNXO: a click on the body opens the target", async () => {
    const onActivate = renderStack();
    showDowngrade();
    const address = visibleToasts()[0].address;
    await userEvent.click(screen.getByTestId("notification-toast-target"));
    expect(onActivate).toHaveBeenCalledWith(address, KEY);
  });
});

describe("the Suppress menu", () => {
  it.each([
    ["1 hour", "hour"],
    ["1 day", "day"],
    ["1 week", "week"],
    ["Forever", "forever"],
  ])(
    "NTF-FR-JXQL, NTF-FR-LHVU: picking %s saves that duration and removes the toast without routing",
    async (label, duration) => {
      const onActivate = renderStack();
      showDowngrade();
      await userEvent.click(control());
      await userEvent.click(screen.getByRole("menuitem", { name: label }));
      await waitFor(() => expect(screen.queryByTestId("notification-toast")).toBeNull());
      expect(saves()).toEqual([duration]);
      expect(onActivate).not.toHaveBeenCalled();
    },
  );

  it("NTF-FR-JXQL: the menu lists the four durations in order, none pre-selected, and opening it does not route", async () => {
    const onActivate = renderStack();
    showDowngrade();
    await userEvent.click(control());
    const menu = screen.getByRole("menu");
    expect(within(menu).getAllByRole("menuitem").map((i) => i.textContent)).toEqual([
      "1 hour",
      "1 day",
      "1 week",
      "Forever",
    ]);
    expect(control()).toHaveAttribute("aria-expanded", "true");
    expect(document.activeElement).toBe(menu);
    expect(onActivate).not.toHaveBeenCalled();
    expect(saves()).toEqual([]);
  });

  it("NTF-FR-PHXY: Arrow keys move between the items and wrap, and Enter picks the focused one", async () => {
    renderStack();
    showDowngrade();
    control().focus();
    await userEvent.keyboard("{Enter}");
    expect(document.activeElement).toBe(screen.getByRole("menu"));
    await userEvent.keyboard("{ArrowDown}");
    expect(document.activeElement).toHaveTextContent("1 hour");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(document.activeElement).toHaveTextContent("1 week");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(document.activeElement).toHaveTextContent("1 hour");
    await userEvent.keyboard("{ArrowUp}");
    expect(document.activeElement).toHaveTextContent("Forever");
    await userEvent.keyboard("{ArrowUp}{Enter}");
    await waitFor(() => expect(screen.queryByTestId("notification-toast")).toBeNull());
    expect(saves()).toEqual(["week"]);
  });

  it("NTF-FR-PHXY: Escape closes the menu and returns focus to the control", async () => {
    renderStack();
    showDowngrade();
    control().focus();
    await userEvent.keyboard("{Enter}{ArrowDown}{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(control());
    expect(control()).toHaveAttribute("aria-expanded", "false");
    expect(saves()).toEqual([]);
  });

  it("NTF-FR-PHXY: a click outside the menu closes it", async () => {
    renderStack();
    showDowngrade();
    await userEvent.click(control());
    await userEvent.click(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByTestId("notification-toast")).toBeInTheDocument();
  });

  it("NTF-FR-LHVU: a failed save keeps the toast and shows an Error toast", async () => {
    saveFails = true;
    renderStack();
    showDowngrade();
    await userEvent.click(control());
    await userEvent.click(screen.getByRole("menuitem", { name: "1 day" }));
    const error = await screen.findByText("Suppression not saved");
    expect(error.closest("[data-testid='notification-toast']")).toHaveAttribute(
      "data-key",
      SUPPRESS_FAILED_KEY,
    );
    expect(visibleToasts().map((t) => t.key)).toEqual([SUPPRESS_FAILED_KEY, KEY]);
    expect(screen.queryByRole("menu")).toBeNull();
    // NTF-FR-PHXY: the keyboard author keeps a place to work from.
    await waitFor(() => expect(document.activeElement).toBe(control()));
  });
});
