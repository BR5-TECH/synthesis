/**
 * The project-wide limit of graduation runs in the start dialog
 * (`../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-LHQY,
 * GSD-FR-KDBU, GSD-FR-NOID).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import type { GraduationCapacity, WorkStreamSummary } from "../types";
import { GraduationStart } from "./GraduationStart";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

type EventHandler = (event: { payload: unknown }) => void;
let handlers: Array<[string, EventHandler]> = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    handlers.push([name, handler]);
    return Promise.resolve(() => {
      handlers = handlers.filter(([, h]) => h !== handler);
    });
  },
}));
const fireQueueChanged = () =>
  act(() => {
    handlers
      .filter(([name]) => name === "graduation-queue-changed")
      .forEach(([, h]) => h({ payload: {} }));
  });

const invoked = vi.mocked(invoke);

const stream = (
  id: string,
  over: Partial<WorkStreamSummary["stream"]> = {},
): WorkStreamSummary => ({
  stream: {
    id,
    name: id,
    projectKey: "/Users/demo/dev/acme",
    branch: `synthesis/stream/${id}`,
    baseBranch: "main",
    baseRevision: "a91bc04",
    worktreePath: `/tmp/${id}`,
    createdAt: "2026-09-06T09:00:00Z",
    isMissing: false,
    ...over,
  },
  queuedRunCount: 0,
  aheadOfBase: 0,
  behindBase: 0,
  baseTipRevision: "a91bc04",
  missingCommits: [],
});

const capacityOf = (
  limit: GraduationCapacity["limit"],
  inUse: number,
): GraduationCapacity => ({ limit, inUse, waitingForSlot: [] });

let streams: WorkStreamSummary[];
let capacity: GraduationCapacity | string | undefined;

beforeEach(() => {
  streams = [stream("editor-work")];
  capacity = capacityOf(2, 0);
  handlers = [];
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    if (command === "list_work_streams") return streams;
    if (command === "get_graduation_capacity") {
      if (typeof capacity === "string") throw capacity;
      return capacity;
    }
    if (command === "list_worktrees_and_branches") {
      return { worktrees: [], branches: [{ name: "main" }] };
    }
    if (command === "create_work_stream") return { id: "made" };
    if (command === "start_graduation") return { id: "g-1" };
    return undefined;
  });
});
afterEach(cleanup);

function draw() {
  const onStarted = vi.fn();
  render(
    <GraduationStart
      draftId="d-1"
      draftName="artifact-window"
      onClose={vi.fn()}
      onStarted={onStarted}
    />,
  );
  return { onStarted };
}

const picker = () => screen.getByRole("combobox", { name: "Work stream" });
const graduate = () => screen.getByTestId("graduation-start-confirm");
const choice = () => screen.queryByTestId("standing-work-choice");
const slotNote = () => screen.queryByTestId("graduation-start-slot-wait");
const position = (value: string) =>
  screen.getByTestId("standing-work-choice").querySelector<HTMLInputElement>(
    `input[type="radio"][value="${value}"]`,
  ) as HTMLInputElement;
const capacityReads = () =>
  invoked.mock.calls.filter(([command]) => command === "get_graduation_capacity").length;

/** Wait until the dialog has read the streams and the capacity. */
async function settled() {
  await waitFor(() => expect(picker()).toBeInTheDocument());
  await waitFor(() => expect(capacityReads()).toBeGreaterThan(0));
  // One more turn, so the answer of the capacity read is rendered.
  await act(async () => {});
}

describe("a project slot that is free", () => {
  it("GSD-FR-WQPD, GSD-FR-LHQY: a free stream with a free slot asks nothing and states no wait", async () => {
    capacity = capacityOf(2, 1);
    const { onStarted } = draw();
    await settled();
    expect(choice()).toBeNull();
    expect(slotNote()).toBeNull();
    expect(screen.queryByTestId("graduation-start-capacity-unread")).toBeNull();
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith("start_graduation", {
      draftId: "d-1",
      streamId: "editor-work",
      standingWork: "commit",
      standingWorkMessage: null,
    });
  });
});

describe("a project limit that is full", () => {
  beforeEach(() => {
    capacity = capacityOf(2, 2);
  });

  it("GSD-FR-LHQY, GSD-FR-WQPD: a free stream asks the choice and states the wait for a project slot", async () => {
    const { onStarted } = draw();
    await settled();
    await waitFor(() => expect(slotNote()).toBeInTheDocument());
    expect(slotNote()).toHaveTextContent(
      /.editor-work. is free, but the project already works its limit of 2 graduation runs\. This one waits for a project slot\./,
    );
    // The stream itself is free, so no busy note stands.
    expect(screen.queryByTestId("graduation-start-busy")).toBeNull();
    expect(choice()).toBeInTheDocument();

    await userEvent.click(position("keep"));
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith("start_graduation", {
      draftId: "d-1",
      streamId: "editor-work",
      standingWork: "keep",
      standingWorkMessage: null,
    });
  });

  it("GSD-FR-LHQY, GSD-FR-MZTB: the message the author typed travels with a choice that commits", async () => {
    draw();
    await settled();
    await waitFor(() => expect(choice()).toBeInTheDocument());
    await userEvent.click(position("commit"));
    await userEvent.type(
      screen.getByRole("textbox", { name: /Commit message/ }),
      "  Wait for the slot  ",
    );
    await userEvent.click(graduate());
    await waitFor(() =>
      expect(invoked).toHaveBeenCalledWith("start_graduation", {
        draftId: "d-1",
        streamId: "editor-work",
        standingWork: "commit",
        standingWorkMessage: "Wait for the slot",
      }),
    );
  });

  it("GSD-FR-KDBU: an occupied stream states both reasons", async () => {
    streams = [stream("editor-work", { busyRunId: "g-9" })];
    draw();
    await settled();
    await waitFor(() => expect(slotNote()).toBeInTheDocument());
    expect(screen.getByTestId("graduation-start-busy")).toHaveTextContent(
      /A run is working in .editor-work. now/,
    );
    expect(slotNote()).toHaveTextContent(/limit of 2 graduation runs/);
    expect(slotNote()).toHaveTextContent(/waits for a project slot too/);
    expect(choice()).toBeInTheDocument();
  });

  it("GSD-FR-VKLD: Work directly asks no choice, whatever the limit is", async () => {
    draw();
    await settled();
    await userEvent.click(screen.getByRole("radio", { name: "Work directly" }));
    expect(choice()).toBeNull();
    expect(slotNote()).toBeNull();
  });
});

