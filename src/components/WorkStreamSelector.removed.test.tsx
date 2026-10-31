import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorkStreamSelector } from "./WorkStreamSelector";
import { refusalText } from "./WorkStreamSelector/refusals";
import { resetLogBufferForTest } from "../logging";
import type { RemovedStreamNotice, WorkStreamSummary } from "../types";
import { summary } from "../test/streamFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type EventHandler = (ev: { payload: unknown }) => void;
let handlers: [string, EventHandler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    handlers.push([name, handler]);
    return Promise.resolve(() => {
      handlers = handlers.filter(([, h]) => h !== handler);
    });
  },
  emit: () => Promise.resolve(),
}));

const notice = (over: Partial<RemovedStreamNotice> = {}): RemovedStreamNotice => ({
  streamId: "w9",
  name: "Change proposals",
  branch: "synthesis/stream/change-proposals",
  branchSurvives: false,
  ...over,
});

/** The listing and the notices, held so a test can change what they answer. */
const held = {
  streams: [] as WorkStreamSummary[],
  notices: [] as RemovedStreamNotice[],
};

function backend() {
  invokeMock.mockImplementation((cmd: string, args?: { streamId?: string }) => {
    if (cmd === "list_work_streams") return Promise.resolve(held.streams);
    if (cmd === "list_removed_work_streams") return Promise.resolve(held.notices);
    if (cmd === "dismiss_removed_work_stream") {
      held.notices = held.notices.filter((n) => n.streamId !== args?.streamId);
      return Promise.resolve(undefined);
    }
    if (cmd === "get_work_stream_uncommitted_paths") return Promise.resolve([]);
    if (cmd === "append_log_records") return Promise.resolve(undefined);
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function renderSelector() {
  render(
    <WorkStreamSelector
      inRepository
      activeBranch="main"
      onOpen={vi.fn()}
      onActivate={vi.fn(async () => ({ ok: true as const }))}
      onRequestMergeCommit={vi.fn(async () => null as string | null)}
      onOpenChanges={vi.fn()}
      onOpenRun={vi.fn()}
    />,
  );
}

const openMenu = async () => {
  await userEvent.click(await screen.findByTestId("stream-selector"));
  return screen.findByTestId("stream-menu");
};

beforeEach(() => {
  invokeMock.mockReset();
  handlers = [];
  held.streams = [];
  held.notices = [];
  resetLogBufferForTest();
  backend();
});
afterEach(cleanup);

describe("the removed stream notices (WSS-FR-RPYH, WSS-FR-OSVE)", () => {
  it("WSS-FR-RPYH: a notice names the stream and says that its working copy is gone", async () => {
    held.notices = [notice()];
    renderSelector();

    const menu = await openMenu();

    const shown = await within(menu).findByTestId("stream-notice-w9");
    expect(shown).toHaveTextContent("Change proposals");
    expect(shown).toHaveTextContent(/working copy is gone/);
    expect(shown).not.toHaveTextContent(/branch/i);
  });

  it("WSS-FR-RPYH: where the branch survives, the notice names it and says that it stays", async () => {
    held.notices = [notice({ branchSurvives: true })];
    renderSelector();

    const shown = await within(await openMenu()).findByTestId("stream-notice-w9");

    expect(shown).toHaveTextContent("synthesis/stream/change-proposals");
    expect(shown).toHaveTextContent(/stays/);
  });

  it("WSS-FR-RPYH: the resting label counts the live streams and no notice", async () => {
    held.streams = [summary()];
    held.notices = [notice(), notice({ streamId: "w8", name: "Other" })];
    renderSelector();

    await within(await openMenu()).findByTestId("stream-notice-w9");

    expect(screen.getByTestId("stream-selector")).toHaveTextContent("1 stream");
  });

  it("WSS-FR-RPYH: the notices are read only while the dropdown is open", async () => {
    held.notices = [notice()];
    renderSelector();
    await screen.findByText("0 streams");
    expect(calls("list_removed_work_streams")).toHaveLength(0);

    await within(await openMenu()).findByTestId("stream-notice-w9");

    expect(calls("list_removed_work_streams").length).toBeGreaterThan(0);
  });

  it("WSS-FR-RPYH: a listing that removed a stream while the dropdown is open shows its notice", async () => {
    held.streams = [summary()];
    renderSelector();
    const menu = await openMenu();
    expect(within(menu).queryByTestId("stream-notice-w1")).toBeNull();

    held.streams = [];
    held.notices = [notice({ streamId: "w1" })];
    const fire = handlers.filter(([name]) => name === "work-streams-changed");
    expect(fire.length).toBeGreaterThan(0);
    for (const [, handler] of fire) handler({ payload: { projectKey: "p" } });

    expect(await within(menu).findByTestId("stream-notice-w1")).toBeInTheDocument();
  });

  it("WSS-FR-OSVE: the dismiss control drops the notice through the backend", async () => {
    held.notices = [notice()];
    renderSelector();
    const menu = await openMenu();
    await within(menu).findByTestId("stream-notice-w9");

    await userEvent.click(within(menu).getByTestId("stream-notice-dismiss-w9"));

    await waitFor(() => expect(within(menu).queryByTestId("stream-notice-w9")).toBeNull());
    expect(calls("dismiss_removed_work_stream")[0][1]).toEqual({ streamId: "w9" });
  });

  it("WSS-FR-OSVE: a read that was out at a dismissal does not bring the notice back", async () => {
    held.notices = [notice()];
    renderSelector();
    const menu = await openMenu();
    await within(menu).findByTestId("stream-notice-w9");
    let finishRead: (answer: RemovedStreamNotice[]) => void = () => {};
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_work_streams") return Promise.resolve([summary()]);
      if (cmd === "list_removed_work_streams") {
        return new Promise((resolve) => (finishRead = resolve));
      }
      return Promise.resolve(undefined);
    });
    for (const [name, handler] of handlers) {
      if (name === "work-streams-changed") handler({ payload: { projectKey: "p" } });
    }
    await waitFor(() => expect(calls("list_removed_work_streams").length).toBeGreaterThan(1));

    await userEvent.click(within(menu).getByTestId("stream-notice-dismiss-w9"));
    await act(async () => finishRead([notice()]));

    expect(within(menu).queryByTestId("stream-notice-w9")).toBeNull();
  });

  it("WSS-FR-OSVE: a notice stays across a close of the dropdown until it is dismissed", async () => {
    held.notices = [notice()];
    renderSelector();
    await within(await openMenu()).findByTestId("stream-notice-w9");
    await userEvent.click(screen.getByTestId("stream-selector"));
    expect(screen.queryByTestId("stream-menu")).toBeNull();

    const menu = await openMenu();

    expect(await within(menu).findByTestId("stream-notice-w9")).toBeInTheDocument();
  });

  it("WSS-FR-OSVE: a notice stays across a remount, because the backend holds it", async () => {
    held.notices = [notice()];
    renderSelector();
    await within(await openMenu()).findByTestId("stream-notice-w9");
    cleanup();

    renderSelector();

    expect(await within(await openMenu()).findByTestId("stream-notice-w9")).toBeInTheDocument();
  });

  it("WSS-FR-OSVE: the empty state does not render while a notice shows, and renders after it", async () => {
    held.notices = [notice()];
    renderSelector();
    const menu = await openMenu();
    await within(menu).findByTestId("stream-notice-w9");
    expect(within(menu).queryByText(/holds no work stream yet/)).toBeNull();

    await userEvent.click(within(menu).getByTestId("stream-notice-dismiss-w9"));

    expect(await within(menu).findByText(/holds no work stream yet/)).toBeInTheDocument();
  });

  it("WSS-FR-RPYH: a read that fails leaves the listing usable", async () => {
    held.streams = [summary()];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_work_streams") return Promise.resolve(held.streams);
      if (cmd === "append_log_records") return Promise.resolve(undefined);
      return Promise.reject(new Error("no notices today"));
    });
    renderSelector();

    const menu = await openMenu();

    expect(await within(menu).findByText("editor work")).toBeInTheDocument();
  });
});

