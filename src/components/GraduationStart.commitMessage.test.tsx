/**
 * The default of the commit message in the start dialog
 * (`../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-HVDN,
 * GSD-FR-MZTB).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import { defaultCommitMessage } from "../state/graduation";
import { makeQueue, makeRun } from "../test/graduationFixtures";
import type { GraduationQueue, GraduationRun, WorkStreamSummary } from "../types";
import { GraduationStart } from "./GraduationStart";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

const invoked = vi.mocked(invoke);

/** One stream with runs queued on it, so the standing-work choice is asked. */
const occupied = (id: string): WorkStreamSummary => ({
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
  },
  queuedRunCount: 1,
  aheadOfBase: 0,
  behindBase: 0,
  baseTipRevision: "a91bc04",
  missingCommits: [],
});

/** One run of a stream, named by the draft it captured. */
const runOf = (
  id: string,
  streamId: string,
  state: GraduationRun["state"],
  draftName: string,
) =>
  makeRun(id, state, {
    streamId,
    input: {
      draftId: `draft-${id}`,
      draftName,
      prompt: "Fix it.",
      promptChecksum: "sha-1",
      capturedAt: "2026-09-06T09:00:00Z",
    },
  });

let streams: WorkStreamSummary[];
let queue: GraduationQueue;
let queueRead: () => Promise<unknown>;
let capacity: unknown;

