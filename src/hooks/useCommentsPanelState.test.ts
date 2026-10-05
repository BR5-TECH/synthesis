/**
 * The Comments panel's filter text (`CMP-comments-panel.md` CMP-FR-15).
 *
 * Session memory, deliberately unlike the Notes panel's persisted filter — so
 * what is pinned down here is as much what it does NOT do (no backend
 * round-trip) as what it does.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook } from "@testing-library/react";

import { useCommentsPanelState } from "./useCommentsPanelState";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

afterEach(() => {
  cleanup();
  invokeMock.mockReset();
});

describe("CMP-FR-15: the filter text is session memory", () => {
  it("starts empty, as it does on a fresh launch", () => {
    const { result } = renderHook(() => useCommentsPanelState());
    expect(result.current.text).toBe("");
  });

  it("holds what is typed into it", () => {
    const { result } = renderHook(() => useCommentsPanelState());
    act(() => result.current.setText("onboarding"));
    expect(result.current.text).toBe("onboarding");
  });

  it("persists nothing — neither on mount nor on a change", () => {
    const { result } = renderHook(() => useCommentsPanelState());
    act(() => result.current.setText("onboarding"));
    // No load on mount and no save on change: the text is not project state, so
    // a relaunch starts empty rather than restoring someone's old search.
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("starts empty again on a fresh mount, carrying nothing over", () => {
    const first = renderHook(() => useCommentsPanelState());
    act(() => first.result.current.setText("onboarding"));
    first.unmount();

    const second = renderHook(() => useCommentsPanelState());
    expect(second.result.current.text).toBe("");
  });
});
