# Agent UI surfaces

**Spec code:** `AUI`

## Intent
An **agent surface** is a small interactive panel that an agent in a conversation composes from a fixed catalog of components. The author reads the panel in the discussion and answers it with one button. The panel is a comparison table, a checklist, a short form, or a set of choices that plain text and one question do not hold well. Four generic **cards** — a choice, a confirmation, a comparison, and a checklist — are whole panels in one component, so an agent fills a few fields instead of assembling rows and buttons, and the answer each card sends has a fixed shape that this module checks. A surface uses the declarative shape of the A2UI protocol, version 1.0: a flat list of components that name their children by id, and a JSON data model that the inputs bind to. This module holds the catalog, validates each surface against it, stores the surface as a record beside its conversation, and turns the author's one action into an ordinary comment of the author. Because one catalog serves every conversation agent, a new component makes a new ability for all of them, and no code from an agent runs anywhere. Out of scope: the tool that an agent calls (`../tools/SSF-show-surface-tool.md`); the rendering and the input handling (`../ui/ASF-agent-surfaces.md`); the proposals, the question sets, and the requests that keep their own records and cards; the A2UI messages that change a surface after it is shown and the function calls between an agent and a renderer; an agent that runs as a command-line process; and A2UI content that an MCP server returns.

## Functional requirements
1. **AUI-FR-CSOS** The **discussion catalog** has the id `synthesis:discussion/v1`. It is one JSON document in the application's resources at `resources/a2ui/discussion-catalog.json`, compiled into the build. The backend validates against it and the frontend renders from it.
2. **AUI-FR-PMXJ** The catalog declares the components `Row`, `Column`, `List`, `Card`, `Tabs`, `Divider`, `Text`, `Icon`, `Button`, `TextField`, `CheckBox`, `ChoicePicker`, `Slider`, `DateTimeInput`, and `Table`, the cards `ChoiceCard`, `ConfirmCard`, `ComparisonCard`, and `ChecklistCard`, with the properties of the contract surface, and no other component.
3. **AUI-FR-PXGX** The catalog declares the functions `formatString`, `formatNumber`, `formatDate`, `pluralize`, `required`, `length`, `numeric`, `and`, `or`, and `not`, and the system function `@index`, with the arguments of the contract surface, and no other function.
   - *Why:* A function outside this list either opens an address, as `openUrl` does, or has a dialect that the frontend and the backend can read differently, as `regex` has.
4. **AUI-FR-PYVB** The catalog holds a short `instructions` text that tells a model the essential rules of a surface and names the built-in `a2ui` skill for the rest (per `DSL-dynamic-skills-loading.md` DSL-FR-HDPD). The text is part of the description of `show_surface` (per `../tools/SSF-show-surface-tool.md` SSF-FR-FKWS).
5. **AUI-FR-EGAB** The internal operation `validate_surface(components, dataModel)` accepts a surface only when every component is one that the catalog declares, carries every required property, and carries no property that its catalog entry does not declare.
6. **AUI-FR-GMJK** A valid surface has exactly one component with the id `root`. Each id is unique in the surface, not blank, at most 64 characters, and made of letters, digits, `_`, and `-`.
7. **AUI-FR-AQBE** Every child reference of a valid surface names a component of that surface. Every component is reachable from `root`, and no component is reachable from itself.
8. **AUI-FR-ZEXT** A valid surface holds at most 100 components, and no path from `root` to a component is longer than 8 components.
9. **AUI-FR-IAGR** The components and the data model of a valid surface together serialize to at most 32 KiB of JSON. The data model is a JSON object.
10. **AUI-FR-HCCU** A dynamic value of a valid surface is a literal of the type its property declares, a `{"@path": …}` binding, or a `{"@call": …, "args": {…}}` call of a function that the catalog declares, with the arguments that function declares.
11. **AUI-FR-LXOY** Every `@path` of a valid surface is a JSON Pointer (RFC 6901). A relative pointer, one that does not start with `/`, occurs only inside the template of a `List` or of a `Row` or `Column` whose children are a template. `@index` occurs only there too.
12. **AUI-FR-JLHW** Every `Button` of a valid surface carries an `action` with an `event`. The event `name` matches `[a-z][a-z0-9_]*` and is at most 64 characters. Each `context` key matches `[A-Za-z][A-Za-z0-9_]*` and is at most 64 characters, and each `context` value is a literal or a `{"@path": …}` binding.
    - *Why:* The keys and the name go into a comment that the author's identity signs, so they hold no line break and no other text an agent could pass off as the author's.