beforeEach(() => {
  streams = [occupied("alpha"), occupied("beta")];
  queue = makeQueue([]);
  capacity = { limit: 1, inUse: 0, waitingForSlot: [] };
  queueRead = async () => queue;
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    if (command === "list_work_streams") return streams;
    if (command === "list_graduation_queue") return queueRead();
    if (command === "get_graduation_capacity") return capacity;
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
  render(
    <GraduationStart
      draftId="d-1"
      draftName="new-draft"
      onClose={vi.fn()}
      onStarted={vi.fn()}
    />,
  );
}

const picker = () => screen.getByRole("combobox", { name: "Work stream" });
const messageField = () =>
  screen.getByRole("textbox", { name: /Commit message/ }) as HTMLInputElement;
const startCalls = () => invoked.mock.calls.filter(([c]) => c === "start_graduation");

describe("the pure default", () => {
  const runs = [
    runOf("r1", "alpha", "queued", "first"),
    runOf("r2", "beta", "working", "other stream"),
    runOf("r3", "alpha", "working", "second"),
    runOf("r4", "alpha", "completed", "finished"),
    runOf("r5", "alpha", "discarded", "dropped"),
  ];

  it("GSD-FR-HVDN: it is the name of the newest non-terminal run of the stream", () => {
    expect(defaultCommitMessage(runs, "alpha")).toBe("second");
    expect(defaultCommitMessage(runs, "beta")).toBe("other stream");
  });

  it("GSD-FR-HVDN: a terminal run is never the source", () => {
    expect(
      defaultCommitMessage(
        [runOf("r1", "alpha", "completed", "done"), runOf("r2", "alpha", "failed", "broke")],
        "alpha",
      ),
    ).toBe("");
  });

  it("GSD-FR-HVDN: a stream with no run, and an empty stream id, have no default", () => {
    expect(defaultCommitMessage(runs, "gamma")).toBe("");
    expect(defaultCommitMessage(runs, "")).toBe("");
  });
});

describe("the commit message of a start on an existing stream", () => {
  it("GSD-FR-HVDN, GSD-FR-MZTB: it is prefilled with the newest non-terminal run's name, not the new draft's", async () => {
    queue = makeQueue([
      runOf("r1", "alpha", "queued", "older"),
      runOf("r2", "alpha", "working", "newest live"),
      runOf("r3", "alpha", "completed", "newer but finished"),
    ]);
    draw();
    await waitFor(() => expect(messageField()).toHaveValue("newest live"));
    expect(invoked.mock.calls.map(([c]) => c)).toContain("list_graduation_queue");
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await waitFor(() => expect(startCalls()).toHaveLength(1));
    expect(startCalls()[0][1]).toMatchObject({
      streamId: "alpha",
      standingWork: "commit",
      standingWorkMessage: "newest live",
    });
  });

  it("GSD-FR-HVDN: a stream whose runs are all terminal has an empty field, and the run keeps its own name", async () => {
    queue = makeQueue([runOf("r1", "alpha", "completed", "finished")]);
    draw();
    await waitFor(() => expect(messageField()).toBeInTheDocument());
    await waitFor(() => expect(invoked.mock.calls.map(([c]) => c)).toContain("list_graduation_queue"));
    expect(messageField()).toHaveValue("");
    expect(messageField()).toHaveAttribute("placeholder", "new-draft");
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await waitFor(() => expect(startCalls()).toHaveLength(1));
    expect(startCalls()[0][1]).toMatchObject({ standingWorkMessage: null });
  });

  it("GSD-FR-HVDN: another existing stream replaces the field, also where the author edited it", async () => {
    queue = makeQueue([
      runOf("r1", "alpha", "working", "alpha work"),
      runOf("r2", "beta", "queued", "beta work"),
    ]);
    draw();
    await waitFor(() => expect(messageField()).toHaveValue("alpha work"));
    await userEvent.clear(messageField());
    await userEvent.type(messageField(), "my own words");
    expect(messageField()).toHaveValue("my own words");
    await userEvent.selectOptions(picker(), "beta");
    expect(messageField()).toHaveValue("beta work");
    await userEvent.selectOptions(picker(), "alpha");
    expect(messageField()).toHaveValue("alpha work");
  });

  it("GSD-FR-HVDN: an edit stays while the stream stays, also through a queue read that lands late", async () => {
    let release: (value: GraduationQueue) => void = () => {};
    queueRead = () => new Promise((resolve) => (release = resolve as typeof release));
    draw();
    await waitFor(() => expect(messageField()).toBeInTheDocument());
    await userEvent.type(messageField(), "typed first");
    release(makeQueue([runOf("r1", "alpha", "working", "alpha work")]));
    await waitFor(() => expect(invoked.mock.calls.map(([c]) => c)).toContain("list_graduation_queue"));
    await userEvent.type(messageField(), "!");
    expect(messageField()).toHaveValue("typed first!");
  });

  it("GSD-FR-HVDN: a queue that cannot be read leaves the field empty and shows no refusal", async () => {
    queueRead = async () => {
      throw "unknown_stream";
    };
    draw();
    await waitFor(() => expect(messageField()).toBeInTheDocument());
    await waitFor(() => expect(invoked.mock.calls.map(([c]) => c)).toContain("list_graduation_queue"));
    expect(messageField()).toHaveValue("");
    expect(screen.queryByTestId("graduation-start-error")).toBeNull();
    await userEvent.selectOptions(picker(), "beta");
    expect(messageField()).toHaveValue("");
  });
});

describe("the commit message when the destination changes", () => {
  it("GSD-FR-HVDN: New stream under a full limit starts empty, and the start carries no message", async () => {
    capacity = { limit: 1, inUse: 1, waitingForSlot: [] };
    queue = makeQueue([runOf("r1", "alpha", "working", "alpha work")]);
    draw();
    await waitFor(() => expect(messageField()).toHaveValue("alpha work"));
    await userEvent.click(screen.getByRole("radio", { name: "New stream" }));
    await waitFor(() => expect(messageField()).toHaveValue(""));
    await userEvent.type(await screen.findByRole("textbox", { name: "Name" }), "fresh");
    expect(messageField()).toHaveValue("");
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await waitFor(() => expect(startCalls()).toHaveLength(1));
    expect(startCalls()[0][1]).toMatchObject({
      streamId: "made",
      standingWorkMessage: null,
    });
  });

  it("GSD-FR-HVDN: a return to Existing stream applies the selected stream's default again", async () => {
    capacity = { limit: 1, inUse: 1, waitingForSlot: [] };
    queue = makeQueue([runOf("r1", "alpha", "working", "alpha work")]);
    draw();
    await waitFor(() => expect(messageField()).toHaveValue("alpha work"));
    await userEvent.click(screen.getByRole("radio", { name: "New stream" }));
    await waitFor(() => expect(messageField()).toHaveValue(""));
    await userEvent.type(messageField(), "typed under new");
    await userEvent.click(screen.getByRole("radio", { name: "Existing stream" }));
    await waitFor(() => expect(messageField()).toHaveValue("alpha work"));
  });
});
