/**
 * The **Page width** field of the Documents section (GLS-FR-IXAQ,
 * GLS-FR-NOWF, GLS-FR-DCMT).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>();
let listeners: Record<string, (event: { payload: unknown }) => void> = {};
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

import { DocumentSettings } from "./DocumentSettings";
import { resetAppPreferencesCache } from "../state/appPreferences";
import {
  PAGE_WIDTH_PROPERTY,
  getEditorPageWidth,
  resetEditorPageWidthForTest,
} from "../state/editorPageWidth";
import { resetLogBufferForTest } from "../logging";
import { APP_PREFERENCES_CHANGED } from "../events";
import { emit } from "@tauri-apps/api/event";

const announced = () =>
  vi.mocked(emit).mock.calls.filter((c) => c[0] === APP_PREFERENCES_CHANGED).length;
const settle = () =>
  act(async () => {
    await new Promise<void>((res) => setTimeout(res, 0));
  });

let stored: Record<string, unknown> = {};
let failSave = false;

function route(cmd: string, args?: Record<string, unknown>): Promise<unknown> {
  switch (cmd) {
    case "load_app_preferences":
      return Promise.resolve({ ...stored });
    case "save_app_preferences":
      if (failSave) return Promise.reject(new Error("disk is full"));
      stored = { ...(args?.preferences as Record<string, unknown>) };
      return Promise.resolve();
    case "append_log_records":
      return Promise.resolve();
    default:
      return Promise.reject(new Error(`unexpected command ${cmd}`));
  }
}

const saves = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_app_preferences")
    .map((c) => (c[1] as { preferences: Record<string, unknown> }).preferences);
const field = () => screen.getByLabelText("Page width") as HTMLInputElement;
const reset = () => screen.getByRole("button", { name: "Reset" });

async function mount(record: Record<string, unknown>) {
  stored = record;
  render(<DocumentSettings />);
  await waitFor(() => expect(field()).not.toBeDisabled());
}

beforeEach(() => {
  listeners = {};
  stored = {};
  failSave = false;
  invokeMock.mockReset();
  invokeMock.mockImplementation(route);
  vi.mocked(emit).mockClear();
  resetAppPreferencesCache();
  resetEditorPageWidthForTest();
  resetLogBufferForTest();
});
afterEach(() => {
  cleanup();
  resetEditorPageWidthForTest();
});

describe("Documents section: Page width", () => {
  it("GLS-FR-IXAQ, GSS-FR-UYXH: the field comes first and shows the stored width, with a note", async () => {
    await mount({ editorPageWidthPx: 1240, docxLandscapeDiagrams: true });
    expect(field().value).toBe("1240");
    const inputs = screen.getAllByRole("textbox");
    expect(inputs[0]).toBe(field());
    expect(
      screen.getByText(/largest width of the text on the document page at 100% zoom/),
    ).toBeInTheDocument();
    expect(screen.getByText(/drag the ruler below the editor/)).toBeInTheDocument();
  });

  it.each([
    ["no width", {}],
    ["a width out of range", { editorPageWidthPx: 3000 }],
  ])("GLS-FR-IXAQ: a record with %s shows 820 and writes nothing", async (_label, record) => {
    await mount(record);
    expect(field().value).toBe("820");
    expect(reset()).toBeDisabled();
    await settle();
    expect(saves()).toEqual([]);
  });

  it("GLS-FR-DCMT, EPW-FR-XOMR: Enter commits once, carries the record, and the page shows the width at once", async () => {
    await mount({ theme: "dark", docxLandscapeThresholdPercent: 40 });
    const user = userEvent.setup();
    await user.clear(field());
    await user.type(field(), "1400{Enter}");
    await waitFor(() => expect(saves()).toHaveLength(1));
    expect(saves()[0]).toMatchObject({
      editorPageWidthPx: 1400,
      theme: "dark",
      docxLandscapeThresholdPercent: 40,
    });
    expect(document.documentElement.style.getPropertyValue(PAGE_WIDTH_PROPERTY)).toBe("1400px");
    // GLS-FR-NOWF / EPW-FR-XOMR: the project windows hear of the write.
    await waitFor(() => expect(announced()).toBe(1));
    // The blur after Enter sends no second save.
    await user.tab();
    await settle();
    expect(saves()).toHaveLength(1);
  });

  it("GLS-FR-DCMT: the limits commit on blur", async () => {
    await mount({});
    const user = userEvent.setup();
    await user.clear(field());
    await user.type(field(), "1800");
    await user.tab();
    await waitFor(() => expect(saves().map((p) => p.editorPageWidthPx)).toEqual([1800]));
    await user.clear(field());
    await user.type(field(), "560");
    await user.tab();
    await waitFor(() => expect(saves().map((p) => p.editorPageWidthPx)).toEqual([1800, 560]));
  });

  it("GLS-FR-DCMT: a value that is empty, not whole, or out of range shows a message and commits nothing", async () => {
    await mount({});
    const user = userEvent.setup();
    for (const typed of ["", "559", "1801", "820.5", "wide"]) {
      await user.clear(field());
      if (typed) await user.type(field(), typed);
      expect(screen.getByText("Enter a whole number from 560 to 1800."), typed).toBeInTheDocument();
      await user.keyboard("{Enter}");
    }
    await user.tab();
    await settle();
    expect(saves()).toEqual([]);
    expect(getEditorPageWidth()).toBe(820);
  });

  it("GLS-FR-DCMT: a refused save restores the stored width on the field and the page, and shows the refusal", async () => {
    await mount({ editorPageWidthPx: 1000 });
    failSave = true;
    const user = userEvent.setup();
    await user.clear(field());
    await user.type(field(), "1500{Enter}");
    expect(await screen.findByText(/The value could not be saved: disk is full/)).toBeInTheDocument();
    expect(field().value).toBe("1000");
    expect(getEditorPageWidth()).toBe(1000);
  });

  it("GLS-FR-NOWF: Reset sets 820 and saves at once, and is disabled at 820", async () => {
    await mount({ editorPageWidthPx: 1300 });
    expect(reset()).toBeEnabled();
    await userEvent.setup().click(reset());
    await waitFor(() => expect(saves().map((p) => p.editorPageWidthPx)).toEqual([820]));
    expect(field().value).toBe("820");
    expect(reset()).toBeDisabled();
    await waitFor(() => expect(announced()).toBe(1));
  });

  it("GLS-FR-NOWF: a refused Reset keeps the stored width and shows the refusal", async () => {
    await mount({ editorPageWidthPx: 1300 });
    failSave = true;
    await userEvent.setup().click(reset());
    expect(await screen.findByRole("alert")).toHaveTextContent(/disk is full/);
    expect(field().value).toBe("1300");
    expect(getEditorPageWidth()).toBe(1300);
  });

  it("GLS-FR-IXAQ, EPW-FR-ZCBT: a width that a project window saves shows in the field, so a commit of 820 saves", async () => {
    await mount({ editorPageWidthPx: 820 });
    await waitFor(() => expect(listeners[APP_PREFERENCES_CHANGED]).toBeTypeOf("function"));
    stored = { editorPageWidthPx: 1000 };
    act(() => listeners[APP_PREFERENCES_CHANGED]({ payload: null }));
    await waitFor(() => expect(field().value).toBe("1000"));
    expect(reset()).toBeEnabled();
    const user = userEvent.setup();
    await user.clear(field());
    await user.type(field(), "820{Enter}");
    await waitFor(() => expect(saves().map((p) => p.editorPageWidthPx)).toEqual([820]));
  });
});
