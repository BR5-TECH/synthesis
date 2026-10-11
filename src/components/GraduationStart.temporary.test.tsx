/**
 * The Temporary stream destination of the start dialog
 * (`../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-BZHW,
 * GSD-FR-QGTC, GSD-FR-TZGT, GSD-FR-WJHN, GSD-FR-XNPD, GSD-FR-RLFC).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import type { DirectGraduationPreflight, WorkStreamSummary } from "../types";
import { GraduationStart } from "./GraduationStart";
import { summary } from "../test/streamFixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

const invoked = vi.mocked(invoke);

const holding = (id: string, temporary: boolean) =>
  summary({ id, name: id, worktreePath: `/data/w/${id}`, temporary });

let streams: WorkStreamSummary[];
let preflight: DirectGraduationPreflight;
let startRefusal: string | null;

beforeEach(() => {
  streams = [];
  preflight = {
    worktreePath: "/data/w/scratch",
    worktreeName: "scratch",
    branch: "synthesis/stream/scratch",
    isDetached: false,
    dirtyPaths: [],
    stream: { streamId: "scratch", streamName: "scratch" },
  };
  startRefusal = null;
  invoked.mockReset();
  invoked.mockImplementation(async (command: string, args?: unknown) => {
    switch (command) {
      case "list_work_streams":
        return streams;
      case "preflight_direct_graduation":
        return preflight;
      case "list_worktrees_and_branches":
        return {
          repositoryRoot: "/r",
          activeWorktreePath: "/r",
          worktrees: [
            {
              path: "/r",
              name: "r",
              branch: "main",
              headShortHash: "a91bc04",
              isDetached: false,
              isActive: true,
              isPrimary: true,
              isMissing: false,
            },
          ],
          branches: [],
        };
      case "create_work_stream": {
        const { name, temporary } = args as { name: string; temporary: boolean };
        const created = holding(name, temporary);
        streams = [...streams, created];
        return created.stream;
      }
      case "start_graduation":
        if (startRefusal) throw startRefusal;
        return { id: "g-1" };
      case "append_log_records":
        return undefined;
      default:
        return undefined;
    }
  });
});
afterEach(cleanup);

function draw() {
  const onStarted = vi.fn();
  render(
    <GraduationStart
      draftId="d-1"
      draftName="Run logs"
      onClose={vi.fn()}
      onStarted={onStarted}
    />,
  );
  return { onStarted };
}

const calls = (command: string) =>
  invoked.mock.calls.filter(([name]) => name === command);
const chooseTemporary = async () => {
  await screen.findByTestId("graduation-destination");
  await userEvent.click(screen.getByRole("radio", { name: "Temporary stream" }));
};

describe("the destination choice", () => {
  it("GSD-FR-BZHW: the destination is one choice of four", async () => {
    streams = [holding("editor-work", false)];
    draw();
    await screen.findByTestId("graduation-destination");
    expect(
      screen.getAllByRole("radio").map((radio) => (radio as HTMLInputElement).value),
    ).toEqual(["stream", "new", "temporary", "direct"]);
    expect(screen.getByRole("radio", { name: "Existing stream" })).toBeChecked();
  });
});

describe("Temporary stream", () => {
  it("GSD-FR-WJHN: the draft's name is the placeholder, and the confirm is enabled with an empty name", async () => {
    draw();
    await chooseTemporary();
    const name = await screen.findByLabelText("Name");
    expect(name).toHaveAttribute("placeholder", "Run logs");
    expect(name).toHaveValue("");
    expect(screen.getByTestId("graduation-start-confirm")).toBeEnabled();
  });

  it("GSD-FR-XNPD: the dialog says the stream is temporary", async () => {
    draw();
    await chooseTemporary();
    expect(await screen.findByTestId("graduation-temporary-note")).toHaveTextContent(
      /temporary.*deleted after the merge/is,
    );
  });

  it("GSD-FR-QGTC, GSD-FR-WJHN, GSD-FR-XNPD: an empty name takes the draft's name, the stream is created temporary, then the run starts on it", async () => {
    const { onStarted } = draw();
    await chooseTemporary();
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await vi.waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("create_work_stream")[0][1]).toEqual({
      name: "Run logs",
      baseBranch: "main",
      temporary: true,
    });
    expect(calls("start_graduation")[0][1]).toMatchObject({
      draftId: "d-1",
      streamId: "Run logs",
    });
  });

  it("GSD-FR-WJHN: a name the author types takes the place of the draft's name", async () => {
    const { onStarted } = draw();
    await chooseTemporary();
    await userEvent.type(await screen.findByLabelText("Name"), "scratch pad");
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await vi.waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("create_work_stream")[0][1]).toMatchObject({
      name: "scratch pad",
      temporary: true,
    });
  });

  it("GSD-FR-QGTC: New stream creates a stream that is not temporary", async () => {
    const { onStarted } = draw();
    await screen.findByTestId("graduation-destination");
    await userEvent.click(screen.getByRole("radio", { name: "New stream" }));
    await userEvent.type(await screen.findByLabelText("Name"), "plain");
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await vi.waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("create_work_stream")[0][1]).toMatchObject({
      name: "plain",
      temporary: false,
    });
  });

  it("GSD-FR-TZGT: a refused start keeps the created stream, and a retry starts on it without a second one", async () => {
    startRefusal = "stream_busy";
    const { onStarted } = draw();
    await chooseTemporary();
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await screen.findByTestId("graduation-start-error");
    expect(calls("create_work_stream")).toHaveLength(1);

    startRefusal = null;
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await vi.waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("create_work_stream")).toHaveLength(1);
    expect(calls("start_graduation")).toHaveLength(2);
  });
});

describe("a temporary stream as a destination", () => {
  it("GSD-FR-RLFC: Existing stream lists it and says in words that it is temporary", async () => {
    streams = [holding("scratch", true), holding("editor-work", false)];
    draw();
    const option = await screen.findByRole("option", { name: /scratch · temporary/ });
    expect(option).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "editor-work" })).toBeInTheDocument();
  });

  it("GSD-FR-RLFC: Work directly is disabled, with the reason in words, in the working copy of a temporary stream", async () => {
    streams = [holding("scratch", true)];
    draw();
    await screen.findByTestId("graduation-destination");
    await userEvent.click(screen.getByRole("radio", { name: "Work directly" }));
    expect(await screen.findByTestId("graduation-direct-blocker")).toHaveTextContent(
      /temporary stream/i,
    );
    expect(screen.getByTestId("graduation-start-confirm")).toBeDisabled();
  });

  it("GSD-FR-RLFC: Work directly in the working copy of a stream that is not temporary stays enabled", async () => {
    streams = [holding("scratch", false)];
    draw();
    await screen.findByTestId("graduation-destination");
    await userEvent.click(screen.getByRole("radio", { name: "Work directly" }));
    await screen.findByTestId("graduation-direct-target");
    expect(screen.queryByTestId("graduation-direct-blocker")).toBeNull();
    expect(screen.getByTestId("graduation-start-confirm")).toBeEnabled();
  });
});

describe("a run on a temporary stream", () => {
  it("GSD-FR-MTKS: a busy temporary stream asks the standing-work choice and the run carries it", async () => {
    streams = [
      summary({
        id: "scratch",
        name: "scratch",
        temporary: true,
        busyRunId: "r-1",
        worktreePath: "/data/w/scratch",
      }),
    ];
    const { onStarted } = draw();
    await screen.findByRole("option", { name: /scratch · temporary/ });
    await userEvent.click(screen.getByRole("radio", { name: /Leave it/ }));
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await vi.waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("start_graduation")[0][1]).toMatchObject({
      streamId: "scratch",
      standingWork: "keep",
    });
  });

  it("GSD-FR-MTKS: a free temporary stream asks nothing and the run carries the resting position", async () => {
    streams = [
      summary({ id: "scratch", name: "scratch", temporary: true, worktreePath: "/data/w/scratch" }),
    ];
    const { onStarted } = draw();
    await screen.findByRole("option", { name: /scratch · temporary/ });
    expect(screen.queryByRole("radio", { name: /Leave it/ })).toBeNull();
    await userEvent.click(screen.getByTestId("graduation-start-confirm"));
    await vi.waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("start_graduation")[0][1]).toMatchObject({ standingWork: "commit" });
  });
});
