import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { flushLogs } from "../../logging";
import type { ChangeDetail, ChangeLocation, OpenSpecChangeTabTarget } from "../../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { OpenSpecChangeWorkspace } from ".";
import { demoChange } from "./fixtures";
import {
  changeBackend,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";

/** What the next reads answer: a detail, or a code to reject with. */
let answers: Array<ChangeDetail | { reject: string }>;
/** A read that waits until the test releases it. */
let hold: { release: () => void } | null;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

/**
 * The rest of the backend a stream location reads: its discussion column and
 * its edit proposals. The change holds no discussion and no proposal here, so
 * each file shows as it is read.
 */
let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  answers = [];
  hold = null;
  invokeMock.mockReset();
  backend = changeBackend({ proposals: [], discussions: [] });
  wireChangeBackend(invokeMock, backend);
  const rest = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "open_settings_window") return undefined;
    if (cmd === "read_openspec_change") {
      if (hold) {
        const held = hold;
        await new Promise<void>((resolve) => {
          held.release = resolve;
        });
      }
      const next = answers.length > 1 ? answers.shift()! : answers[0];
      if (!next) throw new Error("no answer");
      if ("reject" in next) throw next.reject;
      return next;
    }
    return rest(cmd, args);
  });
});
afterEach(() => {
  cleanup();
  expect(backend.unknown).toEqual([]);
});

const STREAM: ChangeLocation = { kind: "stream", streamId: "s-1" };
const WORKTREE: ChangeLocation = { kind: "active_worktree" };

function target(over: Partial<OpenSpecChangeTabTarget> = {}): OpenSpecChangeTabTarget {
  return {
    location: STREAM,
    name: "add-logout-3f2a",
    streamName: "auth-work",
    file: null,
    openSeq: 1,
    ...over,
  };
}

function draw(over: Partial<OpenSpecChangeTabTarget> = {}, tabId = "tab-1") {
  const onOpenArtifact = vi.fn();
  const view = render(
    <OpenSpecChangeWorkspace tabId={tabId} target={target(over)} onOpenArtifact={onOpenArtifact} />,
  );
  return { onOpenArtifact, view };
}

const refresh = () => screen.getByRole("button", { name: "Refresh" });

const artifactButton = (id: string) =>
  screen.getAllByTestId("openspec-artifact").find((b) => b.textContent?.startsWith(id))!;

/** The records the workspace sent to the log. */
function logRecords(): Array<{ level: string; message: string; fields: Record<string, unknown> }> {
  flushLogs();
  return calls("append_log_records").flatMap(
    (c) => (c[1] as { records: Array<{ level: string; message: string; fields: Record<string, unknown> }> }).records,
  );
}

describe("reading the change", () => {
  it("OCW-FR-LABL: reads with its location and name on open, shows a loading state, and reads again on Refresh", async () => {
    answers = [demoChange()];
    hold = { release: () => {} };
    draw();
    expect(screen.getByTestId("openspec-change-loading")).toHaveTextContent("Reading the change");
    await waitFor(() => expect(calls("read_openspec_change")).toHaveLength(1));
    expect(calls("read_openspec_change")[0][1]).toEqual({ location: STREAM, name: "add-logout-3f2a" });
    hold.release();
    hold = null;
    expect(await screen.findByTestId("openspec-reading")).toBeInTheDocument();
    expect(screen.queryByTestId("openspec-change-loading")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(calls("read_openspec_change")).toHaveLength(2));
    // The second read settles before the test ends.
    await waitFor(() => expect(refresh()).not.toBeDisabled());
  });

  it("OCW-FR-LABL / OCW-FR-PFUQ: a new mount, as on reactivation, reads again and shows the kept read meanwhile", async () => {
    answers = [demoChange()];
    const first = draw();
    await screen.findByTestId("openspec-reading");
    first.view.unmount();
    hold = { release: () => {} };
    draw();
    // The kept read shows at once, with no loading state.
    expect(screen.getByTestId("openspec-reading")).toBeInTheDocument();
    expect(screen.queryByTestId("openspec-change-loading")).not.toBeInTheDocument();
    await waitFor(() => expect(calls("read_openspec_change")).toHaveLength(2));
    hold.release();
    hold = null;
    // The second read settles before the test ends.
    await waitFor(() => expect(refresh()).not.toBeDisabled());
    expect(screen.getByTestId("openspec-reading")).toHaveTextContent("Why");
  });

  it("OCW-FR-HPHM: a read emits INFO with the location kind and the counts, and no file text", async () => {
    answers = [demoChange()];
    draw();
    await screen.findByTestId("openspec-reading");
    const record = logRecords().find((r) => r.message === "openspec change read")!;
    expect(record.level).toBe("INFO");
    expect(record.fields).toEqual({ locationKind: "stream", artifactCount: 4, deltaCount: 6 });
    expect(JSON.stringify(logRecords())).not.toContain("log out");
  });

  it("OCW-FR-HPHM: a failed read emits WARN with the location kind and the code", async () => {
    answers = [{ reject: "cli_timed_out" }];
    draw({ location: WORKTREE });
    await screen.findByTestId("openspec-change-failure");
    const record = logRecords().find((r) => r.message === "openspec change could not be read")!;
    expect(record.level).toBe("WARN");
    expect(record.fields).toEqual({ locationKind: "active_worktree", code: "cli_timed_out" });
  });
});

