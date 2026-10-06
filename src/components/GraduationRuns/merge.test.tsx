/**
 * A merge run in the graduation section
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-HDPQ,
 * GRU-FR-NWEC, GRU-FR-ZKYU, GRU-FR-YSTV, GRU-FR-CXLB, GRU-FR-AJGM,
 * GRU-FR-JRMA, GRU-FR-ELFZ, GRU-FR-TOKG, GRU-FR-DBUS, GRU-FR-QMWX,
 * GRU-FR-EMNV, GRU-FR-PAHN; `../../../specifications/ui/GEA-graduation-escalation-answering.md`
 * GEA-FR-MRDK, GEA-FR-WQZH, GEA-FR-HBCF; `../../../specifications/ui/RUN-runs.md`
 * RUN-FR-BWNK).
 *
 * A merge run is a graduation run that carries `merge` data and has no draft.
 * It renders on the terms every run has, with its own title, provenance,
 * publication, unresolved paths and result added, and with every draft-only
 * affordance absent. These tests render the whole section against a mocked
 * backend, so what is asserted is what an author reads and can press.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { resetLogBufferForTest } from "../../logging";
import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import {
  makeEscalation,
  makeMergeRun,
  makeObservability,
  makePass,
  makeQueue,
  makeRun,
} from "../../test/graduationFixtures";
import { pickSelector } from "../../test/selectors";
import type { GraduationQueue, GraduationRun } from "../../types";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

let queue: GraduationQueue;
/** Per command, what the backend refuses with. */
let refusals: Record<string, string>;

beforeEach(() => {
  queue = makeQueue([makeMergeRun("m1", "working")]);
  refusals = {};
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    if (refusals[command]) throw refusals[command];
    switch (command) {
      case "list_graduation_queue":
        return queue;
      case "load_app_preferences":
        return { graduationRailWidthFraction: 0.2 };
      case "list_work_streams":
        return [];
      default:
        return undefined;
    }
  });
  listened.mockReset();
  listened.mockImplementation(async () => () => {});
  resetAppPreferencesCache();
  forgetEverything();
  forgetEveryDraft();
  // A flush timer from the previous test must not fire in this one.
  resetLogBufferForTest();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

async function open(props: Parameters<typeof GraduationRuns>[0] = {}) {
  render(<GraduationRuns {...props} />);
  await screen.findByTestId("graduation-section");
  await waitFor(() =>
    expect(commands()).toContain("list_graduation_queue"),
  );
}

/** Open the section and put the rail in **All**, which lists runs that ended. */
async function openAll() {
  await open();
  await pickSelector("Run view", "all");
}

function commands(): string[] {
  return invoked.mock.calls.map(([command]) => command as string);
}

const calls = (command: string) =>
  invoked.mock.calls.filter(([name]) => name === command);

/** The region of the selected run. */
const region = () => screen.getByLabelText("The selected run");

/** Serve one run and open the section on it. */
async function serve(run: GraduationRun, all = false) {
  queue = makeQueue([run]);
  if (all) await openAll();
  else await open();
  await screen.findByTestId("graduation-provenance");
}