describe("a row that lost part of its stream (WSS-FR-OQYG, WSS-FR-NIRW)", () => {
  it("WSS-FR-OQYG: a missing stream that a run holds says that a discard of the run removes it", async () => {
    held.streams = [summary({ isMissing: true, busyRunId: "g1" })];
    renderSelector();

    const menu = await openMenu();

    expect(await within(menu).findByText(/Discard its run/)).toBeInTheDocument();
    expect(within(menu).queryByText(/make it again/)).toBeNull();
  });

  it("WSS-FR-OQYG: a missing stream with a queued run says the same", async () => {
    held.streams = [summary({ isMissing: true }, { queuedRunCount: 1 })];
    renderSelector();

    expect(await within(await openMenu()).findByText(/Discard its run/)).toBeInTheDocument();
  });

  it("WSS-FR-OQYG: a missing stream with no run promises no discard", async () => {
    held.streams = [summary({ isMissing: true })];
    renderSelector();

    const menu = await openMenu();

    expect(await within(menu).findByText(/Missing/)).toBeInTheDocument();
    expect(within(menu).queryByText(/Discard its run/)).toBeNull();
  });

  it("WSS-FR-NIRW: a row with no branch says so and keeps Delete stream", async () => {
    held.streams = [summary({}, { branchMissing: true })];
    renderSelector();

    const menu = await openMenu();

    expect(await within(menu).findByText(/Branch gone/)).toBeInTheDocument();
    expect(within(menu).getByRole("button", { name: /Delete stream/ })).toBeEnabled();
  });

  it("WSS-FR-NIRW: a row a run holds states no action it does not offer", async () => {
    held.streams = [summary({ busyRunId: "g1" }, { branchMissing: true })];
    renderSelector();

    const menu = await openMenu();

    expect(await within(menu).findByText("Branch gone.")).toBeInTheDocument();
    expect(within(menu).queryByRole("button", { name: /Delete stream/ })).toBeNull();
  });

  it("WSS-FR-NIRW: the row stays selectable", async () => {
    held.streams = [summary({}, { branchMissing: true })];
    renderSelector();

    const menu = await openMenu();

    expect(
      await within(menu).findByRole("menuitem", { name: /Open the work stream/ }),
    ).toBeEnabled();
  });

  it("WSS-FR-NIRW: a healthy row says nothing of a branch", async () => {
    held.streams = [summary()];
    renderSelector();

    const menu = await openMenu();

    await within(menu).findByText("editor work");
    expect(within(menu).queryByText(/Branch gone/)).toBeNull();
  });
});

