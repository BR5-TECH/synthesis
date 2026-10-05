import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// Mocked at the Tauri boundary rather than at `../api` / `../events`, so the
// wrappers' own command names, argument shapes, and event name are exercised.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { LogPage, LogRecord } from "../types";
import { Logs, formatLocalTime, resetLogFilterForTest } from "./Logs";

const invoked = vi.mocked(invoke);
const listened = vi.mocked(listen);

// LOG-logs.md. The panel evaluates no filter itself (LOG-FR-06), so almost
// every test here asserts on the `query_logs` argument the panel sent and on
// its faithful rendering of what came back.

function record(over: Partial<LogRecord> = {}): LogRecord {
  return {
    sequence: 1,
    ts: "2026-08-04T12:04:31.902Z",
    level: "ERROR",
    domains: ["backend"],
    message: "scan aborted: permission denied",
    fields: { path: "vendor/", errno: 13 },
    ...over,
  };
}

function page(over: Partial<LogPage> = {}): LogPage {
  const records = over.records ?? [record()];
  return {
    records,
    generation: 0,
    matchedTotal: records.length,
    bufferTotal: records.length,
    droppedTotal: 0,
    highestSequence: records[records.length - 1]?.sequence ?? null,
    ...over,
  };
}

/** Every `query_logs` call's arguments, in call order. */
const queries = () =>
  invoked.mock.calls
    .filter(([name]) => name === "query_logs")
    .map(([, args]) => args as { filter: Record<string, unknown>; cursor: unknown; limit: number });

const lastFilter = () => queries()[queries().length - 1].filter;

/** Serve a fixed page (or a per-call sequence of them) to every query. */
function servePages(...pages: LogPage[]) {
  let n = 0;
  invoked.mockImplementation(async (name: string) => {
    if (name !== "query_logs") return undefined;
    const p = pages[Math.min(n, pages.length - 1)];
    n += 1;
    return p;
  });
}

/** The handler the panel registered for the append event. */
let appended: ((payload: unknown) => void) | undefined;

beforeEach(() => {
  resetLogFilterForTest();
  invoked.mockReset();
  invoked.mockImplementation(async () => undefined);
  appended = undefined;
  listened.mockReset();
  listened.mockImplementation(async (name, handler) => {
    if (name === "log-records-appended") {
      // The real channel delivers a full `Event`, so the stub does too rather
      // than a bare payload — otherwise a handler that read `ev.id` would pass
      // here and fail in the window.
      appended = (payload) => handler({ event: name, id: 1, payload });
    }
    return () => {};
  });
});

afterEach(cleanup);

/** Render and wait for the panel's first page to land. */
async function renderLogs() {
  render(<Logs />);
  await waitFor(() => expect(queries().length).toBeGreaterThan(0));
  return screen.getByTestId("logs-list");
}

const rows = () => screen.queryAllByTestId("logs-row");