describe("a limit of Unlimited", () => {
  it("GSD-FR-KDBU: it never makes a run wait for a slot", async () => {
    capacity = capacityOf("unlimited", 40);
    draw();
    await settled();
    expect(slotNote()).toBeNull();
    expect(choice()).toBeNull();
  });
});

describe("a capacity that is not read", () => {
  it("GSD-FR-NOID: a refused read is stated with the refusal, and a free stream is not asked", async () => {
    capacity = "no_project_open";
    draw();
    await settled();
    const unread = await screen.findByTestId("graduation-start-capacity-unread");
    expect(unread).toHaveTextContent(/could not be read/);
    expect(unread).toHaveTextContent(/No project is open\./);
    expect(slotNote()).toBeNull();
    expect(choice()).toBeNull();
  });

  it("GSD-FR-NOID: a read that returns nothing is stated as unread and does not crash", async () => {
    capacity = undefined;
    draw();
    await settled();
    expect(
      await screen.findByTestId("graduation-start-capacity-unread"),
    ).toBeInTheDocument();
    expect(choice()).toBeNull();
  });

  it("GSD-FR-NOID, GSD-FR-WQPD: an occupied stream is still asked, for its own reason alone", async () => {
    capacity = "no_project_open";
    streams = [stream("editor-work", { busyRunId: "g-9" })];
    draw();
    await settled();
    await screen.findByTestId("graduation-start-capacity-unread");
    expect(choice()).toBeInTheDocument();
    expect(slotNote()).toBeNull();
  });
});

describe("a new stream", () => {
  const chooseNew = async () => {
    await userEvent.click(screen.getByRole("radio", { name: "New stream" }));
    await userEvent.type(await screen.findByRole("textbox", { name: "Name" }), "fresh");
  };

  it("GSD-FR-QGTC: it asks no choice while the limit is free", async () => {
    capacity = capacityOf(2, 1);
    const { onStarted } = draw();
    await settled();
    await chooseNew();
    expect(choice()).toBeNull();
    expect(slotNote()).toBeNull();
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith("start_graduation", {
      draftId: "d-1",
      streamId: "made",
      standingWork: "commit",
      standingWorkMessage: null,
    });
  });

  it("GSD-FR-QGTC, GSD-FR-LHQY: it asks the choice where the limit is full, and sends it", async () => {
    capacity = capacityOf(1, 1);
    const { onStarted } = draw();
    await settled();
    await chooseNew();
    await waitFor(() => expect(slotNote()).toBeInTheDocument());
    expect(slotNote()).toHaveTextContent(
      /The new stream is free, but the project already works its limit of 1 graduation run\. This run waits for a project slot\./,
    );
    await userEvent.click(position("commit_and_push"));
    await userEvent.type(
      screen.getByRole("textbox", { name: /Commit message/ }),
      "Land it",
    );
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(invoked).toHaveBeenCalledWith("start_graduation", {
      draftId: "d-1",
      streamId: "made",
      standingWork: "commit_and_push",
      standingWorkMessage: "Land it",
    });
  });
});

describe("a capacity that moves while the dialog is open", () => {
  it("GSD-FR-LHQY: it is read again on every queue change", async () => {
    capacity = capacityOf(2, 1);
    draw();
    await settled();
    expect(slotNote()).toBeNull();
    expect(choice()).toBeNull();
    const before = capacityReads();

    capacity = capacityOf(2, 2);
    await fireQueueChanged();
    await waitFor(() => expect(slotNote()).toBeInTheDocument());
    expect(capacityReads()).toBe(before + 1);
    expect(choice()).toBeInTheDocument();

    capacity = capacityOf(2, 0);
    await fireQueueChanged();
    await waitFor(() => expect(slotNote()).toBeNull());
    expect(choice()).toBeNull();
  });

  it("GSD-FR-LHQY: the dialog stops listening when it closes", async () => {
    draw();
    await settled();
    expect(handlers.some(([name]) => name === "graduation-queue-changed")).toBe(true);
    cleanup();
    expect(handlers.some(([name]) => name === "graduation-queue-changed")).toBe(false);
  });
});
