# Agent UI surfaces

**Spec code:** `AUI`

## Intent
An **agent surface** is a small interactive panel that an agent in a conversation composes from a fixed catalog of components. The author reads the panel in the discussion and answers it with one button. The panel is a comparison table, a checklist, a short form, or a set of choices that plain text and one question do not hold well. Four generic **cards** — a choice, a confirmation, a comparison, and a checklist — are whole panels in one component, so an agent fills a few fields instead of assembling rows and buttons, and the answer each card sends has a fixed shape that this module checks. A surface is a log of A2UI protocol messages, version 1.0: a flat list of components that name their children by id, and a JSON data model that the inputs bind to. The catalog is the official A2UI v1.0 basic catalog with the application's own extension components, so the official A2UI renderer draws every surface and each later feature of the protocol is open to the application. This module holds the catalog, validates each surface against it, stores the surface as a record beside its conversation, serves the media that a surface names, and turns the author's one action into an ordinary comment of the author. Because one catalog serves every conversation agent, a new component makes a new ability for all of them, and no code from an agent runs anywhere. Out of scope: the tool that an agent calls (`../tools/SSF-show-surface-tool.md`); the rendering and the input handling (`../ui/ASF-agent-surfaces.md`); the proposals, the question sets, and the requests that keep their own records and cards; a tool that changes a surface after it is shown, and the function calls between an agent and a renderer; an agent that runs as a command-line process; and A2UI content that an MCP server returns.

## Functional requirements
1. **AUI-FR-CSOS** The **discussion catalog** has the id `synthesis:a2ui/v1`. It is one JSON document, `resources/a2ui/discussion-catalog.json`, compiled into the build, that holds the components and the functions of the official A2UI v1.0 basic catalog at the version the build pins, and the extension components of the contract surface. The backend validates against it and the frontend renders from it.
1. **AUI-FR-HOJP** A record of the catalog id `synthesis:discussion/v1` is read as a record of `synthesis:a2ui/v1`.
2. **AUI-FR-PMXJ** The catalog declares every component of the official basic catalog — `Text`, `Image`, `Icon`, `Video`, `AudioPlayer`, `Row`, `Column`, `List`, `Card`, `Tabs`, `Modal`, `Divider`, `Button`, `TextField`, `CheckBox`, `ChoicePicker`, `Slider`, and `DateTimeInput` — with its official properties, and the extension components `Table`, `ChoiceCard`, `ConfirmCard`, `ComparisonCard`, and `ChecklistCard` with the properties of the contract surface. It declares no other component.
3. **AUI-FR-PXGX** The catalog declares every function of the official basic catalog — `required`, `regex`, `length`, `numeric`, `email`, `formatString`, `formatNumber`, `formatCurrency`, `formatDate`, `pluralize`, `openUrl`, `and`, `or`, and `not` — and the system function `@index`, with their official arguments, and no other function.
3. **AUI-FR-MAMC** The `TextField` of the catalog has no `obscured` variant.
   - *Why:* Every value the author sends is stored in plain text in the conversation and read by the agent, so a field that looks secret would mislead the author.
4. **AUI-FR-PYVB** The catalog holds a short `instructions` text that tells a model the essential rules of a surface and names the built-in `a2ui` skill for the rest (per `DSL-dynamic-skills-loading.md` DSL-FR-HDPD). The text is part of the description of `show_surface` (per `../tools/SSF-show-surface-tool.md` SSF-FR-FKWS).
5. **AUI-FR-EGAB** The internal operation `validate_surface(components, dataModel)` accepts a surface only when every component is one that the catalog declares and conforms to its schema in the catalog, the official A2UI v1.0 common types included: it carries every required property and no property that its catalog entry does not declare.
6. **AUI-FR-GMJK** A valid surface has exactly one component with the id `root`. Each id is unique in the surface, not blank, at most 64 characters, and made of letters, digits, `_`, and `-`.
7. **AUI-FR-AQBE** Every child reference of a valid surface names a component of that surface. Every component is reachable from `root`, and no component is reachable from itself.
8. **AUI-FR-ZEXT** A valid surface holds at most 100 components, and no path from `root` to a component is longer than 8 components.
9. **AUI-FR-IAGR** The components and the data model of a valid surface together serialize to at most 32 KiB of JSON. The data model is a JSON object.
10. **AUI-FR-HCCU** A dynamic value of a valid surface is a literal of the type its property declares, a `{"@path": …}` binding, or a `{"@call": …, "args": {…}}` call of a function that the catalog declares, with the arguments that function declares.
11. **AUI-FR-LXOY** Every `@path` of a valid surface is a JSON Pointer (RFC 6901). A relative pointer, one that does not start with `/`, occurs only inside the template of a `List` or of a `Row` or `Column` whose children are a template. `@index` occurs only there too.
12. **AUI-FR-ZJRQ** The `action` of a `Button` of a valid surface is an `event`, or a `functionCall` of `openUrl` alone. A `functionCall` runs in the renderer and is not an action of the author.
12. **AUI-FR-JLHW** Every `event` of a `Button` of a valid surface has a `name` that matches `[a-z][a-z0-9_]*` and is at most 64 characters. Each `context` key matches `[A-Za-z][A-Za-z0-9_]*` and is at most 64 characters, and each `context` value is a literal or a `{"@path": …}` binding.
    - *Why:* The keys and the name go into a comment that the author's identity signs, so they hold no line break and no other text an agent could pass off as the author's.