describe("the record list (LOG-FR-02 / LOG-FR-03 / LOG-FR-02)", () => {
  it("renders time, level, domains, and message in fixed columns", async () => {
    servePages(page({ records: [record({ domains: ["ai", "remote"], level: "WARN", message: "model request retried" })] }));
    await renderLogs();

    await waitFor(() => expect(rows()).toHaveLength(1));
    const row = rows()[0];
    // The stamp is stored UTC and rendered in the viewer's own zone, which is
    // what lets a reader correlate a record with something they remember.
    expect(row.querySelector(".logs__ts")).toHaveTextContent(
      formatLocalTime("2026-08-04T12:04:31.902Z"),
    );
    expect(row.querySelector(".logs__domains")).toHaveTextContent("ai·remote");
    expect(row.querySelector(".logs__msg")).toHaveTextContent("model request retried");
  });

  it("prints the level as a label, so colour is not the only channel", async () => {
    // LOG-FR-03: `WARN` is told from `ERROR` without relying on colour
    // perception and without hovering the row.
    servePages(page({ records: [record({ level: "WARN" })] }));
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));
    const level = rows()[0].querySelector(".logs__lvl");
    expect(level).toHaveTextContent("WARN");
    expect(level).toHaveAttribute("data-level", "WARN");
  });

  it("carries the level on the row, so a level can be styled beyond its label", async () => {
    // LOG-FR-03: the printed label is one channel and a colour treatment is the
    // other. The row-level attribute is what the second channel hangs on — the
    // rail that makes a WARN or an ERROR findable in a list that is mostly
    // INFO — and losing it is a change nothing else in this file would notice.
    servePages(
      page({
        records: [
          record({ sequence: 1, level: "DEBUG" }),
          record({ sequence: 2, level: "ERROR" }),
        ],
      }),
    );
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(2));
    expect(rows()[0]).toHaveAttribute("data-level", "DEBUG");
    expect(rows()[1]).toHaveAttribute("data-level", "ERROR");
  });

  it("renders a millisecond local time, zero-padded", () => {
    // Asserted against a constructed Date rather than a fixed string, because
    // the expected value is zone-dependent and the property under test is the
    // formatting, not the offset.
    const at = new Date("2026-08-04T12:04:31.902Z");
    const pad = (n: number, w = 2) => String(n).padStart(w, "0");
    expect(formatLocalTime("2026-08-04T12:04:31.902Z")).toBe(
      `${pad(at.getHours())}:${pad(at.getMinutes())}:${pad(at.getSeconds())}.902`,
    );
    // A stamp the backend could not have produced is passed through rather
    // than rendered as "Invalid Date".
    expect(formatLocalTime("not-a-date")).toBe("not-a-date");
  });

  it("renders a path in exactly the case it carries on disk (LOG-FR-22 / SNV-FR-57)", async () => {
    servePages(
      page({
        records: [record({ message: "opened LIB-library.md", fields: { path: "LIB-library.md" } })],
      }),
    );
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));

    expect(rows()[0].querySelector(".logs__msg")).toHaveTextContent("LIB-library.md");
    await userEvent.click(rows()[0].querySelector(".logs__line") as HTMLElement);
    const json = screen.getByTestId("logs-json").textContent ?? "";
    expect(json).toContain("LIB-library.md");
    expect(json).not.toContain("LIB-LIBRARY.MD");
  });
});

describe("expanding a record (LOG-FR-04)", () => {
  it("expands in place to the full record, keeping fields the columns did not show", async () => {
    servePages(page({ records: [record({ sequence: 1 }), record({ sequence: 2, message: "second" })] }));
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(2));

    expect(screen.queryAllByTestId("logs-json")).toHaveLength(0);
    await userEvent.click(rows()[0].querySelector(".logs__line") as HTMLElement);

    const json = screen.getByTestId("logs-json").textContent ?? "";
    expect(JSON.parse(json)).toEqual(record({ sequence: 1 }));
    // `errno` never appears in a column; the expansion is where it shows.
    expect(json).toContain("errno");
  });

  it("keeps any number of rows expanded at once, and collapses independently", async () => {
    servePages(page({ records: [record({ sequence: 1 }), record({ sequence: 2 })] }));
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(2));

    await userEvent.click(rows()[0].querySelector(".logs__line") as HTMLElement);
    await userEvent.click(rows()[1].querySelector(".logs__line") as HTMLElement);
    expect(screen.getAllByTestId("logs-json")).toHaveLength(2);

    await userEvent.click(rows()[0].querySelector(".logs__line") as HTMLElement);
    expect(screen.getAllByTestId("logs-json")).toHaveLength(1);
  });

  it("a plain click moves the selection and leaves earlier expansions open", async () => {
    // The two states are independent, and the row band renders one of them: a
    // plain click makes that row *the* selection (LOG-FR-16 reserves building a
    // multi-row selection for a modified click), while an expansion it opened
    // earlier stays open. Nothing else in this file observes `data-selected` in
    // its true state — the attribute the selected-row treatment hangs on.
    servePages(
      page({
        records: [
          record({ sequence: 1 }),
          record({ sequence: 2, message: "second" }),
          record({ sequence: 3, message: "third" }),
        ],
      }),
    );
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(3));

    await userEvent.click(rows()[0].querySelector(".logs__line") as HTMLElement);
    expect(rows()[0]).toHaveAttribute("data-selected", "true");
    expect(screen.getAllByTestId("logs-json")).toHaveLength(1);

    await userEvent.click(rows()[1].querySelector(".logs__line") as HTMLElement);
    expect(rows()[1]).toHaveAttribute("data-selected", "true");
    expect(rows()[0]).toHaveAttribute("data-selected", "false");
    expect(rows()[2]).toHaveAttribute("data-selected", "false");
    // The first row's own expansion is untouched: selecting is not collapsing.
    expect(screen.getAllByTestId("logs-json")).toHaveLength(2);
  });
});

