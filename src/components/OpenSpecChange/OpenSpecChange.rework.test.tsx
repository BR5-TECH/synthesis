/**
 * Rework… and its dialog in a change workspace
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-LWFQ,
 * OCW-FR-FEOA, OCW-FR-SCIA, OCW-FR-BNGB, OCW-FR-FEBP, OCW-FR-NNRY).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { makeRun } from "../../test/graduationFixtures";
import {
  CHANGE,
  STREAM_ID,
  changeBackend,
  changeProposal,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";
import type { ChangeReworkStart, GraduationRun, GraduationRunState } from "../../types";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  invokeMock.mockReset();
  backend = changeBackend({ proposals: [] });
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  cleanup();
  expect(backend.unknown).toEqual([]);
});

function reworkRun(id: string, state: GraduationRunState = "queued"): GraduationRun {
  return makeRun(id, state, {
    streamId: STREAM_ID,
    streamName: "auth-work",
    input: { draftId: "", draftName: "", prompt: "p", promptChecksum: "c", capturedAt: "2026-01-01T00:00:00Z" },
    openspec: { phase: "rework", change: CHANGE, instruction: "Split task 2.", threadId: "t2" },
  });
}

const rework = () => screen.getByRole("button", { name: "Rework…" });
const column = () => screen.getByTestId("openspec-change-discussion");
/** The discussion tab of thread t2 of the fixture: the passage "# Why". */
const t2Tab = () => within(column()).getByTitle("Discussion 2: # Why");
const dialog = () => screen.getByRole("dialog", { name: `Rework ${CHANGE}` });
const field = () => within(dialog()).getByRole("textbox");
const startButton = () => within(dialog()).getByRole("button", { name: "Start rework" });

async function openDialog() {
  const user = userEvent.setup();
  drawChange();
  await screen.findAllByTestId("openspec-artifact");
  await waitFor(() => expect(calls("list_graduation_queue").length).toBeGreaterThan(0));
  await user.click(rework());
  await screen.findByRole("dialog", { name: `Rework ${CHANGE}` });
  return user;
}

