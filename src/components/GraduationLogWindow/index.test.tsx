import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
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
  makeStructuredEntry,
} from "../../test/graduationFixtures";
import type { ProgressStage } from "../../state/runProgress";
import type { GraduationLogPage, GraduationRun } from "../../types";
import type { GraduationObservability } from "../../types/graduationObservability";
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
  it("GLW-FR-OMZA, GLW-FR-BLWH: it names its run and its stage, and holds no run selector", async () => {
    draw();
    expect(await screen.findByTestId("glw-title")).toHaveTextContent(
      "Run r1 · Working",
    );
    expect(
      screen.getByRole("dialog", { name: /Log of Run r1, stage Working/ }),
    ).toBeInTheDocument();
    // GLW-FR-BLWH: nothing in the window names or reaches another run. Every
    // control it holds is one of these, and none of them changes the run.
    const overlay = screen.getByTestId("graduation-log-window");
    expect(within(overlay).getAllByRole("button").map((b) => b.textContent)).toEqual([
      "",
      "Source",
      "Structured",
      "Pass 1",
      "Pass 2",
      "Run-level",
      "Following",
    ]);
    expect(within(overlay).queryAllByRole("combobox")).toHaveLength(0);
    expect(within(overlay).queryAllByRole("listbox")).toHaveLength(0);
    expect(overlay.textContent).not.toMatch(/Run r2/);
  });

  it("GLW-FR-MZUP: the stage is the descriptor the row passed, whatever its id", async () => {
    draw({
      stage: { id: "zeta", label: "Zeta" },
      run: run({
        observability: makeObservability({
          // Ids outside the four the observability record names today, so the
          // window is established to branch on no stage id of its own.
          stageHistory: [
            {
              from: "eta",
              to: "zeta",
              pass: 1,
              at: "1",
              reason: "work_started",
            } as unknown as GraduationObservability["stageHistory"][number],
          ],
        }),
        logs: makeLogs({ source: [{ phaseId: "zeta", pass: 1 }] }),
      }),
    });
    await waitFor(() => expect(reads()).toHaveLength(1));
    expect(reads()[0].phaseId).toBe("zeta");
    expect(screen.getByTestId("glw-title")).toHaveTextContent("Zeta");
  });

  it("GLW-FR-TMRQ, GLW-FR-ELJO: it opens on Source and on the newest pass with records", async () => {
    draw();
    await waitFor(() => expect(reads()).toHaveLength(1));
    expect(reads()[0]).toMatchObject({
      runId: "r1",
      phaseId: "working",
      stream: "source",
      pass: { kind: "pass", pass: 2 },
    });
  });

  it("GLW-FR-CKLZ, GLW-FR-DDXJ, GLW-FR-EUTW: one entry per pass, and a labelled run-level entry", async () => {
    draw();
    const list = await screen.findByTestId("glw-pass-list");
    const names = within(list)
      .getAllByRole("button")
      .map((button) => button.getAttribute("aria-label"));
    // GLW-FR-EUTW: two passes are two entries, and no entry merges them.
    expect(names).toEqual(["Pass 1", "Pass 2", "Run-level output of this stage"]);
    expect(new Set(names).size).toBe(names.length);
  });

  it("GLW-FR-YVKD: a stage holding run-level records alone opens on the run-level entry", async () => {
    draw({
      stage: { id: "done", label: "Done" },
      run: run({
        observability: makeObservability({
          stageHistory: [
            { from: "review", to: "done", pass: 2, at: "1", reason: "finished" },
          ],
        }),
        logs: makeLogs({ source: [{ phaseId: "done", pass: null }] }),
      }),
    });
    await waitFor(() => expect(reads()).toHaveLength(1));
    expect(reads()[0].pass).toEqual({ kind: "run_level" });
  });

  it("GLW-FR-PDGA, GLW-FR-CQXJ: selecting the run-level entry reads the run-level scope", async () => {
    draw();
    await waitFor(() => expect(reads()).toHaveLength(1));
    await userEvent.click(
      screen.getByRole("button", { name: "Run-level output of this stage" }),
    );
    await waitFor(() => expect(reads()).toHaveLength(2));
    expect(reads()[1].pass).toEqual({ kind: "run_level" });
  });

  it("GLW-FR-FCVA, GLW-FR-DKWB: every read names the run, the stage, the scope, and the stream", async () => {
    draw();
    await waitFor(() => expect(reads()).toHaveLength(1));
    await userEvent.click(screen.getByRole("button", { name: "Pass 1" }));
    await waitFor(() => expect(reads()).toHaveLength(2));
    for (const read of reads()) {
      // GLW-FR-DKWB: the phase is the selected one on every read, so a record
      // of another phase is never asked for and never shown here.
      expect(read.runId).toBe("r1");
      expect(read.phaseId).toBe("working");
      expect(read.stream).toBe("source");
      expect(read.pass).toBeTruthy();
    }
  });

  it("GLW-FR-CJBE, GLW-FR-THAX: opening one stage rewrites nothing, and every other stage still opens", async () => {
    const target = run({
      observability: makeObservability({
        stageHistory: [
          { from: "queued", to: "queued", pass: 1, at: "0", reason: "enqueued" },
          { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
        ],
      }),
      logs: makeLogs({
        source: [
          { phaseId: "queued", pass: null },
          { phaseId: "working", pass: 1 },
        ],
      }),
    });
    const before = JSON.stringify(target);
    draw({ run: target, stage: WORKING });
    await waitFor(() => expect(reads()).toHaveLength(1));
    // GLW-FR-THAX: the window holds no source of truth and rewrote nothing of
    // the record it was handed.
    expect(JSON.stringify(target)).toBe(before);
    cleanup();
    // So the other stage's own passes and logs are still there to open.
    draw({ run: target, stage: { id: "queued", label: "Queued" } });
    await waitFor(() => expect(reads()).toHaveLength(2));
    expect(reads()[1].phaseId).toBe("queued");
    expect(reads()[1].pass).toEqual({ kind: "run_level" });
  });

  it("GLW-FR-INLR: the query is sent as typed, and the window offers no regular-expression mode", async () => {
    draw();
    await waitFor(() => expect(reads()).toHaveLength(1));
    await userEvent.type(screen.getByTestId("glw-search"), "GaTe");
    await waitFor(() => expect(reads().some((read) => read.query)).toBe(true));
    const last = reads()[reads().length - 1];
    expect(last.query).toBe("GaTe");
    expect(screen.queryByRole("button", { name: /regular expression|regex/i })).toBeNull();
  });

  it("GLW-FR-KEXN: clearing the search returns the viewport to the unfiltered stream", async () => {
    answer = (args) =>
      makeLogPage({
        status: "available",
        entries: args.query
          ? [makeSourceEntry(2, "matched line")]
          : [makeSourceEntry(1, "every line")],
        matchedTotal: args.query ? 1 : 0,
        search: args.query ? "matched" : "not_requested",
      });
    draw();
    await screen.findByText("every line");
    await userEvent.type(screen.getByTestId("glw-search"), "matched");
    await screen.findByText("matched line");
    await userEvent.clear(screen.getByTestId("glw-search"));
    await waitFor(() => expect(screen.getByText("every line")).toBeInTheDocument());
    expect(screen.queryByTestId("glw-match-count")).not.toBeInTheDocument();
  });

  it("GLW-FR-KDQG, GLW-FR-OGFF: each stream carries its own state, and neither speaks for the other", async () => {
    answer = (args) =>
      args.stream === "source"
        ? makeLogPage({ status: "empty", entries: [] })
        : makeLogPage({
            stream: "structured",
            status: "available",
            entries: [makeStructuredEntry(1, "run.dispatched")],
          });
    draw();
    const empty = await screen.findByTestId("glw-empty");
    // It says what this stream holds and never what the other one holds.
    expect(empty).toHaveTextContent("Source stream");
    expect(empty.textContent).not.toMatch(/Structured stream holds nothing/);
    await userEvent.click(screen.getByRole("button", { name: "Structured" }));
    await waitFor(() =>
      expect(screen.getByText(/run.dispatched/)).toBeInTheDocument(),
    );
    expect(screen.queryByTestId("glw-empty")).not.toBeInTheDocument();
  });

  it("GLW-FR-GJLO, GLW-FR-HCOI: the toggle changes the stream and keeps the selected entry", async () => {
    draw();
    await waitFor(() => expect(reads()).toHaveLength(1));
    await userEvent.click(screen.getByRole("button", { name: "Structured" }));
    await waitFor(() => expect(reads()).toHaveLength(2));
    expect(reads()[1]).toMatchObject({
      stream: "structured",
      pass: { kind: "pass", pass: 2 },
    });
  });

  it("GLW-FR-FPUX, GLW-FR-PHBA: exactly one stream position is selected, in accessible semantics", async () => {
    draw();
    const toggle = await screen.findByTestId("glw-stream-toggle");
    const source = within(toggle).getByRole("button", { name: "Source" });
    const structured = within(toggle).getByRole("button", { name: "Structured" });
    expect(source).toHaveAttribute("aria-pressed", "true");
    expect(structured).toHaveAttribute("aria-pressed", "false");
    await userEvent.click(structured);
    expect(source).toHaveAttribute("aria-pressed", "false");
    expect(structured).toHaveAttribute("aria-pressed", "true");
  });

  it("GLW-FR-FXAL, GLW-FR-SHAF: a source line carries its ten fields and is escaped plain text", async () => {
    answer = () =>
      makeLogPage({
        status: "available",
        entries: [makeSourceEntry(4, "<b>not markup</b>\nsecond line")],
      });
    draw();
    const rows = await screen.findAllByTestId("glw-row");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("<b>not markup</b>");
    expect(rows[0].querySelector("b")).toBeNull();
    // The same ten fields on each line the one chunk decoded to.
    const meta = rows.map((row) => row.querySelector(".glw__meta")?.textContent);
    expect(meta[0]).toBe(meta[1]);
    expect(meta[0]).toContain("stdout");
    expect(meta[0]).toContain("work_turn");
    expect(meta[0]).toContain("#4");
    expect(meta[0]).toContain("r1");
  });

  it("GLW-FR-DDXJ: a run-level row keeps its run-level marker rather than a pass number", async () => {
    answer = (args) =>
      (args.pass as { kind: string }).kind === "run_level"
        ? makeLogPage({
            status: "available",
            entries: [makeSourceEntry(9, "committed", { pass: null, producer: "commit" })],
          })
        : makeLogPage();
    draw();
    await userEvent.click(
      await screen.findByRole("button", { name: "Run-level output of this stage" }),
    );
    const rows = await screen.findAllByTestId("glw-row");
    expect(rows[0]).toHaveAttribute("data-run-level", "true");
    expect(rows[0].querySelector(".glw__meta")?.textContent).toContain("run-level");
  });

  it("GLW-FR-GIWK, GLW-FR-GQQB: the Structured view renders records and no source chunk", async () => {
    answer = (args) =>
      args.stream === "structured"
        ? makeLogPage({
            stream: "structured",
            status: "available",
            entries: [
              makeStructuredEntry(2, "validation.gate_refused", {
                level: "warn",
                fields: { code: "reason_missing" },
              }),
            ],
          })
        : makeLogPage({ status: "available", entries: [makeSourceEntry(1, "raw")] });
    draw();
    expect(await screen.findByText(/raw/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Structured" }));
    await waitFor(() =>
      expect(screen.getByText(/validation.gate_refused/)).toBeInTheDocument(),
    );
    expect(screen.queryByText(/raw/)).not.toBeInTheDocument();
    expect(screen.getByText(/code=reason_missing/)).toBeInTheDocument();
  });

  it("GLW-FR-IMKM, GLW-FR-KBZE: a search names the scope and states the whole scope's count", async () => {
    answer = (args) =>
      makeLogPage({
        status: "available",
        entries: args.query ? [makeSourceEntry(1, "escalate_to_user")] : [],
        matchedTotal: args.query ? 4 : 0,
        search: args.query ? "matched" : "not_requested",
      });
    draw();
    await waitFor(() => expect(reads()).toHaveLength(1));
    await userEvent.type(screen.getByTestId("glw-search"), "escalate");
    await waitFor(() =>
      expect(screen.getByTestId("glw-match-count")).toHaveTextContent("4 matches"),
    );
    const last = reads()[reads().length - 1];
    expect(last).toMatchObject({
      query: "escalate",
      stream: "source",
      pass: { kind: "pass", pass: 2 },
    });
  });

  it("GLW-FR-NMOD, GLW-FR-NYQF: a search with no match is not the empty state", async () => {
    answer = (args) =>
      makeLogPage({
        status: args.query ? "available" : "available",
        entries: [],
        matchedTotal: 0,
        search: args.query ? "search_no_match" : "not_requested",
      });
    draw();
    await userEvent.type(screen.getByTestId("glw-search"), "nothing");
    expect(await screen.findByTestId("glw-no-match")).toBeInTheDocument();
    expect(screen.queryByTestId("glw-empty")).not.toBeInTheDocument();
  });

  it("GLW-FR-NYQF: the empty state names the other stream and never says the run wrote nothing", async () => {
    answer = () => makeLogPage({ status: "empty", entries: [] });
    draw();
    const empty = await screen.findByTestId("glw-empty");
    expect(empty).toHaveTextContent("Source stream");
    expect(empty).toHaveTextContent("Structured stream is read with the toggle");
    expect(empty.textContent).not.toMatch(/run produced no output/i);
  });

  it("GLW-FR-VJNK: an unreadable stream states the typed read failure and how far it got", async () => {
    answer = () =>
      makeLogPage({
        status: "unavailable",
        failure: {
          kind: "read",
          code: "log_stream_corrupt",
          stream: "source",
          message: "damaged",
          at: "2026-09-06T09:00:00Z",
          stoppedSequence: 12,
          byteOffset: 400,
          pendingRecordIds: [],
        },
      });
    draw();
    const state = await screen.findByTestId("glw-unavailable");
    expect(state).toHaveTextContent("log_stream_corrupt");
    expect(state).toHaveTextContent("sequence 12");
    expect(state).toHaveTextContent("run itself is unaffected");
    expect(screen.queryByTestId("glw-persistence-failed")).not.toBeInTheDocument();
    expect(screen.queryByTestId("glw-empty")).not.toBeInTheDocument();
  });

  it("GLW-FR-OATU: a persistence failure says the run stopped and offers no control on it", async () => {
    answer = () =>
      makeLogPage({
        status: "persistence_failed",
        failure: {
          kind: "write",
          code: "log_append_failed",
          stream: "source",
          message: "the stream could not be appended to",
          at: "2026-09-06T09:00:00Z",
          pendingRecordIds: ["rec-1"],
        },
      });
    draw();
    const state = await screen.findByTestId("glw-persistence-failed");
    expect(state).toHaveTextContent("log_append_failed");
    expect(state).toHaveTextContent("Continue retries the writes");
    expect(within(state).queryByRole("button")).toBeNull();
    expect(screen.queryByTestId("glw-unavailable")).not.toBeInTheDocument();
  });

  it("GLW-FR-OJXH: a read that fails leaves the window rendered and says why", async () => {
    answer = () => {
      throw new Error("no_project_open");
    };
    draw();
    expect(await screen.findByTestId("glw-unavailable")).toHaveTextContent(
      "no_project_open",
    );
    expect(screen.getByTestId("graduation-log-window")).toBeInTheDocument();
  });
});