describe("the filter controls (LOG-FR-05 – LOG-FR-08)", () => {
  it("pins the level/domain row above the search row, above the list (LOG-FR-05)", async () => {
    servePages(page());
    await renderLogs();
    const controlRows = document.querySelectorAll(".logs__row");
    expect(controlRows).toHaveLength(2);
    expect(within(controlRows[0] as HTMLElement).getByLabelText("Level")).toBeInTheDocument();
    expect(within(controlRows[1] as HTMLElement).getByPlaceholderText("Filter logs…")).toBeInTheDocument();
    // The list is the only scrolling region, and it follows both rows.
    const controls = document.querySelector(".logs__controls") as HTMLElement;
    const list = screen.getByTestId("logs-list");
    expect(controls.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("opens on defaults that hide nothing (LOG-FR-07)", async () => {
    servePages(page());
    await renderLogs();
    expect(lastFilter()).toEqual({
      minLevel: "DEBUG",
      domains: ["frontend", "ai", "backend", "remote"],
      query: "",
      queryIsRegex: false,
    });
  });

  it("sends the level as a floor rather than narrowing client-side (LOG-FR-06, LOG-FR-07)", async () => {
    // LOG-FR-06: the panel holds no unmatched record and applies no predicate.
    // What it renders is exactly what came back, even when that contradicts the
    // control — which is the only way to prove the narrowing happened remotely.
    servePages(page({ records: [record({ level: "DEBUG", message: "served anyway" })] }));
    await renderLogs();
    await userEvent.selectOptions(screen.getByLabelText("Level"), "WARN");

    await waitFor(() => expect(lastFilter().minLevel).toBe("WARN"));
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toHaveTextContent("served anyway");
  });

  it("treats checking none as checking all (LOG-FR-07)", async () => {
    servePages(page());
    await renderLogs();
    for (const domain of ["frontend", "ai", "backend", "remote"]) {
      await userEvent.click(screen.getByRole("checkbox", { name: domain }));
    }
    // An empty list is what the backend reads as "every domain" (LGC-FR-09), so
    // the panel sends emptiness rather than re-expanding it to all four.
    await waitFor(() => expect(lastFilter().domains).toEqual([]));
  });

  it("sends a domain the moment it is checked", async () => {
    servePages(page());
    await renderLogs();
    await userEvent.click(screen.getByRole("checkbox", { name: "ai" }));
    await waitFor(() => expect(lastFilter().domains).not.toContain("ai"));
    await userEvent.click(screen.getByRole("checkbox", { name: "ai" }));
    await waitFor(() => expect(lastFilter().domains).toContain("ai"));
  });

  it("keeps filter values for the session, across unmount and remount (LOG-FR-07, LOG-FR-08)", async () => {
    // The panel is UNMOUNTED whenever the bottom panel switches surface, so
    // component state would reset every time the user glanced at Git. This is
    // the requirement that forbids exactly that.
    servePages(page());
    await renderLogs();
    await userEvent.selectOptions(screen.getByLabelText("Level"), "WARN");
    await userEvent.type(screen.getByPlaceholderText("Filter logs…"), "vendor");
    await waitFor(() => expect(lastFilter().query).toBe("vendor"));

    cleanup();
    invoked.mockClear();
    render(<Logs />);
    await waitFor(() => expect(queries().length).toBeGreaterThan(0));

    expect(lastFilter().minLevel).toBe("WARN");
    expect(lastFilter().query).toBe("vendor");
    expect(screen.getByPlaceholderText("Filter logs…")).toHaveValue("vendor");

    // Nothing is persisted: a fresh session starts at the defaults.
    cleanup();
    resetLogFilterForTest();
    invoked.mockClear();
    render(<Logs />);
    await waitFor(() => expect(queries().length).toBeGreaterThan(0));
    expect(lastFilter().minLevel).toBe("DEBUG");
    expect(lastFilter().query).toBe("");
  });
});

describe("the search box (LOG-FR-09)", () => {
  it("sends the text as a substring query by default, and as a regex on opt-in", async () => {
    servePages(page());
    await renderLogs();
    await userEvent.type(screen.getByPlaceholderText("Filter logs…"), "ABORT");
    await waitFor(() => expect(lastFilter().query).toBe("ABORT"));
    expect(lastFilter().queryIsRegex).toBe(false);

    await userEvent.click(screen.getByRole("checkbox", { name: ".*" }));
    await waitFor(() => expect(lastFilter().queryIsRegex).toBe(true));
  });

  it("reports an uncompilable pattern beside the box and keeps the last list", async () => {
    // A half-typed pattern must not blank the panel — which is what makes the
    // regex toggle usable at all, since almost every prefix of a real pattern
    // is itself invalid.
    servePages(page({ records: [record({ message: "still here" })] }));
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));

    invoked.mockImplementation(async (name: string) => {
      if (name === "query_logs") throw "invalid query";
      return undefined;
    });
    await userEvent.click(screen.getByRole("checkbox", { name: ".*" }));
    await userEvent.type(screen.getByPlaceholderText("Filter logs…"), "(");

    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("invalid query"));
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toHaveTextContent("still here");
  });
});