describe("the header and the artifact list", () => {
  it("OCW-FR-LSFO: states the name, the schema, the location, the task progress, and one chip per artifact in order", async () => {
    answers = [demoChange()];
    draw();
    await screen.findByTestId("openspec-reading");
    expect(screen.getByRole("heading", { name: "add-logout-3f2a" })).toBeInTheDocument();
    expect(screen.getByText("spec-driven")).toBeInTheDocument();
    expect(screen.getByTestId("openspec-change-location")).toHaveTextContent("stream auth-work");
    expect(screen.getByTestId("openspec-change-tasks")).toHaveTextContent("1 of 4 tasks");
    const chips = screen.getAllByTestId("openspec-chip");
    expect(chips.map((c) => c.textContent)).toEqual([
      "proposal done",
      "specs done",
      "design ready",
      "tasks blocked",
    ]);
  });

  it("OCW-FR-LSFO: the active worktree location is named as such", async () => {
    answers = [demoChange({ location: WORKTREE })];
    draw({ location: WORKTREE, streamName: undefined });
    await screen.findByTestId("openspec-reading");
    expect(screen.getByTestId("openspec-change-location")).toHaveTextContent("active worktree");
  });

  it("OCW-FR-QIZW: lists each artifact with its state as buttons, marks the selected one, and shows the one selected", async () => {
    answers = [demoChange()];
    draw();
    await screen.findByTestId("openspec-reading");
    const items = screen.getAllByTestId("openspec-artifact");
    expect(items.map((b) => b.textContent)).toEqual([
      "proposaldone",
      "specsdone",
      "designready",
      "tasksblocked",
    ]);
    expect(items.every((b) => b.tagName === "BUTTON")).toBe(true);
    expect(artifactButton("proposal")).toHaveAttribute("aria-current", "true");
    await userEvent.click(artifactButton("design"));
    expect(artifactButton("design")).toHaveAttribute("aria-current", "true");
    expect(artifactButton("proposal")).not.toHaveAttribute("aria-current");
  });

  it("OCW-FR-QIZW: an artifact with no file states that the change holds no file for it", async () => {
    answers = [demoChange()];
    draw();
    await screen.findByTestId("openspec-reading");
    await userEvent.click(artifactButton("tasks"));
    expect(screen.getByTestId("openspec-no-file")).toHaveTextContent(
      "The change holds no file for tasks.",
    );
  });
});

