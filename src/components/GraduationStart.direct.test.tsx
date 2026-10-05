/**
 * The destination a graduation starts on: an existing stream, a stream the
 * dialog creates, or the active worktree
 * (`../../specifications/ui/GSD-graduation-start-dialog.md`).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";

import type { DirectGraduationPreflight, WorkStreamSummary } from "../types";
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
const fireContextChanged = () =>
  act(() => {
    handlers
      .filter(([name]) => name === "worktree-context-changed")
      .forEach(([, h]) => h({ payload: {} }));
  });

const invoked = vi.mocked(invoke);

const stream = (id: string): WorkStreamSummary => ({
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
  queuedRunCount: 0,
  aheadOfBase: 0,
  behindBase: 0,
  baseTipRevision: "a91bc04",
  missingCommits: [],
});

const clean: DirectGraduationPreflight = {
  worktreePath: "/Users/demo/dev/acme-main",
  worktreeName: "acme-main",
  branch: "feature/new-window",
  isDetached: false,
  dirtyPaths: [],
};

let streams: WorkStreamSummary[];
let preflight: DirectGraduationPreflight | string;
let startRefusal: string | null;
let createRefusal: string | null;

beforeEach(() => {
  streams = [stream("editor-work")];
  preflight = clean;
  startRefusal = null;
  createRefusal = null;
  handlers = [];
  invoked.mockReset();
  invoked.mockImplementation(async (command: string, args?: unknown) => {
    switch (command) {
      case "list_work_streams":
        return streams;
      case "preflight_direct_graduation":
        if (typeof preflight === "string") throw preflight;
        return preflight;
      case "list_worktrees_and_branches":
        return {
          repositoryRoot: "/Users/demo/dev/acme",
          activeWorktreePath: "/Users/demo/dev/acme",
          worktrees: [
            {
              path: "/Users/demo/dev/acme",
              name: "acme",
              branch: "main",
              headShortHash: "a91bc04",
              isDetached: false,
              isActive: true,
              isPrimary: true,
              isMissing: false,
            },
          ],
          branches: [{ name: "release/2.1", kind: "local", headShortHash: "b1" }],
        };
      case "create_work_stream": {
        if (createRefusal) throw createRefusal;
        const { name } = args as { name: string };
        streams = [...streams, stream(name)];
        return stream(name).stream;
      }
      case "start_graduation":
      case "start_direct_graduation":
        if (startRefusal) throw startRefusal;
        return { id: "g-1" };
      default:
        return undefined;
    }
  });
});
afterEach(cleanup);

function draw() {
  const onClose = vi.fn();
  const onStarted = vi.fn();
  render(
    <GraduationStart
      draftId="d-1"
      draftName="artifact-window"
      onClose={onClose}
      onStarted={onStarted}
    />,
  );
  return { onClose, onStarted };
}

const radio = (name: string) => screen.getByRole("radio", { name });
const graduate = () => screen.getByTestId("graduation-start-confirm");
const calls = (command: string) =>
  invoked.mock.calls.filter(([name]) => name === command);
const chooseDirectly = async () => {
  await screen.findByTestId("graduation-destination");
  await userEvent.click(radio("Work directly"));
};

describe("where the run works", () => {
  it("GSD-FR-BZHW: the destination is one choice of three, resting on an existing stream", async () => {
    draw();
    await screen.findByTestId("graduation-destination");
    expect(radio("Existing stream")).toBeChecked();
    expect(radio("New stream")).not.toBeChecked();
    expect(radio("Work directly")).not.toBeChecked();
    expect(graduate()).toHaveTextContent("Graduate");
  });

  it("GSD-FR-BZHW: each choice shows only its own fields", async () => {
    draw();
    await screen.findByTestId("graduation-destination");
    expect(screen.getByRole("combobox", { name: "Work stream" })).toBeInTheDocument();
    expect(screen.queryByTestId("graduation-new-stream")).toBeNull();
    expect(screen.queryByTestId("graduation-direct")).toBeNull();

    await userEvent.click(radio("New stream"));
    expect(screen.getByTestId("graduation-new-stream")).toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "Work stream" })).toBeNull();
    expect(graduate()).toHaveTextContent("Create stream and graduate");

    await userEvent.click(radio("Work directly"));
    expect(await screen.findByTestId("graduation-direct")).toBeInTheDocument();
    expect(screen.queryByTestId("graduation-new-stream")).toBeNull();
    expect(graduate()).toHaveTextContent("Graduate here");
  });

  it("GSD-FR-BZHW: the choices are operable by keyboard alone", async () => {
    draw();
    await screen.findByTestId("graduation-destination");
    await userEvent.tab();
    // Every choice is a native radio, so the keyboard reaches and selects each.
    act(() => radio("New stream").focus());
    await userEvent.keyboard(" ");
    expect(radio("New stream")).toBeChecked();
    act(() => radio("Work directly").focus());
    await userEvent.keyboard(" ");
    expect(radio("Work directly")).toBeChecked();
  });

  it("GSD-FR-LDGM: a project with no stream rests on New stream, and the other two stay offered", async () => {
    streams = [];
    draw();
    await screen.findByTestId("graduation-start-no-streams");
    expect(radio("New stream")).toBeChecked();
    expect(radio("Existing stream")).toBeDisabled();
    expect(radio("Work directly")).toBeEnabled();
  });
});

describe("working directly", () => {
  it("GSD-FR-FQQP: opening the dialog reads no working copy", async () => {
    draw();
    await screen.findByTestId("graduation-destination");
    expect(calls("preflight_direct_graduation")).toHaveLength(0);
  });

  it("GSD-FR-FQQP: it names the worktree, its path and its branch, and asks nothing else", async () => {
    draw();
    await chooseDirectly();
    const target = await screen.findByTestId("graduation-direct-target");
    expect(target).toHaveTextContent("acme-main");
    expect(target).toHaveTextContent("feature/new-window");
    expect(target).toHaveTextContent("commits there");
    expect(screen.getByTestId("graduation-direct-path")).toHaveTextContent(
      "/Users/demo/dev/acme-main",
    );
    // No standing-work choice and no commit message.
    expect(screen.queryByTestId("standing-work-choice")).toBeNull();
    expect(screen.queryByRole("textbox", { name: /Commit message/ })).toBeNull();
    expect(graduate()).toBeEnabled();
  });

  it("GSD-FR-FQQP: a worktree that is a stream's working copy says which stream", async () => {
    preflight = {
      ...clean,
      stream: { streamId: "s1", streamName: "Editor work" },
    };
    draw();
    await chooseDirectly();
    expect(await screen.findByTestId("graduation-direct-stream")).toHaveTextContent(
      "Editor work",
    );
  });

  it("GSD-FR-USOH: the start is bound to the worktree and branch the dialog showed", async () => {
    const { onStarted } = draw();
    await chooseDirectly();
    await screen.findByTestId("graduation-direct-target");
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalledWith({ id: "g-1" }));
    expect(calls("start_direct_graduation")[0][1]).toEqual({
      draftId: "d-1",
      expectedWorktree: "/Users/demo/dev/acme-main",
      expectedBranch: "feature/new-window",
    });
    expect(calls("start_graduation")).toHaveLength(0);
  });

  it("GSD-FR-USOH: a detached worktree disables the choice and says why in words", async () => {
    preflight = { ...clean, branch: null, isDetached: true };
    draw();
    await chooseDirectly();
    expect(await screen.findByTestId("graduation-direct-blocker")).toHaveTextContent(
      /not on a branch/,
    );
    expect(graduate()).toBeDisabled();
    await userEvent.click(graduate());
    expect(calls("start_direct_graduation")).toHaveLength(0);
  });

  it("GSD-FR-USOH: a dirty worktree names every uncommitted path and is not confirmable", async () => {
    preflight = { ...clean, dirtyPaths: ["a.txt", "src/b.ts"] };
    draw();
    await chooseDirectly();
    const list = await screen.findByTestId("graduation-direct-dirty");
    expect(within(list).getAllByRole("listitem").map((i) => i.textContent)).toEqual([
      "a.txt",
      "src/b.ts",
    ]);
    expect(screen.getByTestId("graduation-direct-blocker")).toHaveTextContent(
      /Commit or discard/,
    );
    expect(graduate()).toBeDisabled();
  });

  it("GSD-FR-USOH: a preflight that cannot be read disables the choice with the refusal beside it", async () => {
    preflight = "not_a_git_repository";
    draw();
    await chooseDirectly();
    const alert = await screen.findByTestId("graduation-direct-unreadable");
    expect(alert).toHaveTextContent(/not in a Git repository/);
    expect(alert.textContent).not.toContain("not_a_git_repository");
    expect(graduate()).toBeDisabled();
  });

  it("GSD-FR-YBLC: the preflight is read again when the worktree or its branch changes", async () => {
    draw();
    await chooseDirectly();
    await screen.findByTestId("graduation-direct-target");
    preflight = { ...clean, branch: "release/2.1" };
    await fireContextChanged();
    await waitFor(() =>
      expect(screen.getByTestId("graduation-direct-target")).toHaveTextContent(
        "release/2.1",
      ),
    );
    expect(calls("preflight_direct_graduation")).toHaveLength(2);
  });

  it("GSD-FR-JYRP, GSD-FR-YBLC: a refusal naming a changed branch is read in words, the dialog stays open, and the state is read again", async () => {
    const { onStarted, onClose } = draw();
    await chooseDirectly();
    await screen.findByTestId("graduation-direct-target");
    startRefusal = "direct_branch_changed";
    preflight = { ...clean, branch: "release/2.1" };
    await userEvent.click(graduate());
    const alert = await screen.findByTestId("graduation-start-error");
    expect(alert).toHaveTextContent(/different branch/);
    expect(alert.textContent).not.toContain("direct_branch_changed");
    expect(onStarted).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByTestId("graduation-direct-target")).toHaveTextContent(
        "release/2.1",
      ),
    );
  });

  it("GSD-FR-JYRP, GSD-FR-YBLC: a dirty refusal names the paths and the dialog reads the worktree again", async () => {
    draw();
    await chooseDirectly();
    await screen.findByTestId("graduation-direct-target");
    startRefusal = "direct_worktree_dirty: a.txt, b.txt";
    preflight = { ...clean, dirtyPaths: ["a.txt", "b.txt"] };
    await userEvent.click(graduate());
    expect(await screen.findByTestId("graduation-start-error")).toHaveTextContent(
      "a.txt, b.txt",
    );
    await waitFor(() => expect(graduate()).toBeDisabled());
  });

  it("GSD-FR-BFOU: one start is in flight at a time", async () => {
    let release: (value: unknown) => void = () => undefined;
    invoked.mockImplementation(async (command: string) => {
      if (command === "list_work_streams") return streams;
      if (command === "preflight_direct_graduation") return clean;
      if (command === "start_direct_graduation") {
        return new Promise((resolve) => {
          release = resolve;
        });
      }
      return undefined;
    });
    draw();
    await chooseDirectly();
    await screen.findByTestId("graduation-direct-target");
    await userEvent.click(graduate());
    expect(graduate()).toBeDisabled();
    expect(radio("New stream")).toBeDisabled();
    await userEvent.click(graduate());
    expect(calls("start_direct_graduation")).toHaveLength(1);
    release({ id: "g-1" });
  });
});

describe("a stream the dialog creates", () => {
  it("GSD-FR-QGTC: it asks a name and the branch, pre-filled with the active worktree's", async () => {
    draw();
    await screen.findByTestId("graduation-destination");
    await userEvent.click(radio("New stream"));
    const branch = await screen.findByRole("combobox", { name: "Created from" });
    await waitFor(() => expect(branch).toHaveValue("main"));
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveFocus();
    // The stream is free, so no standing-work choice is asked.
    expect(screen.queryByTestId("standing-work-choice")).toBeNull();
    expect(graduate()).toBeDisabled();
  });

  it("GSD-FR-QGTC: confirming creates the stream and then starts the run on it", async () => {
    const { onStarted } = draw();
    await screen.findByTestId("graduation-destination");
    await userEvent.click(radio("New stream"));
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Created from" })).toHaveValue("main"),
    );
    await userEvent.type(screen.getByRole("textbox", { name: "Name" }), "fresh");
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("create_work_stream")[0][1]).toEqual({ name: "fresh", baseBranch: "main" });
    expect(calls("start_graduation")[0][1]).toEqual({
      draftId: "d-1",
      streamId: "fresh",
      standingWork: "commit",
      standingWorkMessage: null,
    });
    const order = invoked.mock.calls.map(([name]) => name);
    expect(order.indexOf("create_work_stream")).toBeLessThan(
      order.indexOf("start_graduation"),
    );
  });

  it("GSD-FR-QGTC: a refused name renders against the name field, starts nothing, and the dialog stays", async () => {
    createRefusal = "stream_name_taken";
    const { onStarted } = draw();
    await screen.findByTestId("graduation-destination");
    await userEvent.click(radio("New stream"));
    await userEvent.type(screen.getByRole("textbox", { name: "Name" }), "editor-work");
    await userEvent.click(graduate());
    const refusal = await screen.findByTestId("graduation-new-stream-error");
    expect(refusal).toHaveTextContent(/already uses that name/);
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveAttribute(
      "aria-invalid",
      "true",
    );
    expect(calls("start_graduation")).toHaveLength(0);
    expect(onStarted).not.toHaveBeenCalled();
    // The author can try another name.
    expect(graduate()).toBeEnabled();
  });

  it("GSD-FR-TZGT, GSD-FR-JYRP: a refused start keeps the stream, selects it, and a retry makes no second stream", async () => {
    const { onStarted } = draw();
    await screen.findByTestId("graduation-destination");
    await userEvent.click(radio("New stream"));
    await userEvent.type(screen.getByRole("textbox", { name: "Name" }), "fresh");
    startRefusal = "vendor_image_unconfigured";
    await userEvent.click(graduate());

    expect(await screen.findByTestId("graduation-start-error")).toHaveTextContent(
      /no agent image/,
    );
    // The stream stands, is listed, and is the one selected.
    await waitFor(() => expect(radio("Existing stream")).toBeChecked());
    const picker = screen.getByRole("combobox", { name: "Work stream" });
    expect(picker).toHaveValue("fresh");
    expect(within(picker).getAllByRole("option").map((o) => o.textContent)).toContain(
      "fresh",
    );

    startRefusal = null;
    await userEvent.click(graduate());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("create_work_stream")).toHaveLength(1);
    expect(calls("start_graduation")).toHaveLength(2);
    expect(calls("start_graduation")[1][1]).toMatchObject({ streamId: "fresh" });
  });
});
