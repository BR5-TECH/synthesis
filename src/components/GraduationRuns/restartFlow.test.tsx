/**
 * A restart driven through the whole section: from the rail and from the run
 * region, and what the rail shows once it is accepted
 * (`../../../specifications/ui/GRT-graduation-restart.md` GRT-FR-AMHS,
 * GRT-FR-XHLN, GRT-FR-KSBC).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { resetAppPreferencesCache } from "../../state/appPreferences";
import { graduationErrorMessage } from "../../state/graduation";
import { forgetEveryDraft } from "../../state/escalationDrafts";
import { forgetEverything } from "../../state/graduationSelection";
import { makeQueue, makeRun } from "../../test/graduationFixtures";
import type { GraduationQueue, GraduationRun } from "../../types";
import { pickSelector } from "../../test/selectors";
import { GraduationRuns } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

/** What the backend answers with, per command. */
let queue: GraduationQueue;
/** The listing read back once the restart is accepted. */
let afterRestart: GraduationQueue;
let created: GraduationRun;
let restartAnswer: () => Promise<unknown>;

const discarded = makeRun("r1", "discarded");

function streamSummary(id: string) {
  return {
    stream: {
      id,
      name: "editor-work",
      projectKey: "/Users/demo/dev/acme",
      branch: "synthesis/stream/editor-work",
      baseBranch: "main",
      baseRevision: "a91bc04",
      worktreePath: "/tmp/editor-work",
      createdAt: "2026-09-06T09:00:00Z",
      isMissing: false,
      busyRunId: null,
    },
    queuedRunCount: 0,
    aheadOfBase: 0,
    behindBase: 0,
    baseTipRevision: "a91bc04",
    missingCommits: [],
  };
}