13. **AUI-FR-MTMS** A surface that holds an input component (`TextField`, `CheckBox`, `ChoicePicker`, `Slider`, or `DateTimeInput`) and neither a `Button` nor a card is not valid.
    - *Why:* The author can answer a surface only with a button, so an input on a surface with no button collects a value that nobody can send.
13. **AUI-FR-DYVP** The option values of one `ChoicePicker`, `ChoiceCard`, `ComparisonCard`, or `ChecklistCard` are unique within it, and so are the criterion keys of one `ComparisonCard`. A duplicate is the `duplicate_value` violation.
13. **AUI-FR-GITI** The action names of a card match the pattern of an event name (AUI-FR-JLHW). Each key of the `values` of a `ComparisonCard` option is a criterion key of that card.
14. **AUI-FR-VOYL** `validate_surface` returns the first violation it finds as a typed error `{ rule, componentId?, property? }`. `rule` names the requirement that failed, `componentId` names the component where there is one, and `property` names the property where there is one.
15. **AUI-FR-IAAP** An **agent surface** is a record `{ id, discussionId, target, commentId, catalogId, components, dataModel, authoredBy, createdAt, state, submission }`. `state` is `open` or `submitted`. `submission` is present on a `submitted` record alone.
16. **AUI-FR-DGUP** The record is one JSON file `agent-surfaces/<id>.json` under the store root of the conversation it belongs to, written through `FSA-filesystem-access.md`. `id` is opaque and unique within the project.
17. **AUI-FR-QRCX** The internal operation `record_agent_surface(components, dataModel, message, agent, origin)` validates the surface, writes an `open` record, and then appends one comment through `append_as` (per `CMS-comments-storage.md` CMS-FR-26). The comment has the agent's participant, the message as its body, and one `agent_surface` attachment (CMS-FR-RWQQ).
18. **AUI-FR-ZHXO** When the append of `record_agent_surface` fails, the record is removed, and neither the record nor the comment remains. A surface that is not valid writes nothing and appends nothing.
19. **AUI-FR-QBXZ** `get_agent_surface(surfaceId)` returns the record. An unknown id is the typed error `surface_not_found`.
20. **AUI-FR-KBVY** `submit_surface_action(surfaceId, action)` takes an action `{ name, sourceComponentId, context, dataModel }`. It accepts the action only when the record is `open` and `sourceComponentId` names a `Button` of the surface whose event `name` equals `name`, or a card of the surface that declares an action named `name`. Otherwise the error is `already_submitted` or `unknown_action`.
21. **AUI-FR-AFKN** An accepted action of a `Button` carries in `context` exactly the keys that the button's event declares. A literal value equals the declared literal, and a value declared with an absolute `@path` equals the value at that pointer in `dataModel`, or `null` where there is none. Otherwise the error is `invalid_context`, and nothing changes.
21. **AUI-FR-KJPZ** In every accepted action `context` and `dataModel` together serialize to at most 32 KiB, and `dataModel` is a JSON object. Otherwise the error is `invalid_context`, and nothing changes.
21. **AUI-FR-XCXV** An accepted action of a card carries the context of the contract surface for that card. Each sent value is one of the card's option values. A `ChoiceCard` that is not `multiple` sends at most one, and sends at least one unless `allowOther` is set and `other` is not blank. Otherwise the error is `invalid_context`, and nothing changes.
22. **AUI-FR-AIRH** An accepted action sets the record to `submitted` with `submission { name, sourceComponentId, context, dataModel, by, at, commentId }`. `by` is the acting human participant (per `CMS-comments-storage.md` CMS-FR-11).
23. **AUI-FR-MAXD** An accepted action appends one comment through the human write path (per `CMS-comments-storage.md` CMS-FR-11) into the surface's discussion, as a decision on a proposal appends its comment (per `DCP-draft-change-proposals.md` DCP-FR-PDCP). It returns `{ surface, commentId }`.
24. **AUI-FR-XMUS** The body of that comment names the pressed button by the literal text of its `Text` child, or else by the event `name`. Each context key follows on a line of its own as `- <key>: <value>`. A string value is written as it is, and any other value as compact JSON.
24. **AUI-FR-CGDQ** In that body every run of white space in the label and in a value becomes one space. The label is at most 80 characters and a value at most 500, and a longer one ends in `…`.
24. **AUI-FR-WCRS** The body of a card action names the card's button: the `sendLabel` of a `ChoiceCard` or a `ChecklistCard`, or `Send`; the `label` of the pressed `confirm` or `decline` of a `ConfirmCard`; or the `pick.label` of a `ComparisonCard`, or `Pick`. A sent option value is written as the label of its option.
    - *Why:* The comment is prose that a human and a model both read, so it holds no marker that a parser must find again (per `../tools/ADQ-ask-discussion-questions-tool.md` ADQ-FR-NUEB).
