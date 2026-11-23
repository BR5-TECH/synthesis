import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { DocumentSettings } from "./DocumentSettings";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetLogBufferForTest } from "../logging";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const saved = (n: number) =>
  (calls("save_app_preferences")[n][1] as { preferences: Record<string, unknown> }).preferences;

/** The stored record. A save fails with `failSave`. Other commands reject. */
function backend(prefs: Record<string, unknown>, failSave = false) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_app_preferences") return prefs;
    if (cmd === "save_app_preferences") {
      if (failSave) throw new Error("disk is full");
      return undefined;
    }
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  });
}

const theSwitch = () => screen.getByRole("checkbox", { name: "Landscape pages for wide diagrams" });
const field = (label: string) => screen.getByLabelText(label) as HTMLInputElement;

beforeEach(() => {
  invokeMock.mockReset();
  resetAppPreferencesCache();
  resetLogBufferForTest();
});
afterEach(() => {
  cleanup();
  resetLogBufferForTest();
});

describe("Documents section", () => {
  it("GLS-FR-KWVN, GLS-FR-PBXM, GLS-FR-MRGQ, GSS-FR-JTZE, GSS-FR-TLHZ, GSS-FR-MGNV: a record with no such fields shows the defaults and writes nothing", async () => {
    backend({ theme: "system" });
    render(<DocumentSettings />);
    await waitFor(() => expect(theSwitch()).not.toBeDisabled());
    expect(theSwitch()).not.toBeChecked();
    expect(field("Landscape below")).toHaveValue("75");
    expect(field("Landscape below")).toBeDisabled();
    expect([field("Top"), field("Bottom"), field("Left"), field("Right")].map((f) => f.value)).toEqual(
      ["35", "30", "30", "30"],
    );
    expect(calls("save_app_preferences")).toHaveLength(0);
  });

  it("GLS-FR-KWVN, GLS-FR-PBXM, GLS-FR-MRGQ: shows the stored values, and the threshold field is enabled while the switch is on", async () => {
    backend({
      theme: "system",
      docxLandscapeDiagrams: true,
      docxLandscapeThresholdPercent: 60,
      docxMarginsMm: { top: 12, bottom: 13, left: 14, right: 15 },
    });
    render(<DocumentSettings />);
    await waitFor(() => expect(theSwitch()).toBeChecked());
    expect(field("Landscape below")).toHaveValue("60");
    expect(field("Landscape below")).not.toBeDisabled();
    expect([field("Top"), field("Bottom"), field("Left"), field("Right")].map((f) => f.value)).toEqual(
      ["12", "13", "14", "15"],
    );
  });

  it("GLS-FR-KWVN, GSS-FR-20: the switch saves at once, carries the rest of the record, and turning it off disables the threshold again", async () => {
    backend({ theme: "dark", selectionFollowsTab: false, openProjectBehaviour: "new_tab" });
    render(<DocumentSettings />);
    await waitFor(() => expect(theSwitch()).not.toBeDisabled());

    await userEvent.click(theSwitch());

    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    expect(saved(0)).toMatchObject({
      theme: "dark",
      selectionFollowsTab: false,
      openProjectBehaviour: "new_tab",
      docxLandscapeDiagrams: true,
    });
    expect(theSwitch()).toBeChecked();
    expect(field("Landscape below")).not.toBeDisabled();

    await userEvent.click(theSwitch());
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(2));
    expect(saved(1).docxLandscapeDiagrams).toBe(false);
    expect(field("Landscape below")).toBeDisabled();
  });

  it("GLS-FR-KWVN: a failed switch save rolls the switch back and says so", async () => {
    backend({ theme: "system" }, true);
    render(<DocumentSettings />);
    await waitFor(() => expect(theSwitch()).not.toBeDisabled());

    await userEvent.click(theSwitch());

    expect(await screen.findByRole("alert")).toHaveTextContent("disk is full");
    expect(theSwitch()).not.toBeChecked();
  });

  it("GLS-FR-PBXM, GLS-FR-DCMT: the threshold commits on Enter, once, and carries the rest of the record", async () => {
    // The save waits until the test lets it finish, so the blur after Enter
    // comes while it is on its way.
    let finish: () => void = () => {};
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return { theme: "dark", docxLandscapeDiagrams: true };
      if (cmd === "save_app_preferences") {
        await new Promise<void>((resolve) => (finish = resolve));
        return undefined;
      }
      if (cmd === "append_log_records") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });
    render(<DocumentSettings />);
    await waitFor(() => expect(field("Landscape below")).not.toBeDisabled());

    await userEvent.clear(field("Landscape below"));
    await userEvent.type(field("Landscape below"), "60{Enter}");
    await userEvent.tab();
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    finish();
    await waitFor(() => expect(field("Landscape below")).toHaveValue("60"));
    expect(calls("save_app_preferences")).toHaveLength(1);
    expect(saved(0)).toMatchObject({
      theme: "dark",
      docxLandscapeDiagrams: true,
      docxLandscapeThresholdPercent: 60,
    });
    expect(field("Landscape below")).toHaveValue("60");
  });

  it("GLS-FR-MRGQ, GLS-FR-DCMT: a margin commits on blur and keeps the other sides", async () => {
    backend({ theme: "system", docxMarginsMm: { top: 20, bottom: 21, left: 22, right: 23 } });
    render(<DocumentSettings />);
    await waitFor(() => expect(field("Left")).not.toBeDisabled());

    await userEvent.clear(field("Left"));
    await userEvent.type(field("Left"), "10");
    expect(calls("save_app_preferences")).toHaveLength(0);
    await userEvent.tab();

    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    expect(saved(0).docxMarginsMm).toEqual({ top: 20, bottom: 21, left: 10, right: 23 });

    // A second side keeps the first change.
    await userEvent.clear(field("Top"));
    await userEvent.type(field("Top"), "15{Enter}");
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(2));
    expect(saved(1).docxMarginsMm).toEqual({ top: 15, bottom: 21, left: 10, right: 23 });
  });

  it("GLS-FR-DCMT: a value out of range, not whole, or empty shows a message and commits nothing", async () => {
    backend({ theme: "system", docxLandscapeDiagrams: true });
    render(<DocumentSettings />);
    await waitFor(() => expect(field("Right")).not.toBeDisabled());

    for (const typed of ["4", "61", "12.5", "abc"]) {
      await userEvent.clear(field("Right"));
      await userEvent.type(field("Right"), typed);
      expect(screen.getByText("Right: Enter a whole number from 5 to 60.")).toBeInTheDocument();
      expect(field("Right")).toHaveAttribute("aria-invalid", "true");
      await userEvent.tab();
      expect(field("Right")).toHaveValue("30");
    }
    await userEvent.clear(field("Right"));
    expect(screen.getByText("Right: Enter a whole number from 5 to 60.")).toBeInTheDocument();
    await userEvent.tab();
    expect(field("Right")).toHaveValue("30");
    // Enter on an invalid value commits nothing either.
    await userEvent.clear(field("Right"));
    await userEvent.type(field("Right"), "4{Enter}");
    expect(field("Right")).toHaveValue("4");
    await userEvent.tab();

    for (const typed of ["9", "101"]) {
      await userEvent.clear(field("Landscape below"));
      await userEvent.type(field("Landscape below"), typed);
      expect(screen.getByText("Enter a whole number from 10 to 100.")).toBeInTheDocument();
      await userEvent.tab();
      expect(field("Landscape below")).toHaveValue("75");
    }
    expect(calls("save_app_preferences")).toHaveLength(0);
  });

  it("GLS-FR-DCMT: the range limits commit", async () => {
    backend({ theme: "system", docxLandscapeDiagrams: true });
    render(<DocumentSettings />);
    await waitFor(() => expect(field("Bottom")).not.toBeDisabled());

    await userEvent.clear(field("Bottom"));
    await userEvent.type(field("Bottom"), "5{Enter}");
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    await userEvent.clear(field("Landscape below"));
    await userEvent.type(field("Landscape below"), "100{Enter}");
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(2));
    expect(saved(1)).toMatchObject({
      docxLandscapeThresholdPercent: 100,
      docxMarginsMm: { top: 35, bottom: 5, left: 30, right: 30 },
    });
    await userEvent.clear(field("Top"));
    await userEvent.type(field("Top"), "60{Enter}");
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(3));
    await userEvent.clear(field("Landscape below"));
    await userEvent.type(field("Landscape below"), "10{Enter}");
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(4));
    expect(saved(3)).toMatchObject({
      docxLandscapeThresholdPercent: 10,
      docxMarginsMm: { top: 60, bottom: 5, left: 30, right: 30 },
    });
  });

  it("GLS-FR-DCMT: a refused save restores the stored value and shows the refusal", async () => {
    backend({ theme: "system" }, true);
    render(<DocumentSettings />);
    await waitFor(() => expect(field("Top")).not.toBeDisabled());

    await userEvent.clear(field("Top"));
    await userEvent.type(field("Top"), "20{Enter}");

    expect(
      await screen.findByText("Top: ✗ The value could not be saved: disk is full"),
    ).toBeInTheDocument();
    expect(field("Top")).toHaveAttribute("aria-describedby", "document-margin-top-message");
    expect(field("Top")).toHaveValue("35");
  });

  it("GLS-FR-MRGQ, GLS-FR-DCMT, GSS-FR-20: two margins that commit back to back both reach the store", async () => {
    // The first save waits until the test lets it finish, so the second field
    // commits while the first save is on its way.
    let finishFirst: () => void = () => {};
    let saves = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return { theme: "system" };
      if (cmd === "save_app_preferences") {
        saves += 1;
        if (saves === 1) await new Promise<void>((resolve) => (finishFirst = resolve));
        return undefined;
      }
      if (cmd === "append_log_records") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });
    render(<DocumentSettings />);
    await waitFor(() => expect(field("Top")).not.toBeDisabled());

    await userEvent.clear(field("Top"));
    await userEvent.type(field("Top"), "20{Enter}");
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    await userEvent.clear(field("Left"));
    await userEvent.type(field("Left"), "10{Enter}");
    finishFirst();

    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(2));
    expect(saved(1).docxMarginsMm).toEqual({ top: 20, bottom: 30, left: 10, right: 30 });
    await waitFor(() => expect(field("Top")).toHaveValue("20"));
    expect(field("Left")).toHaveValue("10");
  });

  it("GLS-FR-DCMT, GSS-FR-20: a refused margin save does not reach disk through the save after it", async () => {
    let refuseFirst: (e: Error) => void = () => {};
    let saves = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return { theme: "system" };
      if (cmd === "save_app_preferences") {
        saves += 1;
        if (saves === 1) await new Promise<void>((_, reject) => (refuseFirst = reject));
        return undefined;
      }
      if (cmd === "append_log_records") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });
    render(<DocumentSettings />);
    await waitFor(() => expect(field("Top")).not.toBeDisabled());

    await userEvent.clear(field("Top"));
    await userEvent.type(field("Top"), "20{Enter}");
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    await userEvent.clear(field("Left"));
    await userEvent.type(field("Left"), "10{Enter}");
    refuseFirst(new Error("disk is full"));

    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(2));
    expect(saved(1).docxMarginsMm).toEqual({ top: 35, bottom: 30, left: 10, right: 30 });
    await waitFor(() => expect(field("Top")).toHaveValue("35"));
    await waitFor(() => expect(field("Left")).toHaveValue("10"));
  });

  it("GLS-FR-KWVN, GLS-FR-PBXM, GLS-FR-MRGQ: the notes say what the settings do", async () => {
    backend({ theme: "system" });
    render(<DocumentSettings />);
    await waitFor(() => expect(theSwitch()).not.toBeDisabled());
    expect(screen.getByText(/goes on a\s+landscape page of its own/)).toBeInTheDocument();
    expect(screen.getByText(/% of the diagram's size/)).toBeInTheDocument();
    const margins = screen.getByText(/A PDF export keeps its own margins/);
    expect(margins).toHaveTextContent(/portrait and landscape/);
    expect(margins).toHaveTextContent(/pictures show larger/);
  });
});