describe("the Rework… dialog (OCW-FR-LWFQ, OCW-FR-FEBP)", () => {
  it("OCW-FR-LWFQ: Rework… opens a modal that names the change and the stream, with one instruction field, Start rework, and Cancel", async () => {
    await openDialog();
    expect(dialog()).toHaveTextContent(CHANGE);
    expect(dialog()).toHaveTextContent("auth-work");
    expect(within(dialog()).getAllByRole("textbox")).toHaveLength(1);
    expect(field().tagName).toBe("TEXTAREA");
    expect(within(dialog()).getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    expect(dialog().closest(".scrim")).not.toBeNull();
  });

  it("OCW-FR-LWFQ: Start rework is unavailable while the instruction is blank once trimmed", async () => {
    const user = await openDialog();
    expect(startButton()).toBeDisabled();
    await user.type(field(), "   ");
    expect(startButton()).toBeDisabled();
    await user.type(field(), "Split task 2");
    expect(startButton()).toBeEnabled();
  });

  it("OCW-FR-FEBP: the dialog moves focus to the instruction field when it opens", async () => {
    await openDialog();
    await waitFor(() => expect(field()).toHaveFocus());
  });

  it("OCW-FR-FEBP: Escape closes it, starts nothing, and returns focus to Rework…", async () => {
    const user = await openDialog();
    await user.type(field(), "Split task 2");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("start_change_rework")).toHaveLength(0);
    expect(rework()).toHaveFocus();
  });

  it("OCW-FR-FEBP: Cancel closes it, starts nothing, and returns focus to Rework…", async () => {
    const user = await openDialog();
    await user.click(within(dialog()).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("start_change_rework")).toHaveLength(0);
    expect(rework()).toHaveFocus();
  });

  it("OCW-FR-FEBP: while a start is in flight the dialog's controls are disabled, and Escape does not close it", async () => {
    let release: (start: ChangeReworkStart) => void = () => {};
    const rest = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "start_change_rework") return new Promise<ChangeReworkStart>((resolve) => (release = resolve));
      return rest(cmd, args);
    });
    const user = await openDialog();
    await user.type(field(), "Split task 2");
    await user.click(startButton());
    await waitFor(() => expect(field()).toBeDisabled());
    expect(startButton()).toBeDisabled();
    expect(within(dialog()).getByRole("button", { name: "Cancel" })).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await act(async () => release({ run: reworkRun("g-5"), threadId: "t2" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});

describe("starting a rework (OCW-FR-FEOA, OCW-FR-SCIA)", () => {
  it("OCW-FR-FEOA: Start rework invokes start change rework; on success the dialog closes, the thread is selected, and the strip shows the run", async () => {
    backend.startRework = () => {
      const run = reworkRun("g-5");
      backend.runs = [run];
      return { run, threadId: "t2" };
    };
    const user = await openDialog();
    // Before the start, the discussion column shows another thread.
    await waitFor(() => expect(within(column()).getAllByRole("tab")).toHaveLength(2));
    expect(t2Tab()).toHaveAttribute("aria-selected", "false");
    await user.type(field(), "Split task 2 in two.");
    await user.click(startButton());
    await waitFor(() => expect(calls("start_change_rework")).toHaveLength(1));
    expect(calls("start_change_rework")[0][1]).toEqual({
      streamId: STREAM_ID,
      change: CHANGE,
      instruction: "Split task 2 in two.",
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(t2Tab()).toHaveAttribute("aria-selected", "true"));
    expect(await screen.findByTestId("openspec-run-strip")).toHaveTextContent(`Rework ${CHANGE}`);
  });

  it("OCW-FR-FEOA: a refusal is stated in the dialog, and the dialog keeps the instruction", async () => {
    backend.startRework = () => ({ reject: "openspec_mode_inactive" });
    const user = await openDialog();
    await user.type(field(), "Split task 2.");
    await user.click(startButton());
    const refusal = await within(dialog()).findByTestId("openspec-rework-refusal");
    expect(refusal).toHaveTextContent("OpenSpec mode is not active for this project, so no run started.");
    expect(within(refusal).getByRole("button", { name: "Open OpenSpec settings" })).toBeInTheDocument();
    expect(field()).toHaveValue("Split task 2.");
    expect(field()).toBeEnabled();
  });

  it("OCW-FR-SCIA: proposal_pending states that the change holds a pending edit proposal that the author decides first", async () => {
    backend.startRework = () => ({ reject: "proposal_pending" });
    const user = await openDialog();
    await user.type(field(), "Split task 2.");
    await user.click(startButton());
    expect(await within(dialog()).findByTestId("openspec-rework-refusal")).toHaveTextContent(
      "The change holds a pending edit proposal. Decide it first",
    );
  });

  it("OCW-FR-SCIA: blank_instruction states that the instruction is empty", async () => {
    backend.startRework = () => ({ reject: "blank_instruction" });
    const user = await openDialog();
    await user.type(field(), "x");
    await user.click(startButton());
    expect(await within(dialog()).findByTestId("openspec-rework-refusal")).toHaveTextContent(
      "The instruction is empty.",
    );
  });

  it("OCW-FR-SCIA: a failure that returns a thread id also selects that thread in the discussion column", async () => {
    backend.startRework = () => ({ reject: "write_failed; thread: t2" });
    const user = await openDialog();
    await waitFor(() => expect(within(column()).getAllByRole("tab")).toHaveLength(2));
    expect(t2Tab()).toHaveAttribute("aria-selected", "false");
    await user.type(field(), "Split task 2.");
    await user.click(startButton());
    expect(await within(dialog()).findByTestId("openspec-rework-refusal")).toHaveTextContent(
      "The run could not start (write_failed). No run started.",
    );
    await waitFor(() => expect(t2Tab()).toHaveAttribute("aria-selected", "true"));
  });
});

describe("the availability of Rework… (OCW-FR-BNGB)", () => {
  it("OCW-FR-BNGB: Rework… is unavailable, with a sentence, while a run of the change on this stream is not terminal", async () => {
    backend.runs = [reworkRun("g-5", "working")];
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    await waitFor(() => expect(rework()).toHaveAttribute("aria-disabled", "true"));
    expect(rework()).toHaveAccessibleDescription(/The run “Rework add-logout-3f2a” of this change has not ended./);
    await userEvent.click(rework());
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("OCW-FR-BNGB, OCW-FR-NNRY: Rework… and Apply are unavailable, with one sentence, while the change holds a pending edit proposal", async () => {
    backend.proposals = [changeProposal()];
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    await waitFor(() => expect(rework()).toHaveAttribute("aria-disabled", "true"));
    expect(screen.getByTestId("openspec-rework-block")).toHaveTextContent(
      "The change holds a pending edit proposal. Decide it first.",
    );
    expect(rework()).toHaveAccessibleDescription(/The change holds a pending edit proposal. Decide it first./);
    // OCW-FR-NNRY: a pending proposal holds Apply too, and the reason is said once.
    expect(screen.getByRole("button", { name: "Apply" })).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByTestId("openspec-apply-block")).toHaveTextContent(
      "Apply and Rework… are unavailable. The change holds a pending edit proposal. Decide it first.",
    );
    await userEvent.click(rework());
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("OCW-FR-BNGB: a closed proposal whose commit is owed leaves Rework… available", async () => {
    backend.proposals = [changeProposal(["accepted", "accepted"], { state: "accepted", commitOwed: true })];
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    // Apply is held by the commit owed, which shows the proposals were read.
    await waitFor(() => expect(screen.getByRole("button", { name: "Apply" })).toHaveAttribute("aria-disabled", "true"));
    expect(rework()).not.toHaveAttribute("aria-disabled");
    expect(screen.queryByTestId("openspec-rework-block")).toBeNull();
  });
});
