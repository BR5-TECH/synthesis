import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { useRef } from "react";

import { useVerticalPanel } from "./useVerticalPanel";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import {
  DEFAULT_VPANEL_FRACTION,
  VPANEL_MIN_PX,
} from "../state/panelLayout";

/**
 * SNV-shell-navigation.md SNV-FR-33 – SNV-FR-37 (SNV-FR-36, SNV-FR-34,
 * SNV-FR-35, SNV-FR-37).
 */

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const SHELL_WIDTH = 1244; // inner width 1200 once the 44px activity bar is off.
const INNER = 1200;
/** Client-x of the panel's left edge: the shell's left plus the activity bar. */
const PANEL_LEFT = 44;

let latest: ReturnType<typeof useVerticalPanel> | null = null;

function Probe({
  enabled = true,
  projectKey = "/dev/acme",
}: {
  enabled?: boolean;
  projectKey?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  latest = useVerticalPanel(ref, enabled, projectKey);
  return (
    <div ref={ref} data-testid="shell">
      <span data-testid="width">{latest.widthPx}</span>
      <span data-testid="dragging">{String(latest.dragging)}</span>
      <span data-testid="resizable">{String(latest.resizable)}</span>
    </div>
  );
}

/** Stub the shell element's measured width — jsdom reports zeroes otherwise. */
function stubShellWidth(width: number) {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width,
    height: 800,
    left: 0,
    top: 0,
    right: width,
    bottom: 800,
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

/** Drive a full drag: press on the handle, move to `toClientX`, release. */
async function drag(toClientX: number, opts: { release?: boolean } = {}) {
  await act(async () => {
    latest!.onResizeStart({ clientX: PANEL_LEFT + latest!.widthPx });
  });
  await act(async () => {
    window.dispatchEvent(
      new MouseEvent("pointermove", { clientX: toClientX }),
    );
  });
  if (opts.release !== false) {
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointerup", {}));
    });
  }
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_layout_preferences") return null;
    return undefined;
  });
  stubShellWidth(SHELL_WIDTH);
  resetLayoutPreferencesCache();
  latest = null;
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("useVerticalPanel — restore (SNV-FR-34 / SNV-FR-36)", () => {
  it("renders the default fraction when nothing is persisted", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    expect(latest!.widthPx).toBeCloseTo(DEFAULT_VPANEL_FRACTION * INNER, 5);
  });

  it("restores the persisted fraction, resolved against the live shell", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.3 };
      return undefined;
    });

    render(<Probe />);
    await waitFor(() => expect(latest!.widthPx).toBeCloseTo(0.3 * INNER, 5));
  });

  it("ignores a never-persisted (0) fraction rather than collapsing the panel", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0 };
      return undefined;
    });

    render(<Probe />);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("load_layout_preferences"),
    );
    expect(latest!.widthPx).toBeCloseTo(DEFAULT_VPANEL_FRACTION * INNER, 5);
  });

  it("does not load anything while disabled (picker screen)", async () => {
    render(<Probe enabled={false} />);
    await Promise.resolve();
    await Promise.resolve();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("reads the persisted fraction under its byte-for-byte wire name", async () => {
    // A field named differently from the Rust struct is not a deserialization
    // error — it is silently dropped. Pin the name that actually round-trips.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { vpanelWidth: 900, verticalPanelFraction: 0.4 };
      return undefined;
    });
    render(<Probe />);
    await waitFor(() => expect(latest!.widthPx).toBeCloseTo(0.4 * INNER, 5));
  });
});

