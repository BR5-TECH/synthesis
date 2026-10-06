/**
 * Shared set-up for the graduation log window tests
 * (`../../specifications/ui/GLW-graduation-log-window.md`).
 *
 * A test file that uses this module must mock `@tauri-apps/api/core` and
 * `@tauri-apps/api/event` with `vi.fn`. The mock of `invoke` here routes by
 * command name. It answers `append_log_records` and rejects every command it
 * does not expect, so a background call never throws in a timer.
 */
import { act, fireEvent, render } from "@testing-library/react";
import { vi } from "vitest";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { GRADUATION_LOG_RECORDS_APPENDED } from "../events";
import { resetLogBufferForTest } from "../logging";
import type { ProgressStage } from "../state/runProgress";
import type { GraduationLogPage, GraduationRun } from "../types";
import { GraduationLogWindow } from "../components/GraduationLogWindow";
import {
  makeLogPage,
  makeLogs,
  makeObservability,
  makeRun,
  pageForRead,
} from "./graduationFixtures";

export const WORKING: ProgressStage = { id: "working", label: "Working" };
export const REVIEW: ProgressStage = { id: "review", label: "Review" };

export type Answer = (
  args: Record<string, unknown>,
) => GraduationLogPage | Promise<GraduationLogPage>;

export interface LogBackend {
  /** What the backend answers for one read. */
  answer: Answer;
  /** True where a test hands the page over exactly as it built it. */
  raw: boolean;
  /** The arguments of every `read_graduation_logs` the test provoked. */
  reads(): Array<Record<string, unknown>>;
  /** Every command the test provoked, in order. */
  commands(): string[];
  /** Tell the window that a stream of a run grew, as the backend does. */
  appended(payload: { runId: string; stream: string; latestSequence: number }): Promise<void>;
  /** How many listeners of the appended event are registered now. */
  listeners(): number;
}

/**
 * Reset every mock and every module store the window touches, and route
 * `invoke` and `listen` by name. Call it in `beforeEach`.
 */
export function installLogBackend(): LogBackend {
  const invoked = vi.mocked(invoke);
  const listened = vi.mocked(listen);
  invoked.mockReset();
  listened.mockReset();
  resetLogBufferForTest();

  const handlers = new Set<(event: { payload: unknown }) => void>();
  const backend: LogBackend = {
    answer: () => makeLogPage(),
    raw: false,
    reads: () =>
      invoked.mock.calls
        .filter(([command]) => command === "read_graduation_logs")
        .map(([, args]) => args as Record<string, unknown>),
    commands: () => invoked.mock.calls.map(([command]) => command as string),
    appended: async (payload) => {
      await act(async () => {
        for (const handler of [...handlers]) handler({ payload });
      });
    },
    listeners: () => handlers.size,
  };

  listened.mockImplementation((async (name: string, handler: unknown) => {
    if (name !== GRADUATION_LOG_RECORDS_APPENDED) return () => {};
    const typed = handler as (event: { payload: unknown }) => void;
    handlers.add(typed);
    return () => {
      handlers.delete(typed);
    };
  }) as never);

  invoked.mockImplementation((async (command: string, args?: unknown) => {
    if (command === "append_log_records") return undefined;
    if (command === "read_graduation_logs") {
      const request = (args ?? {}) as Record<string, unknown>;
      const page = await backend.answer(request);
      return backend.raw ? page : pageForRead(request, page);
    }
    throw new Error(`unexpected command ${command}`);
  }) as never);
  return backend;
}

/** A run whose `working` phase ran passes 1 and 2 and also wrote run-level rows. */
export function workingRun(over: Parameters<typeof makeRun>[2] = {}): GraduationRun {
  return makeRun("r1", "working", {
    observability: makeObservability({
      stageHistory: [
        { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
        { from: "review", to: "working", pass: 2, at: "2", reason: "review_revision" },
      ],
    }),
    logs: makeLogs({
      activity: [
        { phaseId: "working", pass: 1 },
        { phaseId: "working", pass: 2 },
        { phaseId: "working", pass: null },
      ],
      structured: [{ phaseId: "working", pass: 1 }],
    }),
    ...over,
  });
}

/**
 * A run that went work, review, back to work, review, and back to work again.
 * It stands in the working phase, and `review` holds passes 1 and 2.
 */
export function revisedRun(over: Parameters<typeof makeRun>[2] = {}): GraduationRun {
  return makeRun("r1", "working", {
    observability: makeObservability({
      currentStage: "working",
      stageHistory: [
        { from: "queued", to: "working", pass: 1, at: "1", reason: "work_started" },
        { from: "working", to: "review", pass: 1, at: "2", reason: "review_started" },
        { from: "review", to: "working", pass: 2, at: "3", reason: "review_revision" },
        { from: "working", to: "review", pass: 2, at: "4", reason: "review_started" },
        { from: "review", to: "working", pass: 3, at: "5", reason: "review_revision" },
      ],
    }),
    logs: makeLogs({
      activity: [
        { phaseId: "working", pass: 1 },
        { phaseId: "review", pass: 1 },
        { phaseId: "working", pass: 2 },
        { phaseId: "working", pass: 3 },
      ],
    }),
    ...over,
  });
}

export function draw(
  props: Partial<Parameters<typeof GraduationLogWindow>[0]> = {},
) {
  const onClose = props.onClose ?? vi.fn();
  const view = render(
    <GraduationLogWindow
      run={props.run ?? workingRun()}
      stage={props.stage ?? WORKING}
      onClose={onClose}
      returnFocus={props.returnFocus ?? null}
    />,
  );
  return { onClose, ...view };
}

/** The pass number a read asked for, or null for the run-level scope. */
export function passOf(args: Record<string, unknown>): number | null {
  const scope = args.pass as { kind: string; pass?: number };
  return scope.kind === "pass" ? (scope.pass ?? null) : null;
}

/** The labels of the entries of the pass list, in order. */
export function entryNames(list: HTMLElement): string[] {
  return Array.from(list.querySelectorAll("button")).map(
    (button) => button.getAttribute("aria-label") ?? "",
  );
}

/**
 * jsdom lays nothing out, so a test gives the viewport the measures it needs.
 * `top` is the scroll offset the reader stands at.
 */
export function measure(
  viewport: HTMLElement,
  sizes: { height: number | (() => number); client?: number },
): void {
  Object.defineProperty(viewport, "scrollHeight", {
    get: () => (typeof sizes.height === "function" ? sizes.height() : sizes.height),
    configurable: true,
  });
  Object.defineProperty(viewport, "clientHeight", {
    value: sizes.client ?? 100,
    configurable: true,
  });
}

/** Move the reader to `top` and let the window see the scroll. */
export function scrollTo(viewport: HTMLElement, top: number): void {
  viewport.scrollTop = top;
  fireEvent.scroll(viewport);
}
