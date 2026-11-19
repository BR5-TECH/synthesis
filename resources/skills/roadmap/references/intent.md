# From a request to rows

A request of the author names an outcome. Find the rows that make that outcome true in the plan. Read the plan issues first, so that you change an existing issue where one holds the work, and you create an issue only where none does.

## Requests and the rows that they need

| The author says | The rows |
| --- | --- |
| "Add X", "we also need X", "do not forget X" | `create_issue` for X. A large X is an epic with its features and tasks, with `parent_row` from each child to its parent. |
| "Break down X", "split X", "plan X in detail" | `create_issue` rows with `parent_issue` set to X. Change the body of X where it must say what moved to the children. |
| "Move X to milestone M", "do X first", "X can wait" | `modify_issue` with `milestone` for X and for each child of X that must move with it. |
| "X belongs under Y", "group these under a new epic" | `modify_issue` with `parent_issue` set to Y. For a new epic, a `create_issue` row for the epic and `modify_issue` rows with `parent_row` set to the key of that row. |
| "Rename X", "X must also cover Z" | `modify_issue` with `title` or `body`. Give the full new body, not only the added part. |
| "Drop X", "we will not do X" | `modify_issue` with `close_reason` `not_planned`. |
| "X is done" | `modify_issue` with `close_reason` `completed`, or with the `status` that means done. |
| "Start X", "X is blocked" | `modify_issue` with the matching `status`. |
| "Label X as Y" | `modify_issue` with `labels_add` or `labels_remove`. Use only labels that exist. |
| "Feature X exists but is not finished" | `new_draft` that finishes X, with the full prompt in `body`. |

## When the request is not clear

- A request with one clear reading: propose.
- A request with two readings that give different rows, for example "move the payments work" when two milestones could take it: ask the author, with the two options.
- A request that names an issue you cannot find: search the plan with other words, then ask.

Do not ask when you can decide from the plan, the plan references, or the specifications.