describe("the selection an open names", () => {
  it("OCW-FR-IAPK: an open that names no file selects the first artifact", async () => {
    answers = [demoChange()];
    draw();
    await screen.findByTestId("openspec-reading");
    expect(artifactButton("proposal")).toHaveAttribute("aria-current", "true");
  });

  it("OCW-FR-IAPK: an open that names a file selects the artifact that holds it", async () => {
    answers = [demoChange()];
    draw({ file: "specs/ui/spec.md" });
    await screen.findByTestId("openspec-reading");
    expect(artifactButton("specs")).toHaveAttribute("aria-current", "true");
    expect(screen.getByTestId("openspec-deltas")).toBeInTheDocument();
  });

  it("OCW-FR-IAPK / TAB-FR-KOTW: a later open of the open tab applies its own selection once", async () => {
    answers = [demoChange()];
    const { view, onOpenArtifact } = draw();
    await screen.findByTestId("openspec-reading");
    await userEvent.click(artifactButton("tasks"));
    view.rerender(
      <OpenSpecChangeWorkspace
        tabId="tab-1"
        target={target({ file: "design.md", openSeq: 2 })}
        onOpenArtifact={onOpenArtifact}
      />,
    );
    await waitFor(() => expect(artifactButton("design")).toHaveAttribute("aria-current", "true"));
    // A rerender with the same open changes nothing the author chose since.
    await userEvent.click(artifactButton("proposal"));
    view.rerender(
      <OpenSpecChangeWorkspace
        tabId="tab-1"
        target={target({ file: "design.md", openSeq: 2 })}
        onOpenArtifact={onOpenArtifact}
      />,
    );
    expect(artifactButton("proposal")).toHaveAttribute("aria-current", "true");
  });
});

describe("the files of an artifact", () => {
  it("OCW-FR-NOTT: renders a file as read-only Markdown with no raw HTML, an inert link, and no remote image", async () => {
    answers = [demoChange()];
    draw();
    const pane = await screen.findByTestId("openspec-reading");
    expect(within(pane).getByRole("heading", { name: "Why" })).toBeInTheDocument();
    expect(pane.querySelector("b")).toBeNull();
    expect(pane.textContent).toContain("<b>cannot</b>");
    expect(within(pane).getByText("the docs")).not.toHaveAttribute("href");
    expect(pane.querySelector("a")).toBeNull();
    expect(pane.querySelector("img")).toBeNull();
    expect(pane.querySelector("textarea, [contenteditable='true']")).toBeNull();
  });

  it("OCW-FR-WXFK: a file the read marks unreadable states that it is not text", async () => {
    answers = [demoChange()];
    draw();
    await screen.findByTestId("openspec-reading");
    await userEvent.click(artifactButton("design"));
    expect(screen.getByTestId("openspec-unreadable")).toHaveTextContent("This file is not text");
  });

  it("OCW-FR-WXFK: an unreadable delta file of specs says so beside the deltas", async () => {
    const change = demoChange();
    change.artifacts[1].files.push({ path: "specs/bin/spec.md", text: null, unreadable: true });
    answers = [change];
    draw({ file: "specs/auth/spec.md" });
    await screen.findByTestId("openspec-deltas");
    expect(screen.getByTestId("openspec-unreadable")).toHaveTextContent(
      "specs/bin/spec.md: this file is not text",
    );
  });

  it("OCW-FR-UOKM: in the active worktree each file offers Open in editor, which opens its project path", async () => {
    answers = [demoChange({ location: WORKTREE })];
    const { onOpenArtifact } = draw({ location: WORKTREE });
    await screen.findByTestId("openspec-reading");
    await userEvent.click(screen.getByRole("button", { name: "Open in editor" }));
    expect(onOpenArtifact).toHaveBeenCalledWith({
      id: "openspec/changes/add-logout-3f2a/proposal.md",
      name: "proposal.md",
    });
    await userEvent.click(artifactButton("specs"));
    const auth = screen.getByRole("region", { name: "Capability auth" });
    await userEvent.click(within(auth).getByRole("button", { name: "Open in editor" }));
    expect(onOpenArtifact).toHaveBeenLastCalledWith({
      id: "openspec/changes/add-logout-3f2a/specs/auth/spec.md",
      name: "spec.md",
    });
  });

  it("OCW-FR-UOKM: on a stream no file offers Open in editor", async () => {
    answers = [demoChange()];
    draw();
    await screen.findByTestId("openspec-reading");
    expect(screen.queryByRole("button", { name: "Open in editor" })).not.toBeInTheDocument();
    await userEvent.click(artifactButton("specs"));
    expect(screen.queryByRole("button", { name: "Open in editor" })).not.toBeInTheDocument();
  });
});