describe("useVerticalPanel — drag (SNV-FR-33, SNV-FR-36)", () => {
  it("moves the boundary live and persists once, on release", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());

    await act(async () => {
      latest!.onResizeStart({ clientX: PANEL_LEFT + latest!.widthPx });
    });
    expect(latest!.dragging).toBe(true);

    // Two moves mid-drag: the width follows the pointer...
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 344 }));
    });
    expect(latest!.widthPx).toBeCloseTo(300, 5);
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 444 }));
    });
    expect(latest!.widthPx).toBeCloseTo(400, 5);

    // ...and nothing has been written yet (SNV-FR-36: on release, not per frame).
    expect(savedPayloads()).toHaveLength(0);

    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointerup", {}));
    });

    expect(latest!.dragging).toBe(false);
    const payloads = savedPayloads();
    expect(payloads).toHaveLength(1);
    expect(payloads[0].verticalPanelFraction).toBeCloseTo(400 / INNER, 5);
  });

  it("stops listening after release, so a stray move does not resize", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    await drag(444);
    const settled = latest!.widthPx;

    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 900 }));
    });
    expect(latest!.widthPx).toBe(settled);
    expect(savedPayloads()).toHaveLength(1);
  });

  it("treats a cancelled drag as a release, so nothing is left listening", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());

    await act(async () => {
      latest!.onResizeStart({ clientX: PANEL_LEFT + latest!.widthPx });
    });
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 444 }));
    });
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointercancel", {}));
    });

    expect(latest!.dragging).toBe(false);
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 900 }));
    });
    expect(latest!.widthPx).toBeCloseTo(400, 5);
  });

  it("preserves the project's other layout fields on save", async () => {
    // A panel drag must not drop the window geometry `useMainWindowState`
    // persists into the same record.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return {
          verticalPanelFraction: 0.25,
          verticalPanelSide: "right",
          bottomPanelHeight: 200,
          mainWindowOuterWidth: 1600,
          mainWindowOuterHeight: 1000,
          mainWindowMaximized: false,
        };
      return undefined;
    });

    render(<Probe />);
    await waitFor(() => expect(latest!.widthPx).toBeCloseTo(300, 5));
    await drag(444);

    const payload = savedPayloads()[0];
    expect(payload.verticalPanelFraction).toBeCloseTo(400 / INNER, 5);
    expect(payload.verticalPanelSide).toBe("right");
    expect(payload.bottomPanelHeight).toBe(200);
    expect(payload.mainWindowOuterWidth).toBe(1600);
    expect(payload.mainWindowOuterHeight).toBe(1000);
    expect(payload.mainWindowMaximized).toBe(false);
  });

  it("sends the payload under the named `preferences` arg", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    await drag(444);

    const call = invokeMock.mock.calls.find(
      (c) => c[0] === "save_layout_preferences",
    );
    const keys = Object.keys(call![1] as Record<string, unknown>);
    expect(keys).toContain("preferences");
    expect(keys).not.toContain("verticalPanelFraction");
  });

  it("keeps the dragged width for the session when the write fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") return null;
      if (cmd === "save_layout_preferences") throw new Error("disk full");
      return undefined;
    });

    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    await drag(444);

    expect(latest!.widthPx).toBeCloseTo(400, 5);
    expect(latest!.dragging).toBe(false);
  });
});

describe("useVerticalPanel — clamps (SNV-FR-34)", () => {
  it("stops at 50% of the shell's inner width", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    await drag(PANEL_LEFT + 5000);
    expect(latest!.widthPx).toBe(INNER / 2);
    expect(savedPayloads()[0].verticalPanelFraction).toBeCloseTo(0.5, 5);
  });

  it("stops at the 180px floor", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    await drag(PANEL_LEFT - 400);
    expect(latest!.widthPx).toBe(VPANEL_MIN_PX);
  });

  it("clamps a persisted fraction that is out of bounds for this shell", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.9 };
      return undefined;
    });
    render(<Probe />);
    await waitFor(() => expect(latest!.widthPx).toBe(INNER / 2));
  });
});

