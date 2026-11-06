/**
 * The read-only **Tasks** list of the Create a PR window
 * (`../../../specifications/ui/CPR-create-pull-request.md` CPR-FR-IDRP).
 *
 * It names the GitHub issue of each published draft whose run's work is on the
 * head branch. A draft that is not published is not listed.
 */
import type { TasksRead } from "./reads";

export function TasksList({ read, labelId }: { read: TasksRead; labelId: string }) {
  return (
    <div className="picker-field create-pr__tasks" data-testid="create-pr-tasks">
      <span className="picker-field__label" id={labelId}>
        Tasks
      </span>
      {read.status === "loading" && <p className="t-meta create-pr__line">Reading the tasks…</p>}
      {read.status === "failed" && (
        <p className="t-meta create-pr__line">The tasks could not be read, so no task is linked.</p>
      )}
      {read.status === "ready" && read.tasks.length === 0 && (
        <p className="t-meta create-pr__line" data-testid="create-pr-tasks-empty">
          No task is linked.
        </p>
      )}
      {read.status === "ready" && read.tasks.length > 0 && (
        <ul className="create-pr__task-list" aria-labelledby={labelId}>
          {read.tasks.map((task) => {
            const ref = `${task.issue.owner}/${task.issue.repository}#${task.issue.number}`;
            return (
              <li key={task.draftId} className="create-pr__task" title={ref}>
                <span className="create-pr__task-ref">#{task.issue.number}</span>{" "}
                <span className="create-pr__task-name">{task.draftName}</span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