describe("following the buffer (LOG-FR-11 – LOG-FR-15)", () => {
  it("takes the newest page on becoming visible, with no cursor (LOG-FR-11, LOG-FR-12)", async () => {
    servePages(page());
    await renderLogs();
    // An absent cursor is what asks for the newest page (LGC-FR-22), so a panel
    // opened onto a long session starts at the end rather than paging to it.
    expect(queries()[0].cursor).toBeNull();
    expect(queries()[0].limit).toBeGreaterThan(0);
  });

  it("asks only for the delta when the buffer grows", async () => {
    servePages(
      page({ records: [record({ sequence: 7 })] }),
      page({ records: [record({ sequence: 8, message: "newer" })] }),
    );
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));

    appended?.({ generation: 0, bufferTotal: 2, droppedTotal: 0, highestSequence: 8 });
    await waitFor(() => expect(rows()).toHaveLength(2));
    // LOG-FR-15: the records after the newest held, not the whole page again.
    expect(queries()[queries().length - 1].cursor).toEqual({ after: 7 });
  });

  it("discards everything and re-queries when the generation changes (LOG-FR-13, LGC-FR-15)", async () => {
    servePages(
      page({ records: [record({ sequence: 1 })], generation: 0 }),
      page({ records: [record({ sequence: 40, message: "after the clear" })], generation: 1 }),
    );
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));
    // Expand and select, so the discard is observable in more than the rows.
    await userEvent.click(rows()[0].querySelector(".logs__line") as HTMLElement);
    expect(screen.getAllByTestId("logs-json")).toHaveLength(1);

    appended?.({ generation: 1, bufferTotal: 1, droppedTotal: 0, highestSequence: 40 });

    await waitFor(() => expect(rows()[0]).toHaveTextContent("after the clear"));
    expect(rows()).toHaveLength(1);
    // The new generation renders nothing expanded and nothing selected. Note
    // what this does *not* prove: sequences never reset across a clear
    // (LGC-FR-05), so a retained key could not have matched a post-clear record
    // anyway. The panel discarding its sets is memory hygiene, and it is not
    // observable from the DOM — this pins the rendering, not the discard.
    expect(screen.queryAllByTestId("logs-json")).toHaveLength(0);
    expect(rows()[0]).toHaveAttribute("data-selected", "false");
    // And it re-queried from scratch rather than asking for a delta.
    expect(queries()[queries().length - 1].cursor).toBeNull();
  });

  it("states how many older records were dropped (LOG-FR-14)", async () => {
    servePages(page({ droppedTotal: 3140, bufferTotal: 20000, matchedTotal: 20000 }));
    await renderLogs();
    await waitFor(() =>
      expect(screen.getByText(/older records were dropped/)).toHaveTextContent("3,140"),
    );
  });

  it("shows no eviction notice while nothing has been evicted", async () => {
    servePages(page({ droppedTotal: 0 }));
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(screen.queryByText(/older records were dropped/)).toBeNull();
  });

  it("asks for the preceding page when scrolled to the top (LOG-FR-15)", async () => {
    servePages(
      page({ records: [record({ sequence: 100 }), record({ sequence: 101 })] }),
      page({ records: [record({ sequence: 98, message: "older" }), record({ sequence: 99 })] }),
    );
    const list = await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(2));

    Object.defineProperty(list, "scrollTop", { value: 0, writable: true, configurable: true });
    Object.defineProperty(list, "scrollHeight", { value: 900, writable: true, configurable: true });
    Object.defineProperty(list, "clientHeight", { value: 300, writable: true, configurable: true });
    list.dispatchEvent(new Event("scroll", { bubbles: true }));

    await waitFor(() =>
      expect(queries()[queries().length - 1].cursor).toEqual({ before: 100 }),
    );
    // Prepended, so the page the user was reading is still there beneath it.
    await waitFor(() => expect(rows()).toHaveLength(4));
    expect(rows()[0]).toHaveTextContent("older");
  });

  it("releases the follow pin when scrolled up and resumes it at the foot (LOG-FR-10)", async () => {
    servePages(page());
    const list = await renderLogs();
    const follow = screen.getByRole("button", { name: "Follow" });
    expect(follow).toHaveAttribute("data-active", "true");

    Object.defineProperty(list, "scrollTop", { value: 200, writable: true, configurable: true });
    Object.defineProperty(list, "scrollHeight", { value: 900, writable: true, configurable: true });
    Object.defineProperty(list, "clientHeight", { value: 300, writable: true, configurable: true });
    list.dispatchEvent(new Event("scroll", { bubbles: true }));
    await waitFor(() => expect(follow).toHaveAttribute("data-active", "false"));

    // Back to the foot: the pin resumes without the user reaching for a control.
    Object.defineProperty(list, "scrollTop", { value: 600, writable: true, configurable: true });
    list.dispatchEvent(new Event("scroll", { bubbles: true }));
    await waitFor(() => expect(follow).toHaveAttribute("data-active", "true"));
  });
});