describe("useVerticalPanel — proportion under window resize (SNV-FR-35)", () => {
  it("keeps the same share of the shell when the window widens", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.25 };
      return undefined;
    });

    render(<Probe />);
    await waitFor(() => expect(latest!.widthPx).toBeCloseTo(300, 5));

    // The window widens: inner goes 1200 -> 2000.
    stubShellWidth(2044);
    await act(async () => {
      window.dispatchEvent(new Event("resize"));
    });

    expect(latest!.widthPx).toBeCloseTo(500, 5);
    // Same proportion, larger window — the viewport absorbed the difference.
    expect(latest!.widthPx / 2000).toBeCloseTo(0.25, 5);
  });

  it("does not persist anything on a window resize", async () => {
    // The fraction did not change, so there is nothing to write — and writing
    // here would fight `useMainWindowState`, which owns the geometry record on
    // the same event.
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());

    stubShellWidth(2044);
    await act(async () => {
      window.dispatchEvent(new Event("resize"));
    });

    expect(savedPayloads()).toHaveLength(0);
  });

  it("re-applies the clamps at the new size", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.45 };
      return undefined;
    });
    render(<Probe />);
    await waitFor(() => expect(latest!.widthPx).toBeCloseTo(540, 5));

    // Shrink to a shell whose 45% falls under the 180px floor.
    stubShellWidth(344); // inner 300 → ceiling 150, floor 180 wins
    await act(async () => {
      window.dispatchEvent(new Event("resize"));
    });
    expect(latest!.widthPx).toBe(VPANEL_MIN_PX);
  });
});

describe("useVerticalPanel — drag lifecycle", () => {
  it("ignores a second pointer-down while a drag is in flight", async () => {
    // A stacked drag would leave the first drag's listeners installed forever:
    // the panel would then resize on any pointer move with no button held, and
    // every later click would fire a spurious save.
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());

    await act(async () => {
      latest!.onResizeStart({ clientX: PANEL_LEFT + latest!.widthPx });
      latest!.onResizeStart({ clientX: PANEL_LEFT + latest!.widthPx });
    });
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 444 }));
    });
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointerup", {}));
    });

    expect(savedPayloads()).toHaveLength(1);

    // Nothing is still listening: a stray move must not move the boundary,
    // and a stray click must not write.
    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 900 }));
      window.dispatchEvent(new MouseEvent("pointerup", {}));
    });
    expect(latest!.widthPx).toBeCloseTo(400, 5);
    expect(savedPayloads()).toHaveLength(1);
  });

  it("detaches a drag still in flight when the shell unmounts", async () => {
    // Otherwise the pending `pointerup` fires after the project is torn down
    // and writes this project's width into whatever slot resolves by then.
    const view = render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());

    await act(async () => {
      latest!.onResizeStart({ clientX: PANEL_LEFT + latest!.widthPx });
    });
    view.unmount();

    await act(async () => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 444 }));
      window.dispatchEvent(new MouseEvent("pointerup", {}));
    });
    expect(savedPayloads()).toHaveLength(0);
  });

  it("allows a fresh drag after the previous one released", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    await drag(344);
    await drag(444);
    const payloads = savedPayloads();
    expect(payloads).toHaveLength(2);
    expect(payloads[1].verticalPanelFraction).toBeCloseTo(400 / INNER, 5);
  });
});