beforeEach(() => {
  created = makeRun("r2", "queued", { restartedFromRunId: "r1" });
  queue = makeQueue([makeRun("r0", "completed"), discarded]);
  afterRestart = makeQueue([makeRun("r0", "completed"), discarded, created]);
  restartAnswer = async () => {
    queue = afterRestart;
    return created;
  };
  invoked.mockReset();
  invoked.mockImplementation(async (command: string) => {
    switch (command) {
      case "list_graduation_queue":
        return queue;
      case "load_app_preferences":
        return { graduationRailWidthFraction: 0.2 };
      case "list_work_streams":
        return [streamSummary(discarded.streamId)];
      case "restart_graduation_run":
        return restartAnswer();
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
  vi.restoreAllMocks();
});

/** Render the section, wait for its first read, and put the rail in `view`. */
async function open(view: "all" | "archived" = "all") {
  render(<GraduationRuns />);
  await screen.findByTestId("graduation-section");
  await waitFor(() =>
    expect(invoked.mock.calls.map(([c]) => c)).toContain("list_graduation_queue"),
  );
  await pickSelector("Run view", view);
  return screen.findByRole("region", { name: "The selected run" });
}

/**
 * A button in the rail. The region's own Restart carries the rail's name
 * (GRT-FR-AMHS), so a lookup across the page would find two.
 */
function rowButton(name: string) {
  return within(screen.getByRole("list", { name: "Graduation run history" })).getByRole(
    "button",
    { name },
  );
}

function restartCalls() {
  return invoked.mock.calls.filter(([c]) => c === "restart_graduation_run");
}

/** Select the discarded run, open its confirmation and confirm it. */
async function restartFromRegion(region: HTMLElement) {
  await userEvent.click(rowButton("Run r1"));
  await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
  await userEvent.click(within(region).getByTestId("graduation-restart"));
  await waitFor(() =>
    expect(within(region).getByRole("combobox", { name: "Work stream" })).toHaveValue(
      discarded.streamId,
    ),
  );
  await userEvent.click(within(region).getByRole("button", { name: "Restart run" }));
}

describe("the rail's Restart", () => {
  it("GRT-FR-AMHS: it selects the discarded run and opens the one confirmation in the run region, with focus on its first question", async () => {
    const region = await open();
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r0"));
    await userEvent.click(rowButton("Restart “Run r1”"));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
    const confirmation = await within(region).findByTestId("graduation-restart-confirm");
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(confirmation).getByRole("combobox", { name: "Work stream" }),
      ),
    );
    expect(screen.getAllByTestId("graduation-restart-confirm")).toHaveLength(1);
    expect(invoked.mock.calls.map(([c]) => c)).not.toContain("restart_graduation_run");
  });

  it("GRT-FR-SQNB: the commit message starts empty, whatever run is queued on the stream, and the restart sends none", async () => {
    const live = makeRun("r9", "queued", { input: { ...discarded.input, draftName: "Other work" } });
    queue = makeQueue([makeRun("r0", "completed"), discarded, live]);
    const stream = streamSummary(discarded.streamId);
    stream.queuedRunCount = 1;
    invoked.mockImplementation(async (command: string) => {
      switch (command) {
        case "list_graduation_queue":
          return queue;
        case "load_app_preferences":
          return { graduationRailWidthFraction: 0.2 };
        case "list_work_streams":
          return [stream];
        case "restart_graduation_run":
          return restartAnswer();
        default:
          return undefined;
      }
    });
    const region = await open();
    await userEvent.click(rowButton("Restart “Run r1”"));
    const confirmation = await within(region).findByTestId("graduation-restart-confirm");
    const field = await within(confirmation).findByRole("textbox", { name: /Commit message/ });
    expect(field).toHaveValue("");
    expect(field).toHaveAttribute("placeholder", discarded.input.draftName);
    await userEvent.click(within(confirmation).getByRole("button", { name: "Restart run" }));
    await waitFor(() => expect(restartCalls()).toHaveLength(1));
    expect(restartCalls()[0][1]).toMatchObject({ standingWorkMessage: null });
  });

  it("GRT-FR-AMHS: both controls carry the same name and tooltip", async () => {
    const region = await open();
    await userEvent.click(rowButton("Run r1"));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
    const inRegion = within(region).getByTestId("graduation-restart");
    const inRail = rowButton("Run r1")
      .closest(".graduation__row")!
      .querySelector<HTMLElement>("button[title^='Restart']")!;
    expect(inRegion.getAttribute("aria-label")).toBe(inRail.getAttribute("aria-label"));
    expect(inRegion.getAttribute("title")).toBe(inRail.getAttribute("title"));
  });

  it("GRT-FR-AMHS: a cancelled request does not open the confirmation again when the run is selected again", async () => {
    const region = await open();
    await userEvent.click(rowButton("Restart “Run r1”"));
    const confirmation = await within(region).findByTestId("graduation-restart-confirm");
    await userEvent.click(within(confirmation).getByRole("button", { name: "Cancel" }));
    await userEvent.click(rowButton("Run r0"));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r0"));
    await userEvent.click(rowButton("Run r1"));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
    expect(within(region).queryByTestId("graduation-restart-confirm")).toBeNull();
  });

  it("GRT-FR-AMHS, GRT-FR-XHLN: a restart confirmed from the rail's request selects the new run", async () => {
    const region = await open();
    await userEvent.click(rowButton("Restart “Run r1”"));
    const confirmation = await within(region).findByTestId("graduation-restart-confirm");
    await waitFor(() =>
      expect(within(confirmation).getByRole("combobox", { name: "Work stream" })).toHaveValue(
        discarded.streamId,
      ),
    );
    await userEvent.click(within(confirmation).getByRole("button", { name: "Restart run" }));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r2"));
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(region).getByRole("button", { name: "Open draft" }),
      ),
    );
  });

  it("GRT-FR-AMHS: on the run already selected, it opens the confirmation, and opens it again after a Cancel", async () => {
    const region = await open();
    await userEvent.click(rowButton("Run r1"));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
    for (let attempt = 0; attempt < 2; attempt++) {
      await userEvent.click(rowButton("Restart “Run r1”"));
      const confirmation = await within(region).findByTestId("graduation-restart-confirm");
      await waitFor(() =>
        expect(document.activeElement).toBe(
          within(confirmation).getByRole("combobox", { name: "Work stream" }),
        ),
      );
      await userEvent.click(within(confirmation).getByRole("button", { name: "Cancel" }));
      await waitFor(() =>
        expect(within(region).queryByTestId("graduation-restart-confirm")).toBeNull(),
      );
    }
    expect(restartCalls()).toHaveLength(0);
  });

  it("GRT-FR-AMHS: pressed again while its confirmation is open, it keeps what the confirmation holds and puts focus back on it", async () => {
    restartAnswer = async () => {
      throw "stream_missing";
    };
    const region = await open();
    await userEvent.click(rowButton("Restart “Run r1”"));
    const confirmation = await within(region).findByTestId("graduation-restart-confirm");
    await waitFor(() =>
      expect(within(confirmation).getByRole("combobox", { name: "Work stream" })).toHaveValue(
        discarded.streamId,
      ),
    );
    const restartRun = within(confirmation).getByRole("button", { name: "Restart run" });
    await userEvent.click(restartRun);
    await within(confirmation).findByRole("alert");
    await waitFor(() => expect(restartRun).toBeEnabled());
    await userEvent.click(rowButton("Restart “Run r1”"));
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(confirmation).getByRole("combobox", { name: "Work stream" }),
      ),
    );
    expect(within(region).getByTestId("graduation-restart-confirm")).toBe(confirmation);
    expect(within(confirmation).getByRole("alert")).toHaveTextContent(
      "No new run was created, and this run is unchanged.",
    );
    expect(restartCalls()).toHaveLength(1);
  });

  it("GRT-FR-AMHS, GRT-FR-KSBC: a refusal of a restart the rail opened renders in the region's confirmation and not in the rail", async () => {
    restartAnswer = async () => {
      throw "stream_missing";
    };
    const region = await open();
    await userEvent.click(rowButton("Restart “Run r1”"));
    const confirmation = await within(region).findByTestId("graduation-restart-confirm");
    await waitFor(() =>
      expect(within(confirmation).getByRole("combobox", { name: "Work stream" })).toHaveValue(
        discarded.streamId,
      ),
    );
    await userEvent.click(within(confirmation).getByRole("button", { name: "Restart run" }));
    const alert = await within(confirmation).findByRole("alert");
    expect(alert).toHaveTextContent("No new run was created, and this run is unchanged.");
    const rail = screen.getByRole("list", { name: "Graduation run history" });
    expect(within(rail).queryByRole("alert")).toBeNull();
    expect(screen.getAllByRole("alert")).toHaveLength(1);
  });

  it("GRT-FR-AMHS, GRU-FR-ZMHB: while a restart runs, the rail's Restart and the confirmation's acts are disabled together", async () => {
    let settle: (run: GraduationRun) => void = () => {};
    restartAnswer = () =>
      new Promise<GraduationRun>((resolve) => {
        settle = resolve;
      });
    const region = await open();
    await restartFromRegion(region);
    const confirmation = within(region).getByTestId("graduation-restart-confirm");
    await waitFor(() =>
      expect(within(confirmation).getByRole("button", { name: "Restart run" })).toBeDisabled(),
    );
    expect(within(confirmation).getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(rowButton("Restart “Run r1”")).toBeDisabled();
    await act(async () => {
      queue = afterRestart;
      settle(created);
    });
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r2"));
    expect(rowButton("Restart “Run r1”")).toBeEnabled();
    expect(
      within(rowButton("Run r2").closest(".graduation__row") as HTMLElement).queryByRole(
        "button",
        { name: /^Restart/ },
      ),
    ).toBeNull();
  });
});