describe("a read that fails", () => {
  it("OCW-FR-RFGG: change_not_found states that the change is not in that location any more", async () => {
    answers = [{ reject: "change_not_found" }];
    draw();
    expect(await screen.findByTestId("openspec-change-failure")).toHaveTextContent(
      "The change “add-logout-3f2a” is not in this work stream any more.",
    );
  });

  it("OCW-FR-RFGG: change_not_found in the active worktree names that location, and offers no Retry", async () => {
    answers = [{ reject: "change_not_found" }];
    draw({ location: WORKTREE, streamName: undefined });
    const failure = await screen.findByTestId("openspec-change-failure");
    expect(failure).toHaveTextContent(
      "The change “add-logout-3f2a” is not in the active worktree any more.",
    );
    expect(within(failure).queryByRole("button")).toBeNull();
    await waitFor(() => expect(refresh()).not.toBeDisabled());
  });

  it("OCW-FR-RFGG: unknown_stream states that the work stream is not there any more, with Retry", async () => {
    answers = [{ reject: "unknown_stream" }];
    draw();
    const failure = await screen.findByTestId("openspec-change-failure");
    expect(failure).toHaveTextContent(
      "The change could not be read: its work stream is not there any more.",
    );
    expect(within(failure).getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(within(failure).queryByRole("button", { name: "Open OpenSpec settings" })).toBeNull();
    await waitFor(() => expect(refresh()).not.toBeDisabled());
  });

  it.each(["openspec_mode_inactive", "cli_unavailable"])(
    "OCW-FR-RFGG: %s states that OpenSpec cannot be read, with a route to Project settings",
    async (code) => {
      answers = [{ reject: code }];
      draw();
      const failure = await screen.findByTestId("openspec-change-failure");
      expect(failure).toHaveTextContent("OpenSpec cannot be read");
      await userEvent.click(within(failure).getByRole("button", { name: "Open OpenSpec settings" }));
      await waitFor(() =>
        expect(calls("open_settings_window")[0][1]).toEqual({ section: "project/openspec" }),
      );
    },
  );

  it("OCW-FR-RFGG: every other code states that the read failed, and Retry reads again", async () => {
    answers = [{ reject: "cli_failed: 2" }, demoChange()];
    draw();
    const failure = await screen.findByTestId("openspec-change-failure");
    expect(failure).toHaveTextContent("The change could not be read");
    await userEvent.click(within(failure).getByRole("button", { name: "Retry" }));
    expect(await screen.findByTestId("openspec-reading")).toBeInTheDocument();
    expect(calls("read_openspec_change")).toHaveLength(2);
  });

  it("OCW-FR-PFUQ: a failure after a read that succeeded keeps the content and states the failure above it, with Retry", async () => {
    answers = [demoChange(), { reject: "cli_output_unreadable" }, demoChange()];
    draw();
    await screen.findByTestId("openspec-reading");
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    const stale = await screen.findByTestId("openspec-change-stale");
    expect(stale).toHaveTextContent("could not be read");
    expect(screen.getByTestId("openspec-reading")).toHaveTextContent("Why");
    await userEvent.click(within(stale).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.queryByTestId("openspec-change-stale")).not.toBeInTheDocument());
    expect(calls("read_openspec_change")).toHaveLength(3);
  });
});

