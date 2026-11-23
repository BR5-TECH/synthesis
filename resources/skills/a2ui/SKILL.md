---
name: a2ui
description: Show the author an interactive panel in a conversation with the show_surface tool - a choice with options, a yes or no confirmation, a comparison of options against criteria, a checklist, a short form, or a table. The panel uses the A2UI v1.0 format and the discussion catalog of the application.
---

# Panels (A2UI)

This skill tells you how to show the author a panel with the `show_surface` tool. A panel is a small interactive part of the conversation. The author fills it and presses one button. The panel then locks, and the button and its values come back to you as the author's comment. Your turn ends when the panel is posted.

## When to show a panel

- Show a panel when the author must choose between options, confirm a step, compare options side by side, select items from a list, or give you several values at once.
- Do not show a panel for one open question. Ask it in your reply, or with `ask_user_comment`.
- Do not show a panel to propose a change to a draft, a prompt, a change, or a plan. Use the proposal tool of that material.
- Show one panel for one purpose. Keep it small.

## Procedure

1. Find the case in the table below. Use a card where one fits: a card is a whole panel in one component.
2. Read the reference file of that case. Load it with the path in the table, for example `references/cards.md`.
3. Write the panel as `components`, a flat list. Give the start values in `data_model`.
4. Write `message`: one or two sentences that tell the author what the panel is for, with one `<spoken>…</spoken>` tag.
5. Check the panel against the rules below. A panel that breaks a rule is refused, and the refusal names the component and the property to fix.

| Case | Use | Reference file |
| --- | --- | --- |
| A question with 2 to 8 options | `ChoiceCard` | `references/cards.md` |
| A yes or no decision | `ConfirmCard` | `references/cards.md` |
| Options against criteria | `ComparisonCard` | `references/cards.md` |
| Items the author keeps or drops | `ChecklistCard` | `references/cards.md` |
| A form, a table, tabs, or any other layout | the components | `references/components.md` |
| Complete panels to start from | examples | `references/examples.md` |

## Rules

- Each component has a unique `id` of letters, digits, `_`, and `-`, and a `component` type of the catalog.
- Exactly one component has the id `root`. Every other component is reachable from `root`.
- A container names its children by id: `children` is a list of ids, `child` is one id.
- A panel holds at most 100 components and is at most 8 components deep.
- An input needs a `Button` or a card on the same panel. A `Button` has `action.event.name` (lowercase letters, digits, `_`) and a `context` with the values it sends.
- Bind a value with `{"@path": "/pointer"}`. Join text with `formatString`, not with `+`.
- Text is plain text. Markdown, HTML, images, and links do not render.
- The values the author sends are not secret. Do not ask for a password, a token, or a key in a panel.
