/**
 * What the graduation section's rail remembers, and what it reveals
 * (`../../../specifications/ui/GRH-graduation-history.md` GRH-FR-ODLT,
 * GRH-FR-QVEX, GRH-FR-XBWU, GRH-FR-WDSH).
 *
 * Split from `index.test.tsx` so neither file passes the thousand lines this
 * project holds a source file to.
 */
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

describe("what the rail remembers", () => {
  const three = () => [
    makeRun("r1", "working"),
    makeRun("r2", "queued"),
    makeRun("r3", "completed"),
  ];

  it("GRH-FR-WDSH: a selection the listing no longer holds falls back to the first row", async () => {
    queue = makeQueue(three());
    await openAll();
    await userEvent.click(await screen.findByRole("button", { name: "Run r3" }));
    queue = makeQueue([makeRun("r1", "working"), makeRun("r2", "queued")]);
    await fire("graduation-queue-changed");
    await waitFor(() =>
      expect(
        document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
      ).toBe("Run r1"),
    );
  });

  it("GRH-FR-WDSH: a view that admits nothing selects nothing", async () => {
    queue = makeQueue(three());
    await open();
    await userEvent.type(
      screen.getByRole("searchbox", { name: "Filter runs" }),
      "zzz",
    );
    await waitFor(() =>
      expect(document.querySelector('[aria-current="true"]')).toBeNull(),
    );
    expect(screen.queryByTestId("graduation-provenance")).toBeNull();
  });

  it("GRH-FR-ODLT: the selection and the narrowing are remembered per project", async () => {
    queue = makeQueue(three());
    await open();
    await userEvent.click(await screen.findByRole("button", { name: "Run r2" }));
    await userEvent.type(screen.getByRole("searchbox", { name: "Filter runs" }), "Run");
    cleanup();
    await open();
    await waitFor(() =>
      expect(
        document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
      ).toBe("Run r2"),
    );
    expect(screen.getByRole("searchbox", { name: "Filter runs" })).toHaveValue("Run");
  });

  it("GRH-FR-ODLT / GRH-FR-QVEX: one project's selection and narrowing are never read for another", async () => {
    queue = makeQueue(three());
    await openAll();
    await userEvent.click(await screen.findByRole("button", { name: "Run r3" }));
    cleanup();
    queue = makeQueue(three(), "/Users/demo/dev/other");
    await open();
    // GRH-FR-QVEX: the second project holds no filter state of its own, so it
    // opens on the resting position rather than on the first project's All.
    await waitFor(() => expect(selectorValue("Run view")).toBe("in-flight"));
    // GRH-FR-WDSH: and it selects the first row that position admits, rather
    // than the run the other project was left reading. Asserted from All,
    // which lists r3 as well: in In-flight alone, a leaked r3 and a correct r1
    // would both read as r1.
    await pickSelector("Run view", "all");
    await waitFor(() =>
      expect(
        document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
      ).toBe("Run r1"),
    );
  });

  it("NTF-FR-39: which run is selected is reported upwards", async () => {
    const onSelectionChange = vi.fn();
    queue = makeQueue(three());
    await open({ onSelectionChange });
    await waitFor(() => expect(onSelectionChange).toHaveBeenCalledWith("r1"));
    await userEvent.click(screen.getByRole("button", { name: "Run r2" }));
    expect(onSelectionChange).toHaveBeenCalledWith("r2");
  });

  it("GRH-FR-ODLT: a route that names no preference takes the selection, and one that does moves no reader", async () => {
    queue = makeQueue(three());
    const { rerender } = render(
      <GraduationRuns selectRun={{ runId: "r3" }} />,
    );
    await waitFor(() =>
      expect(
        document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
      ).toBe("Run r3"),
    );
    rerender(<GraduationRuns selectRun={{ runId: "r2", override: false }} />);
    expect(
      document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
    ).toBe("Run r3");
  });

  it("GRH-FR-ODLT: a route that must not move a reader does not move the one they were left on", async () => {
    queue = makeQueue(three());
    await open();
    // The author reads r3, then looks at the agent output, which unmounts the
    // section. A run started from there asks to be shown without the override.
    await pickSelector("Run view", "all");
    await userEvent.click(await screen.findByRole("button", { name: "Run r3" }));
    cleanup();
    queue = makeQueue([...three(), makeRun("r4", "queued")]);
    render(<GraduationRuns selectRun={{ runId: "r4", override: false }} />);
    await waitFor(() =>
      expect(screen.getAllByTestId("graduation-row")).toHaveLength(4),
    );
    expect(
      document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
    ).toBe("Run r3");
  });

  it("GRH-FR-XBWU: a run another surface names is revealed, and only the filters that hide it move", async () => {
    queue = makeQueue(three());
    render(<GraduationRuns selectRun={{ runId: "r3" }} />);
    // r3 completed, so the resting position hides it. It moves to All, and the
    // empty text filter admits the run already and is left alone.
    await waitFor(() => expect(selectorValue("Run view")).toBe("all"));
    expect(
      document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
    ).toBe("Run r3");
    expect(screen.getByRole("searchbox", { name: "Filter runs" })).toHaveValue("");
  });

  it("GRH-FR-XBWU: a named run the author filed away is revealed in Archived", async () => {
    queue = makeQueue([
      makeRun("r1", "working"),
      makeRun("r2", "completed", { archived: true }),
    ]);
    render(<GraduationRuns selectRun={{ runId: "r2" }} />);
    await waitFor(() => expect(selectorValue("Run view")).toBe("archived"));
    expect(
      document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
    ).toBe("Run r2");
  });

  it("GRH-FR-XBWU: a text filter that hides the named run is cleared, and one that admits it stands", async () => {
    queue = makeQueue(three());
    await open();
    const field = screen.getByRole("searchbox", { name: "Filter runs" });
    await userEvent.type(field, "zzz");
    cleanup();
    // The section comes back on a run the shell names. The text the author
    // left excludes it, so it goes; the position that hides it moves too.
    render(<GraduationRuns selectRun={{ runId: "r3" }} />);
    await waitFor(() => expect(selectorValue("Run view")).toBe("all"));
    expect(screen.getByRole("searchbox", { name: "Filter runs" })).toHaveValue("");

    cleanup();
    await open();
    await userEvent.type(
      screen.getByRole("searchbox", { name: "Filter runs" }),
      "Run",
    );
    cleanup();
    render(<GraduationRuns selectRun={{ runId: "r3" }} />);
    await waitFor(() => expect(selectorValue("Run view")).toBe("all"));
    // "Run" matches every draft name, so the author's own text stays where
    // they typed it.
    expect(screen.getByRole("searchbox", { name: "Filter runs" })).toHaveValue(
      "Run",
    );
  });

  it("GRH-FR-XBWU: a restored position that already admits the remembered run is left alone", async () => {
    queue = makeQueue(three());
    await open();
    await pickSelector("Run view", "completed");
    await userEvent.click(await screen.findByRole("button", { name: "Run r3" }));
    cleanup();
    await open();
    // The project is remembered in Completed reading r3, and Completed admits
    // r3. The reveal relaxes the filters that hide the run and no others, so
    // the author comes back to the position they set rather than to All.
    await waitFor(() => expect(selectorValue("Run view")).toBe("completed"));
    expect(
      document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
    ).toBe("Run r3");
  });

  it("GRH-FR-XBWU: the run this project was left reading is revealed by a restored position that hides it", async () => {
    queue = makeQueue(three());
    await openAll();
    await userEvent.click(await screen.findByRole("button", { name: "Run r3" }));
    // The author then narrows to Archived, which lists none of these runs. The
    // selection falls away, but the run they were reading stays remembered.
    await pickSelector("Run view", "archived");
    await waitFor(() =>
      expect(document.querySelector('[aria-current="true"]')).toBeNull(),
    );
    cleanup();

    // On the way back in, the remembered position hides the remembered run, so
    // the reveal moves it — to All, r3 being no run the author filed away.
    await open();
    await waitFor(() => expect(selectorValue("Run view")).toBe("all"));
    expect(
      document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
    ).toBe("Run r3");
  });

  it("GRH-FR-WDSH: narrowing to a position that hides the selected run relaxes nothing", async () => {
    queue = makeQueue(three());
    await openAll();
    await userEvent.click(await screen.findByRole("button", { name: "Run r3" }));
    // The author's own act hides the run they are reading. The rail follows
    // them and falls back within the position they chose, rather than relaxing
    // itself back to the one that showed r3.
    await pickSelector("Run view", "in-flight");
    await waitFor(() =>
      expect(
        document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
      ).toBe("Run r1"),
    );
    expect(selectorValue("Run view")).toBe("in-flight");
  });

  it("GRH-FR-XBWU: a route and a remembered run in one mount relax from what the restore put back", async () => {
    // The project is left reading r3 in All with a text filter that admits
    // every run. A route then names r4, which that position already admits.
    queue = makeQueue([...three(), makeRun("r4", "failed")]);
    await openAll();
    await userEvent.type(
      screen.getByRole("searchbox", { name: "Filter runs" }),
      "Run",
    );
    await userEvent.click(await screen.findByRole("button", { name: "Run r3" }));
    cleanup();

    render(<GraduationRuns selectRun={{ runId: "r4", nonce: 1 }} />);
    await waitFor(() =>
      expect(
        document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
      ).toBe("Run r4"),
    );
    // The route relaxes what the restore put back rather than what the section
    // mounted with: All and "Run" both admit r4, so neither moves.
    expect(selectorValue("Run view")).toBe("all");
    expect(screen.getByRole("searchbox", { name: "Filter runs" })).toHaveValue(
      "Run",
    );
  });

  it("GRH-FR-XBWU: a filed-away run the text also hides moves both controls at once", async () => {
    queue = makeQueue([
      makeRun("r1", "working"),
      makeRun("r2", "completed", { archived: true }),
    ]);
    await open();
    await userEvent.type(
      screen.getByRole("searchbox", { name: "Filter runs" }),
      "zzz",
    );
    cleanup();
    render(<GraduationRuns selectRun={{ runId: "r2", nonce: 3 }} />);
    // The position moves to Archived rather than to All, r2 being filed away,
    // and the text that excluded it goes.
    await waitFor(() => expect(selectorValue("Run view")).toBe("archived"));
    expect(screen.getByRole("searchbox", { name: "Filter runs" })).toHaveValue("");
    expect(
      document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
    ).toBe("Run r2");
  });

  it("GRH-FR-XBWU: a route naming a run the listing does not hold reveals nothing", async () => {
    queue = makeQueue(three());
    render(<GraduationRuns selectRun={{ runId: "gone", nonce: 2 }} />);
    await screen.findByTestId("graduation-section");
    // Nothing is relaxed for a run that is not there, and GRH-FR-WDSH takes
    // the selection within the position in force.
    await waitFor(() =>
      expect(
        document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
      ).toBe("Run r1"),
    );
    expect(selectorValue("Run view")).toBe("in-flight");
  });

  it("GRH-FR-XBWU: a route the section has answered is not answered again when it comes back", async () => {
    queue = makeQueue(three());
    const route = { runId: "r3", nonce: 7 } as const;
    render(<GraduationRuns selectRun={route} />);
    await waitFor(() => expect(selectorValue("Run view")).toBe("all"));
    // The author moves on: back to In-flight, reading r2.
    await pickSelector("Run view", "in-flight");
    await userEvent.click(screen.getByRole("button", { name: "Run r2" }));
    // They look at the agent output, which unmounts the section, and come back.
    // The shell is still holding the same request; it has been answered.
    cleanup();
    render(<GraduationRuns selectRun={route} />);
    await waitFor(() => expect(selectorValue("Run view")).toBe("in-flight"));
    expect(
      document.querySelector('[aria-current="true"]')?.getAttribute("aria-label"),
    ).toBe("Run r2");
  });

  it("GRH-FR-OFHS: the selected row is announced when it changes", async () => {
    queue = makeQueue(three());
    await open();
    await waitFor(() =>
      expect(screen.getByTestId("graduation-row-announcement")).toHaveTextContent(
        "Showing “Run r1”.",
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "Run r2" }));
    expect(screen.getByTestId("graduation-row-announcement")).toHaveTextContent(
      "Showing “Run r2”.",
    );
  });
});

