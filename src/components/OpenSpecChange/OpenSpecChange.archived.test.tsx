/**
 * The change workspace of an archived change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-AKVG,
 * OCW-FR-GVZT, OCW-FR-LABL, OCW-FR-RFGG, OCW-FR-PFUQ; `../../../specifications/core/OSM-openspec-mode.md`
 * OSM-FR-GMKV).
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

import {
  changeBackend,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";
import { archivedAsOf } from "../../state/openspec/changeView";
import { archivedAsOfError } from "./failure";
import { visibleChangesOf } from "../../hooks/useChangeWorkingObserver";
import type { Tab } from "../../types";

const ARCHIVED = "change_archived: 2026-11-11-add-logout-3f2a";
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  invokeMock.mockReset();
  backend = changeBackend({ proposals: [], discussions: [] });
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  cleanup();
  expect(backend.unknown).toEqual([]);
});

/** OCW-FR-GVZT: nothing of the workspace but the statement shows. */
function expectNothingButTheStatement() {
  for (const name of ["History", "Information", "Apply", "Rework…", "Retry", "Refresh"]) {
    expect(screen.queryByRole("button", { name })).toBeNull();
  }
  expect(screen.queryByTestId("openspec-change-discussion")).toBeNull();
  expect(screen.queryByTestId("openspec-run-strip")).toBeNull();
  expect(screen.queryByTestId("openspec-artifact")).toBeNull();
  expect(screen.queryByTestId("openspec-change-failure")).toBeNull();
}

