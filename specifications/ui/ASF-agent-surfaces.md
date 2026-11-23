# Agent surfaces

**Spec code:** `ASF`

## Intent
How an agent surface appears in a discussion and how the author answers it. An agent that has shown a surface (per `../tools/SSF-show-surface-tool.md`) waits on the author. The surface is a small panel below the agent's comment, made of the discussion catalog's components (`../core/AUI-agent-ui-surfaces.md`): text, tables, checkboxes, choices, fields, and buttons, or one of its four cards — a choice, a confirmation, a comparison, or a checklist. The author fills it and presses one button. The panel then locks and shows what was sent, and the author's comment with the values brings the agent back. One renderer draws every surface in every discussion surface, with the application's own controls and look, so a surface reads as part of the IDE and not as a page from somewhere else. Out of scope: the record, the catalog, and the validation (`../core/AUI-agent-ui-surfaces.md`); the tool (`../tools/SSF-show-surface-tool.md`); answering a surface by voice; and every proposal card, request card, and question-set block, which keep their own surfaces.

## Functional requirements
1. **ASF-FR-ASQV** A comment with an `agent_surface` attachment (per `../core/CMS-comments-storage.md` CMS-FR-RWQQ) shows its **surface panel** under its body, in every discussion surface variant that renders the comment (per `CMT-comments.md` CMT-FR-DBQC). The panel reads its record through `"get agent surface (id)"`.
2. **ASF-FR-HRQL** One renderer draws the panel in every variant. It draws each component of the record with the application's own controls, type, spacing, and colour variables. It draws no component that the discussion catalog does not declare.
3. **ASF-FR-SERF** A component whose type the renderer does not know draws the line "This part cannot be shown." in its place, and the rest of the panel draws as usual.
4. **ASF-FR-RRXP** While the record loads, the panel shows a placeholder of one line. A read that fails shows "This panel could not be loaded." with a **Retry** button that reads the record again.
5. **ASF-FR-ENRG** The panel holds a **local data model** that starts as a copy of the record's `dataModel`. A value bound with `{"@path": …}` reads from it, and an input writes to it at once. Every component bound to the same path shows the new value at once.
6. **ASF-FR-JLUJ** The local data model is held in memory per surface id. It stays when the owner surface of the discussion changes (per `CVP-conversation-presentation.md` CVP-FR-05), and it does not survive a relaunch.
7. **ASF-FR-GHUZ** The renderer evaluates the functions of the catalog itself. A call that fails, or that names a function the catalog does not declare, gives an empty value, and the panel still draws.
8. **ASF-FR-TURS** A `List`, `Row`, or `Column` whose children are a template draws the template once for each item of the list at its path, in list order. A relative path in the template reads from its item, and `@index` gives the item's index.
9. **ASF-FR-GZFO** A `Table` draws a table with one header cell for each column and one row for each object of `rows`. A cell shows the value of its column's `key` as text. A wide table scrolls horizontally inside the panel.
10. **ASF-FR-GKYM** A `Button` whose `@checks` hold a condition that is false is disabled, and the `message` of each false check shows under the button. The `@checks` of an input show their messages under that input.
11. **ASF-FR-QBTR** Pressing an enabled button resolves its event `context`: a `@path` value reads from the local data model, and a literal stays as it is. The panel then invokes `"submit surface action (id, action)"` once, with the event name, the button's id, the context, and the local data model.
12. **ASF-FR-XOSG** While a submission is in flight, every button and every input of the panel is disabled.
13. **ASF-FR-YCTE** A `submitted` record draws the panel **read-only**: no input takes input, no button is enabled, and every bound value reads from `submission.dataModel`. Above the panel's foot a line names the pressed button, the author who pressed it, and when.
14. **ASF-FR-CUEI** The panel follows `"agent surface changed"` for its surface and draws the record that the event carries.
15. **ASF-FR-NLJW** A submission that returns `already_submitted` reads the record again and draws it. Any other typed error shows its message under the panel, and the buttons and inputs take input again.
16. **ASF-FR-JSTI** A panel in a resolved discussion draws read-only, as a `submitted` panel does, with no submission line where the record is `open`.
17. **ASF-FR-DHGO** A submission that returns a `commentId` brings the agents back on the terms that a decision on a proposal does (per `DCR-draft-change-review.md` DCR-FR-15, `CTA-comment-agent-turns.md` CTA-FR-SSUZ).
17. **ASF-FR-EKZW** A `ChoiceCard` draws its question, then one row for each option with its label and its description, as radio buttons, or as checkboxes where it is `multiple`. With `allowOther` it draws one more row with a text field for the author's own answer. Its Send button is disabled until an option is chosen or the own answer is not blank.
17. **ASF-FR-IBTY** A `ConfirmCard` draws its title, its body, a primary button with the `confirm` label, and a button with the `decline` label where it has one. With the `danger` tone the confirm button is the danger button of the application.
17. **ASF-FR-EOLY** A `ComparisonCard` draws a table with one header cell for each criterion and one row for each option. The row of a `recommended` option carries the word "Recommended" beside its label. With `pick` each row has a radio button, and the Pick button is disabled until a row is chosen.
17. **ASF-FR-MXGW** A `ChecklistCard` draws its title and one checkbox for each item with its label and its description, checked where the item is `checked`. Its Send button sends the checked values and is enabled with none checked.
17. **ASF-FR-MLKG** A card sends the context of the contract surface of `../core/AUI-agent-ui-surfaces.md` for its kind. It keeps its selection for as long as the local data model is kept (ASF-FR-JLUJ), and a submitted card shows the sent selection read-only.
18. **ASF-FR-TAVD** Every input has an accessible name from its `label`, or from its `@accessibility.label` where it has no label. A `Table` is a table with header cells. `Tabs` follow the WAI-ARIA tabs pattern. Every control of the panel is reachable and operable from the keyboard.
19. **ASF-FR-JPNA** The panel fills the width of the comment it belongs to and has no fixed height. A `Card` draws as a bordered block with the panel's inner spacing.
20. **ASF-FR-MGVD** The panel logs under the `frontend` domain: a `WARN` record with the surface id when its record does not read, and a `WARN` record with the surface id and the typed error when a submission fails. No record carries a component, a text, a context value, or the data model.

## UI contract boundary
- **Owned by the UI**: the surface panel, its renderer, and the drawing of each catalog component; the local data model and its bindings; the evaluation of the catalog's functions and checks; the resolution of an action's context; the enablement of the buttons and the inputs; the read-only state; the error text; and the dispatch of agent turns after a submission.
- **Delegated to backend (abstract)**:
  - `"get agent surface (id)"`, `"submit surface action (id, action)"`, and the event `"agent surface changed"` — owned by `../core/AUI-agent-ui-surfaces.md`.
  - `"dispatch agent turn"` — owned by `../core/AGC-agent-conversations.md` — brings the agents back after a submission (ASF-FR-DHGO).

## Non-functional requirements
- The renderer reads the catalog from the same JSON document that the backend validates against, so the two never declare different components.
- The renderer fetches nothing. No component loads an address, and no text renders as HTML.
- A panel of 100 components draws without a delay that the author can see.
