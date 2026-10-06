/**
 * The frontend's emit side of the session log
 * (`../specifications/ui/LOG-logs.md` LOG-FR-19,
 * `../specifications/core/LGC-logging.md`).
 *
 * One module the whole UI logs through, rather than an `invoke` per emit site.
 * That is what makes LOG-FR-19's two guarantees possible at all:
 *
 * - **Emitting never blocks a render.** Nothing here awaits anything. A record
 *   goes into a pending array and returns; the round trip happens on a timer.
 * - **A batch the backend does not accept is dropped**, rather than retried
 *   indefinitely or surfaced. A failure to record a diagnostic is not itself
 *   worth interrupting the user over, and a retry loop over a failing channel is
 *   how a logging system becomes the outage.
 *
 * Batching is bounded on both axes (LOG non-functional requirements): a flush is
 * due `FLUSH_DELAY_MS` after the first pending record, or immediately once
 * `MAX_BATCH` have accumulated, so a render loop that emits per frame costs one
 * round trip per flush rather than one per record.
 *
 * **No secret reaches a record** (LOG-FR-20). Neither this module nor the
 * backend inspects or masks what it is given (LGC-FR-16), so the emit site is
 * the only place that is enforced — put a value a message should not name into
 * `fields`, and put no token, key, or authorization header into either.
 */
import { appendLogRecords } from "./api";
import type { LogDomain, LogFields, LogInput, LogLevel } from "./types";

/** How long a pending batch waits for company before it is flushed. */
export const FLUSH_DELAY_MS = 200;

/**
 * The size at which a batch flushes without waiting out the timer, so a burst
 * is bounded by count as well as by time.
 */
export const MAX_BATCH = 64;

let pending: LogInput[] = [];
let timer: ReturnType<typeof setTimeout> | null = null;

/**
 * Hand the pending batch to the backend and clear it.
 *
 * The batch is taken *before* the call, so records emitted while the round trip
 * is in flight accumulate into the next batch rather than being lost to this
 * one's `catch`. A rejection, a synchronous throw, and a return that is not a
 * promise are all dropped on purpose (LOG-FR-19).
 */
function flush(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  if (pending.length === 0) return;
  const batch = pending;
  pending = [];
  // This function runs from a timer. A throw here has no caller to catch it, so
  // it would become an uncaught exception. Every failure is dropped and not
  // retried. See the module comment.
  try {
    void Promise.resolve(appendLogRecords(batch)).catch(() => {});
  } catch {
    // Dropped.
  }
}

/** Schedule a flush if one is not already due. */
function schedule(): void {
  if (pending.length >= MAX_BATCH) {
    flush();
    return;
  }
  if (timer !== null) return;
  timer = setTimeout(flush, FLUSH_DELAY_MS);
}

/**
 * Emit one record. Returns immediately, whatever the backend is doing.
 *
 * `domains` must be non-empty: a record the backend cannot attribute is
 * rejected there (LGC-FR-03), so an empty set is dropped here instead of making
 * a round trip that discards it.
 */
export function log(
  level: LogLevel,
  domains: LogDomain[],
  message: string,
  fields: LogFields = {},
): void {
  if (domains.length === 0) return;
  pending.push({
    // LGC-FR-06: the instant the record was emitted, not the instant the batch
    // reached the backend — which is the whole reason the caller supplies it.
    ts: new Date().toISOString(),
    level,
    domains,
    message,
    fields,
  });
  schedule();
}

/** `log` at DEBUG. */
export const logDebug = (
  domains: LogDomain[],
  message: string,
  fields?: LogFields,
): void => log("DEBUG", domains, message, fields);

/** `log` at INFO. */
export const logInfo = (
  domains: LogDomain[],
  message: string,
  fields?: LogFields,
): void => log("INFO", domains, message, fields);

/** `log` at WARN. */
export const logWarn = (
  domains: LogDomain[],
  message: string,
  fields?: LogFields,
): void => log("WARN", domains, message, fields);

/** `log` at ERROR. */
export const logError = (
  domains: LogDomain[],
  message: string,
  fields?: LogFields,
): void => log("ERROR", domains, message, fields);

/**
 * Flush anything pending right now.
 *
 * For the paths that end the window's life — a project close, a quit — where
 * the timer would otherwise never fire and the last records of the session
 * would be the ones explaining why it ended.
 */
export const flushLogs = (): void => flush();

/** Discard anything pending without sending it. For tests. */
export function resetLogBufferForTest(): void {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  pending = [];
}