describe("the title, the provenance and the publication (GRU-FR-HDPQ, GRU-FR-NWEC, GRU-FR-ZKYU)", () => {
  it("GRU-FR-HDPQ: the rail row and the region carry `Merge <stream>` where a draft run carries its draft's name", async () => {
    await serve(makeMergeRun("m1", "working"));

    expect(screen.getByTestId("graduation-row")).toHaveAttribute(
      "aria-label",
      "Merge editor-work",
    );
    expect(within(region()).getByText("Merge editor-work")).toBeInTheDocument();
    // The title is the merge's own, and no draft name stands anywhere.
    expect(region().textContent).not.toMatch(/Run m1/);
  });

  it("GRU-FR-HDPQ: the rail's text filter matches the merge title", async () => {
    queue = makeQueue([
      makeMergeRun("m1", "working", { merge: { name: "Merge docs-pass" } }),
      makeRun("r1", "working"),
    ]);
    await open();
    await waitFor(() => expect(screen.getAllByTestId("graduation-row")).toHaveLength(2));

    await userEvent.type(screen.getByLabelText("Filter runs"), "merge docs");

    await waitFor(() => expect(screen.getAllByTestId("graduation-row")).toHaveLength(1));
    expect(screen.getByTestId("graduation-row")).toHaveAttribute(
      "aria-label",
      "Merge docs-pass",
    );
  });

  it("GRU-FR-NWEC: the provenance names the stream, the stream's branch and the base branch", async () => {
    await serve(makeMergeRun("m1", "working"));

    const line = screen.getByTestId("graduation-provenance").textContent ?? "";
    expect(line).toContain("editor-work");
    expect(line).toContain("synthesis/stream/editor-work");
    expect(line).toContain("main");
    expect(line).toMatch(/into the base branch/);
  });

  it("GRU-FR-NWEC: an uncommitted publication says the result is left uncommitted in the base worktree", async () => {
    await serve(makeMergeRun("m1", "working"));

    expect(screen.getByTestId("graduation-merge-publication")).toHaveTextContent(
      /left uncommitted in the base worktree/,
    );
  });

  it("GRU-FR-NWEC, GRU-FR-MRPE: a commit publication states the author's message as escaped text", async () => {
    await serve(
      makeMergeRun("m1", "working", {
        merge: {
          publication: { kind: "commit", message: "<b>bold</b> merge the specs" },
        },
      }),
    );

    const line = screen.getByTestId("graduation-merge-publication");
    expect(line).toHaveTextContent(/committed under the author's message/);
    // Escaped: the markup is text, and no element was made from it.
    expect(line).toHaveTextContent("<b>bold</b> merge the specs");
    expect(line.querySelector("b")).toBeNull();
  });

  it("GRU-FR-ZKYU: no text, label or accessible name of a merge run calls it a graduation of a draft", async () => {
    await serve(makeMergeRun("m1", "working"));

    const row = screen.getByTestId("graduation-row");
    expect(row.textContent).not.toMatch(/draft/i);
    expect(row.getAttribute("aria-label")).not.toMatch(/draft|graduation of/i);
    expect(region().textContent).not.toMatch(/draft/i);
    for (const labelled of region().querySelectorAll("[aria-label]")) {
      expect(labelled.getAttribute("aria-label")).not.toMatch(/draft/i);
    }
    // The stage row names itself for the run by its title.
    expect(screen.getByTestId("run-progress").textContent).toContain(
      "Progress of Merge editor-work",
    );
  });
});

describe("the stage row (GRU-FR-YSTV)", () => {
  const labels = () =>
    [...screen.getByTestId("run-progress").querySelectorAll("[data-stage]")].map(
      (el) => el.getAttribute("data-stage"),
    );
  const stageText = (id: string) =>
    screen.getByTestId("run-progress").querySelector(`[data-stage="${id}"]`)
      ?.textContent ?? "";

  it("GRU-FR-YSTV: the same four stages in the same order, labelled Queued, Reconciling, Reviewing and Merged", async () => {
    await serve(makeMergeRun("m1", "working"));

    expect(labels()).toEqual(["queued", "working", "review", "done"]);
    expect(stageText("queued")).toContain("Queued");
    expect(stageText("working")).toContain("Reconciling");
    expect(stageText("review")).toContain("Reviewing");
    expect(stageText("done")).toContain("Merged");
  });

  it("GRU-FR-YSTV: a draft run's stages keep their own labels", async () => {
    await serve(makeRun("r1", "working"));

    expect(stageText("working")).toContain("Working");
    expect(stageText("review")).toContain("Review");
    expect(stageText("done")).toContain("Done");
    expect(stageText("working")).not.toContain("Reconciling");
  });

  it("GRU-FR-YSTV: a run working shows Reconciling with its pass, and a run reviewing shows Reviewing with its pass", async () => {
    await serve(makeMergeRun("m1", "working", { checkpoint: { pass: 1, passLimit: 2 } }));
    expect(screen.getByTestId("graduation-state")).toHaveTextContent("Reconciling");
    expect(screen.getByTestId("run-progress").textContent).toMatch(
      /Reconciling · pass 1 of 2/,
    );

    cleanup();
    await serve(
      makeMergeRun("m1", "reviewing", {
        checkpoint: { pass: 2, passLimit: 2 },
        observability: makeObservability({ currentStage: "review" }),
      }),
    );
    expect(screen.getByTestId("graduation-state")).toHaveTextContent("Reviewing");
    expect(screen.getByTestId("run-progress").textContent).toMatch(
      /Reviewing · pass 2 of 2/,
    );
  });

  it("GRU-FR-YSTV: a completed merge run reads Merged", async () => {
    await serve(
      makeMergeRun("m1", "completed", {
        merge: {
          result: { published: "uncommitted", commit: null, mergedPaths: ["a.md"] },
        },
        observability: makeObservability({
          currentStage: "done",
          stageCondition: "complete",
        }),
      }),
      true,
    );
    expect(screen.getByTestId("graduation-state")).toHaveTextContent("Merged");
  });
});

describe("what a merge run renders on the terms every run has (GRU-FR-CXLB)", () => {
  it("GRU-FR-CXLB: the pass history and its findings render", async () => {
    await serve(
      makeMergeRun("m1", "working", {
        checkpoint: { pass: 2, passLimit: 2 },
        observability: makeObservability({
          passes: [
            makePass({
              pass: 1,
              status: "failed",
              verdict: "revise",
              rationale: "A requirement id was dropped in the merge.",
              findings: [
                {
                  severity: "critical",
                  description: "GRD-FR-AQNW no longer stands.",
                  affectedFiles: ["specifications/core/GRD-graduation.md"],
                  correction: "Restore the requirement.",
                },
              ],
            }),
            makePass({ pass: 2, status: "working" }),
          ],
        }),
      }),
    );

    expect(screen.getByText("First pass")).toBeInTheDocument();
    expect(screen.getByText("Pass 2")).toBeInTheDocument();
    await userEvent.click(screen.getByText("First pass"));
    expect(await screen.findByText("GRD-FR-AQNW no longer stands.")).toBeInTheDocument();
  });

  it("GRU-FR-CXLB: a queued merge run states its place in its stream's queue", async () => {
    await serve(
      makeMergeRun("m1", "queued", {
        observability: makeObservability({
          currentStage: "queued",
          stageCondition: "waiting",
        }),
      }),
    );

    expect(screen.getByTestId("run-progress").textContent).toMatch(
      /Next in the stream/,
    );
    expect(screen.getByTestId("graduation-row").textContent).toMatch(/Next/);
  });

  it("GRU-FR-CXLB: a queued merge run that waits for a project slot says so", async () => {
    queue = makeQueue([makeMergeRun("m1", "queued")]);
    invoked.mockImplementation(async (command: string) => {
      switch (command) {
        case "list_graduation_queue":
          return queue;
        case "get_graduation_capacity":
          return { limit: 1, inUse: 1, waitingForSlot: ["m1"] };
        case "load_app_preferences":
          return { graduationRailWidthFraction: 0.2 };
        default:
          return undefined;
      }
    });
    await open();

    expect(await screen.findByTestId("graduation-run-slot-wait")).toHaveTextContent(
      /project slot/i,
    );
  });

  it("GRU-FR-CXLB, GRU-FR-ZVTC: every queue-changed event reloads the listing", async () => {
    await serve(makeMergeRun("m1", "working"));
    const before = calls("list_graduation_queue").length;
    queue = makeQueue([makeMergeRun("m1", "reviewing")]);

    const handlers = listened.mock.calls
      .filter(([name]) => name === "graduation-queue-changed")
      .map(([, handler]) => handler as (event: { payload: unknown }) => void);
    expect(handlers.length).toBeGreaterThan(0);
    for (const handler of handlers) handler({ payload: {} });

    await waitFor(() =>
      expect(screen.getByTestId("graduation-state")).toHaveTextContent("Reviewing"),
    );
    expect(calls("list_graduation_queue").length).toBeGreaterThan(before);
  });

  it("GRU-FR-CXLB, GRU-FR-NBRO: the state is in words and announced", async () => {
    await serve(makeMergeRun("m1", "blocked"));
    const badge = screen.getByTestId("graduation-state");
    expect(badge).toHaveAttribute("role", "status");
    expect(badge).toHaveAttribute("aria-live", "polite");
    expect(badge).toHaveTextContent("Blocked");
  });
});

describe("the paths a merge changes (GRU-FR-AJGM)", () => {
  it("GRU-FR-AJGM: unresolved paths show what each side did to them, and how many paths changed in all", async () => {
    await serve(
      makeMergeRun("m1", "working", {
        merge: {
          changedPaths: ["a/x.md", "a/y.md", "b/z.ts", "c.ts"],
          unresolvedPaths: ["a/x.md", "b/z.ts"],
          conflicts: [
            { path: "a/x.md", baseChange: "updated", streamChange: "deleted" },
            { path: "b/z.ts", baseChange: "created", streamChange: "created" },
          ],
        },
      }),
    );

    const list = screen.getByTestId("graduation-unresolved");
    expect(list.textContent).toMatch(/Unresolved · 2 paths · 4 paths changed in all/);
    const fates = within(list)
      .getAllByTestId("graduation-path-fate")
      .map((el) => el.textContent);
    expect(fates).toEqual([
      "main: updated · stream: deleted",
      "main: created · stream: created",
    ]);
  });

  it("GRU-FR-AJGM: the paths render as a folder tree whose folders open and close", async () => {
    await serve(makeMergeRun("m1", "working"));

    const list = screen.getByTestId("graduation-unresolved");
    const folder = within(list).getAllByRole("button", { expanded: true })[1];
    expect(folder).toHaveAttribute("aria-expanded", "true");
    await userEvent.click(folder);
    expect(folder).toHaveAttribute("aria-expanded", "false");
  });

  it("GRU-FR-AJGM: a path is read text and opens nothing", async () => {
    await serve(makeMergeRun("m1", "working"));
    const acts = () =>
      commands().filter(
        (name) =>
          // The log flush is a background command, and its timer can fire
          // during the click on a slow runner.
          ![
            "list_graduation_queue",
            "load_app_preferences",
            "get_graduation_capacity",
            "append_log_records",
          ].includes(name),
      ).length;
    const before = acts();

    const file = within(screen.getByTestId("graduation-unresolved")).getByText(
      "GRD-graduation.md",
    );
    await userEvent.click(file);

    // Not a control, and nothing was invoked by pressing it.
    expect(file.closest("button")).toBeNull();
    expect(acts()).toBe(before);
  });

  it("GRU-FR-AJGM: a merge Git settled alone says how many paths it changed", async () => {
    await serve(
      makeMergeRun("m1", "working", {
        merge: { unresolvedPaths: [], conflicts: [], changedPaths: ["a.md", "b.md"] },
      }),
    );
    expect(screen.queryByTestId("graduation-unresolved")).toBeNull();
    expect(screen.getByTestId("graduation-merge-changed")).toHaveTextContent(
      /2 paths changed in all/,
    );
  });
});

describe("the result (GRU-FR-JRMA)", () => {
  const completed = (merge: Partial<NonNullable<GraduationRun["merge"]>>) =>
    makeMergeRun("m1", "completed", {
      merge,
      observability: makeObservability({
        currentStage: "done",
        stageCondition: "complete",
      }),
    });

  it("GRU-FR-JRMA: a merge published as a commit names the short commit with the whole id in semantics and lists the merged paths", async () => {
    const id = "9f2c1ab4d5e6f708192a3b4c5d6e7f8091a2b3c4";
    await serve(
      completed({
        result: {
          published: "commit",
          commit: id,
          mergedPaths: ["docs/a.md", "docs/b.md", "src/c.ts"],
        },
      }),
      true,
    );

    const result = screen.getByTestId("graduation-merge-result");
    expect(result.textContent).toMatch(/Merged into main as a commit/);
    const commit = within(result).getByTestId("graduation-merge-commit");
    expect(commit).toHaveTextContent("9f2c1ab");
    expect(commit.textContent).not.toContain(id);
    expect(commit).toHaveAttribute("title", id);
    expect(commit).toHaveAttribute("aria-label", `Commit ${id}`);
    expect(screen.getByTestId("graduation-merged").textContent).toMatch(/Merged · 3 paths/);
  });

  it("GRU-FR-JRMA: a merge published uncommitted says it was left as an uncommitted change in the base worktree", async () => {
    await serve(
      completed({
        result: { published: "uncommitted", commit: null, mergedPaths: ["docs/a.md"] },
      }),
      true,
    );

    const result = screen.getByTestId("graduation-merge-result");
    expect(result.textContent).toMatch(/uncommitted change in the base worktree/);
    expect(within(result).queryByTestId("graduation-merge-commit")).toBeNull();
  });

  it("GRU-FR-JRMA: a merge run that is not completed renders no result", async () => {
    await serve(
      makeMergeRun("m1", "working", {
        merge: {
          result: { published: "uncommitted", commit: null, mergedPaths: ["a.md"] },
        },
      }),
    );
    expect(screen.queryByTestId("graduation-merge-result")).toBeNull();
  });

  it("GRU-FR-QYEE, GRU-FR-JRMA: a completed merge run states no draft run's commit summary", async () => {
    await serve(
      completed({
        result: { published: "uncommitted", commit: null, mergedPaths: ["a.md"] },
      }),
      true,
    );
    expect(screen.queryByTestId("graduation-commits")).toBeNull();
  });
});

describe("what a merge run does not offer (GRU-FR-ELFZ)", () => {
  const draftOnly = ["Open draft", "Restart", "Revert"];
  const offered = () =>
    [...region().querySelectorAll("button")].map(
      (b) => b.textContent?.trim() ?? "",
    );

  it.each(["queued", "working", "reviewing", "blocked", "interrupted", "awaiting_author", "failed"] as const)(
    "GRU-FR-ELFZ: a %s merge run offers no Open draft, Restart or Revert",
    async (state) => {
      await serve(makeMergeRun("m1", state, { commits: ["abc1234"] }), true);
      for (const label of draftOnly) {
        expect(offered().some((text) => text.startsWith(label))).toBe(false);
      }
      expect(screen.queryByRole("button", { name: /Restart/ })).toBeNull();
    },
  );

  it("GRU-FR-ELFZ, GRU-FR-WDWB: a discarded merge run offers no Restart, in the region or on the rail", async () => {
    await serve(makeMergeRun("m1", "discarded"), true);
    expect(screen.queryByRole("button", { name: /Restart/ })).toBeNull();
    expect(screen.queryByText("Restart")).toBeNull();
  });

  it("GRU-FR-ELFZ, GRU-FR-DVWY: a completed merge run that made a commit offers no Revert", async () => {
    await serve(
      makeMergeRun("m1", "completed", {
        commits: ["9f2c1ab"],
        merge: {
          result: { published: "commit", commit: "9f2c1ab", mergedPaths: ["a.md"] },
        },
      }),
      true,
    );
    expect(screen.queryByRole("button", { name: "Revert" })).toBeNull();
  });

  it("GRU-FR-ELFZ: the region has no source-draft route, no graduated badge and no link to a draft", async () => {
    await serve(makeMergeRun("m1", "completed"), true);
    expect(region().querySelector("a")).toBeNull();
    expect(region().textContent).not.toMatch(/graduated/i);
    expect(screen.queryByRole("button", { name: /draft/i })).toBeNull();
  });

  it("GRU-FR-ELFZ: a draft run in the same list still offers Open draft", async () => {
    queue = makeQueue([makeRun("r1", "working")]);
    await open();
    await screen.findByTestId("graduation-provenance");
    expect(screen.getByRole("button", { name: "Open draft" })).toBeInTheDocument();
  });
});

describe("the controls every run has (GRU-FR-TOKG)", () => {
  const press = async (name: string | RegExp) =>
    userEvent.click(within(region()).getByRole("button", { name }));

  it("GRU-FR-TOKG: a working merge run offers Pause and Discard run, and Pause invokes pause_graduation_run", async () => {
    await serve(makeMergeRun("m1", "working"));

    expect(within(region()).getByRole("button", { name: "Discard run" })).toBeInTheDocument();
    await press("Pause");
    await waitFor(() => expect(calls("pause_graduation_run")).toHaveLength(1));
    expect(calls("pause_graduation_run")[0][1]).toEqual({ runId: "m1" });
  });

  it("GRU-FR-TOKG: a reviewing merge run offers Pause too", async () => {
    await serve(makeMergeRun("m1", "reviewing"));
    expect(within(region()).getByRole("button", { name: "Pause" })).toBeInTheDocument();
  });

  it.each(["interrupted", "blocked"] as const)(
    "GRU-FR-TOKG: a %s merge run offers Continue, which invokes continue_graduation_run",
    async (state) => {
      await serve(makeMergeRun("m1", state, { interruption: undefined }));
      await press(/Continue|Resume/);
      await waitFor(() => expect(calls("continue_graduation_run")).toHaveLength(1));
      expect(calls("continue_graduation_run")[0][1]).toEqual({ runId: "m1" });
    },
  );

  it("GRU-FR-TOKG: a merge run at rest on a spent pass budget offers Continue", async () => {
    await serve(makeMergeRun("m1", "awaiting_author", { escalation: null }));
    expect(within(region()).getByRole("button", { name: "Continue" })).toBeInTheDocument();
    expect(region().textContent).toMatch(/Waiting for your decision/);
  });

  it("GRU-FR-TOKG: Discard run invokes discard_graduation_run against the merge run", async () => {
    await serve(makeMergeRun("m1", "interrupted"));
    await press("Discard run");
    await waitFor(() => expect(calls("discard_graduation_run")).toHaveLength(1));
    expect(calls("discard_graduation_run")[0][1]).toEqual({ runId: "m1" });
  });

  it("GRU-FR-TOKG, GRU-FR-XQVG: a queued merge run offers its arrangement on the rail", async () => {
    await serve(makeMergeRun("m1", "queued"));
    const auto = screen.getByRole("button", { name: /Auto-start is on/ });
    await userEvent.click(auto);
    await waitFor(() => expect(calls("set_graduation_auto_start")).toHaveLength(1));
    expect(calls("set_graduation_auto_start")[0][1]).toEqual({
      runId: "m1",
      enabled: false,
    });
  });

  it("GRU-FR-TOKG: archive and unarchive are offered on the rail", async () => {
    await serve(makeMergeRun("m1", "working"));
    await userEvent.click(
      screen.getByRole("button", { name: /^Archive “Merge editor-work”/ }),
    );
    await waitFor(() => expect(calls("archive_graduation_run")).toHaveLength(1));
    expect(calls("archive_graduation_run")[0][1]).toEqual({ runId: "m1" });
  });

  it("GRU-FR-TOKG, GRU-FR-KQPE: a refusal renders inline against the run", async () => {
    refusals.discard_graduation_run = "run_state_not_permitted";
    await serve(makeMergeRun("m1", "interrupted"));
    await press("Discard run");

    expect(await screen.findByTestId("graduation-action-refusal")).toBeInTheDocument();
  });
});

describe("a blocked apply (GRU-FR-DBUS)", () => {
  it.each([
    ["merge_dirty_side", /working copy of the stream or of the base branch holds uncommitted changes/],
    ["merge_guard_held", /Another merge or update of the repository holds the repository/],
    ["merge_apply_failed", /could not write the merge result/],
  ])("GRU-FR-DBUS: %s says which side blocks, that nothing was written, that no pass is spent and that Continue retries", async (code, side) => {
    await serve(
      makeMergeRun("m1", "blocked", {
        blocker: {
          code,
          message: "raw backend text with /Users/someone/path",
          clearsBy: "backend wording",
          attempt: 1,
        },
      }),
    );

    const text = screen.getByTestId("graduation-blocker").textContent ?? "";
    expect(text).toMatch(side);
    expect(text).toMatch(/Nothing was written to either branch/);
    expect(text).toMatch(/No new pass is spent/);
    expect(text).toMatch(/Continue retries the apply/);
    // The code itself is never what the author reads.
    expect(text).not.toContain(code);
    expect(within(region()).getByRole("button", { name: "Continue" })).toBeInTheDocument();
  });
});

describe("a branch that moved (GRU-FR-QMWX, GRU-FR-EMNV)", () => {
  it("GRU-FR-QMWX: a run failed on merge_branch_moved says a branch moved, that nothing was written, and to start again from the selector", async () => {
    await serve(
      makeMergeRun("m1", "failed", {
        failure: {
          code: "merge_branch_moved",
          message: "internal text about tips abc and def",
          retryable: false,
        },
      }),
      true,
    );

    const failure = screen.getByTestId("graduation-failure");
    expect(failure.textContent).toMatch(/stream branch or the base branch moved/);
    expect(failure.textContent).toMatch(/Nothing was written to either branch/);
    expect(failure.textContent).toMatch(/work stream selector/);
    expect(failure.textContent).not.toMatch(/internal text/);
    // GRU-FR-QYEE: Discard run alone.
    const buttons = [
      ...region().querySelectorAll(".graduation__actions button"),
    ].map((b) => b.textContent);
    expect(buttons).toEqual(["Discard run"]);
  });

  it("GRU-FR-EMNV: a Continue refused with merge_branch_moved renders inline, keeps the state and keeps Discard run", async () => {
    refusals.continue_graduation_run = "merge_branch_moved";
    await serve(makeMergeRun("m1", "interrupted"));
    await userEvent.click(within(region()).getByRole("button", { name: /Continue|Resume/ }));

    const refusal = await screen.findByTestId("graduation-action-refusal");
    expect(refusal.textContent).toMatch(/stream branch or the base branch moved/);
    expect(refusal.textContent).toMatch(/Nothing was written/);
    // The run keeps the state it had.
    expect(screen.getByTestId("graduation-state")).toHaveTextContent("Interrupted");
    expect(within(region()).getByRole("button", { name: "Discard run" })).toBeInTheDocument();
  });
});

describe("the escalation of a merge run (GEA-FR-MRDK, GEA-FR-WQZH, GEA-FR-HBCF)", () => {
  const asking = () =>
    makeMergeRun("m1", "awaiting_author", {
      merge: {
        publication: { kind: "commit", message: "<i>merge</i> the specs" },
        unresolvedPaths: ["specifications/core/GRD-graduation.md", "src/a.ts"],
        conflicts: [
          {
            path: "specifications/core/GRD-graduation.md",
            baseChange: "updated",
            streamChange: "updated",
          },
          { path: "src/a.ts", baseChange: "updated", streamChange: "deleted" },
        ],
      },
      escalation: makeEscalation({ origin: "work" }),
    });

  it("GEA-FR-MRDK: it is answered in the run region, with the reason and its questions", async () => {
    await serve(asking());

    const form = within(region()).getByTestId("graduation-escalation");
    expect(form).toHaveTextContent("The log window can page by record or by segment.");
    expect(within(form).getByText(/Which unit should paging move by\?/)).toBeInTheDocument();
  });

  it("GEA-FR-WQZH: it names the unresolved paths as read text and the publication, with the message escaped", async () => {
    await serve(asking());

    const form = screen.getByTestId("graduation-escalation");
    const paths = within(form).getByTestId("graduation-escalation-paths");
    expect(paths.textContent).toMatch(/Paths the merge asks about · 2 paths/);
    expect(within(paths).getByText("GRD-graduation.md")).toBeInTheDocument();
    expect(within(paths).getByText("a.ts")).toBeInTheDocument();
    // Read text: a path opens nothing.
    expect(within(paths).getByText("a.ts").closest("button")).toBeNull();

    const publication = within(form).getByTestId("graduation-escalation-publication");
    expect(publication).toHaveTextContent(/committed under the author's message/);
    expect(publication).toHaveTextContent("<i>merge</i> the specs");
    expect(publication.querySelector("i")).toBeNull();
  });

  it("GEA-FR-WQZH: an uncommitted publication is named too", async () => {
    await serve(
      makeMergeRun("m1", "awaiting_author", { escalation: makeEscalation() }),
    );
    expect(screen.getByTestId("graduation-escalation-publication")).toHaveTextContent(
      /left uncommitted in the base worktree/,
    );
  });

  it("GEA-FR-MRDK, GEA-FR-VWQH: Send answers invokes answer_graduation_escalation once with the whole ordered set, against the run", async () => {
    await serve(asking());

    await userEvent.click(
      within(region()).getByRole("radio", { name: /By segment/ }),
    );
    await userEvent.click(within(region()).getByTestId("graduation-send-answers"));

    await waitFor(() => expect(calls("answer_graduation_escalation")).toHaveLength(1));
    expect(calls("answer_graduation_escalation")[0][1]).toEqual({
      runId: "m1",
      answers: [{ position: 1, answer: "segment", summary: "By segment" }],
    });
    // The update's operation is never reached from a merge run's escalation.
    expect(calls("answer_work_stream_update_escalation")).toHaveLength(0);
  });

  it("GEA-FR-MRDK, GEA-FR-XTUE: the unsent answer is held against the run's id and survives a reload of the listing", async () => {
    await serve(asking());
    await userEvent.click(within(region()).getByRole("radio", { name: /By record/ }));

    queue = makeQueue([asking()]);
    const handlers = listened.mock.calls
      .filter(([name]) => name === "graduation-run-changed")
      .map(([, handler]) => handler as (event: { payload: unknown }) => void);
    for (const handler of handlers) handler({ payload: {} });

    await waitFor(() => expect(calls("list_graduation_queue").length).toBeGreaterThan(1));
    expect(within(region()).getByRole("radio", { name: /By record/ })).toBeChecked();
  });

  it("GEA-FR-HBCF, GEA-FR-TEMG: an answer refused with merge_branch_moved says a branch moved and keeps every answer", async () => {
    refusals.answer_graduation_escalation = "merge_branch_moved";
    await serve(asking());
    await userEvent.click(within(region()).getByRole("radio", { name: /By segment/ }));
    await userEvent.click(within(region()).getByTestId("graduation-send-answers"));

    const alert = await screen.findByTestId("graduation-error");
    expect(alert.textContent).toMatch(/stream branch or the base branch moved/);
    expect(alert.textContent).toMatch(/Nothing was written/);
    // Every entered answer stays, every question stays reachable, and the run
    // still waits on the author with Discard run on offer.
    expect(within(region()).getByRole("radio", { name: /By segment/ })).toBeChecked();
    expect(screen.getByTestId("graduation-state")).toHaveTextContent("Waiting on you");
    expect(within(region()).getByRole("button", { name: "Discard run" })).toBeInTheDocument();
  });
});

describe("selecting a run by its id (GRU-FR-PAHN)", () => {
  const two = () =>
    makeQueue([
      makeRun("r1", "working"),
      makeMergeRun("m1", "working", { merge: { name: "Merge docs-pass" } }),
    ]);

  it("GRU-FR-PAHN: a request naming a merge run the listing holds selects it and renders its region", async () => {
    queue = two();
    await open({ selectRun: { runId: "m1", nonce: 101 } });

    await waitFor(() =>
      expect(region()).toHaveAttribute("data-run", "m1"),
    );
    expect(within(region()).getByText("Merge docs-pass")).toBeInTheDocument();
  });

  it("GRU-FR-PAHN: it makes the rail's view show that run, relaxing a filter that hides it", async () => {
    queue = makeQueue([
      makeRun("r1", "working"),
      makeMergeRun("m1", "completed", {
        merge: { result: { published: "uncommitted", commit: null, mergedPaths: [] } },
      }),
    ]);
    await open({ selectRun: { runId: "m1", nonce: 102 } });

    // A completed run is not in the In-flight view the rail rests on.
    await waitFor(() => expect(region()).toHaveAttribute("data-run", "m1"));
    expect(
      screen.getAllByTestId("graduation-row").some(
        (row) => row.getAttribute("aria-label") === "Merge editor-work",
      ),
    ).toBe(true);
  });

  it("GRU-FR-PAHN: it selects a draft run by its id as well", async () => {
    queue = two();
    await open({ selectRun: { runId: "r1", nonce: 103 } });
    await waitFor(() => expect(region()).toHaveAttribute("data-run", "r1"));
  });

  it("GRU-FR-PAHN: a run the listing does not hold makes the section reload once, then say the run is not found and leave the selection", async () => {
    queue = two();
    await open();
    await waitFor(() => expect(region()).toHaveAttribute("data-run", "r1"));
    const reads = calls("list_graduation_queue").length;

    // Render again, naming a run that does not exist anywhere.
    cleanup();
    await open({ selectRun: { runId: "gone", nonce: 104 } });

    expect(await screen.findByTestId("graduation-run-not-found")).toHaveTextContent(
      /not found/i,
    );
    // The first mount read once; this mount read once, and the request read
    // exactly once more.
    expect(calls("list_graduation_queue").length - reads).toBe(2);
    // The selection is whatever the section chose without being asked.
    expect(region().getAttribute("data-run")).not.toBe("gone");
  });

  it("GRU-FR-PAHN: a run that appears in the one reload is selected", async () => {
    queue = makeQueue([makeRun("r1", "working")]);
    let reads = 0;
    invoked.mockImplementation(async (command: string) => {
      switch (command) {
        case "list_graduation_queue":
          reads += 1;
          // The second read is the request's reload, and by then the merge run
          // the handoff made is in the listing.
          if (reads >= 2) {
            return makeQueue([makeRun("r1", "working"), makeMergeRun("m1", "queued")]);
          }
          return queue;
        case "load_app_preferences":
          return { graduationRailWidthFraction: 0.2 };
        default:
          return undefined;
      }
    });
    render(<GraduationRuns selectRun={{ runId: "m1", nonce: 105 }} />);

    await waitFor(() => expect(region()).toHaveAttribute("data-run", "m1"));
    expect(screen.queryByTestId("graduation-run-not-found")).toBeNull();
  });
});
