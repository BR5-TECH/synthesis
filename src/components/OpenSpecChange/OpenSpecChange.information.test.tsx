/**
 * The Information modal of a change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-ZZCS,
 * on the terms of `DFI-draft-information.md` DFI-FR-HSYB, DFI-FR-CVAX,
 * DFI-FR-NTPG, DFI-FR-EBIL).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: unknown }) => void) => {
    const set = listeners.get(event) ?? new Set();
    set.add(cb);
    listeners.set(event, set);
    return () => set.delete(cb);
  }),
}));

import {
  CHANGE,
  STREAM_ID,
  changeBackend,
  changeStatistics,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb({ payload });
}

let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  listeners.clear();
  invokeMock.mockReset();
  backend = changeBackend({ proposals: [], discussions: [] });
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  cleanup();
  expect(backend.unknown).toEqual([]);
});

const NONE = { value: null, availability: "unavailable" as const };

/** Open the change and its Information, and wait for the figures. */
async function openInformation() {
  const user = userEvent.setup();
  drawChange();
  await screen.findAllByTestId("openspec-artifact");
  await user.click(screen.getByRole("button", { name: "Information" }));
  const dialog = await screen.findByRole("dialog", { name: `Information for ${CHANGE}` });
  return { user, dialog };
}

/** The value of the row with this label. */
function valueOf(dialog: HTMLElement, label: string): string {
  const row = within(dialog)
    .getAllByText(label)
    .map((el) => el.closest(".draft-info__row"))
    .find(Boolean) as HTMLElement;
  return row.querySelector(".draft-info__value")?.textContent ?? "";
}