describe("useVerticalPanel — shared layout record", () => {
  it("carries the window geometry through a panel drag (lost-update guard)", async () => {
    // `useMainWindowState` writes geometry into this same record on a window
    // resize. If the drag wrote a snapshot taken at mount, the geometry the
    // user just set by resizing the window would be reverted on relaunch.
    let stored: Record<string, unknown> = { verticalPanelFraction: 0.25 };
    invokeMock.mockImplementation(
      async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "load_layout_preferences") return stored;
        if (cmd === "save_layout_preferences") {
          stored = (args as { preferences: Record<string, unknown> })
            .preferences;
          return undefined;
        }
        return undefined;
      },
    );

    render(<Probe />);
    await waitFor(() => expect(latest!.widthPx).toBeCloseTo(300, 5));

    // Another writer persists geometry into the same record after mount.
    const { patchLayoutPreferences } = await import(
      "../state/layoutPreferences"
    );
    await act(async () => {
      await patchLayoutPreferences("/dev/acme", {
        mainWindowOuterWidth: 1600,
        mainWindowOuterHeight: 1000,
      });
    });

    await drag(444);

    // The drag's write composed with the geometry write rather than reverting it.
    expect(stored.mainWindowOuterWidth).toBe(1600);
    expect(stored.mainWindowOuterHeight).toBe(1000);
    expect(stored.verticalPanelFraction).toBeCloseTo(400 / INNER, 5);
  });

  it("re-reads the fraction when the open project changes (SNV-FR-35, SNV-FR-40, OVW-FR-11)", async () => {
    // A switch opens a different slot in the backend. Serving A's fraction
    // would render B at A's width AND write A's layout into B's slot.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.25 };
      return undefined;
    });

    const view = render(<Probe projectKey="/dev/a" />);
    await waitFor(() => expect(latest!.widthPx).toBeCloseTo(300, 5));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.4 };
      return undefined;
    });
    view.rerender(<Probe projectKey="/dev/b" />);

    await waitFor(() => expect(latest!.widthPx).toBeCloseTo(480, 5));
  });

  it("falls back to the default when the new project has no persisted layout", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.45 };
      return undefined;
    });
    const view = render(<Probe projectKey="/dev/a" />);
    await waitFor(() => expect(latest!.widthPx).toBeCloseTo(540, 5));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") return null;
      return undefined;
    });
    view.rerender(<Probe projectKey="/dev/b" />);

    await waitFor(() =>
      expect(latest!.widthPx).toBeCloseTo(DEFAULT_VPANEL_FRACTION * INNER, 5),
    );
  });

  it("never writes a bare partial that would zero the geometry fields", async () => {
    // The Rust struct's geometry fields are plain f64 under #[serde(default)],
    // so a payload omitting them stores 0.0 — and the next launch would call
    // setSize(0, 0). A first-ever launch (no persisted slot) followed by a
    // drag is the most likely path into that.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences") return null;
      return undefined;
    });
    render(<Probe />);
    await waitFor(() => expect(latest).not.toBeNull());
    await drag(444);

    const payload = savedPayloads()[0];
    expect(payload.verticalPanelFraction).toBeCloseTo(400 / INNER, 5);
    // Absent, not zero: the backend defaults absent fields, and
    // `useMainWindowState` treats a zero as "no persisted geometry".
    expect(payload.mainWindowOuterWidth).toBeUndefined();
    expect(payload.mainWindowOuterHeight).toBeUndefined();
  });
});

describe("useVerticalPanel — collapse / hide (SNV-FR-37)", () => {
  it("reports no splitter when the persisted panel is collapsed", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.3, verticalPanelCollapsed: true };
      return undefined;
    });
    render(<Probe />);
    await waitFor(() => expect(latest!.resizable).toBe(false));
  });

  it("reports no splitter when the persisted panel is hidden", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.3, verticalPanelHidden: true };
      return undefined;
    });
    render(<Probe />);
    await waitFor(() => expect(latest!.resizable).toBe(false));
  });

  it("keeps the persisted width across a collapsed load (SNV-FR-37)", async () => {
    // Collapsing must not rewrite the fraction — expanding returns the panel to
    // exactly the width it had.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_layout_preferences")
        return { verticalPanelFraction: 0.3, verticalPanelCollapsed: true };
      return undefined;
    });
    render(<Probe />);
    await waitFor(() => expect(latest!.resizable).toBe(false));
    expect(latest!.widthPx).toBeCloseTo(0.3 * INNER, 5);
    expect(savedPayloads()).toHaveLength(0);
  });

  it("offers the splitter for a panel at full width", async () => {
    render(<Probe />);
    await waitFor(() => expect(latest!.resizable).toBe(true));
  });
});
