import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { CHANGE_NAME_RULE } from "../state/openspec/changeName";
import type { OpenSpecStatus, WorkStreamSummary } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

import { GraduationStart } from "./GraduationStart";
import { DIRECT_PROPOSE_REASON } from "./GraduationStart/proposeChange";

const stream = (id: string): WorkStreamSummary =>
  ({
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
  }) as WorkStreamSummary;

const ACTIVE: OpenSpecStatus = {
  setting: "auto",
  usesOpenspec: true,
  active: true,
  cli: { state: "available", path: "/usr/local/bin/openspec", version: "1.17.0" },
};

/** What `get_openspec_status` answers, or a code it rejects with. */
let status: OpenSpecStatus | { reject: string };
let refusal: unknown;
let streams: WorkStreamSummary[];

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  status = ACTIVE;
  refusal = null;
  streams = [stream("editor-work")];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "append_log_records":
        return undefined;
      case "get_openspec_status":
        if ("reject" in status) throw status.reject;
        return status;
      case "list_work_streams":
        return streams;
      case "get_graduation_capacity":
        return { limit: 2, inUse: 0, waitingForSlot: [] };
      case "list_graduation_queue":
        return { projectKey: "/Users/demo/dev/acme", runs: [] };
      case "list_worktrees_and_branches":
        return { worktrees: [], branches: [{ name: "main" }] };
      case "get_branch_write_guard":
        return { guarded: false };
      case "preflight_direct_graduation":
        return {
          worktreePath: "/Users/demo/dev/acme",
          worktreeName: "acme",
          branch: "feature",
          isDetached: false,
          dirtyPaths: [],
        };
      case "create_work_stream":
        return { id: "made", name: "made" };
      case "open_settings_window":
        return undefined;
      case "start_graduation":
        if (refusal) throw refusal;
        return { id: "g-1" };
      case "start_direct_graduation":
        return { id: "g-2" };
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
});
afterEach(cleanup);

function draw(draftName = "Add Logout Button", draftId = "3f2a9c11") {
  const onStarted = vi.fn();
  render(
    <GraduationStart draftId={draftId} draftName={draftName} onClose={vi.fn()} onStarted={onStarted} />,
  );
  return { onStarted };
}

const confirm = () => screen.getByTestId("graduation-start-confirm");
const graduateAs = () => screen.queryByRole("radiogroup", { name: "Graduate as" });
const nameField = () => screen.getByRole("textbox", { name: "Change name" });
const segment = (name: string) => screen.getByRole("radio", { name });

/** Wait until the dialog has read the streams and the status. */
async function settled() {
  await screen.findByRole("combobox", { name: "Work stream" });
  await waitFor(() => expect(calls("get_openspec_status")).toHaveLength(1));
  await waitFor(() => expect(confirm()).not.toBeDisabled());
}

