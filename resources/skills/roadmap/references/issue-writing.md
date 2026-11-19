# Writing an issue

## Title

- Name the outcome in a few words, for example `Refund a card payment`.
- Start a task with a verb. Name an epic or a feature by what it gives the user.
- Do not put the Type, the milestone, or a number in the title.

## Body

Write the body in Markdown, with these parts:

1. **Goal**: one or two sentences about what the work gives and why.
2. **Scope**: what is in the work and, where it helps, what is not.
3. **Acceptance criteria**: a list of checks that are true when the work is done. Each check is one observable result. Where the project uses Gherkin, write the checks as `Given`, `When`, `Then` lines in a fenced block with the language `gherkin`.
4. **References**: the part of the HLD or the specification that the work rests on, by its section title or its requirement ID.

Keep the body short. A task needs less text than a feature.

## Fields

- **Type**: the Type that matches the level: an epic, a feature, or a task, by the names that the repository offers.
- **Milestone**: the milestone in which the work must be done. Leave it out when the author did not decide it and the plan does not show it.
- **Parent**: an epic holds features, and a feature holds tasks. Set `parent_issue` for an existing parent, or `parent_row` for a new parent of the same proposal.
- **Labels**: only labels that exist. Do not add a label only to decorate the issue.

## A change to an issue

- Read the issue first.
- Change only the fields that the request changes.
- A new body replaces the whole body: keep the parts that stay.