25. **AUI-FR-FWQX** When the append of an accepted action fails, the record goes back to `open`, the typed error is `comment_failed`, and no submission remains.
26. **AUI-FR-EGAU** One exclusion per surface holds `submit_surface_action` from the read of the record to its write. Of two actions on one `open` surface, one is accepted and the other is `already_submitted`.
27. **AUI-FR-GTXG** An action on a surface whose discussion is resolved (per `CMS-comments-storage.md` CMS-FR-17) or gone is the typed error `discussion_resolved`, and nothing changes.
28. **AUI-FR-WDZG** Each change of a record emits `"agent surface changed"` with the record: when it is recorded and when it is submitted. A removed record emits nothing.
29. **AUI-FR-XSAO** The module logs under the `backend` domain: an `INFO` record with the surface id, the discussion id, and the component count when a surface is recorded; an `INFO` record with the surface id when an action is accepted; and a `WARN` record with the surface id and the typed error when an action is refused.
29. **AUI-FR-DNZK** No log record of this module carries a component, a text, a context value, the data model, or the message.
30. **AUI-FR-BPDT** With no project open, every operation of this module fails with `no_project_open`.

## Contract surface
The module owns the discussion catalog, the agent surface records, and the Tauri commands and the event below. The UI consumer is `../ui/ASF-agent-surfaces.md`. The tool `../tools/SSF-show-surface-tool.md` uses the internal operations through the module's Rust API.

### Tauri commands
- `"get agent surface (id)"` → `get_agent_surface(surfaceId)` → `AgentSurface`. No side effect.
- `"submit surface action (id, action)"` → `submit_surface_action(surfaceId, action: SurfaceAction)` → `{ surface: AgentSurface, commentId: string }`. Side effect: writes the record and appends one comment.

```
AgentSurface {
  id: string,
  discussionId: string,
  target: DiscussionTarget,
  commentId: string,             // the agent comment that carries the surface
  catalogId: "synthesis:discussion/v1",
  components: Component[],       // as the agent gave them, validated (AUI-FR-EGAB)
  dataModel: object,             // the start values the agent gave; {} when none
  authoredBy: Participant,       // the agent participant
  createdAt: string,             // RFC 3339 UTC
  state: "open" | "submitted",
  submission?: {
    name: string,                // the event name of the pressed button
    sourceComponentId: string,
    context: object,             // the resolved context
    dataModel: object,           // the author's data model when they pressed it
    by: Participant,
    at: string,
    commentId: string            // the author's comment (AUI-FR-MAXD)
  }
}

SurfaceAction {
  name: string,
  sourceComponentId: string,     // the pressed Button, or the card
  context: object,               // each declared key with its resolved value, or the card's context
  dataModel: object              // the author's data model at the press
}
```

Typed errors: `no_project_open`, `surface_not_found`, `already_submitted`, `unknown_action`, `invalid_context`, `discussion_resolved`, `comment_failed`, `write_failed`.

### Events
- `"agent surface changed"` → `AgentSurface` (AUI-FR-WDZG).

### Internal (no Tauri reach)
- `validate_surface(components, dataModel)` → `()` or `SurfaceViolation { rule, componentId?, property? }` (AUI-FR-VOYL).
- `record_agent_surface(components, dataModel, message, agent, origin)` → `AgentSurface`, or a `SurfaceViolation`, or a write or append error.

### The discussion catalog
The catalog is JSON Schema (draft 2020-12) in the A2UI v1.0 catalog format, with the top-level keys `$schema`, `$id`, `catalogId`, `title`, `instructions`, `components`, `functions`, and `$defs`. Each component carries `id` and `component`, and may carry `weight` (a number, for a direct child of a `Row` or a `Column`) and `@accessibility { label?: DynamicString, description?: DynamicString }`. An input component and `Button` may carry `@checks: CheckRule[]`.