describe("the dropdown stays inside the window (WSS-FR-OSVE)", () => {
  const boxOf = (left: number, right: number) =>
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      left,
      right,
    } as DOMRect);

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("WSS-FR-OSVE: a dropdown past the window's edge moves back inside it", async () => {
    vi.stubGlobal("innerWidth", 800);
    boxOf(514, 909);
    held.notices = [notice()];
    renderSelector();

    const menu = await openMenu();

    expect(menu.style.left).toBe("-117px");
  });

  it("WSS-FR-OSVE: a dropdown inside the window stays at its control", async () => {
    vi.stubGlobal("innerWidth", 1280);
    boxOf(514, 909);
    renderSelector();

    const menu = await openMenu();

    expect(menu.style.left).toBe("");
  });
});

describe("a name whose branch exists (WSS-FR-KPER)", () => {
  it("WSS-FR-KPER: the refusal names the branch and both ways out of it", () => {
    const text = refusalText("stream_branch_exists: synthesis/stream/again");

    expect(text).toContain("synthesis/stream/again");
    expect(text).toMatch(/another name/);
    expect(text).toMatch(/Git panel/);
  });

  it("WSS-FR-KPER: the New stream dialog renders it", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_work_streams") return Promise.resolve([]);
      if (cmd === "list_removed_work_streams") return Promise.resolve([]);
      if (cmd === "list_worktrees_and_branches") {
        return Promise.resolve({ worktrees: [], branches: [{ name: "main" }] });
      }
      if (cmd === "create_work_stream") {
        return Promise.reject(new Error("stream_branch_exists: synthesis/stream/again"));
      }
      return Promise.resolve(undefined);
    });
    renderSelector();
    await openMenu();
    await userEvent.click(await screen.findByText(/New stream/));
    const dialog = await screen.findByRole("dialog");

    await userEvent.type(within(dialog).getByLabelText("Name"), "again");
    await userEvent.click(within(dialog).getByRole("button", { name: /Create/ }));

    expect(await within(dialog).findByText(/already exists/)).toHaveTextContent(
      "synthesis/stream/again",
    );
  });
});