13. **AUI-FR-MTMS** A surface that holds an input component (`TextField`, `CheckBox`, `ChoicePicker`, `Slider`, or `DateTimeInput`) and neither a `Button` nor a card is not valid.
    - *Why:* The author can answer a surface only with a button, so an input on a surface with no button collects a value that nobody can send.
13. **AUI-FR-GUUJ** Each literal `url` of an `Image`, a `Video`, or an `AudioPlayer` of a valid surface is an `https` address, a `project:<path>` with a project-relative path, or an `attachment:<digest>`. Each literal `url` of `openUrl` is an `https`, `http`, or `mailto` address. Otherwise the violation is `bad_url`.
13. **AUI-FR-DYVP** The option values of one `ChoicePicker`, `ChoiceCard`, `ComparisonCard`, or `ChecklistCard` are unique within it, and so are the criterion keys of one `ComparisonCard`. A duplicate is the `duplicate_value` violation.
13. **AUI-FR-GITI** The action names of a card match the pattern of an event name (AUI-FR-JLHW). Each key of the `values` of a `ComparisonCard` option is a criterion key of that card.
14. **AUI-FR-VOYL** `validate_surface` returns the first violation it finds as a typed error `{ rule, componentId?, property? }`. `rule` names the requirement that failed, `componentId` names the component where there is one, and `property` names the property where there is one.
15. **AUI-FR-IAAP** An **agent surface** is a record `{ id, discussionId, target, commentId, catalogId, messages, authoredBy, createdAt, state, submission }`. `messages` is the ordered log of A2UI v1.0 messages of the surface. `state` is `open` or `submitted`. `submission` is present on a `submitted` record alone.
15. **AUI-FR-TYBX** The first message of the log is a `createSurface` whose `surfaceId` is the record id, whose `catalogId` is the record's, and which holds the `components` and the `dataModel`. The **current state** of a surface is the fold of its log in order: `updateComponents` replaces or adds components by id, and `updateDataModel` sets the value at its `path`.
15. **AUI-FR-JYKT** A stored record that holds `components` and `dataModel` in place of `messages` is read as a record whose log is one `createSurface` of them.
16. **AUI-FR-DGUP** The record is one JSON file `agent-surfaces/<id>.json` under the store root of the conversation it belongs to, written through `FSA-filesystem-access.md`. `id` is opaque and unique within the project.
17. **AUI-FR-QRCX** The internal operation `record_agent_surface(components, dataModel, message, agent, origin)` validates the surface, writes an `open` record whose log is one `createSurface` (AUI-FR-TYBX), and then appends one comment through `append_as` (per `CMS-comments-storage.md` CMS-FR-26). The comment has the agent's participant, the message as its body, and one `agent_surface` attachment (CMS-FR-RWQQ).
18. **AUI-FR-ZHXO** When the append of `record_agent_surface` fails, the record is removed, and neither the record nor the comment remains. A surface that is not valid writes nothing and appends nothing.
19. **AUI-FR-QBXZ** `get_agent_surface(surfaceId)` returns the record. An unknown id is the typed error `surface_not_found`.
19. **AUI-FR-JUDL** `read_surface_media(surfaceId, source)` returns `{ mediaType, data }`, the bytes as base64, of one media source of the surface. A `project:<path>` source is a regular file inside the project, read through `FSA-filesystem-access.md`, whose extension names an image, a video, or an audio media type. An `attachment:<digest>` source is a `blob` attachment of a comment of the surface's discussion.
19. **AUI-FR-YXMD** `read_surface_media` refuses a source that the current state of the surface does not name as a literal `url`, a path outside the project, a file that is not media, a file over 25 MiB, and a digest that no comment of the discussion holds. Each refusal is a typed error of the contract surface, and nothing is read.
20. **AUI-FR-KBVY** `submit_surface_action(surfaceId, action)` takes an action `{ name, sourceComponentId, context, dataModel }`. It accepts the action only when the record is `open` and `sourceComponentId` names a `Button` of the surface whose event `name` equals `name`, or a card of the surface that declares an action named `name`. Otherwise the error is `already_submitted` or `unknown_action`.
21. **AUI-FR-AFKN** An accepted action of a `Button` carries in `context` exactly the keys that the button's event declares. A literal value equals the declared literal, and a value declared with an absolute `@path` equals the value at that pointer in `dataModel`, or `null` where there is none. A value declared with a relative `@path` reads from the item of a template (per AUI-FR-LXOY). The backend does not know which item the author pressed, so it accepts the value that the author sends for that key, and the author comment carries it. Otherwise the error is `invalid_context`, and nothing changes.
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
- `"read surface media (id, source)"` → `read_surface_media(surfaceId, source)` → `{ mediaType: string, data: string }`. No side effect.
- `"submit surface action (id, action)"` → `submit_surface_action(surfaceId, action: SurfaceAction)` → `{ surface: AgentSurface, commentId: string }`. Side effect: writes the record and appends one comment.

