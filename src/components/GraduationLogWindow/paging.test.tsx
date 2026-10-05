import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import {
  makeLogPage,
  makeLogs,
  makeObservability,
  makeRun,
  makeSourceEntry,
  pageForRead,
} from "../../test/graduationFixtures";
import type { ProgressStage } from "../../state/runProgress";
import type { GraduationLogPage, GraduationRun } from "../../types";
import { ABSENT_FIELD } from "./records";
import { PAGE_LIMIT } from "./useLogScope";
import { GraduationLogWindow } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

/** The arguments of every `read_graduation_logs` this test provoked. */
function reads(): Array<Record<string, unknown>> {
  return invoked.mock.calls
    .filter(([command]) => command === "read_graduation_logs")
    .map(([, args]) => args as Record<string, unknown>);
}

/** What the backend answers, keyed by the scope kind and the stream. */
let answer: (args: Record<string, unknown>) => GraduationLogPage | Promise<GraduationLogPage>;

beforeEach(() => {
  invoked.mockReset();
  listened.mockReset();
  listened.mockImplementation(async () => () => {});
  answer = () => makeLogPage();
  invoked.mockImplementation(async (command: string, args) => {
    if (command === "read_graduation_logs") {
      const request = (args ?? {}) as Record<string, unknown>;
      return pageForRead(request, await answer(request));
    }
    return undefined;
  });
});

afterEach(cleanup);

const WORKING: ProgressStage = { id: "working", label: "Working" };

