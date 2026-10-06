/**
 * Shared parts of the `invoke` mocks that route by command name.
 *
 * Read `.claude/skills/engineer/references/conventions.md` ("Tests that pass on
 * a slow runner") for the rule this file applies.
 */

/** The commands that the app sends in the background, which no test is about. */
const BACKGROUND_COMMANDS = new Set(["append_log_records"]);

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