```
Component properties (required marked !):

Row            children!: ChildList, justify?: "start"|"center"|"end"|"spaceBetween"|"spaceAround"|"spaceEvenly"|"stretch",
               align?: "start"|"center"|"end"|"stretch"
Column         children!: ChildList, justify?, align?        // the same enums as Row
List           children!: ChildList, direction?: "vertical"|"horizontal", align?
Card           child!: ComponentId
Tabs           tabs!: [{ title!: DynamicString, child!: ComponentId }]   // at least 1
Divider        axis?: "horizontal"|"vertical"
Text           text!: DynamicString, variant?: "body"|"caption"
Icon           name!: IconName
Button         child!: ComponentId, action!: Action, variant?: "default"|"primary"|"borderless"
TextField      label!: DynamicString, value!: DynamicString, placeholder?: DynamicString,
               variant?: "shortText"|"longText"|"number"
CheckBox       label!: DynamicString, value!: DynamicBoolean
ChoicePicker   options!: [{ label!: DynamicString, value!: string }], value!: DynamicStringList,
               label?: DynamicString, variant?: "mutuallyExclusive"|"multipleSelection",
               displayStyle?: "checkbox"|"chips", filterable?: boolean
Slider         max!: number, value!: DynamicNumber, min?: number, steps?: integer ≥ 1, label?: DynamicString
DateTimeInput  value!: DynamicString, label?: DynamicString, enableDate?: boolean, enableTime?: boolean,
               min?: DynamicString, max?: DynamicString                  // ISO 8601
Table          columns!: [{ key!: string, title!: DynamicString }],     // 1 to 12 columns
               rows!: DynamicValue, caption?: DynamicString             // a list of objects; a cell is row[key]

Cards (every text of a card is a literal string):
ChoiceCard     question!: string, options!: [{ value!: string, label!: string, description?: string }],   // 2 to 8
               multiple?: boolean, allowOther?: boolean, sendLabel?: string, action!: { name!: string }
               // context: { selected: string[], other?: string }      // other only with allowOther
ConfirmCard    title!: string, body?: string, confirm!: { label!: string, name!: string },
               decline?: { label!: string, name!: string }, tone?: "default"|"danger"
               // context: {}
ComparisonCard title?: string, criteria!: [{ key!: string, title!: string }],                         // 1 to 8
               options!: [{ value!: string, label!: string, values!: { [key]: string|number|boolean },
                            recommended?: boolean }],                                                 // 2 to 8
               pick?: { name!: string, label?: string }
               // context: { picked: string }
ChecklistCard  title?: string, items!: [{ value!: string, label!: string, description?: string,
                                          checked?: boolean }],                                       // 1 to 30
               sendLabel?: string, action!: { name!: string }
               // context: { checked: string[] }

ComponentId       = string
ChildList         = ComponentId[] | { componentId!: ComponentId, path!: string }   // a static list or a template
DynamicString     = string | DataBinding | FunctionCall
DynamicNumber     = number | DataBinding | FunctionCall
DynamicBoolean    = boolean | DataBinding | FunctionCall
DynamicStringList = string[] | DataBinding | FunctionCall
DynamicValue      = a JSON literal (an object holds no "@path" or "@call" key) | DataBinding | FunctionCall
DataBinding       = { "@path"!: string }
FunctionCall      = { "@call"!: FunctionName, args!: object }
Action            = { event!: { name!: string, context?: { [key]: DynamicValue } } }
CheckRule         = { condition!: DataBinding | FunctionCall, message?: string }
IconName          = the icon names of the A2UI v1.0 basic catalog

Functions:
formatString   value!: DynamicString                     → string   // interpolates ${/pointer}
formatNumber   value!: DynamicNumber, decimals?, grouping? → string
formatDate     value!: DynamicValue, format!: DynamicString → string // Unicode TR35 pattern
pluralize      value!: DynamicNumber, other!, zero?, one?, two?, few?, many? → string
required       value!                                    → boolean
length         value!: DynamicString, min?, max?         → boolean  // at least one of min, max
numeric        value!: DynamicNumber, min?, max?         → boolean  // at least one of min, max
and, or        values!: DynamicBoolean[]                 → boolean  // at least 2
not            value!: DynamicBoolean                    → boolean
@index         offset?: DynamicNumber                    → number   // inside a template alone
```

## Non-functional requirements
- A record is one small file. Recording a surface costs one record write and one comment append, and an action costs one record write and one comment append.
- Validation reads only the catalog that is compiled into the build. It reaches no network and loads no schema from an address.
- The catalog follows A2UI v1.0. A later version of the protocol is a new catalog id, so a stored record always renders with the catalog it was recorded against.
