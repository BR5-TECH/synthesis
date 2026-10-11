import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { makeActivityEntry, makeLogPage } from "../../test/graduationFixtures";
import {
  WORKING,
  draw,
  installLogBackend,
  measure,
  workingRun,
  type LogBackend,
} from "../../test/graduationLogWindowHarness";
import type { GraduationLogCursor, GraduationLogPage } from "../../types";
import { PAGE_LIMIT } from "./useLogScope";
import { GraduationLogWindow } from ".";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

let backend: LogBackend;

beforeEach(() => {
  backend = installLogBackend();
});

afterEach(cleanup);

const RUN_LEVEL = "Run-level agent activity of this phase";

/** The cursor an older page of the scope is asked for with. */
function before(sequence: number): GraduationLogCursor {
  return { runId: "r1", stream: "activity", direction: "before", sequence };
}

describe("the graduation log window", () => {
  it("GLW-FR-QDWA, GLW-FR-RQEV: scrolling toward the leading edge asks for the page before", async () => {
    const older = before(10);
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({
            status: "available",
            entries: [makeActivityEntry(9, "older")],
            oldestReached: true,
            olderCursor: null,
          })
        : makeLogPage({
            status: "available",
            entries: [makeActivityEntry(10, "newer")],
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
    expect(rows.map((row) => row.querySelector(".runs-line__msg")?.textContent)).toEqual([
      "older",
      "newer",
    ]);
    const cursored = backend.reads().filter((read) => read.cursor);
    expect(cursored).toHaveLength(1);
    expect(cursored[0].cursor).toEqual(older);
  });

  it("GLW-FR-TCQP, GLW-FR-UZAB: an older page keeps the reader in place and is its own indication", async () => {
    let releaseOlder: ((page: GraduationLogPage) => void) | null = null;
    backend.answer = (args) =>
      args.cursor
        ? new Promise<GraduationLogPage>((resolve) => {
            releaseOlder = resolve;
          })
        : makeLogPage({
            status: "available",
            entries: [makeActivityEntry(10, "newer")],
            oldestReached: false,
            olderCursor: before(10),
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
          entries: [makeActivityEntry(9, "older")],
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
    backend.answer = (args) => {
      if (args.cursor && fail) throw new Error("log_stream_unreadable");
      if (args.cursor) {
        return makeLogPage({
          status: "available",
          entries: [makeActivityEntry(9, "older")],
          oldestReached: true,
        });
      }
      return makeLogPage({
        status: "available",
        entries: [makeActivityEntry(10, "newer")],
        oldestReached: false,
        olderCursor: before(10),
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
    backend.answer = () =>
      makeLogPage({
        status: "available",
        entries: [makeActivityEntry(1, "only")],
        oldestReached: true,
        olderCursor: null,
      });
    draw();
    await screen.findByText("only");
    const viewport = screen.getByTestId("glw-viewport");
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    expect(backend.reads().filter((read) => read.cursor)).toHaveLength(0);
  });

  it("GLW-FR-LUQI, GLW-FR-KKUM: an append is read back under the window's own cursor", async () => {
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({
            status: "available",
            entries: [makeActivityEntry(11, "arrived")],
          })
        : makeLogPage({ status: "available", entries: [makeActivityEntry(10, "held")] });
    draw();
    await screen.findByText("held");
    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 11 });
    await waitFor(() => expect(screen.getByText("arrived")).toBeInTheDocument());
    const after = backend.reads().find((read) => read.cursor);
    expect(after?.cursor).toMatchObject({ direction: "after", sequence: 10 });
  });

  it("GLW-FR-HCOI, GLW-FR-GZWN: an append of the structured stream is ignored", async () => {
    backend.answer = () => makeLogPage({ status: "available", entries: [makeActivityEntry(1, "held")] });
    draw();
    await screen.findByText("held");
    const before = backend.reads().length;
    await backend.appended({ runId: "r1", stream: "structured", latestSequence: 12 });
    expect(backend.reads()).toHaveLength(before);
  });

  it("GLW-FR-KTWX: scrolling away releases follow, and only the labelled control resumes it", async () => {
    backend.answer = () => makeLogPage({ status: "available", entries: [makeActivityEntry(1, "line")] });
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

  it("GLW-FR-KKUM, GLW-FR-KTWX: while follow is on, rows that arrive keep the viewport at the end", async () => {
    let height = 300;
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeActivityEntry(11, "arrived")] })
        : makeLogPage({ status: "available", entries: [makeActivityEntry(10, "held")] });
    draw();
    await screen.findByText("held");
    const viewport = screen.getByTestId("glw-viewport");
    measure(viewport, { height: () => height });
    height = 420;
    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 11 });
    await screen.findByText("arrived");
    expect(viewport).toHaveAttribute("data-following", "true");
    expect(viewport.scrollTop).toBe(420);
  });

  it("GLW-FR-KVXI: a record that arrives while follow is off does not move the reader", async () => {
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeActivityEntry(11, "arrived")] })
        : makeLogPage({ status: "available", entries: [makeActivityEntry(10, "held")] });
    draw();
    await screen.findByText("held");
    const viewport = screen.getByTestId("glw-viewport");
    Object.defineProperty(viewport, "scrollHeight", { value: 500, configurable: true });
    Object.defineProperty(viewport, "clientHeight", { value: 100, configurable: true });
    viewport.scrollTop = 120;
    act(() => viewport.dispatchEvent(new Event("scroll", { bubbles: true })));
    expect(viewport).toHaveAttribute("data-following", "false");

    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 11 });
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
    backend.answer = (args) => {
      const scope = args.pass as { kind: string; pass?: number };
      if (scope.kind === "pass" && scope.pass === 2) {
        return new Promise<GraduationLogPage>((resolve) => {
          releaseFirst = resolve;
        });
      }
      return makeLogPage({
        status: "available",
        entries: [makeActivityEntry(2, "pass one output")],
      });
    };
    draw();
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    await userEvent.click(screen.getByRole("button", { name: "Pass 1" }));
    await screen.findByText("pass one output");
    // The read of the scope the reader left now answers, late.
    await act(async () => {
      releaseFirst?.(
        makeLogPage({
          status: "available",
          entries: [makeActivityEntry(9, "stale pass two output")],
        }),
      );
    });
    expect(screen.queryByText("stale pass two output")).not.toBeInTheDocument();
    expect(screen.getByText("pass one output")).toBeInTheDocument();
  });

  it("GLW-FR-NFLN: a stale older page is discarded and does not release the new scope's guard", async () => {
    const releases: Array<(page: GraduationLogPage) => void> = [];
    backend.answer = (args) => {
      if (args.cursor) {
        return new Promise<GraduationLogPage>((resolve) => releases.push(resolve));
      }
      const scope = args.pass as { kind: string; pass?: number };
      return makeLogPage({
        status: "available",
        entries: [makeActivityEntry(scope.pass === 1 ? 20 : 10, `page of ${scope.pass}`)],
        oldestReached: false,
        olderCursor: {
          runId: "r1",
          stream: "activity",
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
          entries: [makeActivityEntry(9, "stale older page")],
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
          entries: [makeActivityEntry(19, "its own older page")],
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
    await waitFor(() => expect(backend.reads()).toHaveLength(1));

    await userEvent.keyboard("{ArrowUp}");
    await waitFor(() => expect(backend.reads()).toHaveLength(2));
    expect(backend.reads()[1].pass).toEqual({ kind: "pass", pass: 1 });
    expect(within(list).getByRole("button", { name: "Pass 1" })).toHaveFocus();

    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    await waitFor(() => expect(backend.reads()).toHaveLength(4));
    expect(backend.reads()[3].pass).toEqual({ kind: "run_level" });
  });

  it("GLW-FR-OOYK, GLW-FR-ONEV: Escape closes it and focus returns to the opener", async () => {
    const opener = document.createElement("button");
    opener.textContent = "Working";
    document.body.appendChild(opener);
    const onClose = vi.fn();
    const { unmount } = render(
      <GraduationLogWindow
        run={workingRun()}
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
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeActivityEntry(9, "older")] })
        : makeLogPage({
            status: "available",
            entries: [makeActivityEntry(10, "newer")],
            oldestReached: false,
            olderCursor: before(10),
          });
    draw();
    await screen.findByText("newer");
    act(() =>
      screen
        .getByTestId("glw-viewport")
        .dispatchEvent(new Event("scroll", { bubbles: true })),
    );
    await waitFor(() => expect(backend.reads().length).toBeGreaterThan(1));
    for (const read of backend.reads()) {
      expect(typeof read.limit).toBe("number");
      expect(read.limit as number).toBeGreaterThan(0);
      expect(read.limit).toBe(PAGE_LIMIT);
    }
  });

  it("GLW-FR-KWHL: changing the scope releases follow to its opening position", async () => {
    backend.answer = () => makeLogPage({ status: "available", entries: [makeActivityEntry(1, "line")] });
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
    await userEvent.click(screen.getByRole("button", { name: RUN_LEVEL }));
    await waitFor(() =>
      expect(screen.getByTestId("glw-viewport")).toHaveAttribute(
        "data-following",
        "true",
      ),
    );
  });

  it("GLW-FR-RQEV: pages that overlap hold no record twice and stay in sequence order", async () => {
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({
            status: "available",
            // The older page overlaps the newest one by a record.
            entries: [makeActivityEntry(8, "eight"), makeActivityEntry(9, "nine")],
            oldestReached: true,
          })
        : makeLogPage({
            status: "available",
            entries: [makeActivityEntry(9, "nine"), makeActivityEntry(10, "ten")],
            oldestReached: false,
            olderCursor: before(9),
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
      .map((row) => row.querySelector(".runs-line__msg")?.textContent);
    expect(rows).toEqual(["eight", "nine", "ten"]);
  });

  it("GLW-FR-LUQI, GLW-FR-GZWN: an append that lands before the first page is read after it rather than lost", async () => {
    let releaseFirst: ((page: GraduationLogPage) => void) | null = null;
    backend.answer = (args) =>
      args.cursor
        ? makeLogPage({ status: "available", entries: [makeActivityEntry(11, "arrived")] })
        : new Promise<GraduationLogPage>((resolve) => {
            releaseFirst = resolve;
          });
    draw();
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    await backend.appended({ runId: "r1", stream: "activity", latestSequence: 11 });
    // GLW-FR-GZWN: the append waits for the newest page rather than reading
    // after a sequence the window does not yet hold.
    expect(backend.reads()).toHaveLength(1);
    await act(async () => {
      releaseFirst?.(
        makeLogPage({ status: "available", entries: [makeActivityEntry(10, "held")] }),
      );
    });
    await waitFor(() => expect(screen.getByText("arrived")).toBeInTheDocument());
    expect(screen.getByText("held")).toBeInTheDocument();
    expect(backend.reads()[1].cursor).toMatchObject({ direction: "after", sequence: 10 });
  });

  it("GLW-FR-ONEV, GLW-FR-QMRV: focus stays inside, and every control is keyboard-operable", async () => {
    draw();
    const overlay = await screen.findByTestId("graduation-log-window");
    const controls = [
      screen.getByRole("button", { name: "Close" }),
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
    backend.answer = () =>
      makeLogPage({
        status: "available",
        entries: [makeActivityEntry(1, "the first line")],
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
    await waitFor(() => expect(backend.reads()).toHaveLength(1));
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    // The session diagnostic channel is not a route to a log record here
    // (GLW-FR-TIKK): the window reads nothing from it and writes only the
    // application's own diagnostics to it.
    const commands = backend
      .commands()
      .filter((command) => command !== "append_log_records");
    expect(new Set(commands)).toEqual(new Set(["read_graduation_logs"]));
  });
});
