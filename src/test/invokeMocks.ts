/**
 * Shared parts of the `invoke` mocks that route by command name.
 *
 * Read `.claude/skills/engineer/references/conventions.md` ("Tests that pass on
 * a slow runner") for the rule this file applies.
 */
import type { DiscussionContext } from "../types";

/** The commands that the app sends in the background, which no test is about. */
const BACKGROUND_COMMANDS = new Set([
  "append_log_records",
  // WTS-FR-BIOJ: the read of a linked worktree's health.
  "active_root_state",
  // WSS-FR-RPYH: the read of an open dropdown's notices.
  "list_removed_work_streams",
]);

/**
 * The answer to a command that the mock does not route.
 *
 * The log flush (`../logging`) sends `append_log_records` from a timer, and on
 * a slow runner it can do so after the test that emitted the record. Every
 * other command rejects rather than throws: a synchronous throw from a timer
 * callback is an uncaught exception that no test owns, and it fails the run
 * when every test passes.
 */
export function otherCommand(cmd: string): Promise<unknown> {
  if (BACKGROUND_COMMANDS.has(cmd)) return Promise.resolve();
  return Promise.reject(new Error(`unexpected command ${cmd}`));
}

/**
 * DDS-FR-KXWQ: an answer to `estimate_discussion_context`, the read that the
 * discussion column sends when it mounts. A mock for a test that is not about
 * the context indicator answers with this, so the column logs no failure and
 * starts no log flush that can reach the next test.
 */
export const CONTEXT_ESTIMATE: DiscussionContext = {
  window: 1_000_000,
  estimatedTokens: 0,
  split: { instructions: 0, document: 0, history: 0, currentComment: 0, toolResults: 0 },
  document: null,
  limitTokens: 300_000,
  aboveLimit: false,
};