describe("copying and exporting (LOG-FR-16 / LOG-FR-17)", () => {
  it("copies the selected records as JSON, one per line (LOG-FR-16)", async () => {
    const writeText = vi.fn(async (_text: string) => {});
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    servePages(
      page({
        records: [
          record({ sequence: 1 }),
          record({ sequence: 2, message: "second" }),
          record({ sequence: 3, message: "third" }),
        ],
      }),
    );
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(3));

    await userEvent.click(rows()[0].querySelector(".logs__line") as HTMLElement);
    // A modified click extends the selection without unfolding fifty blocks of
    // JSON on the way. `fireEvent` rather than `userEvent`, because the
    // modifier has to reach the handler as event state and `userEvent.click`'s
    // second argument is its own options object, not an event init.
    fireEvent.click(rows()[2].querySelector(".logs__line") as HTMLElement, {
      ctrlKey: true,
    });
    expect(screen.getAllByTestId("logs-json")).toHaveLength(1);
    // Several rows carry the selection at once — the state the row band has to
    // render, and the one the clipboard payload below is derived from.
    expect(rows()[0]).toHaveAttribute("data-selected", "true");
    expect(rows()[1]).toHaveAttribute("data-selected", "false");
    expect(rows()[2]).toHaveAttribute("data-selected", "true");
    await userEvent.click(screen.getByRole("button", { name: "Copy" }));

    const lines = (writeText.mock.calls[0][0] as string).split("\n");
    expect(lines).toHaveLength(2);
    // The full records, not the rendered row text: `errno` shows in neither
    // column, and a defect report needs it.
    expect(JSON.parse(lines[0])).toEqual(record({ sequence: 1 }));
    expect(JSON.parse(lines[1]).message).toBe("third");
    expect(lines[0]).toContain("errno");
  });

  it("disables Copy while nothing is selected", async () => {
    servePages(page());
    await renderLogs();
    expect(screen.getByRole("button", { name: "Copy" })).toBeDisabled();
  });

  it("exports the whole match set under the current filter (LOG-FR-17)", async () => {
    invoked.mockImplementation(async (name: string) => {
      if (name === "query_logs") return page({ matchedTotal: 120, bufferTotal: 1208 });
      if (name === "browse_for_save_path") return { selected: { path: "/tmp/logs.jsonl" } };
      if (name === "export_logs") return 120;
      return undefined;
    });
    await renderLogs();
    await userEvent.selectOptions(screen.getByLabelText("Level"), "ERROR");
    await waitFor(() => expect(lastFilter().minLevel).toBe("ERROR"));

    await userEvent.click(screen.getByRole("button", { name: "Export" }));

    await waitFor(() =>
      expect(invoked).toHaveBeenCalledWith("export_logs", {
        // The panel's current filter — not the pages loaded, and not the rows
        // on screen.
        filter: expect.objectContaining({ minLevel: "ERROR" }),
        destinationPath: "/tmp/logs.jsonl",
      }),
    );
    await waitFor(() => expect(screen.getByText("Exported 120 records.")).toBeInTheDocument());
  });

  it("invokes nothing when the save dialog is cancelled", async () => {
    invoked.mockImplementation(async (name: string) => {
      if (name === "query_logs") return page();
      if (name === "browse_for_save_path") return "cancelled";
      return undefined;
    });
    await renderLogs();
    await userEvent.click(screen.getByRole("button", { name: "Export" }));

    await waitFor(() =>
      expect(invoked.mock.calls.some(([n]) => n === "browse_for_save_path")).toBe(true),
    );
    expect(invoked.mock.calls.some(([n]) => n === "export_logs")).toBe(false);
  });

  it("reports a failed export in the panel", async () => {
    invoked.mockImplementation(async (name: string) => {
      if (name === "query_logs") return page();
      if (name === "browse_for_save_path") return { selected: { path: "/nope/logs.jsonl" } };
      if (name === "export_logs") throw "export failed";
      return undefined;
    });
    await renderLogs();
    await userEvent.click(screen.getByRole("button", { name: "Export" }));
    await waitFor(() =>
      expect(screen.getByText(/Export failed: export failed/)).toBeInTheDocument(),
    );
  });
});

