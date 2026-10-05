import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorkStreamSelector } from "./WorkStreamSelector";
import { summary } from "../test/streamFixtures";
import type { WorkStreamSummary } from "../types";

// The delete confirmation of the Streams overlay (WSS-FR-BRMT, WSS-FR-NHCV,
// WSS-FR-PKQD). The selector reaches the backend only through `invoke`, so
// these tests assert what the confirmation ASKS FOR and what it renders from
// the answers.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

interface Backend {
  paths?: () => Promise<string[]>;
  remove?: () => Promise<void>;
}

/** Answer the listing, the uncommitted-path read and the delete itself. */
function backend(streams: WorkStreamSummary[], over: Backend = {}) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_work_streams") return streams;
    if (cmd === "get_work_stream_uncommitted_paths")
      return over.paths ? over.paths() : [];
    if (cmd === "delete_work_stream") return over.remove ? over.remove() : undefined;
    return undefined;
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

const openDelete = async () => {
  await userEvent.click(await screen.findByTestId("stream-selector"));
  await userEvent.click(await screen.findByTestId("stream-delete-w1"));
  return screen.findByTestId("stream-row-w1");
};

const deleteButton = (row: HTMLElement) =>
  within(row).getByRole("button", { name: /^(Delete|Discard changes and delete)$/ });

beforeEach(() => {
  invokeMock.mockReset();
});
afterEach(cleanup);

