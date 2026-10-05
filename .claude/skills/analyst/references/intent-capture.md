# Intent capture

The job of this phase is to leave behind enough context that the implementer and test architect, opening this spec months from now, can answer "why does this exist?" without needing to ask anyone.

The user knows what they want. They often don't volunteer *why* unprompted. Your job is to draw it out.

## What to capture

Not every spec needs all of these, but you should consciously consider each:

- **What this surface or feature is.** A single sentence answering "what is the thing?"
- **Who it's for.** The persona(s) — returning user, new user, collaborator, ops, etc.
- **What problem it solves.** What's currently broken or missing.
- **What it explicitly is *not*.** Non-goals — things the user might assume are in scope but aren't.
- **Where it sits in the surface graph.** What opens it; what it opens; what it replaces.
- **Constraints the user already has in mind.** Performance, persistence, network posture, accessibility.

## How to interview

Default to one or two open questions at a time. Use `AskUserQuestion` when you need a discrete choice; use prose questions in the message when you need narrative.

Good first questions (pick one or two):

- "What's the one-sentence pitch for this surface — what does it let the user do that they can't do today?"
- "Who is this for, and what are they trying to accomplish when they reach for it?"
- "What's explicitly out of scope? What might I assume is included but shouldn't be?"
- "Where does this sit relative to existing surfaces — what opens it, what does it open, does it replace anything?"

After the user answers, look for follow-ups that resolve ambiguity. If the user says "the user can open a project from Git", press on: from which entry point? What happens if the URL is malformed? Is auth in scope?

## What goes where in the spec

| What you learned | Section |
|---|---|
| One-sentence pitch | Intent (first sentence) |
| Problem + persona context | Intent (rest) |
| Discrete personas with motivations | User stories |
| Non-goals | Intent (a short "Out of scope:" sentence stating what the feature deliberately does not do) |
| Constraints | Non-functional requirements |
| Surface graph relationships | Intent + cross-references in FRs |

Closed-off alternatives — the options you considered and ruled out — are worth understanding so you write the right target state, but they do **not** go in the spec. The spec describes what the feature IS, not the path of reasoning that led there. Use what you learn to sharpen the Intent and FRs; don't record the alternatives themselves.

## When to stop interviewing

You've captured enough when:

- You can write a draft Intent paragraph the user would recognize as theirs.
- You can name every persona the surface serves.
- You can articulate at least one non-goal.
- You can describe what triggers and what is triggered by this surface.

If two or three of these still feel thin after a round of questions, do one more round. If they still feel thin, draft anyway and let the user correct from concrete prose — sometimes a draft is the fastest interview.
