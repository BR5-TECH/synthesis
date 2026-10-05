import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { useRef } from "react";

import { useBottomPanel } from "./useBottomPanel";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import { BPANEL_MIN_PX, DEFAULT_BPANEL_PX } from "../state/panelLayout";

/**
 * SNV-shell-navigation.md SNV-FR-50 – SNV-FR-53 (SNV-FR-50, SNV-FR-51,
 * SNV-FR-52, SNV-FR-53).
 */

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const SHELL_HEIGHT = 1000;
/** Client-y of the shell's bottom edge, which the drag measures against. */
const SHELL_BOTTOM = 1000;

let latest: ReturnType<typeof useBottomPanel> | null = null;

function Probe({
  enabled = true,
  projectKey = "/dev/acme",
}: {
  enabled?: boolean;
  projectKey?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  latest = useBottomPanel(ref, enabled, projectKey);
  return (
    <div ref={ref} data-testid="shell">
      <span data-testid="height">{latest.heightPx}</span>
      <span data-testid="dragging">{String(latest.dragging)}</span>
    </div>
  );
}

/** Stub the shell element's measured height — jsdom reports zeroes otherwise. */
function stubShellHeight(height: number) {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width: 1244,
    height,
    left: 0,
    top: 0,
    right: 1244,
    bottom: height,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  } as DOMRect);
}

function savedPayloads(): Record<string, unknown>[] {
  return invokeMock.mock.calls
    .filter((c) => c[0] === "save_layout_preferences")
    .map(
      (c) =>
        (c[1] as { preferences: Record<string, unknown> }).preferences ?? {},
    );
}

/**
 * Drive a full drag: press on the handle, move the pointer to `toClientY`,
 * release. A lower `toClientY` means dragging upward, which grows the panel.
 */
async function drag(toClientY: number, opts: { release?: boolean } = {}) {
  await act(async () => {
    latest!.onResizeStart({ clientY: SHELL_BOTTOM - latest!.heightPx });
  });
  await act(async () => {
    window.dispatchEvent(new MouseEvent("pointermove", { clientY: toClientY }));
  });
  if (opts.release !== false) {
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointerup", {}));
    });
  }
}

let stored: Record<string, unknown> | null;

beforeEach(() => {
  stored = null;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_layout_preferences") return stored;
    return undefined;
  });
  stubShellHeight(SHELL_HEIGHT);
  resetLayoutPreferencesCache();
  latest = null;
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("useBottomPanel — restore (SNV-FR-53)", () => {
  it("renders the default height when nothing is persisted", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    expect(latest!.heightPx).toBe(DEFAULT_BPANEL_PX);
  });

  it("restores the persisted height", async () => {
    stored = { bottomPanelHeight: 420 };
    render(<Probe />);
    await waitFor(() => expect(latest!.heightPx).toBe(420));
  });

  it("clamps a persisted height that no longer fits the shell (SNV-FR-52)", async () => {
    // 600px was set on a taller window; this shell is 400px, so 75% is 300.
    stored = { bottomPanelHeight: 600 };
    stubShellHeight(400);
    render(<Probe />);
    await waitFor(() => expect(latest!.heightPx).toBe(300));
    // The stored value itself is untouched — nothing was written on restore.
    expect(savedPayloads()).toHaveLength(0);
  });

  it("reads nothing while disabled", async () => {
    render(<Probe enabled={false} />);
    await waitFor(() => expect(latest).not.toBeNull());
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_layout_preferences"),
    ).toHaveLength(0);
  });
});

describe("useBottomPanel — drag (SNV-FR-50 / SNV-FR-53)", () => {
  it("grows the panel as the pointer moves up and shrinks it as it moves down", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest!.heightPx).toBe(DEFAULT_BPANEL_PX));

    await drag(SHELL_BOTTOM - 500, { release: false });
    expect(latest!.heightPx).toBe(500);
    expect(latest!.dragging).toBe(true);

    await act(async () => {
      window.dispatchEvent(
        new MouseEvent("pointermove", { clientY: SHELL_BOTTOM - 200 }),
      );
    });
    expect(latest!.heightPx).toBe(200);

    await act(async () => void window.dispatchEvent(new MouseEvent("pointerup")));
    expect(latest!.dragging).toBe(false);
  });

  it("persists once, on release, and not during the drag (SNV-FR-53)", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest!.heightPx).toBe(DEFAULT_BPANEL_PX));

    await drag(SHELL_BOTTOM - 460, { release: false });
    expect(savedPayloads()).toHaveLength(0);

    await act(async () => void window.dispatchEvent(new MouseEvent("pointerup")));
    await waitFor(() => expect(savedPayloads()).toHaveLength(1));
    expect(savedPayloads()[0]).toMatchObject({ bottomPanelHeight: 460 });
  });

  it("merges into the record rather than replacing it", async () => {
    // The same record carries the vertical panel's fraction and the window
    // geometry; a drag here must not drop them.
    stored = { verticalPanelFraction: 0.3, mainWindowMaximized: true };
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());

    await drag(SHELL_BOTTOM - 400);
    await waitFor(() => expect(savedPayloads()).toHaveLength(1));
    expect(savedPayloads()[0]).toEqual({
      verticalPanelFraction: 0.3,
      mainWindowMaximized: true,
      bottomPanelHeight: 400,
    });
  });
});

