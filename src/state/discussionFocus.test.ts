/**
 * The one-instance registry of discussion surfaces
 * (`CVP-conversation-presentation.md` CVP-FR-02, CVP-FR-05, CVP-FR-42).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  consumePendingFocus,
  focusDiscussion,
  registerSurface,
  requestPendingFocus,
  resetDiscussionFocus,
  surfaceCount,
  surfaceOwner,
} from "./discussionFocus";

beforeEach(() => resetDiscussionFocus());

describe("discussion focus registry", () => {
  it("CVP-FR-02: a discussion no surface renders is closed", () => {
    expect(focusDiscussion("d1")).toBe("closed");
    expect(surfaceOwner("d1")).toBeNull();
  });

  it("CVP-FR-06: focusing a discussion focuses the surface that shows it", () => {
    const focus = vi.fn();
    registerSurface("d1", "rail", focus);
    expect(focusDiscussion("d1")).toBe("focused");
    expect(focus).toHaveBeenCalledWith("discussion");
    expect(focusDiscussion("d1", "composer")).toBe("focused");
    expect(focus).toHaveBeenLastCalledWith("composer");
  });

  it("CVP-FR-05: a second surface of one discussion waits and is never active together with the first", () => {
    const first = registerSurface("d1", "rail", vi.fn());
    const second = registerSurface("d1", "tab", vi.fn());
    expect(first.isActive()).toBe(true);
    expect(second.isActive()).toBe(false);
    expect(surfaceCount("d1")).toBe(2);
    expect(surfaceOwner("d1")).toBe("rail");
  });

  it("CVP-FR-05: focusing reaches only the active surface", () => {
    const railFocus = vi.fn();
    const tabFocus = vi.fn();
    registerSurface("d1", "rail", railFocus);
    registerSurface("d1", "tab", tabFocus);
    focusDiscussion("d1");
    expect(railFocus).toHaveBeenCalledTimes(1);
    expect(tabFocus).not.toHaveBeenCalled();
  });

  it("CVP-FR-05: when the active surface goes the waiting one takes over", () => {
    const first = registerSurface("d1", "rail", vi.fn());
    const second = registerSurface("d1", "tab", vi.fn());
    first.release();
    expect(second.isActive()).toBe(true);
    expect(surfaceOwner("d1")).toBe("tab");
  });

  it("CVP-FR-05: releasing the last surface closes the discussion", () => {
    const only = registerSurface("d1", "rail", vi.fn());
    only.release();
    only.release();
    expect(focusDiscussion("d1")).toBe("closed");
    expect(surfaceCount("d1")).toBe(0);
  });

  it("CVP-FR-02: two discussions never affect each other", () => {
    registerSurface("d1", "rail", vi.fn());
    const other = registerSurface("d2", "rail", vi.fn());
    expect(other.isActive()).toBe(true);
  });

  it("CVP-FR-42: a focus request for a surface that mounts later is held and consumed once", () => {
    expect(consumePendingFocus("d1")).toBeNull();
    requestPendingFocus("d1", "composer");
    expect(consumePendingFocus("d1")).toBe("composer");
    expect(consumePendingFocus("d1")).toBeNull();
  });
});