describe("a read after a reactivation", () => {
  it("OCW-FR-PFUQ / OCW-FR-LABL: a remount whose read fails shows the kept read with the failure above it, and Retry reads again", async () => {
    answers = [demoChange()];
    const first = draw();
    await screen.findByTestId("openspec-reading");
    first.view.unmount();
    answers = [{ reject: "cli_failed" }, demoChange({ completedTasks: 3 })];
    draw();
    const stale = await screen.findByTestId("openspec-change-stale");
    expect(stale).toHaveTextContent("The change could not be read: the OpenSpec CLI failed.");
    // The failure stands above the kept content, not in its place.
    expect(screen.queryByTestId("openspec-change-failure")).toBeNull();
    expect(screen.getByTestId("openspec-reading")).toHaveTextContent("Why");
    expect(screen.getByTestId("openspec-change-tasks")).toHaveTextContent("1 of 4 tasks");
    expect(
      stale.compareDocumentPosition(screen.getByTestId("openspec-reading")) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    await userEvent.click(within(stale).getByRole("button", { name: "Retry" }));
    await waitFor(() =>
      expect(screen.getByTestId("openspec-change-tasks")).toHaveTextContent("3 of 4 tasks"),
    );
    expect(screen.queryByTestId("openspec-change-stale")).toBeNull();
    expect(calls("read_openspec_change")).toHaveLength(3);
    await waitFor(() => expect(refresh()).not.toBeDisabled());
  });
});

describe("reads that overlap", () => {
  /** Each read waits on its own answer, which the test gives in any order. */
  function deferReads() {
    const pending: Array<{ answer: Promise<unknown>; settle: (v: ChangeDetail | { reject: string }) => void }> = [];
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd !== "read_openspec_change") return base(cmd, args);
      let settle: (v: ChangeDetail | { reject: string }) => void = () => {};
      const answer = new Promise<unknown>((resolve, reject) => {
        settle = (v) => ("reject" in v ? reject(v.reject) : resolve(v));
      });
      pending.push({ answer, settle });
      return answer;
    });
    return pending;
  }

  /**
   * A first read that succeeds, a Refresh that fails, and two Retry reads
   * that are both in progress. Retry is offered while a read is in progress,
   * so two reads overlap.
   */
  async function twoReadsInProgress() {
    const pending = deferReads();
    draw();
    await waitFor(() => expect(pending).toHaveLength(1));
    await act(async () => {
      pending[0].settle(demoChange());
      await pending[0].answer;
    });
    await userEvent.click(refresh());
    await waitFor(() => expect(pending).toHaveLength(2));
    await act(async () => {
      pending[1].settle({ reject: "cli_timed_out" });
      await pending[1].answer.catch(() => {});
    });
    const stale = screen.getByTestId("openspec-change-stale");
    await userEvent.click(within(stale).getByRole("button", { name: "Retry" }));
    await userEvent.click(within(stale).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(pending).toHaveLength(4));
    return pending;
  }

  it.each<[string, ChangeDetail | { reject: string }]>([
    ["an answer", demoChange({ completedTasks: 2 })],
    ["a failure", { reject: "cli_failed" }],
  ])(
    "OCW-FR-LABL / OCW-FR-PFUQ: the earlier read that gives %s first changes nothing while the later read is in progress",
    async (_, early) => {
      const pending = await twoReadsInProgress();
      await act(async () => {
        pending[2].settle(early);
        await pending[2].answer.catch(() => {});
      });
      // The screen still shows the state before the two reads, and a read in progress.
      expect(screen.getByTestId("openspec-change-tasks")).toHaveTextContent("1 of 4 tasks");
      expect(screen.getByTestId("openspec-change-stale")).toHaveTextContent(
        "the OpenSpec CLI did not answer in time",
      );
      expect(refresh()).toBeDisabled();
      await act(async () => {
        pending[3].settle(demoChange({ completedTasks: 4 }));
        await pending[3].answer;
      });
      expect(screen.getByTestId("openspec-change-tasks")).toHaveTextContent("4 of 4 tasks");
      expect(screen.queryByTestId("openspec-change-stale")).toBeNull();
      expect(refresh()).not.toBeDisabled();
    },
  );

  it.each<[string, ChangeDetail | { reject: string }]>([
    ["an answer", demoChange({ completedTasks: 2 })],
    ["a failure", { reject: "cli_failed" }],
  ])(
    "OCW-FR-LABL / OCW-FR-PFUQ: the later read stands when the earlier read gives %s last",
    async (_, late) => {
      const pending = await twoReadsInProgress();
      await act(async () => {
        pending[3].settle(demoChange({ completedTasks: 4 }));
        await pending[3].answer;
      });
      expect(screen.getByTestId("openspec-change-tasks")).toHaveTextContent("4 of 4 tasks");
      expect(screen.queryByTestId("openspec-change-stale")).toBeNull();
      expect(refresh()).not.toBeDisabled();
      await act(async () => {
        pending[2].settle(late);
        await pending[2].answer.catch(() => {});
      });
      expect(screen.getByTestId("openspec-change-tasks")).toHaveTextContent("4 of 4 tasks");
      expect(screen.queryByTestId("openspec-change-stale")).toBeNull();
      expect(refresh()).not.toBeDisabled();
      // The answer that came too late is not logged as a read.
      const reads = logRecords().filter((r) => r.message.startsWith("openspec change"));
      expect(reads.map((r) => r.message)).toEqual([
        "openspec change read",
        "openspec change could not be read",
        "openspec change read",
      ]);
    },
  );
});