```
AgentSurface {
  id: string,
  discussionId: string,
  target: DiscussionTarget,
  commentId: string,             // the agent comment that carries the surface
  catalogId: "synthesis:a2ui/v1",
  messages: A2uiMessage[],       // the log; the first is createSurface (AUI-FR-TYBX)
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

A2uiMessage =                    // A2UI v1.0 envelopes
  | { version: "1.0", createSurface: { surfaceId, catalogId, components: Component[], dataModel: object } }
  | { version: "1.0", updateComponents: { surfaceId, components: Component[] } }
  | { version: "1.0", updateDataModel: { surfaceId, path?: string, value?: any } }

SurfaceAction {
  name: string,
  sourceComponentId: string,     // the pressed Button, or the card
  context: object,               // each declared key with its resolved value, or the card's context
  dataModel: object              // the author's data model at the press
}
```

Typed errors: `no_project_open`, `surface_not_found`, `already_submitted`, `unknown_action`, `invalid_context`, `discussion_resolved`, `comment_failed`, `write_failed`, `media_not_named`, `media_outside_project`, `media_not_found`, `not_media`, `media_too_large`.

### Events
- `"agent surface changed"` → `AgentSurface` (AUI-FR-WDZG).

### Internal (no Tauri reach)
- `validate_surface(components, dataModel)` → `()` or `SurfaceViolation { rule, componentId?, property? }` (AUI-FR-VOYL).
- `record_agent_surface(components, dataModel, message, agent, origin)` → `AgentSurface`, or a `SurfaceViolation`, or a write or append error.

### The discussion catalog
The catalog is JSON Schema (draft 2020-12) in the A2UI v1.0 catalog format, with the top-level keys `$schema`, `$id`, `catalogId`, `title`, `instructions`, `components`, `functions`, and `$defs`. The components and the functions of the official basic catalog keep their official schemas and the official common types (`ComponentId`, `ChildList`, `DynamicString`, `DynamicNumber`, `DynamicBoolean`, `DynamicStringList`, `DynamicValue`, `DataBinding`, `FunctionCall`, `Action`, `CheckRule`, `Checkable`, and the common component properties). The extension components below use the same common types.

```
Extension components (required marked !):

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

```

## Non-functional requirements
- A record is one small file. Recording a surface costs one record write and one comment append, and an action costs one record write and one comment append.
- Validation reads only the catalog that is compiled into the build. It reaches no network and loads no schema from an address.
- The catalog follows A2UI v1.0 and the pinned version of the official packages. A later version of the protocol is a new catalog id, so a stored record always renders with the catalog it was recorded against.
- `read_surface_media` reads one file of at most 25 MiB and reaches no network. An `https` media address is loaded by the renderer, not by this module.
