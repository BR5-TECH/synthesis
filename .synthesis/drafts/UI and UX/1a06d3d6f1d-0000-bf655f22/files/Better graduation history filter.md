## Intent

Improve the graduation history filter so queued runs are visible with other active work and the default view shows work that may need attention.

## Requirements

- Replace the separate `Queued` and `In-flight` filters with one `In-flight` filter.
- The `In-flight` filter includes all runs currently admitted by the existing `In-flight` filter and all runs currently admitted by the existing `Queued` filter (`queued` and `implementation_queued`).
- Keep `Completed` limited to completed runs, and keep `Archived` limited to archived runs, including the existing archived-run exception already defined by the graduation history specification.
- Provide exactly four filters, in this order: `In-flight`, `Completed`, `Archived`, `All`.
- Make `In-flight` the default filter when the panel has no active filter state for the current project in the current application session.
- On a fresh application session, keep `In-flight` selected even when the selection fallback in `GRH-FR-60` selects a completed or archived run. Do not relax the filter to `All` or `Archived` only to reveal that fallback selection; the rail may show the corresponding empty result until the author changes the filter.
- Preserve the existing filter behavior in all other respects, including exclusive selection, text-filter combination, empty-result handling, per-project in-memory session behavior, and reset behavior after application relaunch.

## Scope and specification impact

- Update `specifications/ui/GRH-graduation-history.md` to replace the five-position filter model with the four-position model above.
- Update all related requirements, user stories, UI contract text, wireframes, and default-state references in that specification, including the fresh-session default in `GRH-FR-58` and any selection logic that refers to the filter positions.
- Update the `GRH-FR-60` fresh-session behavior so its completed or archived fallback selection does not override the required `In-flight` default. Preserve filter relaxation when an already remembered or explicitly named run must be revealed, unless the updated specification defines otherwise.
- Keep filter state scoped to the open project, as currently defined by `GRH-FR-58`; do not make it panel-wide or shared between projects during the application session.
- Do not change the backend run states, run records, queue behavior, archive behavior, or the meaning of `Completed` and `Archived` beyond the filter membership described above.
- Add or update frontend coverage for filter order, the default `In-flight` selection, merged queued membership, the completed/archived fallback case on a fresh session, per-project session-state isolation, and the unchanged behavior of text filtering and archived runs.
