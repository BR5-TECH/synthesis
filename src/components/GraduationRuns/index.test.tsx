import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { resetAppPreferencesCache } from "../../state/appPreferences";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import {
  makeEscalation,
  makeObservability,
  makePass,
  makeQueue,
  makeRun,
  PROJECT_KEY,
} from "../../test/graduationFixtures";
import type { GraduationQueue, GraduationRun } from "../../types";
import { pickSelector, selectorValue } from "../../test/selectors";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

/** What the backend answers with, per command. */
let queue: GraduationQueue;
let railFraction: number;
let refusals: Record<string, string>;

beforeEach(() => {
  queue = makeQueue([makeRun("r1", "working")]);
  railFraction = 0.2;
  refusals = {};
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    if (refusals[command]) throw refusals[command];
    switch (command) {
      case "list_graduation_queue":
        return queue;
      case "load_app_preferences":
        return { graduationRailWidthFraction: railFraction };
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
});
afterEach(() => {
  cleanup();
  // The drag test stubs a method on HTMLElement.prototype, which every later
  // test in this file would otherwise inherit.
  vi.restoreAllMocks();
});

/** Render the section and wait for its first read of the queue. */
async function open(props: Parameters<typeof GraduationRuns>[0] = {}) {
  render(<GraduationRuns {...props} />);
  await screen.findByTestId("graduation-section");
  await waitFor(() => expect(commands()).toContain("list_graduation_queue"));
  return props;
}

/**
 * Open the section and put the rail in **All**.
 *
 * The rail rests on **In-flight** (GRH-FR-WIMY), so a test about a run that has
 * ended has to ask for the position that lists it, exactly as its author would.
 */
async function openAll(props: Parameters<typeof GraduationRuns>[0] = {}) {
  const opened = await open(props);
  await pickSelector("Run view", "all");
  return opened;
}

/** Fire one of the two events the section reloads on. */
async function fire(event: string) {
  const handlers = listened.mock.calls
    .filter(([name]) => name === event)
    .map(([, handler]) => handler as (payload: unknown) => void);
  expect(handlers.length).toBeGreaterThan(0);
  await act(async () => {
    for (const handler of handlers) handler({ payload: {} });
  });
}

/** Whether a command is a read of the listing. */
const isListing = (command: string) => command === "list_graduation_queue";

/** The command names the section has invoked, in order. */
function commands(): string[] {
  return invoked.mock.calls.map(([command]) => command as string);
}

describe("the run region", () => {
  it("GRU-FR-MYFA: it renders the project's runs in the project's own run order", async () => {
    // The project's order, the alphabetical order and the timestamp order all
    // disagree, so a sort of any kind is visible.
    const named = (id: string, name: string, at: string) =>
      makeRun(id, "queued", {
        createdAt: at,
        input: { ...makeRun(id).input, draftName: name },
      });
    queue = makeQueue([
      named("r3", "Zulu", "2026-09-06T11:00:00Z"),
      named("r1", "Alpha", "2026-09-06T09:00:00Z"),
      named("r2", "Mike", "2026-09-06T10:00:00Z"),
    ]);
    await open();
    await waitFor(() =>
      expect(screen.getAllByTestId("graduation-row")).toHaveLength(3),
    );
    expect(
      screen
        .getAllByTestId("graduation-row")
        .map((row) => row.getAttribute("aria-label")),
    ).toEqual(["Zulu", "Alpha", "Mike"]);
  });

  it("GRU-FR-UKNC: the selected run names the stream it runs in", async () => {
    await open();
    expect(await screen.findByTestId("graduation-provenance")).toHaveTextContent(
      "editor-work",
    );
  });

  it("GRU-FR-IZKI: the stage row renders what the persisted record holds", async () => {
    queue = makeQueue([
      makeRun("r1", "reviewing", {
        checkpoint: { pass: 2 },
        observability: makeObservability({
          currentStage: "review",
          stageCondition: "active",
          passes: [makePass({ pass: 1 }), makePass({ pass: 2 })],
        }),
      }),
    ]);
    await open();
    const row = await screen.findByTestId("run-progress");
    // GRU-FR-LBPR: the stage the record stands at, and the pass it is on.
    expect(row).toHaveTextContent("Review · pass 2 of 2");
  });

  it("GRU-FR-HKBD: a queued run renders its place in its own stream's queue", async () => {
    queue = makeQueue([
      makeRun("r1", "queued"),
      makeRun("r2", "queued", { autoStart: false }),
    ]);
    await open();
    await userEvent.click(await screen.findByRole("button", { name: "Run r2" }));
    const row = await screen.findByTestId("run-progress");
    // One run stands ahead of it in that stream's queue, and it says so as
    // well as saying that it will not start on its own.
    expect(row).toHaveTextContent("1 ahead of it in the stream");
    expect(row).toHaveTextContent("auto-start off");
  });

  it("GRU-FR-UKNC: a run whose stream is gone names it and says it is gone", async () => {
    queue = makeQueue([makeRun("r1", "completed", { streamId: "" })]);
    await openAll();
    expect(await screen.findByTestId("graduation-provenance")).toHaveTextContent(
      /no longer exists/,
    );
  });

  it("GRU-FR-CKOB: a record this build does not recognise loses the stage row and nothing else", async () => {
    queue = makeQueue([
      makeRun("r1", "queued", {
        autoStart: false,
        observability: makeObservability({ observabilityVersion: 2 }),
        checkpoint: { changedPaths: ["src/a.ts"] },
      }),
    ]);
    await open();
    expect(screen.queryByTestId("run-progress")).toBeNull();
    // Everything else about the run still renders, including what the stage
    // row used to be the only carrier of.
    expect(await screen.findByTestId("graduation-provenance")).toBeInTheDocument();
    expect(leafPaths(screen.getByTestId("graduation-changes"))).toEqual(["src/a.ts"]);
    expect(screen.getByTestId("graduation-condition")).toHaveTextContent(
      /auto-start off/,
    );
  });

  it("GRU-FR-LBPR: a blocked run states what blocks it and the act that clears it", async () => {
    queue = makeQueue([
      makeRun("r1", "blocked", {
        blocker: {
          code: "review_checkout_failed",
          message: "The review checkout could not be created.",
          clearsBy: "Continue the run.",
          attempt: 1,
        },
      }),
    ]);
    await open();
    const blocker = await screen.findByTestId("graduation-blocker");
    expect(blocker).toHaveTextContent("The review checkout could not be created.");
    expect(blocker).toHaveTextContent("Continue the run.");
  });

  it("GRU-FR-FZCN: a run resting for the author on a repeated blocker states it and offers Continue", async () => {
    queue = makeQueue([
      makeRun("r1", "awaiting_author", {
        blocker: {
          code: "review_checkout_failed",
          message:
            "symlink refused: /Users/someone/.synthesis/g/g1/rv/node_modules/.pnpm/mdn-data",
          clearsBy: "Continue the run to try the review again.",
          attempt: 2,
        },
      }),
    ]);
    await open();

    // GRU-FR-LBPR: the account of what stopped it renders in this state too.
    const blocker = await screen.findByTestId("graduation-blocker");
    expect(blocker).toHaveTextContent("The review could not be given a checkout to stand in.");
    expect(blocker).toHaveTextContent("It stopped here 2 times in a row.");

    // The backend's own string is detail beneath, never the sentence itself.
    const detail = await screen.findByTestId("graduation-blocker-detail");
    expect(detail).toHaveTextContent("symlink refused");
    expect(
      blocker.firstChild?.textContent ?? "",
      "the headline carries the internal path",
    ).not.toContain("/Users/");

    // GRU-FR-FZCN: Continue is still offered — the rail's and the region's.
    expect(screen.getAllByRole("button", { name: /continue/i }).length).toBeGreaterThan(0);
  });

  it("GRU-FR-BHJO: an interrupted run states why it stopped and offers Continue", async () => {
    queue = makeQueue([
      makeRun("r1", "interrupted", {
        interruption: {
          reason: "application_shutdown",
          detail: "The application closed while it was working.",
          streamReleased: true,
          at: "2026-09-06T09:20:00Z",
        },
      }),
    ]);
    await open();
    expect(await screen.findByTestId("graduation-interruption")).toHaveTextContent(
      "Stopped when the application closed",
    );
    const region = screen.getByRole("region", { name: "The selected run" });
    expect(within(region).getByRole("button", { name: "Continue" })).toBeInTheDocument();
  });

  it("GRU-FR-BHJO, GRU-FR-QLRQ, GRU-FR-JAEY: a run stopped at its time limit names it and routes to the setting", async () => {
    queue = makeQueue([
      makeRun("r1", "interrupted", {
        observability: makeObservability({ stageCondition: "stopped", passes: [makePass()] }),
        interruption: {
          reason: "execution_timeout",
          detail: "The agent turn ran for 2 h and reached the execution time limit of 2 h.",
          streamReleased: true,
          at: "2026-10-03T22:02:36Z",
        },
      }),
    ]);
    const onOpenSettings = vi.fn();
    await open({ onOpenSettings });
    const notice = await screen.findByTestId("graduation-interruption");
    expect(notice).toHaveTextContent("Stopped — the turn reached its time limit");
    expect(notice).toHaveTextContent("reached the execution time limit of 2 h");
    // GRU-FR-JAEY: the pass the turn stopped in reads as stopped, not working.
    expect(screen.getByTestId("graduation-pass")).toHaveTextContent(
      "Stopped — the turn reached its time limit",
    );
    await userEvent.click(within(notice).getByRole("button", { name: "Change the time limit" }));
    expect(onOpenSettings).toHaveBeenCalledWith("graduation");
    // GRU-FR-QLRQ: the route opens settings and does not continue the run.
    expect(commands()).not.toContain("continue_graduation_run");
  });

  it("GRU-FR-QLRQ, GRU-FR-BHJO: a run stopped on any other cause offers no route to the time limit", async () => {
    queue = makeQueue([
      makeRun("r1", "interrupted", {
        interruption: {
          reason: "agent_exited",
          detail: "The agent process stopped with exit code 137 before it gave an answer.",
          streamReleased: true,
          at: "2026-10-03T22:02:36Z",
        },
      }),
    ]);
    await open({ onOpenSettings: vi.fn() });
    const notice = await screen.findByTestId("graduation-interruption");
    expect(notice).toHaveTextContent("Stopped — the agent process exited");
    expect(within(notice).queryByRole("button", { name: "Change the time limit" })).toBeNull();
  });

  it("GRU-FR-TFEZ: a run whose stream branch the remote did not take says so, once, and is not blocked by it", async () => {
    queue = makeQueue([
      makeRun("r1", "completed", {
        standingWorkOutcome: {
          commit: "a91bc04",
          pushed: false,
          pushFailure: { code: "github_unreachable" },
        },
      }),
    ]);
    await openAll();
    const said = await screen.findAllByTestId("graduation-push-refused");
    expect(said).toHaveLength(1);
    expect(said[0]).toHaveTextContent(/was not pushed: the remote could not be reached/);
    expect(said[0]).toHaveTextContent(/not stopped by it/);
    // It is a report and not a condition: the run is where its own state puts
    // it, and no blocker stands beside it.
    expect(screen.queryByTestId("graduation-blocker")).toBeNull();
    expect(
      within(screen.getByRole("region", { name: "The selected run" })).queryByRole(
        "button",
        { name: /Push/ },
      ),
    ).toBeNull();
  });

  it("GRU-FR-TFEZ: a run whose push landed, and one that asked for none, say nothing about it", async () => {
    queue = makeQueue([
      makeRun("r1", "completed", {
        standingWorkOutcome: { commit: "a91bc04", pushed: true },
      }),
      makeRun("r2", "completed", { standingWorkOutcome: { commit: "a91bc04" } }),
    ]);
    await openAll();
    await screen.findByRole("region", { name: "The selected run" });
    expect(screen.queryByTestId("graduation-push-refused")).toBeNull();
  });

  it("GRU-FR-QYEE: a failed run states the typed failure and offers Discard run alone", async () => {
    queue = makeQueue([
      makeRun("r1", "failed", {
        observability: makeObservability({ observabilityVersion: 2 }),
        failure: {
          code: "container_unavailable",
          message: "No container was available.",
          retryable: false,
        },
      }),
    ]);
    await openAll();
    // Read from the run rather than from the stage row, so a record this build
    // cannot render never takes it away.
    expect(await screen.findByTestId("graduation-failure")).toHaveTextContent(
      "No container was available.",
    );
    const region = screen.getByRole("region", { name: "The selected run" });
    expect(
      within(region).getByRole("button", { name: "Discard run" }),
    ).toBeInTheDocument();
    expect(within(region).queryByRole("button", { name: "Revert" })).toBeNull();
    expect(within(region).queryByRole("button", { name: "Pause" })).toBeNull();
  });

  it("GRU-FR-RRNN, GRU-FR-TXLW: the change set is the paths and no judgement of them", async () => {
    const paths = ["src/a.ts", "src/a.test.ts", "specifications/ui/EDT-editor.md"];
    queue = makeQueue([makeRun("r1", "working", { checkpoint: { changedPaths: paths } })]);
    await open();
    const changes = await screen.findByTestId("graduation-changes");
    expect(changes).toHaveTextContent("Changed · 3 paths");
    // A list this short opens every folder (GRU-FR-HEQB), so every path is a
    // row, in tree order: folders first, then names.
    expect(leafPaths(changes)).toEqual([
      "specifications/ui/EDT-editor.md",
      "src/a.test.ts",
      "src/a.ts",
    ]);
    // Each row states its name and nothing else: no verdict on a path.
    expect(
      Array.from(changes.querySelectorAll<HTMLElement>("[data-path]")).map(
        (row) => row.textContent,
      ),
    ).toEqual(["EDT-editor.md", "a.test.ts", "a.ts"]);
    // And each folder states its name and its count and nothing else.
    expect(
      Array.from(changes.querySelectorAll<HTMLElement>("[data-folder]")).map(
        (row) => row.textContent,
      ),
    ).toEqual(["specifications/ui1, 1 path", "src2, 2 paths"]);
    expect(
      within(changes).getByRole("button", { name: "specifications/ui, 1 path" }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(
      within(changes).getByRole("button", { name: "src, 2 paths" }),
    ).toHaveAttribute("aria-expanded", "true");
  });

  it("GRU-FR-WJHV: work an ignore rule hides is counted and named", async () => {
    queue = makeQueue([
      makeRun("r1", "working", {
        checkpoint: { hiddenPaths: [".synthesis/drafts/a.md"], hiddenPathsOmitted: 5 },
      }),
    ]);
    await open();
    const hidden = await screen.findByTestId("graduation-hidden");
    expect(hidden).toHaveTextContent("Hidden by ignore rules · 1 path");
    expect(leafPaths(hidden)).toEqual([".synthesis/drafts/a.md"]);
    expect(hidden).toHaveTextContent("5 more are not listed.");
  });

  it.each([
    { name: "paths and passes", paths: ["src/a.ts"], passes: 1, split: "true" },
    { name: "paths alone", paths: ["src/a.ts"], passes: 0, split: undefined },
    { name: "passes alone", paths: [], passes: 1, split: undefined },
    { name: "hidden paths and passes", paths: [], hidden: ["a/"], passes: 1, split: "true" },
  ])(
    "GRU-FR-MCYF: a run with $name sets two columns only where both sides hold something",
    async ({ paths, hidden, passes, split }) => {
      queue = makeQueue([
        makeRun("r1", "working", {
          checkpoint: { changedPaths: paths, hiddenPaths: hidden ?? [] },
          observability: makeObservability({
            passes: Array.from({ length: passes }, (_, i) => makePass({ pass: i + 1 })),
          }),
        }),
      ]);
      await open();
      const region = await screen.findByRole("region", { name: "The selected run" });
      const columns = region.querySelector<HTMLElement>(".graduation__columns");
      expect(columns).not.toBeNull();
      // A direct child of the region, so the region is the container the
      // two-column query reads.
      expect(columns!.parentElement).toBe(region);
      expect(columns!.dataset.split).toBe(split);
      // A side with nothing in it takes no column.
      expect(region.querySelector(".graduation__columns-paths") !== null).toBe(
        paths.length + (hidden?.length ?? 0) > 0,
      );
    },
  );

  it("GRU-FR-NUCJ, GRU-FR-HEQB: a reload of the same run keeps what the author closed", async () => {
    const run = (paths: string[]) =>
      makeRun("r1", "working", { checkpoint: { changedPaths: paths } });
    queue = makeQueue([run(["src/a/x.ts", "src/b/y.ts"])]);
    await open();
    await userEvent.click(await screen.findByRole("button", { name: "a, 1 path" }));
    await userEvent.click(screen.getByRole("button", { name: "Changed · 2 paths" }));

    queue = makeQueue([run(["src/a/x.ts", "src/b/y.ts", "src/c/z.ts"])]);
    await fire("graduation-run-changed");
    const head = await screen.findByRole("button", { name: "Changed · 3 paths" });
    expect(head).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(head);
    expect(screen.getByRole("button", { name: "a, 1 path" })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(screen.getByRole("button", { name: "c, 1 path" })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });

  it("GRU-FR-NUCJ: another run's list opens by its own rule", async () => {
    queue = makeQueue([
      makeRun("r1", "working", { checkpoint: { changedPaths: ["src/a.ts"] } }),
      makeRun("r2", "working", { checkpoint: { changedPaths: ["src/b.ts"] } }),
    ]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    const selectedRun = region.dataset.run;
    await userEvent.click(
      await within(region).findByRole("button", { name: "Changed · 1 path" }),
    );
    expect(
      within(region).getByRole("button", { name: "Changed · 1 path" }),
    ).toHaveAttribute("aria-expanded", "false");

    const other = screen
      .getAllByTestId("graduation-row")
      .find((row) => !row.getAttribute("aria-label")?.endsWith(selectedRun ?? ""));
    expect(other).toBeDefined();
    await userEvent.click(other!);
    await waitFor(() => expect(region.dataset.run).not.toBe(selectedRun));
    expect(
      within(region).getByRole("button", { name: "Changed · 1 path" }),
    ).toHaveAttribute("aria-expanded", "true");
  });

  it("GRU-FR-MCYF: a run with no paths and no passes renders no columns", async () => {
    queue = makeQueue([
      makeRun("r1", "working", {
        observability: makeObservability({ passes: [] }),
      }),
    ]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    await screen.findByTestId("graduation-provenance");
    expect(region.querySelector(".graduation__columns")).toBeNull();
  });

  it("GRU-FR-OZAR: the action row stands in the foot bar, the last thing in the region", async () => {
    queue = makeQueue([makeRun("r1", "working", { checkpoint: { changedPaths: ["src/a.ts"] } })]);
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    await screen.findByTestId("graduation-provenance");
    const foot = region.querySelector<HTMLElement>(".graduation__foot");
    expect(foot).not.toBeNull();
    expect(region.lastElementChild).toBe(foot);
    expect(foot!.querySelector(".graduation__actions")).not.toBeNull();
    expect(
      within(foot!).getByRole("button", { name: "Open draft" }),
    ).toBeInTheDocument();
  });

  it("GRU-FR-QYEE / GRU-FR-DVWY: a completed run names its commits and offers Revert", async () => {
    queue = makeQueue([
      makeRun("r1", "completed", { commits: ["c0ffee1", "beefcaf"] }),
    ]);
    await openAll();
    expect(await screen.findByTestId("graduation-commits")).toHaveTextContent(
      /2 commits/,
    );
    const region = screen.getByRole("region", { name: "The selected run" });
    await userEvent.click(within(region).getByRole("button", { name: "Revert" }));
    await waitFor(() => expect(commands()).toContain("revert_graduation_run"));
    expect(screen.getByTestId("graduation-row-announcement")).toHaveTextContent(
      "Reverted the run's commits.",
    );
  });

  it("GRU-FR-RZDI: every run routes to its source draft, by id", async () => {
    const onOpenDraft = vi.fn();
    await open({ onOpenDraft });
    await userEvent.click(await screen.findByRole("button", { name: "Open draft" }));
    expect(onOpenDraft).toHaveBeenCalledWith("draft-r1");
  });

  it("GRU-FR-GLSO: the section offers no way to graduate a draft", async () => {
    await open();
    const region = await screen.findByRole("region", { name: "The selected run" });
    // The region is rendering a run, and what it offers on it is what the run's
    // state affords and nothing that would start one.
    expect(within(region).getByTestId("graduation-state")).toBeInTheDocument();
    // What it offers on the run is what that run's state affords, and nothing
    // that would start a graduation.
    expect(
      [...region.querySelectorAll(".graduation__actions button")].map(
        (button) => button.textContent,
      ),
    ).toEqual(["Open draft", "Pause", "Discard run"]);
    expect(screen.queryByRole("button", { name: /graduate/i })).toBeNull();
  });

  it("GRU-FR-MRPE: agent text renders as escaped plain text", async () => {
    queue = makeQueue([
      makeRun("r1", "blocked", {
        blocker: {
          code: "x",
          message: "# Head <b>bold</b>",
          clearsBy: "Continue it.",
          attempt: 1,
        },
      }),
    ]);
    await open();
    const blocker = await screen.findByTestId("graduation-blocker");
    expect(blocker.querySelector("b")).toBeNull();
    expect(blocker.textContent).toContain("<b>bold</b>");
  });

  it("GRU-FR-FZCN: an escalation renders once, and the action row carries no second route to it", async () => {
    queue = makeQueue([
      makeRun("r1", "awaiting_author", { escalation: makeEscalation() }),
    ]);
    await open();
    expect(await screen.findByTestId("graduation-escalation")).toBeInTheDocument();
    expect(screen.getAllByTestId("graduation-send-answers")).toHaveLength(1);
    expect(screen.queryByRole("button", { name: /^Answer/ })).toBeNull();
  });

  it("GRU-FR-YYXN: the pass history follows the run the author selects", async () => {
    queue = makeQueue([
      makeRun("r1", "working"),
      makeRun("r2", "completed", {
        observability: makeObservability({
          passes: [
            makePass({ pass: 1, status: "failed", verdict: "revise" }),
            makePass({ pass: 2, status: "passed", verdict: "ready" }),
          ],
        }),
      }),
    ]);
    await openAll();
    await userEvent.click(await screen.findByRole("button", { name: "Run r2" }));
    await waitFor(() =>
      expect(screen.getAllByTestId("graduation-pass")).toHaveLength(2),
    );
    // The open row belongs to the run it was opened on: the newest pass of the
    // run now selected, rather than the row left open on the run before it.
    const open_ = screen.getAllByRole("button", { expanded: true });
    expect(open_).toHaveLength(1);
    expect(open_[0]).toHaveTextContent("Pass 2");
  });
});

describe("what the section reads, and when", () => {
  it("GRU-FR-ZVTC / GRD-FR-EFAU: a burst of events inside one window reads the listing once", async () => {
    vi.useFakeTimers();
    try {
      render(<GraduationRuns />);
      await vi.waitFor(() =>
        expect(commands()).toContain("list_graduation_queue"),
      );
      const before = commands().filter((c) => c === "list_graduation_queue").length;
      await fire("graduation-run-changed");
      await fire("graduation-run-changed");
      await fire("graduation-queue-changed");
      await vi.advanceTimersByTimeAsync(500);
      const after = commands().filter((c) => c === "list_graduation_queue").length;
      expect(after).toBe(before + 1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("GRU-FR-ZVTC: a read that fails renders inline and clears on the next good read", async () => {
    refusals = { list_graduation_queue: "project_not_open" };
    render(<GraduationRuns />);
    const alert = await screen.findByTestId("graduation-error");
    expect(alert).toHaveAttribute("role", "alert");
    refusals = {};
    await fire("graduation-queue-changed");
    await waitFor(() => expect(screen.queryByTestId("graduation-error")).toBeNull());
  });

  it("GRU-FR-XQVG: a refused act renders against the run region, and the run is unchanged", async () => {
    await open();
    refusals = { pause_graduation_run: "run_not_working" };
    await userEvent.click(
      await screen.findByRole("button", { name: "Pause “Run r1”" }),
    );
    const region = screen.getByRole("region", { name: "The selected run" });
    expect(await within(region).findByTestId("graduation-error")).toBeInTheDocument();
    expect(screen.getByTestId("graduation-state")).toHaveTextContent("Working");
  });

  it("GRU-FR-ZVTC: the row renders what the backend reports rather than what was attempted", async () => {
    await open();
    // The pause resolves, and the listing read back after it is what the state
    // is taken from — here a state the surface never attempted.
    queue = makeQueue([
      makeRun("r1", "interrupted", {
        interruption: {
          reason: "author_pause",
          detail: "You paused it.",
          streamReleased: true,
          at: "2026-09-06T09:20:00Z",
        },
      }),
    ]);
    await userEvent.click(
      await screen.findByRole("button", { name: "Pause “Run r1”" }),
    );
    await waitFor(() =>
      expect(screen.getByTestId("graduation-state")).toHaveTextContent(
        "Interrupted",
      ),
    );
  });
});

describe("the rail and the region together", () => {
  const listedRows = () =>
    screen.getAllByTestId("graduation-row").map((row) => row.getAttribute("aria-label"));

  it("GRH-FR-WIMY / GRH-FR-KZTP / GRH-FR-DYNU: picking a position changes what the section lists", async () => {
    queue = makeQueue([
      makeRun("r1", "queued"),
      makeRun("r2", "working"),
      makeRun("r3", "completed"),
      makeRun("r4", "failed"),
      makeRun("r5", "completed", { archived: true }),
    ]);
    await open();
    // GRH-FR-KZTP: the resting position lists the queued run beside the
    // working one, and lists neither of the two that ended.
    await waitFor(() => expect(listedRows()).toEqual(["Run r1", "Run r2"]));

    await pickSelector("Run view", "completed");
    await waitFor(() => expect(listedRows()).toEqual(["Run r3"]));

    await pickSelector("Run view", "archived");
    await waitFor(() => expect(listedRows()).toEqual(["Run r5"]));

    await pickSelector("Run view", "all");
    await waitFor(() =>
      expect(listedRows()).toEqual([
        "Run r1",
        "Run r2",
        "Run r3",
        "Run r4",
        "Run r5",
      ]),
    );
  });

  it("GRH-FR-YMIM: a filed-away run holding a stream is in In-flight and in Archived", async () => {
    queue = makeQueue([
      makeRun("r1", "queued"),
      makeRun("r2", "working", { archived: true }),
    ]);
    await open();
    await waitFor(() => expect(listedRows()).toEqual(["Run r1", "Run r2"]));
    await pickSelector("Run view", "archived");
    await waitFor(() => expect(listedRows()).toEqual(["Run r2"]));
  });

  it("GRH-FR-WIMY / GRH-FR-WDSH: a project whose runs have all ended opens on an empty In-flight", async () => {
    queue = makeQueue([
      makeRun("r1", "completed"),
      makeRun("r2", "failed"),
    ]);
    await open();
    // GRH-FR-WDSH: the fallback selects the first row the filters admit, and
    // they admit none. Nothing is relaxed to reveal a run the author did not
    // ask for, so the rail states that the filter matches nothing.
    await waitFor(() =>
      expect(screen.getByText("No run matches this filter.")).toBeInTheDocument(),
    );
    expect(selectorValue("Run view")).toBe("in-flight");
    expect(screen.queryByTestId("graduation-row")).toBeNull();
    expect(document.querySelector('[aria-current="true"]')).toBeNull();
    expect(screen.queryByTestId("graduation-provenance")).toBeNull();
    // The author's own act reaches them.
    await pickSelector("Run view", "all");
    await waitFor(() => expect(listedRows()).toEqual(["Run r1", "Run r2"]));
  });

  it("GRU-FR-QYEE: discarding a run invokes the operation and says what was done", async () => {
    queue = makeQueue([makeRun("r1", "failed")]);
    await openAll();
    const region = await screen.findByRole("region", { name: "The selected run" });
    await userEvent.click(
      within(region).getByRole("button", { name: "Discard run" }),
    );
    await waitFor(() => expect(commands()).toContain("discard_graduation_run"));
    expect(screen.getByTestId("graduation-row-announcement")).toHaveTextContent(
      "Discarded the run.",
    );
  });

  it("GEA-FR-QULY: an unsent answer survives a re-list of the queue", async () => {
    queue = makeQueue([
      makeRun("r1", "awaiting_author", { escalation: makeEscalation() }),
    ]);
    await open();
    const field = await screen.findByRole("textbox", {
      name: "Your answer to question 1",
    });
    await userEvent.type(field, "By record, please");
    await fire("graduation-queue-changed");
    await waitFor(() => expect(commands().filter(isListing).length).toBe(2));
    expect(
      screen.getByRole("textbox", { name: "Your answer to question 1" }),
    ).toHaveValue("By record, please");
  });

  it("GRU-FR-WDWB: a restart confirmation belongs to the run it was opened on", async () => {
    queue = makeQueue([
      makeRun("r1", "discarded"),
      makeRun("r2", "discarded"),
    ]);
    await openAll();
    await userEvent.click(await screen.findByTestId("graduation-restart"));
    expect(
      await screen.findByTestId("graduation-restart-confirm"),
    ).toBeInTheDocument();
    // The author moves to another discarded run without confirming. The
    // confirmation is the first run's, and it does not come with them.
    await userEvent.click(screen.getByRole("button", { name: "Run r2" }));
    await waitFor(() =>
      expect(screen.queryByTestId("graduation-restart-confirm")).toBeNull(),
    );
    expect(screen.getByTestId("graduation-restart")).toBeInTheDocument();
  });

  it("GEA-FR-BVMQ: the rest of the queue stays operable while an escalation stands", async () => {
    queue = makeQueue([
      makeRun("r1", "awaiting_author", { escalation: makeEscalation() }),
      makeRun("r2", "working"),
    ]);
    await open();
    expect(await screen.findByTestId("graduation-escalation")).toBeInTheDocument();
    // No focus trap and no modal: the rail's own controls answer as they do in
    // every other state.
    await userEvent.click(screen.getByRole("button", { name: "Run r2" }));
    await waitFor(() =>
      expect(
        document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
      ).toBe("Run r2"),
    );
    expect(screen.queryByTestId("graduation-escalation")).toBeNull();
  });
});

describe("the boundary between the two regions", () => {
  const divider = () => screen.getByTestId("graduation-divider");
  const section = () => screen.getByTestId("graduation-section");

  it("GRH-FR-MCHQ: it is a named separator carrying the width it stands at", async () => {
    await open();
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "20"));
    expect(divider()).toHaveAttribute("role", "separator");
    expect(divider()).toHaveAttribute("aria-orientation", "vertical");
    expect(divider()).toHaveAttribute("aria-label", "History width");
    expect(divider()).toHaveAttribute("aria-valuemin", "5");
    expect(divider()).toHaveAttribute("aria-valuemax", "30");
    expect(divider()).toHaveAttribute(
      "aria-valuetext",
      "History width 20 percent",
    );
  });

  it("GRH-FR-MCHQ: the rail opens at the stored width, brought inside its bounds", async () => {
    railFraction = 0.9;
    await open();
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "30"));
    expect(section().style.getPropertyValue("--graduation-rail")).toBe("30%");
  });

  it("GRH-FR-OFHS: the width is set by keyboard alone, and what the author set is stored", async () => {
    await open();
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "20"));
    divider().focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(divider()).toHaveAttribute("aria-valuenow", "21");
    expect(section().style.getPropertyValue("--graduation-rail")).toBe("21%");
    await waitFor(() =>
      expect(invoked).toHaveBeenCalledWith(
        "save_app_preferences",
        expect.objectContaining({
          preferences: expect.objectContaining({
            graduationRailWidthFraction: expect.closeTo(0.21, 5),
          }),
        }),
      ),
    );
    await userEvent.keyboard("{Home}");
    expect(divider()).toHaveAttribute("aria-valuenow", "5");
    await userEvent.keyboard("{End}");
    expect(divider()).toHaveAttribute("aria-valuenow", "30");
  });

  it("GRH-FR-OFHS: a key the boundary does not answer to is left to the surface", async () => {
    await open();
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "20"));
    divider().focus();
    await userEvent.keyboard("{ArrowUp}");
    expect(divider()).toHaveAttribute("aria-valuenow", "20");
    expect(commands()).not.toContain("save_app_preferences");
  });

  it("GRH-FR-MCHQ: a drag costs one write rather than one per frame", async () => {
    await open();
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "20"));
    // jsdom lays nothing out, so every box is told what it is.
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      left: 0,
      width: 1000,
      top: 0,
      right: 1000,
      bottom: 100,
      height: 100,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    } as DOMRect);
    const target = divider();
    const move = (clientX: number) =>
      target.dispatchEvent(
        Object.assign(new Event("pointermove", { bubbles: true }), {
          clientX,
          pointerId: 1,
        }),
      );
    target.dispatchEvent(
      Object.assign(new Event("pointerdown", { bubbles: true }), {
        clientX: 200,
        pointerId: 1,
      }),
    );
    for (const x of [210, 220, 230, 240, 250]) move(x);
    target.dispatchEvent(
      Object.assign(new Event("pointerup", { bubbles: true }), {
        clientX: 250,
        pointerId: 1,
      }),
    );
    await waitFor(() =>
      expect(commands().filter((c) => c === "save_app_preferences")).toHaveLength(1),
    );
    expect(section().style.getPropertyValue("--graduation-rail")).toBe("25%");
  });

  it("GRH-FR-MCHQ: a move with no gesture under way, and a cancelled one, write nothing", async () => {
    await open();
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "20"));
    const target = divider();
    target.dispatchEvent(
      Object.assign(new Event("pointermove", { bubbles: true }), {
        clientX: 400,
        pointerId: 1,
      }),
    );
    expect(divider()).toHaveAttribute("aria-valuenow", "20");
    target.dispatchEvent(
      Object.assign(new Event("pointerdown", { bubbles: true }), {
        clientX: 200,
        pointerId: 1,
      }),
    );
    target.dispatchEvent(new Event("pointercancel", { bubbles: true }));
    expect(commands()).not.toContain("save_app_preferences");
  });
});

/** The full path of every path row a list renders, in order. */
function leafPaths(list: HTMLElement): string[] {
  return Array.from(list.querySelectorAll<HTMLElement>("[data-path]")).map(
    (row) => row.dataset.path ?? "",
  );
}