describe("where Restart stands", () => {
  it.each(["queued", "working", "interrupted", "completed", "failed"] as const)(
    "GRT-FR-CTNO: a %s run's region offers no Restart",
    async (state) => {
      queue = makeQueue([makeRun("r1", state)]);
      const region = await open();
      await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
      expect(within(region).queryByTestId("graduation-restart")).toBeNull();
    },
  );

  it("GRT-FR-CTNO: Restart is the last control of a discarded run's action row, and stands directly before Archive in its rail row", async () => {
    queue = makeQueue([makeRun("r1", "discarded", { commits: ["c0ffee1"] })]);
    const region = await open();
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
    const actions = region.querySelector(".graduation__actions") as HTMLElement;
    const inRow = within(actions).getAllByRole("button");
    expect(inRow[inRow.length - 1]).toBe(within(region).getByTestId("graduation-restart"));
    const cluster = within(rowButton("Run r1").closest(".graduation__row") as HTMLElement)
      .getAllByRole("button")
      .map((button) => button.getAttribute("aria-label"));
    expect(cluster.indexOf("Restart “Run r1”")).toBeGreaterThan(0);
    expect(cluster.indexOf("Restart “Run r1”")).toBe(cluster.indexOf("Archive “Run r1”") - 1);
  });
});

