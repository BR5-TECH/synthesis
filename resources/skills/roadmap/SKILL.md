---
name: roadmap
description: Plan and change the delivery roadmap and the backlog of a project in a Map discussion. Use it for delivery planning and roadmap alignment - turn a request of the author into proposed rows - new issues, edits to existing issues, re-parenting, milestones, epics, features, tasks, and new changes - and for a gap analysis of the plan against the high-level design (HLD), the breakdown of an epic into issues, and the title, body, and acceptance criteria of an issue.
---

# Roadmap and backlog planning

This skill tells you how to work in a Map discussion. A Map discussion is about the delivery plan of the project: the backlog, the deliverables, and the roadmap. The author uses it to change the plan. Thus the deliverable of the discussion is an **edit to the plan**: new issues, edits to existing issues, and new changes that the author accepts on a proposal card.

## The main rule

When the author asks for an edit to the plan, the edit is a **proposal**. Do not only describe the edit in your comment. Record it with `propose_plan_changes`, so that the author can accept it. Examples:

- "Move the export work to the next milestone" is an edit to the milestone of each export issue.
- "We also need audit logging" is a new issue, or a new epic with its features and tasks.
- "Split issue 12" is new issues under issue 12, and maybe an edit to the body of issue 12.
- "Drop the offline mode" is a close of each offline mode issue as `not_planned`.
- "The refund feature is half done" is a new change that finishes it.

A comment with advice and no proposal is correct only when the author asks a question, asks for an explanation, or asks you to compare options. Then answer, and offer to propose the edit.

When your tools do not include `propose_plan_changes`, you cannot change the plan. Give the edit as advice, and say that you could not record a proposal.

## Procedure

1. Find what the author wants to change. Read `references/intent.md` when you are not sure which rows a request needs.
2. Read the plan before you propose: list or search the plan issues, and read each issue that you will change. Use only the labels, Types, milestones, and Status options that exist.
3. When a request has more than one reading and the difference changes the rows, ask the author before you propose. Do not ask about a thing that you can find in the plan or in the project.
4. Write the rows. Read `references/issue-writing.md` for the title, the body, and the acceptance criteria of a new issue.
5. Put the whole step in one proposal. Read `references/batching.md` for the size of a step, for updates to the proposal in the same turn, and for the turn after the author decided.
6. When the request is about the design or the order of the work, check the plan against the plan references. Read `references/roadmap-alignment.md`.
7. Give a short rationale that says what the rows do and why, with the part of the design that they rest on.

## Reference files

| File | Read it when |
| --- | --- |
| `references/intent.md` | You must find which rows a request of the author needs. |
| `references/issue-writing.md` | You write a new issue or change the body of an issue. |
| `references/batching.md` | You decide how many rows go in one proposal, or you change a proposal. |
| `references/roadmap-alignment.md` | You check the plan against the HLD, the milestones, or the order of the work. |

## Common mistakes

- A comment that says "I would add these issues" and records no proposal.
- A proposal of two or three rows, when the step needs eight.
- A new issue for work that an existing issue already holds. Change that issue instead.
- A label, a Type, a milestone, or a Status that the plan does not have.
- An edit to an issue that you did not read first.