describe("the two empty states (LOG-FR-18)", () => {
  it("states an empty session as a block, with the controls still present", async () => {
    servePages(page({ records: [], matchedTotal: 0, bufferTotal: 0, highestSequence: null }));
    await renderLogs();
    await waitFor(() => expect(screen.getByText("No logs yet")).toBeInTheDocument());
    // The controls stay: they are what the user reaches for once records arrive.
    expect(screen.getByLabelText("Level")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Filter logs…")).toBeInTheDocument();
    expect(screen.queryByText(/No records match/)).toBeNull();
  });

  it("distinguishes a list narrowed to nothing from an empty buffer", async () => {
    // The distinction is load-bearing: one says nothing of the kind exists yet,
    // the other says the controls already in the author's hands admit nothing.
    servePages(page({ records: [], matchedTotal: 0, bufferTotal: 1208 }));
    render(<Logs />);
    await waitFor(() => expect(screen.getByText(/No records match/)).toBeInTheDocument());
    expect(screen.queryByText("No logs yet")).toBeNull();
    expect(screen.getByPlaceholderText("Filter logs…")).toBeInTheDocument();
  });

  it("reports the match count against the buffer's own total", async () => {
    servePages(page({ matchedTotal: 42, bufferTotal: 1208 }));
    await renderLogs();
    await waitFor(() =>
      expect(screen.getByTestId("logs-counts")).toHaveTextContent("42 of 1208"),
    );
  });
});

describe("keeping up with the buffer under load", () => {
  it("keeps paging until it holds the newest record after a large burst", async () => {
    // The backend coalesces a thousand appends into a couple of events, so one
    // page-sized delta per event silently leaves the tail unfetched while the
    // Follow pin still claims to be at the end. The panel must catch up.
    const first = Array.from({ length: 200 }, (_, i) =>
      record({ sequence: 100 + i, message: `m${i}` }),
    );
    const second = [record({ sequence: 300, message: "the actual newest" })];
    servePages(
      page({ records: [record({ sequence: 99 })], highestSequence: 99 }),
      // A full page whose highestSequence is far beyond its last record: there
      // is more to come.
      page({ records: first, highestSequence: 300 }),
      page({ records: second, highestSequence: 300 }),
    );
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));

    appended?.({ generation: 0, bufferTotal: 202, droppedTotal: 0, highestSequence: 300 });

    await waitFor(() =>
      expect(rows()[rows().length - 1]).toHaveTextContent("the actual newest"),
    );
    // It asked twice, the second time from the end of the first delta.
    const cursors = queries().map((q) => q.cursor);
    expect(cursors).toContainEqual({ after: 99 });
    expect(cursors).toContainEqual({ after: 299 });
  });

  it("never merges a page belonging to a different filter", async () => {
    // A filter change and an append event can be in flight together. Merging
    // the new filter's delta into the old filter's rows would render records
    // the current filter excludes — exactly what LOG-FR-06 forbids.
    servePages(page({ records: [record({ sequence: 1, level: "DEBUG", message: "debug row" })] }));
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));

    // Hold the filter-change query open, so the append event lands while it is
    // still in flight. Without the overlap the replace simply finishes first
    // and the merge never has an old-filter row to corrupt — the race IS the
    // requirement here.
    let releaseReplace: (() => void) | undefined;
    invoked.mockImplementation(async (name: string, args: unknown) => {
      if (name !== "query_logs") return undefined;
      const { cursor } = args as { cursor: unknown };
      if (cursor === null) {
        await new Promise<void>((resolve) => {
          releaseReplace = resolve;
        });
        return page({ records: [record({ sequence: 2, level: "ERROR", message: "error row" })] });
      }
      // The delta, answered under the NEW filter while the replace is pending.
      return page({ records: [record({ sequence: 3, level: "ERROR", message: "newer error" })] });
    });

    await userEvent.selectOptions(screen.getByLabelText("Level"), "WARN");
    await waitFor(() => expect(lastFilter().minLevel).toBe("WARN"));

    appended?.({ generation: 0, bufferTotal: 3, droppedTotal: 0, highestSequence: 3 });
    await waitFor(() => expect(queries().length).toBeGreaterThan(2));
    releaseReplace?.();

    await waitFor(() => expect(rows().length).toBeGreaterThan(0));
    // The DEBUG row belonged to the previous filter. A merge keyed only on
    // generation would have kept it, rendering a record the current filter
    // excludes.
    const text = rows()
      .map((r) => r.textContent ?? "")
      .join(" ");
    expect(text).not.toContain("debug row");
  });

  it("stops asking for older pages once there is nothing older", async () => {
    // A buffer whose head has been evicted answers a `before` cursor with an
    // empty page. Without an exhausted flag every scroll gesture at the top
    // re-issues the same query forever.
    servePages(
      page({ records: [record({ sequence: 500 })] }),
      page({ records: [], matchedTotal: 1, bufferTotal: 1 }),
    );
    const list = await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));

    const scrollToTop = () => {
      Object.defineProperty(list, "scrollTop", { value: 0, writable: true, configurable: true });
      Object.defineProperty(list, "scrollHeight", { value: 900, writable: true, configurable: true });
      Object.defineProperty(list, "clientHeight", { value: 300, writable: true, configurable: true });
      list.dispatchEvent(new Event("scroll", { bubbles: true }));
    };

    scrollToTop();
    await waitFor(() =>
      expect(queries().some((q) => q.cursor && "before" in (q.cursor as object))).toBe(true),
    );
    const afterFirst = queries().length;

    scrollToTop();
    scrollToTop();
    await new Promise((r) => setTimeout(r, 30));
    expect(queries()).toHaveLength(afterFirst);
  });

  it("holds the reading position when an older page is prepended", async () => {
    // Rows inserted ABOVE what the user is reading push it off-screen unless
    // the scroll offset is re-anchored — and because scrollTop stays at 0, the
    // next scroll event requests yet another page.
    //
    // The correction anchors on the row's own `offsetTop`, deliberately NOT on
    // the list's height: at the MAX_LOADED cap a prepend adds rows at the head
    // and drops as many from the tail, so the height delta is ~0 while the
    // content has moved a full page. jsdom lays nothing out, so `offsetTop` is
    // stubbed to a row's index among its siblings — enough to prove the
    // mechanism; the real geometry is verified in a browser.
    const ROW_H = 100;
    const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetTop");
    Object.defineProperty(HTMLElement.prototype, "offsetTop", {
      configurable: true,
      get(this: HTMLElement) {
        const entries = Array.from(
          this.parentElement?.querySelectorAll(".logs__entry") ?? [],
        );
        const index = entries.indexOf(this);
        return index < 0 ? 0 : index * ROW_H;
      },
    });

    try {
      const older = Array.from({ length: 5 }, (_, i) => record({ sequence: 10 + i }));
      let call = 0;
      invoked.mockImplementation(async (name: string) => {
        if (name !== "query_logs") return undefined;
        call += 1;
        if (call === 1) return page({ records: [record({ sequence: 100 })] });
        return page({ records: older });
      });

      const list = await renderLogs();
      await waitFor(() => expect(rows()).toHaveLength(1));
      Object.defineProperty(list, "scrollTop", { value: 0, writable: true, configurable: true });
      Object.defineProperty(list, "scrollHeight", { value: 900, configurable: true });
      Object.defineProperty(list, "clientHeight", { value: 300, configurable: true });

      list.dispatchEvent(new Event("scroll", { bubbles: true }));
      await waitFor(() => expect(rows()).toHaveLength(6));

      // The anchor row went from index 0 to index 5, so the view follows it
      // down by five rows and the user is still looking at the same record.
      await waitFor(() => expect(list.scrollTop).toBe(5 * ROW_H));
    } finally {
      if (original) Object.defineProperty(HTMLElement.prototype, "offsetTop", original);
      else delete (HTMLElement.prototype as unknown as Record<string, unknown>).offsetTop;
    }
  });
});