describe("what the run makes while OpenSpec mode is active", () => {
  it("GSD-FR-DCLQ / GSD-FR-VKLD: reads the status once on open and asks Graduate as, resting on Propose change", async () => {
    draw();
    await settled();
    expect(graduateAs()).toBeInTheDocument();
    expect(segment("Propose change")).toHaveAttribute("aria-checked", "true");
    expect(segment("Implement")).toHaveAttribute("aria-checked", "false");
    expect(calls("get_openspec_status")).toHaveLength(1);
  });

  it("GSD-FR-WMTD, GSD-FR-DCLQ: on open, focus goes to the selected Graduate as segment, the first thing asked", async () => {
    draw();
    await settled();
    await waitFor(() => expect(segment("Propose change")).toHaveFocus());
  });

  it("GSD-FR-WMTD: while the mode is not active, focus goes to the stream picker as before", async () => {
    status = { ...ACTIVE, active: false };
    draw();
    await settled();
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Work stream" })).toHaveFocus());
  });

  it("GSD-FR-DCLQ: while the mode is not active the dialog asks nothing of it and starts a draft run", async () => {
    status = { ...ACTIVE, active: false };
    const { onStarted } = draw();
    await settled();
    expect(graduateAs()).toBeNull();
    expect(screen.queryByRole("textbox", { name: "Change name" })).toBeNull();
    await userEvent.click(confirm());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("start_graduation")[0][1]).toMatchObject({ openspecChange: null });
  });

  it("GSD-FR-DCLQ: a status that cannot be read shows no choice and starts a draft run", async () => {
    status = { reject: "no_open_project" };
    const { onStarted } = draw();
    await settled();
    expect(graduateAs()).toBeNull();
    await userEvent.click(confirm());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("start_graduation")[0][1]).toMatchObject({ openspecChange: null });
  });

  it("GSD-FR-CKSV: Propose change shows the Change name field, prefilled from the draft name and id", async () => {
    draw("Add Logout Button!", "3F2A9c-77");
    await settled();
    expect(nameField()).toHaveValue("add-logout-button-3f2a");
  });

  it("GSD-FR-DCBH: a name that is not valid disables the confirm and states the rule under the field", async () => {
    draw();
    await settled();
    await userEvent.clear(nameField());
    await userEvent.type(nameField(), "Bad_Name");
    expect(confirm()).toBeDisabled();
    expect(screen.getByTestId("graduation-change-name-error")).toHaveTextContent(CHANGE_NAME_RULE);
    expect(nameField()).toHaveAttribute("aria-invalid", "true");
    await userEvent.clear(nameField());
    await userEvent.type(nameField(), "good-name");
    expect(confirm()).not.toBeDisabled();
    expect(screen.queryByTestId("graduation-change-name-error")).toBeNull();
  });

  it.each([
    ["openspec_change_exists", "already on that stream"],
    ["invalid_change_name", CHANGE_NAME_RULE],
  ])("GSD-FR-DCBH: a refusal %s renders against the field", async (code, words) => {
    refusal = code;
    const { onStarted } = draw();
    await settled();
    await userEvent.click(confirm());
    const error = await screen.findByTestId("graduation-change-name-error");
    expect(error).toHaveTextContent(words);
    expect(screen.queryByTestId("graduation-start-error")).toBeNull();
    expect(onStarted).not.toHaveBeenCalled();
  });

  it("GSD-FR-AGPL: Propose change offers the three stream destinations and disables Work directly with the reason", async () => {
    draw();
    await settled();
    expect(segment("Existing stream")).not.toBeDisabled();
    expect(segment("New stream")).not.toBeDisabled();
    expect(segment("Temporary stream")).not.toBeDisabled();
    expect(segment("Work directly")).toBeDisabled();
    expect(screen.getByTestId("graduation-direct-reason")).toHaveTextContent(DIRECT_PROPOSE_REASON);
  });

  it("GSD-FR-AGPL: Implement offers all four destinations", async () => {
    draw();
    await settled();
    await userEvent.click(segment("Implement"));
    expect(segment("Work directly")).not.toBeDisabled();
    expect(screen.queryByTestId("graduation-direct-reason")).toBeNull();
    expect(screen.queryByRole("textbox", { name: "Change name" })).toBeNull();
  });

  it("GSD-FR-AGPL: a choice of Propose change after Work directly moves the run to a stream", async () => {
    draw();
    await settled();
    await userEvent.click(segment("Implement"));
    await userEvent.click(segment("Work directly"));
    await userEvent.click(segment("Propose change"));
    expect(segment("Existing stream")).toHaveAttribute("aria-checked", "true");
  });

  it("GSD-FR-ZAFX: confirming Propose change invokes start_graduation with the change name, and the confirm names it", async () => {
    const { onStarted } = draw();
    await settled();
    expect(confirm().textContent).toBe("Propose change");
    await userEvent.click(confirm());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("start_graduation")[0][1]).toEqual({
      draftId: "3f2a9c11",
      streamId: "editor-work",
      standingWork: "commit",
      standingWorkMessage: null,
      openspecChange: "add-logout-button-3f2a",
    });
  });

  it("GSD-FR-ZAFX: Propose change on a new stream creates the stream first, then starts with the change name", async () => {
    const { onStarted } = draw();
    await settled();
    await userEvent.click(segment("New stream"));
    await userEvent.type(screen.getByRole("textbox", { name: "Name" }), "made");
    // The confirm names the propose run, not the destination.
    expect(confirm().textContent).toBe("Propose change");
    await userEvent.click(confirm());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    const order = invokeMock.mock.calls.map(([cmd]) => cmd);
    expect(order.indexOf("create_work_stream")).toBeLessThan(order.indexOf("start_graduation"));
    expect(calls("start_graduation")[0][1]).toMatchObject({
      streamId: "made",
      openspecChange: "add-logout-button-3f2a",
    });
  });

  it("GSD-FR-ZAFX / GSD-FR-BZHW: confirming Implement starts what the destination names, with no change name", async () => {
    const { onStarted } = draw();
    await settled();
    await userEvent.click(segment("Implement"));
    // The confirm names the destination, Existing stream, and not the propose run.
    expect(confirm().textContent).toBe("Graduate");
    await userEvent.click(confirm());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("start_graduation")[0][1]).toMatchObject({ openspecChange: null });
  });

  it("GSD-FR-ZAFX: the confirm of Propose change names the propose run on a temporary stream too", async () => {
    draw();
    await settled();
    await userEvent.click(segment("Temporary stream"));
    expect(segment("Temporary stream")).toHaveAttribute("aria-checked", "true");
    expect(confirm().textContent).toBe("Propose change");
  });

  it("GSD-FR-ZAFX / GSD-FR-BZHW: the confirm of Implement names each other destination", async () => {
    draw();
    await settled();
    await userEvent.click(segment("Implement"));
    await userEvent.click(segment("New stream"));
    expect(confirm().textContent).toBe("Create stream and graduate");
    await userEvent.click(segment("Temporary stream"));
    expect(confirm().textContent).toBe("Create temporary stream and graduate");
    await userEvent.click(segment("Work directly"));
    await waitFor(() => expect(confirm()).not.toBeDisabled());
    expect(confirm().textContent).toBe("Graduate here");
  });

  it("GSD-FR-ZAFX: Implement with Work directly invokes the direct start", async () => {
    const { onStarted } = draw();
    await settled();
    await userEvent.click(segment("Implement"));
    await userEvent.click(segment("Work directly"));
    await waitFor(() => expect(confirm()).not.toBeDisabled());
    await userEvent.click(confirm());
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("start_direct_graduation")).toHaveLength(1);
    expect(calls("start_graduation")).toHaveLength(0);
  });

  it.each<[unknown, string]>([
    [{ code: "cli_unavailable", detail: "not_found", diagnostics: [] }, "The OpenSpec CLI is not installed on this machine."],
    [
      { code: "cli_unavailable", detail: "failed", diagnostics: [] },
      "The OpenSpec CLI on this machine does not start or does not report its version.",
    ],
    [{ code: "openspec_mode_inactive", diagnostics: [] }, "OpenSpec mode is off for this project."],
  ])(
    "GSD-FR-RFIG, OFN-FR-EPIG, OFN-FR-HEVB: a propose start refused with %# renders its notice, its settings route, and that no run started",
    async (reject, sentence) => {
      refusal = reject;
      draw();
      await settled();
      await userEvent.click(confirm());
      const error = await screen.findByTestId("graduation-start-error");
      expect(within(error).getByRole("alert")).toHaveTextContent(`${sentence} No run started.`);
      await userEvent.click(within(error).getByRole("button", { name: "Open OpenSpec settings" }));
      await waitFor(() =>
        expect(invokeMock.mock.calls.find((c) => c[0] === "open_settings_window")?.[1]).toEqual({
          section: "project/openspec",
        }),
      );
      // The dialog stays open, with no other sentence of the same failure.
      expect(screen.getAllByText(sentence, { exact: false })).toHaveLength(1);
    },
  );
});

describe("a status that arrives late", () => {
  it("GSD-FR-AGPL: a status read after Work directly was chosen moves the propose run to a stream", async () => {
    let answer: (value: OpenSpecStatus) => void = () => {};
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((cmd: string, args?: unknown) =>
      cmd === "get_openspec_status"
        ? new Promise<OpenSpecStatus>((resolve) => {
            answer = resolve;
          })
        : base(cmd, args),
    );
    draw();
    await screen.findByRole("combobox", { name: "Work stream" });
    expect(confirm()).toBeDisabled();
    await userEvent.click(segment("Work directly"));
    answer(ACTIVE);
    await waitFor(() => expect(segment("Existing stream")).toHaveAttribute("aria-checked", "true"));
    expect(segment("Work directly")).toBeDisabled();
  });
});
