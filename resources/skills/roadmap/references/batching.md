# Steps, proposals, and turns

## One step in one proposal

- A **step** is every row that belongs together: an epic with its features and tasks, every change that one part of the design needs, or every issue that moves to one milestone.
- Put the whole step in one proposal, up to 20 rows. A step of 5 to 10 rows is normal.
- Do not split one step into several proposals of a few rows. The author decides one card faster than three.
- Do not put the work of later steps in the same proposal. When the plan needs more than one step, propose the first and say what comes next.
- When one row is refused, the tool tells you each refused row at once. Correct them all and send the call again.

## In the same turn

- Your turn goes on after you record a proposal. You can read more issues and call `propose_plan_changes` again with `updates` to change the same proposal: `change` a row, `add` a row, or `withdraw` a row.
- Only the first call of a turn posts its rationale. Later calls post nothing.
- End the turn with a short reply. It is posted only where you changed the proposal after its rationale, so then say in it what you changed.
- In one turn you post one proposal. You cannot record a second one, and you cannot ask the author a question after it. Put a question in your reply instead.

## While the card waits

- While a row of your proposal waits, you cannot record a new proposal.
- When the author asks for a different change, send `updates` to the waiting proposal.
- A rejected row can be changed with `change`, and the author decides it again. An accepted or failed row cannot be changed.

## After the author decided

- When the author decided every row and accepted one or more, you get a comment of Synthesis with the results. It names each new issue with its number.
- Then propose the next step that depends on these results, for example the tasks under a new epic, with `parent_issue` set to its new number.
- You do not have to propose. When the step is done, say so in one or two sentences, and stop.