/** A run whose `working` stage ran two passes and also wrote run-level output. */
function run(over: Parameters<typeof makeRun>[2] = {}): GraduationRun {
  return makeRun("r1", "working", {
    observability: makeObservability({
      stageHistory: [
        { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
        { from: "review", to: "working", pass: 2, at: "2", reason: "review_revision" },
      ],
    }),
    logs: makeLogs({
      source: [
        { phaseId: "working", pass: 1 },
        { phaseId: "working", pass: 2 },
        { phaseId: "working", pass: null },
      ],
      structured: [{ phaseId: "working", pass: 1 }],
    }),
    ...over,
  });
}

function draw(
  props: Partial<Parameters<typeof GraduationLogWindow>[0]> = {},
) {
  const onClose = vi.fn();
  render(
    <GraduationLogWindow
      run={props.run ?? run()}
      stage={props.stage ?? WORKING}
      onClose={props.onClose ?? onClose}
      returnFocus={props.returnFocus ?? null}
    />,
  );
  return { onClose: props.onClose ?? onClose };
}

describe("the graduation log window", () => {
  it("GLW-FR-QDWA, GLW-FR-RQEV: scrolling toward the leading edge asks for the page before", async () => {
    const older = {
      runId: "r1",
      stream: "source" as const,
      direction: "before" as const,
      sequence: 10,
    };
    answer = (args) =>
      args.cursor
        ? makeLogPage({
            status: "available",
            entries: [makeSourceEntry(9, "older")],
            oldestReached: true,
            olderCursor: null,
          })
        : makeLogPage({
            status: "available",
            entries: [makeSourceEntry(10, "newer")],
            oldestReached: false,
            olderCursor: older,
          });
    draw();
    await screen.findByText("newer");
    const viewport = screen.getByTestId("glw-viewport");
    act(() => {
      viewport.dispatchEvent(new Event("scroll", { bubbles: true }));
    });
    await waitFor(() => expect(screen.getByText("older")).toBeInTheDocument());
    // GLW-FR-RQEV: merged in ascending order and never duplicated.
    const rows = screen.getAllByTestId("glw-row");
    expect(rows.map((row) => row.querySelector(".glw__text")?.textContent)).toEqual([
      "older",
      "newer",
    ]);
    const cursored = reads().filter((read) => read.cursor);
    expect(cursored).toHaveLength(1);
    expect(cursored[0].cursor).toEqual(older);
  });

  it("GLW-FR-TCQP, GLW-FR-UZAB: an older page keeps the reader in place and is its own indication", async () => {
    let releaseOlder: ((page: GraduationLogPage) => void) | null = null;
    answer = (args) =>
      args.cursor
        ? new Promise<GraduationLogPage>((resolve) => {
            releaseOlder = resolve;
          })
        : makeLogPage({
            status: "available",
            entries: [makeSourceEntry(10, "newer")],
            oldestReached: false,
            olderCursor: {
              runId: "r1",
              stream: "source",
              direction: "before",
              sequence: 10,
            },
          });
    draw();
    await screen.findByText("newer");
    const viewport = screen.getByTestId("glw-viewport");
    let height = 200;
    Object.defineProperty(viewport, "scrollHeight", {
      get: () => height,
      configurable: true,
    });
    Object.defineProperty(viewport, "clientHeight", { value: 100, configurable: true });
    viewport.scrollTop = 20;
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));

    // GLW-FR-UZAB: its own indication, and none of the five states — what is
    // already read stays on screen and readable throughout.
    await waitFor(() =>
      expect(screen.getByTestId("glw-loading-older")).toBeInTheDocument(),
    );
    expect(screen.getByText("newer")).toBeInTheDocument();
    expect(screen.queryByTestId("glw-loading")).not.toBeInTheDocument();
    expect(screen.queryByTestId("glw-empty")).not.toBeInTheDocument();

    // GLW-FR-TCQP: the rows that arrive go above what the reader is looking at,
    // and the scroll offset moves by exactly the height they added.
    height = 320;
    await act(async () => {
      releaseOlder?.(
        makeLogPage({
          status: "available",
          entries: [makeSourceEntry(9, "older")],
          oldestReached: true,
          olderCursor: null,
        }),
      );
    });
    await waitFor(() => expect(screen.getByText("older")).toBeInTheDocument());
    expect(viewport.scrollTop).toBe(140);
  });

  it("GLW-FR-UZAB: an older page that fails leaves what is held on screen and offers to ask again", async () => {
    let fail = true;
    answer = (args) => {
      if (args.cursor && fail) throw new Error("log_stream_unreadable");
      if (args.cursor) {
        return makeLogPage({
          status: "available",
          entries: [makeSourceEntry(9, "older")],
          oldestReached: true,
        });
      }
      return makeLogPage({
        status: "available",
        entries: [makeSourceEntry(10, "newer")],
        oldestReached: false,
        olderCursor: {
          runId: "r1",
          stream: "source",
          direction: "before",
          sequence: 10,
        },
      });
    };
    draw();
    await screen.findByText("newer");
    const viewport = screen.getByTestId("glw-viewport");
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    const failed = await screen.findByTestId("glw-older-failed");
    expect(screen.getByText("newer")).toBeInTheDocument();
    fail = false;
    await userEvent.click(within(failed).getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.getByText("older")).toBeInTheDocument());
  });

  it("GLW-FR-QDWA: it stops asking once a page reports it holds the oldest record", async () => {
    answer = () =>
      makeLogPage({
        status: "available",
        entries: [makeSourceEntry(1, "only")],
        oldestReached: true,
        olderCursor: null,
      });
    draw();
    await screen.findByText("only");
    const viewport = screen.getByTestId("glw-viewport");
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    expect(reads().filter((read) => read.cursor)).toHaveLength(0);
  });

  it("GLW-FR-LUQI, GLW-FR-KKUM: an append is read back under the window's own cursor", async () => {
    let appended: ((payload: unknown) => void) | null = null;
    listened.mockImplementation(async (name, handler) => {
      if (name === "graduation-log-records-appended") {
        appended = (payload: unknown) =>
          (handler as unknown as (event: { payload: unknown }) => void)({ payload });
      }
      return () => {};
    });
    answer = (args) =>
      args.cursor
        ? makeLogPage({
            status: "available",
            entries: [makeSourceEntry(11, "arrived")],
          })
        : makeLogPage({ status: "available", entries: [makeSourceEntry(10, "held")] });
    draw();
    await screen.findByText("held");
    await act(async () => {
      appended?.({ runId: "r1", stream: "source", latestSequence: 11 });
    });
    await waitFor(() => expect(screen.getByText("arrived")).toBeInTheDocument());
    const after = reads().find((read) => read.cursor);
    expect(after?.cursor).toMatchObject({ direction: "after", sequence: 10 });
  });

  it("GLW-FR-HCOI: an append of the other stream is ignored", async () => {
    let appended: ((payload: unknown) => void) | null = null;
    listened.mockImplementation(async (name, handler) => {
      if (name === "graduation-log-records-appended") {
        appended = (payload: unknown) =>
          (handler as unknown as (event: { payload: unknown }) => void)({ payload });
      }
      return () => {};
    });
    answer = () => makeLogPage({ status: "available", entries: [makeSourceEntry(1, "held")] });
    draw();
    await screen.findByText("held");
    const before = reads().length;
    await act(async () => {
      appended?.({ runId: "r1", stream: "structured", latestSequence: 12 });
    });
    expect(reads()).toHaveLength(before);
  });

  it("GLW-FR-KTWX: scrolling away releases follow, and only the labelled control resumes it", async () => {
    answer = () => makeLogPage({ status: "available", entries: [makeSourceEntry(1, "line")] });
    draw();
    await screen.findByText("line");
    const viewport = screen.getByTestId("glw-viewport");
    expect(viewport).toHaveAttribute("data-following", "true");
    expect(screen.getByTestId("glw-follow")).toHaveTextContent("Following");

    // jsdom reports a zero-height viewport, so a scrollTop of its own is what
    // stands for "away from the end".
    Object.defineProperty(viewport, "scrollHeight", { value: 500, configurable: true });
    Object.defineProperty(viewport, "clientHeight", { value: 100, configurable: true });
    viewport.scrollTop = 100;
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    expect(viewport).toHaveAttribute("data-following", "false");
    const resume = screen.getByTestId("glw-follow");
    expect(resume).toHaveTextContent("Resume following");

    // Scrolling back to the end does not resume it.
    viewport.scrollTop = 400;
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    expect(viewport).toHaveAttribute("data-following", "false");

    await userEvent.click(resume);
    expect(viewport).toHaveAttribute("data-following", "true");
  });

  it("GLW-FR-KVXI: a record that arrives while follow is off does not move the reader", async () => {
    let appended: ((payload: unknown) => void) | null = null;
    listened.mockImplementation(async (name, handler) => {
      if (name === "graduation-log-records-appended") {
        appended = (payload: unknown) =>
          (handler as unknown as (event: { payload: unknown }) => void)({ payload });
      }
      return () => {};
    });
    answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeSourceEntry(11, "arrived")] })
        : makeLogPage({ status: "available", entries: [makeSourceEntry(10, "held")] });
    draw();
    await screen.findByText("held");
    const viewport = screen.getByTestId("glw-viewport");
    Object.defineProperty(viewport, "scrollHeight", { value: 500, configurable: true });
    Object.defineProperty(viewport, "clientHeight", { value: 100, configurable: true });
    viewport.scrollTop = 120;
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    expect(viewport).toHaveAttribute("data-following", "false");

    await act(async () => {
      appended?.({ runId: "r1", stream: "source", latestSequence: 11 });
    });
    // The record is read in and reachable by scrolling, and the reader's
    // position is where they left it.
    await waitFor(() => expect(screen.getByText("arrived")).toBeInTheDocument());
    expect(viewport.scrollTop).toBe(120);
    expect(viewport).toHaveAttribute("data-following", "false");
  });

  it("GLW-FR-SLEK: it is built from the application's own overlay primitives and brings no treatment of its own", async () => {
    draw();
    const overlay = await screen.findByTestId("graduation-log-window");
    // The same scrim, modal shell, head and button primitives every other
    // overlay of this window uses, rather than a shell of its own.
    expect(overlay.parentElement?.className).toContain("scrim");
    expect(overlay.className.split(" ")).toContain("modal");
    expect(overlay.querySelector(".modal__head")).not.toBeNull();
    expect(overlay.querySelector(".modal__title")).not.toBeNull();
    for (const button of within(overlay).getAllByRole("button")) {
      expect(button.className).toMatch(/\b(btn|glw__pass)\b/);
    }
    // No colour, font, or spacing written into the markup: every treatment is
    // the stylesheet's, so both themes come free.
    for (const node of overlay.querySelectorAll<HTMLElement>("[style]")) {
      expect(node.getAttribute("style")).toBe("");
    }
  });

  it("GLW-FR-NFLN: an answer for a scope the reader has left is discarded", async () => {
    let releaseFirst: ((page: GraduationLogPage) => void) | null = null;
    answer = (args) => {
      const scope = args.pass as { kind: string; pass?: number };
      if (scope.kind === "pass" && scope.pass === 2) {
        return new Promise<GraduationLogPage>((resolve) => {
          releaseFirst = resolve;
        });
      }
      return makeLogPage({
        status: "available",
        entries: [makeSourceEntry(2, "pass one output")],
      });
    };
    draw();
    await waitFor(() => expect(reads()).toHaveLength(1));
    await userEvent.click(screen.getByRole("button", { name: "Pass 1" }));
    await screen.findByText("pass one output");
    // The read of the scope the reader left now answers, late.
    await act(async () => {
      releaseFirst?.(
        makeLogPage({
          status: "available",
          entries: [makeSourceEntry(9, "stale pass two output")],
        }),
      );
    });
    expect(screen.queryByText("stale pass two output")).not.toBeInTheDocument();
    expect(screen.getByText("pass one output")).toBeInTheDocument();
  });

  it("GLW-FR-NFLN: a stale older page is discarded and does not release the new scope's guard", async () => {
    const releases: Array<(page: GraduationLogPage) => void> = [];
    answer = (args) => {
      if (args.cursor) {
        return new Promise<GraduationLogPage>((resolve) => releases.push(resolve));
      }
      const scope = args.pass as { kind: string; pass?: number };
      return makeLogPage({
        status: "available",
        entries: [makeSourceEntry(scope.pass === 1 ? 20 : 10, `page of ${scope.pass}`)],
        oldestReached: false,
        olderCursor: {
          runId: "r1",
          stream: "source",
          direction: "before",
          sequence: scope.pass === 1 ? 20 : 10,
        },
      });
    };
    draw();
    await screen.findByText("page of 2");
    const viewport = screen.getByTestId("glw-viewport");
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    await waitFor(() => expect(releases).toHaveLength(1));

    // The reader leaves that scope while the older page is still in flight.
    await userEvent.click(screen.getByRole("button", { name: "Pass 1" }));
    await screen.findByText("page of 1");
    await act(async () => {
      releases[0](
        makeLogPage({
          status: "available",
          entries: [makeSourceEntry(9, "stale older page")],
          oldestReached: true,
        }),
      );
    });
    expect(screen.queryByText("stale older page")).not.toBeInTheDocument();

    // And the scope the reader is on now can still ask for its own older page.
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    await waitFor(() => expect(releases).toHaveLength(2));
    await act(async () => {
      releases[1](
        makeLogPage({
          status: "available",
          entries: [makeSourceEntry(19, "its own older page")],
          oldestReached: true,
        }),
      );
    });
    await waitFor(() =>
      expect(screen.getByText("its own older page")).toBeInTheDocument(),
    );
  });

  it("GLW-FR-QMRV, GLW-FR-QNHH: the list is operable from the keyboard, and moving selects", async () => {
    draw();
    const list = await screen.findByTestId("glw-pass-list");
    // The window opened on pass 2, which is the entry the reader stands on.
    const opened = within(list).getByRole("button", { name: "Pass 2" });
    opened.focus();
    await waitFor(() => expect(reads()).toHaveLength(1));

    await userEvent.keyboard("{ArrowUp}");
    await waitFor(() => expect(reads()).toHaveLength(2));
    expect(reads()[1].pass).toEqual({ kind: "pass", pass: 1 });
    expect(within(list).getByRole("button", { name: "Pass 1" })).toHaveFocus();

    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    await waitFor(() => expect(reads()).toHaveLength(4));
    expect(reads()[3].pass).toEqual({ kind: "run_level" });
  });

  it("GLW-FR-OOYK, GLW-FR-ONEV: Escape closes it and focus returns to the opener", async () => {
    const opener = document.createElement("button");
    opener.textContent = "Working";
    document.body.appendChild(opener);
    const onClose = vi.fn();
    const { unmount } = render(
      <GraduationLogWindow
        run={run()}
        stage={WORKING}
        onClose={onClose}
        returnFocus={opener}
      />,
    );
    await waitFor(() =>
      expect(screen.getByTestId("graduation-log-window").contains(document.activeElement)).toBe(
        true,
      ),
    );
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
    unmount();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });

  it("GLW-FR-ZPUH: every read the window makes is bounded to one page", async () => {
    answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeSourceEntry(9, "older")] })
        : makeLogPage({
            status: "available",
            entries: [makeSourceEntry(10, "newer")],
            oldestReached: false,
            olderCursor: {
              runId: "r1",
              stream: "source",
              direction: "before",
              sequence: 10,
            },
          });
    draw();
    await screen.findByText("newer");
    act(() =>
      screen
        .getByTestId("glw-viewport")
        .dispatchEvent(new Event("scroll", { bubbles: true })),
    );
    await waitFor(() => expect(reads().length).toBeGreaterThan(1));
    for (const read of reads()) {
      expect(typeof read.limit).toBe("number");
      expect(read.limit as number).toBeGreaterThan(0);
      expect(read.limit).toBe(PAGE_LIMIT);
    }
  });

  it("GLW-FR-KWHL: changing the scope or the stream releases follow to its opening position", async () => {
    answer = () => makeLogPage({ status: "available", entries: [makeSourceEntry(1, "line")] });
    draw();
    await screen.findByText("line");
    const viewport = screen.getByTestId("glw-viewport");
    Object.defineProperty(viewport, "scrollHeight", { value: 500, configurable: true });
    Object.defineProperty(viewport, "clientHeight", { value: 100, configurable: true });
    viewport.scrollTop = 50;
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    expect(viewport).toHaveAttribute("data-following", "false");

    // A new scope opens at its end, following, exactly as the window itself did.
    await userEvent.click(screen.getByRole("button", { name: "Pass 1" }));
    await waitFor(() =>
      expect(screen.getByTestId("glw-viewport")).toHaveAttribute(
        "data-following",
        "true",
      ),
    );

    viewport.scrollTop = 50;
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    expect(screen.getByTestId("glw-viewport")).toHaveAttribute(
      "data-following",
      "false",
    );
    await userEvent.click(screen.getByRole("button", { name: "Structured" }));
    await waitFor(() =>
      expect(screen.getByTestId("glw-viewport")).toHaveAttribute(
        "data-following",
        "true",
      ),
    );
  });

  it("GLW-FR-RQEV: pages that overlap hold no record twice and stay in sequence order", async () => {
    answer = (args) =>
      args.cursor
        ? makeLogPage({
            status: "available",
            // The older page overlaps the newest one by a record.
            entries: [makeSourceEntry(8, "eight"), makeSourceEntry(9, "nine")],
            oldestReached: true,
          })
        : makeLogPage({
            status: "available",
            entries: [makeSourceEntry(9, "nine"), makeSourceEntry(10, "ten")],
            oldestReached: false,
            olderCursor: {
              runId: "r1",
              stream: "source",
              direction: "before",
              sequence: 9,
            },
          });
    draw();
    await screen.findByText("ten");
    act(() =>
      screen
        .getByTestId("glw-viewport")
        .dispatchEvent(new Event("scroll", { bubbles: true })),
    );
    await waitFor(() => expect(screen.getByText("eight")).toBeInTheDocument());
    const rows = screen
      .getAllByTestId("glw-row")
      .map((row) => row.querySelector(".glw__text")?.textContent);
    expect(rows).toEqual(["eight", "nine", "ten"]);
  });

  it("GLW-FR-LUQI, GLW-FR-GZWN: an append that lands before the first page is read after it rather than lost", async () => {
    let appended: ((payload: unknown) => void) | null = null;
    listened.mockImplementation(async (name, handler) => {
      if (name === "graduation-log-records-appended") {
        appended = (payload: unknown) =>
          (handler as unknown as (event: { payload: unknown }) => void)({ payload });
      }
      return () => {};
    });
    let releaseFirst: ((page: GraduationLogPage) => void) | null = null;
    answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeSourceEntry(11, "arrived")] })
        : new Promise<GraduationLogPage>((resolve) => {
            releaseFirst = resolve;
          });
    draw();
    await waitFor(() => expect(reads()).toHaveLength(1));
    await act(async () => {
      appended?.({ runId: "r1", stream: "source", latestSequence: 11 });
    });
    // GLW-FR-GZWN: the append waits for the newest page rather than reading
    // after a sequence the window does not yet hold.
    expect(reads()).toHaveLength(1);
    await act(async () => {
      releaseFirst?.(
        makeLogPage({ status: "available", entries: [makeSourceEntry(10, "held")] }),
      );
    });
    await waitFor(() => expect(screen.getByText("arrived")).toBeInTheDocument());
    expect(screen.getByText("held")).toBeInTheDocument();
    expect(reads()[1].cursor).toMatchObject({ direction: "after", sequence: 10 });
  });

  it("GLW-FR-FXAL: a field the record holds as null renders as absent rather than invented", async () => {
    answer = () =>
      makeLogPage({
        status: "available",
        entries: [
          makeSourceEntry(3, "line", { agent: null, container: null, source: "stderr" }),
        ],
      });
    draw();
    const row = (await screen.findAllByTestId("glw-row"))[0];
    const meta = row.querySelector(".glw__meta")?.textContent ?? "";
    // The ten fields GLW-FR-FXAL names, in the order it names them.
    expect(meta).toContain("#3");
    expect(meta).toContain("stderr");
    expect(meta).toContain("executor");
    expect(meta).toContain("work_turn");
    expect(meta).toContain("working");
    expect(meta).toContain("r1");
    // `agent` and `container` do not apply, so they read as absent.
    expect(meta.split(" · ").filter((field) => field === ABSENT_FIELD)).toHaveLength(2);
    expect(meta).not.toContain("null");
    expect(meta).not.toContain("undefined");
  });

  it("GLW-FR-HPWG: output that is not text still renders as lines", async () => {
    answer = () =>
      makeLogPage({
        status: "available",
        entries: [
          // Base64 of `before <0xFF> after`, which is no UTF-8 sequence.
          makeSourceEntry(1, "", { data_base64: "YmVmb3Jl/2FmdGVy" }),
          // And a chunk whose base64 does not decode at all.
          makeSourceEntry(2, "", { data_base64: "!!!not base64!!!" }),
        ],
      });
    draw();
    const rows = await screen.findAllByTestId("glw-row");
    expect(rows).toHaveLength(2);
    expect(rows[0].querySelector(".glw__text")?.textContent).toContain("before");
    expect(rows[0].querySelector(".glw__text")?.textContent).toContain("\uFFFD");
    // The undecodable chunk is still a row, with its metadata, rather than gone.
    expect(rows[1].querySelector(".glw__meta")?.textContent).toContain("#2");
  });

  it("GLW-FR-ONEV, GLW-FR-QMRV: focus stays inside, and every control is keyboard-operable", async () => {
    draw();
    const overlay = await screen.findByTestId("graduation-log-window");
    const controls = [
      screen.getByRole("button", { name: "Close" }),
      screen.getByRole("button", { name: "Source" }),
      screen.getByRole("button", { name: "Structured" }),
      screen.getByTestId("glw-search"),
      screen.getByRole("button", { name: "Pass 1" }),
      screen.getByTestId("glw-follow"),
    ];
    for (const control of controls) {
      control.focus();
      expect(control).toHaveFocus();
      expect(overlay.contains(document.activeElement)).toBe(true);
    }
    // GLW-FR-ONEV: Tab from the last control wraps to the first rather than
    // leaving the overlay.
    controls[controls.length - 1].focus();
    const last = overlay.querySelectorAll<HTMLElement>(
      'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
    );
    last[last.length - 1].focus();
    await userEvent.tab();
    expect(overlay.contains(document.activeElement)).toBe(true);
    expect(document.activeElement).toBe(last[0]);
  });

  it("GLW-FR-QDWA: once the beginning is on screen the window says so", async () => {
    answer = () =>
      makeLogPage({
        status: "available",
        entries: [makeSourceEntry(1, "the first line")],
        oldestReached: true,
        olderCursor: null,
      });
    draw();
    await screen.findByText("the first line");
    expect(await screen.findByTestId("glw-oldest-reached")).toHaveTextContent(
      "beginning of this log is on screen",
    );
  });

  it("GLW-FR-OPQQ, GLW-FR-TIKK: the read is the only operation, and no session log is reached", async () => {
    draw();
    await waitFor(() => expect(reads()).toHaveLength(1));
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    // The session diagnostic channel is not a route to a log record here
    // (GLW-FR-TIKK): the window reads nothing from it and writes only the
    // application's own diagnostics to it.
    const commands = invoked.mock.calls
      .map(([command]) => command as string)
      .filter((command) => command !== "append_log_records");
    expect(new Set(commands)).toEqual(new Set(["read_graduation_logs"]));
  });
});