describe("after an accepted restart", () => {
  it("GRT-FR-XHLN: the new run is selected with aria-current in the rail, and the discarded run stays unchanged and selectable", async () => {
    const region = await open();
    await restartFromRegion(region);
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r2"));
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(region).getByRole("button", { name: "Open draft" }),
      ),
    );
    expect(rowButton("Run r2")).toHaveAttribute("aria-current", "true");
    expect(rowButton("Run r1")).not.toHaveAttribute("aria-current");
    await userEvent.click(rowButton("Run r1"));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
    expect(within(region).getByTestId("graduation-state")).toHaveTextContent("Discarded");
  });

  it("GRT-FR-XHLN: an Archived view that hides the new run moves to All, selects it and focuses its first control", async () => {
    const archived = makeRun("r1", "discarded", { archived: true });
    queue = makeQueue([archived]);
    afterRestart = makeQueue([archived, created]);
    const region = await open("archived");
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
    await userEvent.click(within(region).getByTestId("graduation-restart"));
    await waitFor(() =>
      expect(within(region).getByRole("combobox", { name: "Work stream" })).toHaveValue(
        discarded.streamId,
      ),
    );
    await userEvent.click(within(region).getByRole("button", { name: "Restart run" }));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r2"));
    // All lists the archived discarded run beside the new one; Archived would not.
    expect(rowButton("Run r1")).toBeInTheDocument();
    expect(rowButton("Run r2")).toHaveAttribute("aria-current", "true");
    await waitFor(() =>
      expect(document.activeElement).toBe(
        within(region).getByRole("button", { name: "Open draft" }),
      ),
    );
  });

  it("GRT-FR-XHLN: a text filter that hides the new run is cleared", async () => {
    const region = await open();
    const filter = screen.getByRole("searchbox", { name: "Filter runs" });
    await userEvent.type(filter, "discarded");
    await restartFromRegion(region);
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r2"));
    expect(filter).toHaveValue("");
  });

  it("GRT-FR-XHLN: a text filter that admits the new run stays as the author typed it", async () => {
    const region = await open();
    const filter = screen.getByRole("searchbox", { name: "Filter runs" });
    await userEvent.type(filter, "Run r");
    await restartFromRegion(region);
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r2"));
    expect(filter).toHaveValue("Run r");
    expect(rowButton("Run r0")).toBeInTheDocument();
  });

  it("GRT-FR-XHLN, GRU-FR-ZMHB: a run the author selected while the restart ran gives way to the new run, and focus they moved stays", async () => {
    let settle: (run: GraduationRun) => void = () => {};
    restartAnswer = () =>
      new Promise<GraduationRun>((resolve) => {
        settle = resolve;
      });
    const region = await open();
    await restartFromRegion(region);
    await userEvent.click(rowButton("Run r0"));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r0"));
    const moved = rowButton("Run r0");
    await act(async () => {
      queue = afterRestart;
      settle(created);
    });
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r2"));
    expect(document.activeElement).toBe(moved);
  });
});