describe("Information (OCW-FR-ZZCS)", () => {
  it("OCW-FR-ZZCS: a stream location offers Information, a centred modal of the eight statistics in order with a footer", async () => {
    const { dialog } = await openInformation();
    await waitFor(() => expect(within(dialog).getByText("Working time")).toBeInTheDocument());
    expect(calls("read_change_statistics")[0][1]).toEqual({ streamId: STREAM_ID, change: CHANGE });
    const labels = [...dialog.querySelectorAll(".draft-info__label, .draft-info__group")].map((el) => el.textContent);
    expect(labels).toEqual([
      "Working time",
      "AI interactions",
      "Versions",
      "Accepted proposals",
      "Rejected proposals",
      "Conversation tokens",
      "Input",
      "Output",
      "Run time",
      "Run tokens",
      "Input",
      "Output",
    ]);
    expect(valueOf(dialog, "Working time")).toBe("1h 30m");
    expect(valueOf(dialog, "Versions")).toBe("3");
    expect(valueOf(dialog, "Input")).toBe("12 345");
    expect(within(dialog).getByText(/Statistics cover activity captured since/)).toBeInTheDocument();
    expect(dialog.closest(".scrim")).not.toBeNull();
  });

  it("OCW-FR-ZZCS / CST-FR-HJGG / CST-FR-CGRU: Run time and Run tokens show the run figures, and a record without them reads not captured", async () => {
    const { dialog } = await openInformation();
    await waitFor(() => expect(within(dialog).getByText("Run time")).toBeInTheDocument());
    const values = [...dialog.querySelectorAll(".draft-info__value")].map((el) => el.textContent);
    expect(valueOf(dialog, "Run time")).toBe("25m");
    // Run time starts a group of its own: it has a divider and stands outside the conversation tokens.
    const runTime = within(dialog).getByText("Run time").closest(".draft-info__row")!;
    expect(runTime.parentElement).toHaveClass("draft-info__section");
    expect(runTime.parentElement).toBe(within(dialog).getByTestId("change-information-runs"));
    // The two token groups each have an Input and an Output row; the run pair is the second.
    expect(values.slice(-2)).toEqual(["54 321", "4 321"]);
    cleanup();

    const old = changeStatistics() as Partial<ReturnType<typeof changeStatistics>>;
    delete old.runTimeMs;
    delete old.runTokens;
    backend.statistics = old as ReturnType<typeof changeStatistics>;
    const again = await openInformation();
    await waitFor(() => expect(within(again.dialog).getByText("Run time")).toBeInTheDocument());
    expect(valueOf(again.dialog, "Run time")).toBe("not captured");
    const later = [...again.dialog.querySelectorAll(".draft-info__value")].map((el) => el.textContent);
    expect(later.slice(-2)).toEqual(["not captured", "not captured"]);
  });

  it("OCW-FR-ZZCS / DFI-FR-HSYB: an unavailable figure reads not captured, never 0, and an incomplete one carries the partial marker", async () => {
    backend.statistics = changeStatistics({
      versions: NONE,
      aiInteractions: { value: 4, availability: "incomplete" },
      conversationTokens: { input: NONE, output: { value: 0, availability: "available" } },
    });
    const { dialog } = await openInformation();
    await waitFor(() => expect(within(dialog).getByText("Working time")).toBeInTheDocument());
    expect(valueOf(dialog, "Versions")).toBe("not captured");
    expect(valueOf(dialog, "Input")).toBe("not captured");
    expect(valueOf(dialog, "Output")).toBe("0");
    expect(within(dialog).getAllByTestId("draft-info-partial")).toHaveLength(1);
    expect(within(dialog).getByLabelText("AI interactions, 4, the captured record for this figure is partial")).toBeInTheDocument();
  });

  it("OCW-FR-ZZCS / DFI-FR-NTPG: a change with no boundary states that nothing has been captured", async () => {
    backend.statistics = changeStatistics({ boundaryAt: null, workingTimeMs: NONE });
    const { dialog } = await openInformation();
    expect(await within(dialog).findByText("Nothing has been captured for this change yet.")).toBeInTheDocument();
    expect(valueOf(dialog, "Working time")).toBe("not captured");
  });

  it("OCW-FR-ZZCS / DFI-FR-CVAX: a failed read is stated with Retry, and a later failure keeps the figures", async () => {
    backend.statistics = { reject: "read_failed" };
    const { user, dialog } = await openInformation();
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("This change's statistics could not be read.");
    backend.statistics = changeStatistics();
    await user.click(within(dialog).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(within(dialog).queryByRole("alert")).toBeNull());
    expect(valueOf(dialog, "Versions")).toBe("3");

    backend.statistics = { reject: "read_failed" };
    act(() => emit("change-statistics-changed", { streamId: STREAM_ID, change: CHANGE }));
    expect(await within(dialog).findByRole("alert")).toBeInTheDocument();
    expect(valueOf(dialog, "Versions")).toBe("3");
  });

  it("OCW-FR-ZZCS: re-reads on change statistics changed for its change, and not for another", async () => {
    const { dialog } = await openInformation();
    await waitFor(() => expect(within(dialog).getByText("Working time")).toBeInTheDocument());
    await waitFor(() => expect(listeners.get("change-statistics-changed")?.size ?? 0).toBe(1));
    act(() => emit("change-statistics-changed", { streamId: STREAM_ID, change: "other" }));
    act(() => emit("change-statistics-changed", { streamId: "s-2", change: CHANGE }));
    expect(calls("read_change_statistics")).toHaveLength(1);
    backend.statistics = changeStatistics({ aiInteractions: { value: 13, availability: "available" } });
    act(() => emit("change-statistics-changed", { streamId: STREAM_ID, change: CHANGE }));
    await waitFor(() => expect(valueOf(dialog, "AI interactions")).toBe("13"));
    expect(calls("read_change_statistics")).toHaveLength(2);
  });

  it("OCW-FR-ZZCS / DFI-FR-EBIL: the close control, Escape, and the backdrop dismiss it, and dismissing writes nothing", async () => {
    const { user, dialog } = await openInformation();
    const before = invokeMock.mock.calls.length;
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();

    await user.click(screen.getByRole("button", { name: "Information" }));
    await screen.findByRole("dialog");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();

    await user.click(screen.getByRole("button", { name: "Information" }));
    const again = await screen.findByRole("dialog");
    await user.click(again.closest(".scrim") as HTMLElement);
    expect(screen.queryByRole("dialog")).toBeNull();
    // Each open reads again, and nothing but the reads was invoked.
    expect(calls("read_change_statistics")).toHaveLength(3);
    const sent = invokeMock.mock.calls
      .slice(before)
      .map((c) => c[0])
      .filter((cmd) => cmd !== "append_log_records");
    expect(sent).toEqual(["read_change_statistics", "read_change_statistics"]);
    expect(listeners.get("change-statistics-changed")?.size ?? 0).toBe(0);
  });
});