describe("what the panel never does", () => {
  it("renders and copies a record verbatim, masking nothing (LOG-FR-20)", async () => {
    // LGC-FR-16 puts redaction at the emit site and nowhere else. A "helpful"
    // scrub here would be a silent contract change — the panel would start
    // showing something other than what was logged, and an exported record
    // would disagree with a copied one.
    const writeText = vi.fn(async (_text: string) => {});
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    const token = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    servePages(page({ records: [record({ message: token, fields: { token } })] }));
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));

    expect(rows()[0].querySelector(".logs__msg")).toHaveTextContent(token);
    await userEvent.click(rows()[0].querySelector(".logs__line") as HTMLElement);
    await userEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(writeText.mock.calls[0][0]).toContain(token);
  });

  it("does not claim an empty session when the query itself failed", async () => {
    // `bufferTotal` is 0 because nobody managed to ask, not because the session
    // emitted nothing — the two read identically in the state and must not read
    // identically on screen.
    invoked.mockImplementation(async (name: string) => {
      if (name === "query_logs") throw "backend unavailable";
      return undefined;
    });
    render(<Logs />);
    await waitFor(() => expect(screen.getByRole("status")).toBeInTheDocument());
    expect(screen.queryByText("No logs yet")).toBeNull();
  });

  it("leaves a text selection to the browser's own copy (LOG-FR-16)", async () => {
    const writeText = vi.fn(async (_text: string) => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    servePages(page());
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));
    await userEvent.click(rows()[0].querySelector(".logs__line") as HTMLElement);

    // With nothing highlighted the accelerator copies the selected records...
    fireEvent.keyDown(document.querySelector(".logs") as HTMLElement, {
      key: "c",
      ctrlKey: true,
    });
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(JSON.parse(writeText.mock.calls[0][0])).toEqual(record());

    // ...but a user who highlighted text inside the expanded JSON means the
    // browser's copy. Overriding it would make the one place with text worth
    // selecting the one place selection does not work.
    const selection = { toString: () => "permission denied" } as Selection;
    vi.spyOn(window, "getSelection").mockReturnValue(selection);
    fireEvent.keyDown(document.querySelector(".logs") as HTMLElement, {
      key: "c",
      ctrlKey: true,
    });
    expect(writeText).toHaveBeenCalledTimes(1);
    vi.mocked(window.getSelection).mockRestore();
  });

  it("puts the follow state in more than one channel (LOG-FR-10)", async () => {
    servePages(page());
    await renderLogs();
    const follow = screen.getByRole("button", { name: "Follow" });
    // A ghost button carries no active treatment of its own, so `data-active`
    // alone was conveying the state in neither the visual nor the a11y channel.
    expect(follow).toHaveAttribute("aria-pressed", "true");
    await userEvent.click(follow);
    expect(follow).toHaveAttribute("aria-pressed", "false");
  });

  it("shows every domain a record carries (LOG-FR-02)", async () => {
    // The column ellipsised exactly the multi-domain records it exists to
    // distinguish; the full set must be present in the DOM either way.
    servePages(
      page({
        records: [record({ domains: ["frontend", "ai", "backend", "remote"] })],
      }),
    );
    await renderLogs();
    await waitFor(() => expect(rows()).toHaveLength(1));
    const cell = rows()[0].querySelector(".logs__domains") as HTMLElement;
    expect(cell).toHaveTextContent("frontend·ai·backend·remote");
    expect(cell).toHaveAttribute("title", "frontend·ai·backend·remote");
  });

  it("subscribes to the append channel by the name the backend emits on", async () => {
    servePages(page());
    await renderLogs();
    // A name mismatch is silently dead on both sides — `listen` and `emit`
    // reject the same strings — so the channel is asserted rather than assumed.
    expect(listened.mock.calls.map(([name]) => name)).toContain("log-records-appended");
  });
});