describe("useBottomPanel — clamps during a drag (SNV-FR-51)", () => {
  it("stops at 75% of the shell's height however far the pointer goes", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());

    // Drag well past the top of the shell.
    await drag(-500, { release: false });
    expect(latest!.heightPx).toBe(750);

    await act(async () => void window.dispatchEvent(new MouseEvent("pointerup")));
    // What was persisted is what was rendered — never an out-of-bounds value.
    await waitFor(() => expect(savedPayloads()).toHaveLength(1));
    expect(savedPayloads()[0]).toMatchObject({ bottomPanelHeight: 750 });
  });

  it("stops at the 120px floor, and persists the clamped value", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());

    await drag(SHELL_BOTTOM + 300);
    expect(latest!.heightPx).toBe(BPANEL_MIN_PX);
    // What is written is what was rendered — never the raw pointer distance.
    await waitFor(() => expect(savedPayloads()).toHaveLength(1));
    expect(savedPayloads()[0]).toMatchObject({
      bottomPanelHeight: BPANEL_MIN_PX,
    });
  });
});

describe("useBottomPanel — project switch (OVW-FR-11)", () => {
  it("re-reads the height when the open project changes", async () => {
    // A switch opens a different slot. Serving A's height would render B at
    // A's height AND write A's layout into B's slot.
    stored = { bottomPanelHeight: 300 };
    const view = render(<Probe projectKey="/dev/a" />);
    await waitFor(() => expect(latest!.heightPx).toBe(300));

    stored = { bottomPanelHeight: 500 };
    resetLayoutPreferencesCache();
    view.rerender(<Probe projectKey="/dev/b" />);

    await waitFor(() => expect(latest!.heightPx).toBe(500));
  });

  it("falls back to the default when the new project has no persisted height", async () => {
    stored = { bottomPanelHeight: 500 };
    const view = render(<Probe projectKey="/dev/a" />);
    await waitFor(() => expect(latest!.heightPx).toBe(500));

    stored = null;
    resetLayoutPreferencesCache();
    view.rerender(<Probe projectKey="/dev/b" />);

    await waitFor(() => expect(latest!.heightPx).toBe(DEFAULT_BPANEL_PX));
  });
});

describe("useBottomPanel — a write that fails", () => {
  it("keeps the dragged height for the session", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_layout_preferences") throw new Error("disk full");
      if (cmd === "load_layout_preferences") return stored;
      return undefined;
    });

    await drag(SHELL_BOTTOM - 500);

    // The panel stays where the user dropped it; the next successful save
    // carries the same value.
    expect(latest!.heightPx).toBe(500);
  });
});

describe("useBottomPanel — window resize (SNV-FR-52)", () => {
  it("re-clamps on resize and returns to the stored height when the room comes back", async () => {
    stored = { bottomPanelHeight: 600 };
    render(<Probe />);
    await waitFor(() => expect(latest!.heightPx).toBe(600));

    stubShellHeight(400);
    await act(async () => void window.dispatchEvent(new Event("resize")));
    expect(latest!.heightPx).toBe(300);

    stubShellHeight(SHELL_HEIGHT);
    await act(async () => void window.dispatchEvent(new Event("resize")));
    expect(latest!.heightPx).toBe(600);
    // Re-clamping is presentational: it never rewrote what was stored.
    expect(savedPayloads()).toHaveLength(0);
  });
});

describe("useBottomPanel — drag teardown", () => {
  it("does not write after the shell unmounts mid-drag", async () => {
    // A pointerup landing after a project teardown would write this project's
    // height into whichever slot the backend has since moved to.
    const { unmount } = render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    await drag(SHELL_BOTTOM - 500, { release: false });

    unmount();
    await act(async () => void window.dispatchEvent(new MouseEvent("pointerup")));

    expect(savedPayloads()).toHaveLength(0);
  });

  it("ignores a second pointerdown while a drag is already in flight", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());

    await drag(SHELL_BOTTOM - 500, { release: false });
    await act(async () => {
      latest!.onResizeStart({ clientY: SHELL_BOTTOM - 500 });
    });
    await act(async () => void window.dispatchEvent(new MouseEvent("pointerup")));

    // One drag, one write — not two sets of listeners each firing their own.
    await waitFor(() => expect(savedPayloads()).toHaveLength(1));
  });
});