describe("reading the uncommitted paths (WSS-FR-BRMT)", () => {
  it("WSS-FR-BRMT: the confirmation says it is checking and keeps Delete disabled until the read answers", async () => {
    let answer: (paths: string[]) => void = () => {};
    backend([summary()], {
      paths: () => new Promise<string[]>((resolve) => (answer = resolve)),
    });
    renderSelector();
    const row = await openDelete();

    expect(calls("get_work_stream_uncommitted_paths")[0][1]).toEqual({
      streamId: "w1",
    });
    expect(within(row).getByRole("status").textContent).toBe(
      "Checking for uncommitted changes…",
    );
    expect(deleteButton(row)).toBeDisabled();
    // Cancel stays available while the read runs.
    expect(within(row).getByRole("button", { name: "Cancel" })).toBeEnabled();

    answer([]);
    await waitFor(() => expect(deleteButton(row)).toBeEnabled());
    expect(within(row).queryByText("Checking for uncommitted changes…")).toBeNull();
  });

  it("WSS-FR-BRMT: a failed read shows its error, offers Retry and keeps Delete disabled", async () => {
    let failing = true;
    backend([summary()], {
      paths: async () => {
        if (failing) throw new Error("unknown_stream");
        return [];
      },
    });
    renderSelector();
    const row = await openDelete();

    const alert = await within(row).findByRole("alert");
    expect(alert.textContent).toMatch(/Could not check for uncommitted changes/);
    expect(alert.textContent).toMatch(/not there any more/);
    expect(alert.textContent).not.toMatch(/unknown_stream/);
    expect(deleteButton(row)).toBeDisabled();

    failing = false;
    await userEvent.click(within(row).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(deleteButton(row)).toBeEnabled());
    expect(calls("get_work_stream_uncommitted_paths")).toHaveLength(2);
    expect(within(row).queryByRole("alert")).toBeNull();
    expect(calls("delete_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-BRMT: a stream ahead of its base offers merging and reads no path", async () => {
    backend([summary({}, { aheadOfBase: 2 })]);
    renderSelector();
    const row = await openDelete();
    expect(within(row).getByRole("button", { name: "Merge instead" })).toBeEnabled();
    expect(calls("get_work_stream_uncommitted_paths")).toHaveLength(0);
  });
});

describe("confirming a delete (WSS-FR-NHCV)", () => {
  it("WSS-FR-NHCV: a stream with no uncommitted path deletes with discard false and no warning", async () => {
    backend([summary()]);
    renderSelector();
    const row = await openDelete();
    await waitFor(() => expect(deleteButton(row)).toBeEnabled());
    expect(within(row).queryByRole("alert")).toBeNull();

    await userEvent.click(deleteButton(row));
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    expect(calls("delete_work_stream")[0][1]).toEqual({
      streamId: "w1",
      force: false,
      discardUncommitted: false,
    });
  });

  it("WSS-FR-NHCV, WSS-FR-JMWA: uncommitted paths give a warning with the count and the first paths, and the confirm names the discard", async () => {
    const paths = Array.from({ length: 5 }, (_, i) => `src/file-${i}.ts`);
    backend([summary()], { paths: async () => paths });
    renderSelector();
    const row = await openDelete();

    const alert = await within(row).findByRole("alert");
    expect(alert.textContent).toMatch(/discards 5 uncommitted changes/);
    expect(alert.textContent).toContain(
      "src/file-0.ts, src/file-1.ts, src/file-2.ts and 2 more",
    );
    expect(alert.textContent).not.toContain("file-4.ts");
    // WSS-FR-JMWA: the whole set stands in accessible semantics.
    expect(within(alert).getByTitle(paths.join(", "))).toBeInTheDocument();
    expect(
      within(row).getByRole("button", { name: "Discard changes and delete" }),
    ).toBeEnabled();
    expect(within(row).queryByRole("button", { name: "Delete" })).toBeNull();
  });

  it("WSS-FR-NHCV: one uncommitted path reads in the singular", async () => {
    backend([summary()], { paths: async () => ["a.ts"] });
    renderSelector();
    const row = await openDelete();
    const alert = await within(row).findByRole("alert");
    expect(alert.textContent).toMatch(/discards 1 uncommitted change in/);
    expect(alert.textContent).toContain("a.ts");
  });

  it("WSS-FR-NHCV: Cancel changes nothing and deletes nothing", async () => {
    backend([summary()], { paths: async () => ["a.ts", "b.ts"] });
    renderSelector();
    const row = await openDelete();
    await within(row).findByRole("alert");

    await userEvent.click(within(row).getByRole("button", { name: "Cancel" }));
    expect(within(row).queryByRole("alert")).toBeNull();
    expect(within(row).getByTestId("stream-delete-w1")).toBeInTheDocument();
    expect(calls("delete_work_stream")).toHaveLength(0);
  });

  it("WSS-FR-NHCV: confirming the discard deletes with discard true and no force", async () => {
    backend([summary()], { paths: async () => ["a.ts", "b.ts"] });
    renderSelector();
    const row = await openDelete();
    await userEvent.click(
      await within(row).findByRole("button", { name: "Discard changes and delete" }),
    );
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    expect(calls("delete_work_stream")[0][1]).toEqual({
      streamId: "w1",
      force: false,
      discardUncommitted: true,
    });
  });

  it("WSS-FR-NHCV, WSS-FR-RWLB: the confirm is operable by keyboard alone", async () => {
    backend([summary()]);
    renderSelector();
    const row = await openDelete();
    await waitFor(() => expect(deleteButton(row)).toBeEnabled());
    deleteButton(row).focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
  });
});

describe("refusals of a delete (WSS-FR-PKQD)", () => {
  it("WSS-FR-PKQD: a stream_dirty refusal shows the discard warning again with the paths it carries", async () => {
    let attempts = 0;
    backend([summary()], {
      remove: async () => {
        attempts += 1;
        if (attempts === 1) throw "stream_dirty: x.ts, y/z.md";
      },
    });
    renderSelector();
    const row = await openDelete();
    await waitFor(() => expect(deleteButton(row)).toBeEnabled());
    expect(within(row).queryByRole("alert")).toBeNull();

    await userEvent.click(deleteButton(row));
    const alert = await within(row).findByRole("alert");
    expect(alert.textContent).toMatch(/discards 2 uncommitted changes/);
    expect(alert.textContent).toContain("x.ts, y/z.md");
    expect(alert.textContent).not.toMatch(/stream_dirty/);
    expect(
      within(row).getByRole("button", { name: "Discard changes and delete" }),
    ).toBeEnabled();
    expect(calls("delete_work_stream")[0][1]).toMatchObject({
      discardUncommitted: false,
    });

    // The author may confirm, and the second call carries the discard.
    await userEvent.click(
      within(row).getByRole("button", { name: "Discard changes and delete" }),
    );
    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(2));
    expect(calls("delete_work_stream")[1][1]).toEqual({
      streamId: "w1",
      force: false,
      discardUncommitted: true,
    });
  });

  it.each([
    ["stream_busy", /A run holds this stream/],
    ["stream_has_runs", /run of this stream is not finished/],
    ["stream_active", /active worktree/],
    ["stream_unmerged: 3", /holds commits its base branch does not/],
  ])(
    "WSS-FR-PKQD: %s renders on the stream's own row and offers no way around it",
    async (code, words) => {
      backend([summary()], {
        remove: async () => {
          throw code;
        },
      });
      renderSelector();
      const row = await openDelete();
      await waitFor(() => expect(deleteButton(row)).toBeEnabled());
      await userEvent.click(deleteButton(row));

      const alert = await within(row).findByRole("alert");
      expect(alert.textContent).toMatch(words);
      expect(alert.textContent).not.toMatch(/stream_/);

      // No control forces the removal, and no call asked for one.
      expect(
        within(row).queryByRole("button", { name: /force|anyway|bypass|override/i }),
      ).toBeNull();
      expect(
        within(row).queryByRole("button", { name: "Discard changes and delete" }),
      ).toBeNull();
      for (const call of calls("delete_work_stream")) {
        expect(call[1]).toMatchObject({ force: false });
      }
    },
  );
});
