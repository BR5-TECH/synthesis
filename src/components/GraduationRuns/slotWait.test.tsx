import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import { makeQueue, makeRun } from "../../test/graduationFixtures";
import type { GraduationCapacity, GraduationQueue } from "../../types";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

/** What the backend answers with, per command. */
let queue: GraduationQueue;
let capacity: GraduationCapacity | undefined;
let capacityRefusal: string | null;

beforeEach(() => {
  capacity = undefined;
  capacityRefusal = null;
  // A run holds the stream the second run waits in, and the third run waits in
  // a stream of its own.
  queue = makeQueue([
    makeRun("r1", "working"),
    makeRun("r2", "queued"),
    makeRun("r3", "queued", { streamId: "s-2", streamName: "other-work" }),
  ]);
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    switch (command) {
      case "list_graduation_queue":
        return queue;
      case "get_graduation_capacity":
        if (capacityRefusal) throw capacityRefusal;
        return capacity;
      case "load_app_preferences":
        return { graduationRailWidthFraction: 0.2 };
      case "list_work_streams":
        return [];
      default:
        return undefined;
    }
  });
  listened.mockReset();
  listened.mockImplementation(async () => () => {});
  resetAppPreferencesCache();
  forgetEverything();
  forgetEveryDraft();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const capacityReads = () =>
  invoked.mock.calls.filter(([command]) => command === "get_graduation_capacity")
    .length;

/** Render the section and wait for its first reads. */
async function open() {
  render(<GraduationRuns />);
  await screen.findByTestId("graduation-section");
  await waitFor(() => expect(capacityReads()).toBeGreaterThan(0));
}

/** Select one run in the rail. */
async function select(id: string) {
  await userEvent.click(await screen.findByRole("button", { name: `Run ${id}` }));
}

/** Fire the event the section reloads on. */
async function fire(event: string) {
  const handlers = listened.mock.calls
    .filter(([name]) => name === event)
    .map(([, handler]) => handler as (payload: unknown) => void);
  expect(handlers.length).toBeGreaterThan(0);
  await act(async () => {
    for (const handler of handlers) handler({ payload: {} });
  });
}

describe("a queued run that waits for a project slot", () => {
  it("GRU-FR-KMNF: a run the capacity lists states the project slot, the limit and the slots held", async () => {
    capacity = { limit: 2, inUse: 2, waitingForSlot: ["r3"] };
    await open();
    await select("r3");
    const sentence = await screen.findByTestId("graduation-run-slot-wait");
    expect(sentence).toHaveTextContent(
      "Waiting for a project slot — the project works 2 of 2 graduation runs.",
    );
    // The two sentences are never mixed.
    expect(screen.getByTestId("run-progress")).not.toHaveTextContent(
      "in the stream",
    );
  });

  it("GRU-FR-KMNF: a limit of one is said in the singular", async () => {
    capacity = { limit: 1, inUse: 1, waitingForSlot: ["r3"] };
    await open();
    await select("r3");
    expect(await screen.findByTestId("graduation-run-slot-wait")).toHaveTextContent(
      "the project works 1 of 1 graduation run.",
    );
  });

  it("GRU-FR-KMNF: the rail row of the run says that it waits for a project slot", async () => {
    capacity = { limit: 2, inUse: 2, waitingForSlot: ["r3"] };
    await open();
    expect(await screen.findByText("Next · project slot")).toBeInTheDocument();
  });

  it("GRU-FR-HKBD / GRU-FR-KMNF: a run that waits in its own queue keeps the own-queue wording", async () => {
    capacity = { limit: 2, inUse: 2, waitingForSlot: ["r3"] };
    await open();
    await select("r2");
    const row = await screen.findByTestId("run-progress");
    expect(row).toHaveTextContent("Next in the stream");
    expect(row).not.toHaveTextContent("project slot");
    expect(screen.queryByTestId("graduation-run-slot-wait")).toBeNull();
  });

  it("GRU-FR-QKDB: the capacity is read again on a queue change, and a run that is no longer listed leaves the slot wait", async () => {
    capacity = { limit: 2, inUse: 2, waitingForSlot: ["r3"] };
    await open();
    await select("r3");
    await screen.findByTestId("graduation-run-slot-wait");
    const before = capacityReads();
    capacity = { limit: 2, inUse: 1, waitingForSlot: [] };
    await fire("graduation-queue-changed");
    await waitFor(() => expect(capacityReads()).toBeGreaterThan(before));
    await waitFor(() =>
      expect(screen.queryByTestId("graduation-run-slot-wait")).toBeNull(),
    );
    expect(screen.getByTestId("run-progress")).toHaveTextContent(
      "Next in the stream",
    );
  });

  it("GRU-FR-QKDB: a capacity that returns nothing leaves the own-queue sentence and renders no guess", async () => {
    capacity = undefined;
    await open();
    await select("r3");
    const row = await screen.findByTestId("run-progress");
    expect(row).toHaveTextContent("Next in the stream");
    expect(screen.queryByTestId("graduation-run-slot-wait")).toBeNull();
    expect(screen.queryByText(/project slot/i)).toBeNull();
  });

  it("GRU-FR-QKDB: a capacity read that fails leaves the own-queue sentence and does not hide the queue", async () => {
    capacityRefusal = "project_not_open";
    await open();
    await select("r3");
    expect(await screen.findByTestId("run-progress")).toHaveTextContent(
      "Next in the stream",
    );
    expect(screen.queryByTestId("graduation-run-slot-wait")).toBeNull();
    expect(screen.queryByTestId("graduation-error")).toBeNull();
    expect(screen.getAllByTestId("graduation-row")).toHaveLength(3);
  });

  it("GRU-FR-QKDB: a capacity that fails after a good read takes the slot wait back", async () => {
    capacity = { limit: 2, inUse: 2, waitingForSlot: ["r3"] };
    await open();
    await select("r3");
    await screen.findByTestId("graduation-run-slot-wait");
    capacityRefusal = "project_not_open";
    await fire("graduation-queue-changed");
    await waitFor(() =>
      expect(screen.queryByTestId("graduation-run-slot-wait")).toBeNull(),
    );
  });

  it("GRU-FR-HKBD: a run whose auto-start is off still says so", async () => {
    capacity = { limit: 2, inUse: 2, waitingForSlot: [] };
    queue = makeQueue([
      makeRun("r1", "working"),
      makeRun("r2", "queued", { autoStart: false }),
    ]);
    await open();
    await select("r2");
    expect(await screen.findByTestId("run-progress")).toHaveTextContent(
      "auto-start off, so it will not start on its own",
    );
    expect(screen.queryByTestId("graduation-run-slot-wait")).toBeNull();
  });
});