describe("a refused restart", () => {
  it("GRT-FR-KSBC, GRT-FR-XHLN: it keeps the discarded run selected, renders one alert in the confirmation that says nothing was created, calls once, moves no filter, and leaves Restart offered", async () => {
    let refuse = true;
    restartAnswer = async () => {
      if (refuse) throw "stream_missing";
      queue = afterRestart;
      return created;
    };
    const region = await open();
    const filter = screen.getByRole("searchbox", { name: "Filter runs" });
    await userEvent.type(filter, "Run r");
    await restartFromRegion(region);
    const confirmation = within(region).getByTestId("graduation-restart-confirm");
    const restartRun = within(confirmation).getByRole("button", { name: "Restart run" });
    await within(confirmation).findByRole("alert");
    await waitFor(() => expect(restartRun).toBeEnabled());
    const alerts = screen.getAllByRole("alert");
    expect(alerts).toHaveLength(1);
    expect(confirmation).toContainElement(alerts[0]);
    expect(alerts[0]).toHaveTextContent(graduationErrorMessage("stream_missing"));
    expect(alerts[0]).toHaveTextContent("No new run was created, and this run is unchanged.");
    expect(restartCalls()).toHaveLength(1);
    expect(region).toHaveAttribute("data-run", "r1");
    expect(within(screen.getByRole("list", { name: "Graduation run history" })).queryByRole("button", { name: "Run r2" })).toBeNull();
    expect(filter).toHaveValue("Run r");
    // GRT-FR-KSBC: the surface retries nothing by itself, and the author can.
    refuse = false;
    await userEvent.click(restartRun);
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r2"));
    expect(restartCalls()).toHaveLength(2);
  });

  it("GRT-FR-KSBC, GRU-FR-KQPE: a refusal that lands after the author selected another run renders nowhere and moves nothing", async () => {
    let refuse: (reason: unknown) => void = () => {};
    restartAnswer = () =>
      new Promise((_, reject) => {
        refuse = reject;
      });
    const region = await open();
    const filter = screen.getByRole("searchbox", { name: "Filter runs" });
    await userEvent.type(filter, "Run r");
    await restartFromRegion(region);
    await userEvent.click(rowButton("Run r0"));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r0"));
    await act(async () => {
      refuse("stream_missing");
    });
    await waitFor(() => expect(rowButton("Archive “Run r0”")).toBeEnabled());
    expect(screen.queryAllByRole("alert")).toHaveLength(0);
    expect(region).toHaveAttribute("data-run", "r0");
    expect(filter).toHaveValue("Run r");
    await userEvent.click(rowButton("Run r1"));
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r1"));
    expect(within(region).getByTestId("graduation-restart")).toBeEnabled();
    expect(within(region).queryByTestId("graduation-restart-confirm")).toBeNull();
    expect(screen.queryAllByRole("alert")).toHaveLength(0);
  });
});

describe("what a restart leaves as it was", () => {
  it("GRT-FR-XHLN: the rail keeps its width and its scrolling list, and no preference is written", async () => {
    const region = await open();
    // The list the rail scrolls is the same element afterwards, so its scroll
    // position is not reset by a new one taking its place.
    const scroller = document.querySelector(".graduation__rail");
    const width = screen
      .getByRole("separator", { name: "History width" })
      .getAttribute("aria-valuenow");
    await restartFromRegion(region);
    await waitFor(() => expect(region).toHaveAttribute("data-run", "r2"));
    expect(document.querySelector(".graduation__rail")).toBe(scroller);
    expect(screen.getByRole("separator", { name: "History width" })).toHaveAttribute(
      "aria-valuenow",
      width ?? "",
    );
    expect(invoked.mock.calls.map(([c]) => c)).not.toContain("patch_app_preferences");
  });
});
