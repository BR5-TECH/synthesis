/**
 * The page width hooks (`EPW-editor-page-width.md` EPW-FR-ZCBT): the startup
 * read and the sync with the other windows.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render } from "@testing-library/react";

const invokeMock = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();
let listeners: Record<string, (event: { payload: unknown }) => void> = {};
let storedRecord: Record<string, unknown> = {};
/** When set, a save waits until the test calls it. */
let holdSave = false;
let releaseSave: (() => void) | null = null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invokeMock(cmd, args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: unknown }) => void) => {
    listeners[event] = cb;
    return () => {
      delete listeners[event];
    };
  }),
  emit: vi.fn(async () => {}),
}));

import { useEditorPageWidth, useEditorPageWidthSync } from "./useEditorPageWidth";
import {
  PAGE_WIDTH_PROPERTY,
  commitEditorPageWidth,
  resetEditorPageWidthForTest,
} from "../state/editorPageWidth";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetLogBufferForTest } from "../logging";
import { APP_PREFERENCES_CHANGED } from "../events";

function route(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "load_app_preferences":
      return Promise.resolve({ ...storedRecord });
    case "save_app_preferences": {
      const write = () => {
        storedRecord = { ...(args?.preferences as Record<string, unknown>) };
      };
      if (!holdSave) {
        write();
        return Promise.resolve();
      }
      return new Promise<void>((res) => {
        releaseSave = () => {
          write();
          res();
        };
      });
    }
    case "append_log_records":
      return Promise.resolve();
    default:
      return Promise.reject(new Error(`unexpected command ${cmd}`));
  }
}

function SyncHost() {
  useEditorPageWidthSync();
  return <span data-testid="width">{useEditorPageWidth()}</span>;
}

beforeEach(() => {
  listeners = {};
  storedRecord = {};
  holdSave = false;
  releaseSave = null;
  invokeMock.mockReset();
  invokeMock.mockImplementation(route);
  resetEditorPageWidthForTest();
  resetAppPreferencesCache();
  resetLogBufferForTest();
});
afterEach(() => {
  cleanup();
  resetEditorPageWidthForTest();
});

const changed = async () => {
  await vi.waitFor(() => expect(listeners[APP_PREFERENCES_CHANGED]).toBeTypeOf("function"));
  act(() => listeners[APP_PREFERENCES_CHANGED]({ payload: null }));
};

describe("useEditorPageWidthSync", () => {
  it("EPW-FR-ZCBT: the window reads the stored width at startup", async () => {
    storedRecord = { editorPageWidthPx: 1200 };
    const { findByText } = render(<SyncHost />);
    await findByText("1200");
    expect(document.documentElement.style.getPropertyValue(PAGE_WIDTH_PROPERTY)).toBe("1200px");
  });

  it("EPW-FR-ZCBT, EPW-FR-XOMR: a width that another window saves shows with no reload or remount", async () => {
    const { findByText, getByTestId } = render(<SyncHost />);
    await findByText("820");
    const node = getByTestId("width");
    storedRecord = { editorPageWidthPx: 1500 };
    await changed();
    await findByText("1500");
    expect(getByTestId("width")).toBe(node);
    storedRecord = { editorPageWidthPx: 3 };
    await changed();
    await findByText("820");
  });

  it("EPW-FR-ZCBT: a stale record read while this window's save is out does not move the width back", async () => {
    const { findByText, getByTestId } = render(<SyncHost />);
    await findByText("820");
    holdSave = true;
    let pending: Promise<void> | undefined;
    act(() => {
      pending = commitEditorPageWidth(1300);
    });
    await findByText("1300");
    // Another window's announcement is read while the save is out, so the
    // refresh queues behind the save and the disk still says 820 meanwhile.
    await changed();
    await vi.waitFor(() => expect(releaseSave).toBeTypeOf("function"));
    await act(async () => {
      releaseSave!();
      await pending;
    });
    await act(async () => {
      await new Promise<void>((res) => setTimeout(res, 0));
    });
    expect(getByTestId("width")).toHaveTextContent("1300");
    expect(storedRecord.editorPageWidthPx).toBe(1300);
  });

  it("EPW-FR-ZCBT: a host that unmounts before the read answers adopts nothing and stops listening", async () => {
    storedRecord = { editorPageWidthPx: 1400 };
    const { unmount } = render(<SyncHost />);
    unmount();
    await act(async () => {
      await new Promise<void>((res) => setTimeout(res, 0));
    });
    expect(document.documentElement.style.getPropertyValue(PAGE_WIDTH_PROPERTY)).toBe("");
    expect(listeners[APP_PREFERENCES_CHANGED]).toBeUndefined();
  });
});