describe("an archived change (OCW-FR-AKVG, OCW-FR-GVZT)", () => {
  it("OCW-FR-AKVG, OCW-FR-GVZT: a first read that fails with change_archived states that the change is archived, names its archive folder, and offers nothing else", async () => {
    backend.readReject = ARCHIVED;
    drawChange();
    const statement = await screen.findByTestId("openspec-change-archived");
    expect(statement).toHaveTextContent("The change “add-logout-3f2a” is archived.");
    expect(statement).toHaveTextContent("2026-11-11-add-logout-3f2a");
    expect(statement).toHaveTextContent("openspec/changes/archive/");
    expectNothingButTheStatement();
  });

  it("OCW-FR-GVZT, OCW-FR-LABL: a first open of an archived change reads the change once and nothing else: no proposal, no request, no discussion, no version, no run", async () => {
    backend.readReject = ARCHIVED;
    drawChange();
    await screen.findByTestId("openspec-change-archived");
    // The statement is the end state: a further turn sends nothing more.
    await act(async () => {});
    const sent = invokeMock.mock.calls
      .map((c) => c[0] as string)
      .filter((cmd) => cmd !== "append_log_records");
    expect(sent).toEqual(["read_openspec_change"]);
    expectNothingButTheStatement();
  });

  it("OCW-FR-GVZT, OCW-FR-LABL: while the first read is open, a stream tab shows the loading state alone and reads nothing else", async () => {
    let answer!: () => void;
    const first = new Promise<void>((resolve) => (answer = resolve));
    const route = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "read_openspec_change") await first;
      return route(cmd, args);
    });
    drawChange();
    expect(await screen.findByTestId("openspec-change-loading")).toBeInTheDocument();
    await waitFor(() => expect(calls("read_openspec_change")).toHaveLength(1));
    await act(async () => {});
    expect(
      invokeMock.mock.calls.map((c) => c[0] as string).filter((cmd) => cmd !== "append_log_records"),
    ).toEqual(["read_openspec_change"]);
    expect(screen.queryByRole("button", { name: "History" })).toBeNull();
    expect(screen.queryByTestId("openspec-change-discussion")).toBeNull();

    // The answer mounts the parts of a stream location, and they read their stores.
    answer();
    await screen.findAllByTestId("openspec-artifact");
    expect(await screen.findByRole("button", { name: "History" })).toBeInTheDocument();
    await waitFor(() => expect(calls("list_change_edit_proposals").length).toBeGreaterThan(0));
    expect(calls("read_openspec_change")).toHaveLength(1);
  });

  it("OCW-FR-AKVG, OCW-FR-PFUQ: a later read that fails with change_archived replaces the content that an earlier read gave", async () => {
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    backend.readReject = ARCHIVED;
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await screen.findByTestId("openspec-change-archived");
    // OCW-FR-PFUQ states a later failure above the content; an archive is not that.
    expect(screen.queryByTestId("openspec-change-stale")).toBeNull();
    expectNothingButTheStatement();
  });

  it("OCW-FR-AKVG: the active worktree location states it the same way", async () => {
    backend.readReject = ARCHIVED;
    drawChange({ location: { kind: "active_worktree" }, streamName: undefined });
    const statement = await screen.findByTestId("openspec-change-archived");
    expect(statement).toHaveTextContent("2026-11-11-add-logout-3f2a");
    expect(screen.getByTestId("openspec-change-location")).toHaveTextContent("active worktree");
    expect(screen.queryByTestId("openspec-change-no-discussion")).toBeNull();
  });

  it("OCW-FR-GVZT: a reactivation of the archived tab reads the change and nothing else: no proposal, no request, no version, no run", async () => {
    backend.readReject = ARCHIVED;
    const first = drawChange();
    await screen.findByTestId("openspec-change-archived");
    first.view.unmount();
    invokeMock.mockClear();

    drawChange();
    await screen.findByTestId("openspec-change-archived");
    await waitFor(() => expect(calls("read_openspec_change")).toHaveLength(1));
    const read = invokeMock.mock.calls
      .map((c) => c[0] as string)
      .filter((cmd) => cmd !== "append_log_records");
    expect(read).toEqual(["read_openspec_change"]);
    expectNothingButTheStatement();
  });

  it("OCW-FR-AKVG: a reactivation whose read succeeds shows the change again", async () => {
    backend.readReject = ARCHIVED;
    const first = drawChange();
    await screen.findByTestId("openspec-change-archived");
    first.view.unmount();
    backend.readReject = undefined;

    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    expect(screen.queryByTestId("openspec-change-archived")).toBeNull();
    expect(archivedAsOf("tab-1")).toBeNull();
  });

  it("OCW-FR-RFGG: a code other than change_archived still states that the read failed, with Retry", async () => {
    backend.readReject = "cli_failed";
    drawChange();
    const failure = await screen.findByTestId("openspec-change-failure");
    expect(within(failure).getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.queryByTestId("openspec-change-archived")).toBeNull();
  });
});

describe("the code of an archived read (OSM-FR-GMKV)", () => {
  it("OSM-FR-GMKV, OCW-FR-AKVG: the archive folder name comes from the code in every shape a rejection takes", () => {
    expect(archivedAsOfError(ARCHIVED)).toBe("2026-11-11-add-logout-3f2a");
    expect(archivedAsOfError(new Error(ARCHIVED))).toBe("2026-11-11-add-logout-3f2a");
    expect(archivedAsOfError({ code: ARCHIVED })).toBe("2026-11-11-add-logout-3f2a");
    expect(archivedAsOfError("change_not_found")).toBeNull();
    expect(archivedAsOfError("cli_failed: change_archived: x")).toBeNull();
  });
});

describe("the working observer (OCW-FR-GVZT, OCW-FR-MIEY)", () => {
  it("OCW-FR-GVZT: a change tab that states an archived change is no related surface", () => {
    const tab = {
      id: "c",
      label: "x",
      kind: "openspecChange",
      openspecChange: {
        location: { kind: "stream", streamId: "s-1" },
        name: "add-logout",
        file: null,
        openSeq: 1,
      },
    } as Tab;
    expect(visibleChangesOf([tab], "c", () => false)).toEqual([
      { streamId: "s-1", change: "add-logout" },
    ]);
    expect(visibleChangesOf([tab], "c", (id) => id === "c")).toEqual([]);
  });
});
