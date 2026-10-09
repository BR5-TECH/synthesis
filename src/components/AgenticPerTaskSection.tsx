import { Fragment, type ReactNode } from "react";
import { AGENTIC_TURN_KINDS } from "../types";

/**
 * An agentic tab's single per-task section — AII-FR-QDLW / AII-FR-PMZK.
 *
 * The tab holds **one** of these, below both default selectors rather than
 * under either one of them: the two defaults are the pair of values the backend
 * runs on, and this is where a single kind of work departs from that pair. Each
 * row is one kind of work followed by its own model selector and its own effort
 * selector on one line, so what a turn resolves is read across a row rather than
 * matched between two lists.
 *
 * The two selectors of a row are independent of each other and of every other
 * row. Nothing here couples them: each renders from its own override map and
 * invokes its own operation, so returning one to "same as default" returns that
 * kind to one default and never to two.
 *
 * The summary counts **rows**, not selectors (AII-FR-PMZK) — a kind of work that
 * overrides its model, its effort, or both counts once — so the line answers
 * "how many kinds of work are configured apart?" rather than "how many controls
 * were touched?".
 *
 * The rows are in the order a run reaches them and are labelled for the author
 * rather than by the identifier each carries — the last of them is the
 * semantic-rebase turn under the name the rest of the application gives it.
 */
export function PerTaskSection({
  modelOverrides,
  effortOverrides,
  hasEffortColumn,
  renderModel,
  renderEffort,
}: {
  /**
   * The turn kinds the model selection differs on. Never null-valued: a kind
   * that follows the default is absent from it rather than mapped to null.
   */
  modelOverrides: Record<string, string>;
  /** The same, for the effort selection. */
  effortOverrides: Record<string, string>;
  /**
   * Whether the vendor declares effort levels at all. A vendor that declares
   * none renders no effort column and no disabled control in its place
   * (AII-FR-24), and a stored effort override it cannot show is not counted —
   * a summary must never report a difference the author has no control for.
   */
  hasEffortColumn: boolean;
  renderModel: (kind: (typeof AGENTIC_TURN_KINDS)[number]) => ReactNode;
  renderEffort: (kind: (typeof AGENTIC_TURN_KINDS)[number]) => ReactNode;
}) {
  const total = AGENTIC_TURN_KINDS.length;
  const differing = AGENTIC_TURN_KINDS.filter(
    (kind) =>
      modelOverrides[kind.id] !== undefined ||
      (hasEffortColumn && effortOverrides[kind.id] !== undefined),
  ).length;
  return (
    <details className="agentic-per-task" data-testid="agentic-per-task">
      {/* The summary counts rather than names, so a backend configured one way
          for everything reads as the two defaults and one closed line. */}
      <summary data-testid="agentic-per-task-summary">
        Per task ·{" "}
        {differing === 0
          ? `${total} follow the defaults`
          : `${differing} of ${total} ${
              differing === 1 ? "differs" : "differ"
            } from the defaults`}
      </summary>
      <div
        className={`agentic-per-task__grid${
          hasEffortColumn ? "" : " agentic-per-task__grid--no-effort"
        }`}
      >
        {/* The column headings name the default each column follows, so a
            column reads down as one dimension and a row reads across as one
            kind of work. The first cell is the task-label column's own. */}
        <span />
        <span className="agentic-per-task__heading">Model</span>
        {hasEffortColumn && (
          <span className="agentic-per-task__heading">Effort</span>
        )}
        {AGENTIC_TURN_KINDS.map((kind) => (
          <Fragment key={kind.id}>
            <span className="agentic-per-task__task">{kind.label}</span>
            <div className="agentic-per-task__cell">{renderModel(kind)}</div>
            {hasEffortColumn && (
              <div className="agentic-per-task__cell">{renderEffort(kind)}</div>
            )}
          </Fragment>
        ))}
      </div>
    </details>
  );
}
