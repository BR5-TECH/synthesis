# Show surface tool

**Spec code:** `SSF`

## Intent
The tool that an agent uses to show the author a small interactive panel in the conversation: a table that compares options, a checklist, a short form, or a set of choices. The agent composes the panel from the components of the discussion catalog (`../core/AUI-agent-ui-surfaces.md`). The tool validates it, records it, and posts it as one comment of the agent that carries the panel. The author fills the panel and presses one of its buttons. The values come back into the conversation as the author's comment, and that comment brings the agent back. One generic tool serves every conversation agent, so a component added to the catalog is a new ability of all of them without a new tool. Out of scope: the record, the catalog, and the validation (`../core/AUI-agent-ui-surfaces.md`); the rendering (`../ui/ASF-agent-surfaces.md`); one question, which is `AUC-ask-user-comment-tool.md`'s; several questions with proposed answers, which are `ADQ-ask-discussion-questions-tool.md`'s; and every edit to a change, a prompt, an OpenSpec change, or a plan, which the proposal tools keep.

## Functional requirements
1. **SSF-FR-CETL** The tool is a `rig` portable tool named `show_surface` and conforms to `TLC-tool-conventions.md` in full. It is one of the tools of TLC-FR-17 that declare a side effect, and its side effect is the one the contract surface declares.
2. **SSF-FR-FKWS** Its `description()` returns the fixed text of the contract surface, then the **compact catalog**, then the `instructions` of the discussion catalog (per `../core/AUI-agent-ui-surfaces.md` AUI-FR-PYVB). Its `parameters()` returns the small JSON Schema of the contract surface with its parameter descriptions (per TLC-FR-05, TLC-FR-06).
   - *Why:* The full schemas of the catalog cost thousands of tokens on every turn, and the backend validation already tells the model exactly what to fix.
2. **SSF-FR-XKQA** The compact catalog is generated from the discussion catalog. It holds one line for each component, which names the component and its properties and marks the required ones, and one line for each function, which names the function and its arguments.
3. **SSF-FR-ZIRH** The tool is constructed per turn (per TLC-FR-15). Its constructor binds it to the origin of the turn's conversation and to the turn's agent participant. It takes no argument that names a conversation or a participant.
4. **SSF-FR-DNZK** The tool is attached to every turn (per `../ai/CVL-conversation-loop.md` CVL-FR-08), but an advisor turn (CVL-FR-HNGB). With no project open, a call is the shared **no open project** refusal (per TLC-FR-13).
5. **SSF-FR-EFSU** A `message` that is blank once trimmed is the **blank message** refusal. A `message` with no spoken part (per `../ai/CVL-conversation-loop.md` CVL-FR-AGNN) is the **no spoken part** refusal.
6. **SSF-FR-CVBB** `components` and `data_model` go to `validate_surface` (per `../core/AUI-agent-ui-surfaces.md` AUI-FR-EGAB). An absent `data_model` is `{}`. A violation is the **invalid surface** refusal. Its message names the rule, the component id and the property where the violation has them, and what a valid value looks like.
7. **SSF-FR-NHDH** A conversation whose folded state is resolved (per `../core/CMS-comments-storage.md` CMS-FR-17) is the **conversation resolved** refusal: `PermissionDenied`, not retryable.
8. **SSF-FR-FCDS** A call that passes every check records one surface through `../core/AUI-agent-ui-surfaces.md` AUI-FR-QRCX. The comment body is the `message` less its citation markers (per `../ai/CVL-conversation-loop.md` CVL-FR-VSSU).
9. **SSF-FR-WJHF** A failure to record the surface or to append its comment is the **could not record** refusal: `Other`, retryable. Nothing is recorded.
10. **SSF-FR-BWTD** A successful call returns `{ shown: true }` and **ends the turn** (per `../ai/CVL-conversation-loop.md` CVL-FR-15). It returns at once and waits for no action.
11. **SSF-FR-YIXE** A reply that asks for two calls of this tool dispatches the first. Each other call of the tool in that reply is not dispatched and gets the **one at a time** refusal (per `../ai/CVL-conversation-loop.md` CVL-FR-15). One turn records at most one surface.
12. **SSF-FR-TRIB** In an agent group, the tool records its surface on a participant turn as on any other turn. A group holds no surface for its synthesis turn (per `../core/AGC-agent-conversations.md` AGC-FR-DIZM).
13. **SSF-FR-BVSL** Each call logs under the `ai` and `backend` domains (per TLC-FR-14): an `INFO` record with the tool, the discussion id, and the component count when a surface is recorded, and a `WARN` record with the tool, the reason, and the rule of an invalid surface when it refuses.
13. **SSF-FR-JPNA** No log record of this tool carries a component, a text, the data model, or the message.
14. **SSF-FR-PTFV** The tool reaches no network and writes no file but the surface record and the comment. It commits nothing.

## Contract surface
One tool, conforming to `TLC-tool-conventions.md` in full. It is attached by `../ai/CVL-conversation-loop.md` (CVL-FR-08), and has no Tauri command, event, or frontend reach of its own.

### The tool
`show_surface` — a `rig` portable tool, constructed per turn (per TLC-FR-15). Its constructor takes the origin of the conversation the turn belongs to and the agent participant the turn answers as.

### The description
The fixed text the model reads (per TLC-FR-05). The compact catalog and the catalog's `instructions` follow it:

> Show the author a small interactive panel in the conversation: a table that compares options, a checklist, a short form, or a set of choices. Use it when the author must see structured information side by side, or must give you several values at once, and plain text or one question is not enough. Before you compose a panel, load the `a2ui` skill with `load_skill`: it has the rules, the cards, and examples. For a common case use one card: `ChoiceCard`, `ConfirmCard`, `ComparisonCard`, or `ChecklistCard`, as the only component with the id `root`. Text is Markdown. An image, a video, or an audio `url` is an `https` address, `project:<path>`, or `attachment:<digest>`. Build the panel in `components` as a flat list from the components of the catalog below. Each entry has a unique `id` and a `component` type, and a container names its children by id. Exactly one entry has the id `root`. Bind a value to the data model with `{"@path": "/name"}`, and give the start values in `data_model`. An input needs a Button. A Button has `action.event.name` and a `context` that names the values it sends back, usually with `@path`. When the author presses a button, the panel locks, and the button and its values come back to you in the conversation as the author's comment. Tell the author in `message`, in a sentence or two, what the panel is for. Put one `<spoken>…</spoken>` tag in `message`: one short sentence that a voice can read aloud. A message with no tag is refused. Your turn ends when the panel is posted. To ask one question, use `ask_user_comment`. To propose an edit to a change, a prompt, an OpenSpec change, or a plan, use its proposal tool.

### Arguments
```
ShowSurfaceArgs {
  message:    string,        // required; the comment the author reads
  components: Component[],   // required; 1 to 100, the components of the discussion catalog
  data_model: object?        // the start values; {} when absent
}
```

In the parameter schema, each entry of `components` is an object with a required `id` (a string) and a required `component` (one of the component names of the catalog), and it may carry other properties. The schema holds nothing else of the catalog.

```
Row: id!, component!, children!, justify, align, weight, @accessibility
Button: id!, component!, child!, action!, variant, weight, @accessibility, @checks
…                                    // one line for each component (SSF-FR-XKQA)
formatString(value!)
…                                    // one line for each function
```

Their descriptions in the parameter schema (per TLC-FR-06):

- `message` — *"What the panel is for, in a sentence or two. It holds one `<spoken>…</spoken>` tag with one short sentence that a voice can read aloud."*
- `components` — *"The panel as a flat list of components of the catalog. Each has a unique `id` and a `component` type, and exactly one has the id `root`. A container names its children by id."*
- `data_model` — *"A JSON object with the start values that the components bind to with `@path`. Leave it out when nothing binds."*

### Output
Data (per TLC-FR-08): `{ shown: true }`. The turn ends on the call that showed the surface (per `../ai/CVL-conversation-loop.md` CVL-FR-15).

### The comment
The `message`, its spoken part hidden (per `../ui/CMT-comments.md` CMT-FR-BMCI). Beside it the comment carries exactly one `agent_surface` attachment (per `../core/CMS-comments-storage.md` CMS-FR-RWQQ).

### Refusals
- **no open project** — the shared refusal of `TLC-tool-conventions.md`: kind `NotFound`, not retryable.
- **blank message** — kind `InvalidArgs`, retryable: *"Tell the author in `message` what the panel is for."*
- **no spoken part** — kind `InvalidArgs`, retryable: *"The message has no spoken part. Put one short sentence in one `<spoken>…</spoken>` tag in `message`."*
- **invalid surface** — kind `InvalidArgs`, retryable: *"The panel is not valid: <rule> at component `<id>`, property `<property>`. <what a valid value looks like>. Fix it and call again."* The parts that the violation does not have are left out.
- **conversation resolved** — kind `PermissionDenied`, not retryable: *"This conversation is resolved and takes no further comments, so nothing was shown. Answer with what you already have; asking again will not succeed."*
- **could not record** — kind `Other`, retryable: *"The panel could not be recorded."*
- **one at a time** — the result of a call that is not dispatched (SSF-FR-YIXE): *"Only one panel can be shown in a reply, so this one was not recorded. Show the next panel after the author answered this one."*

### Side effects
Declared in full (per TLC-FR-17). A successful call produces **exactly one agent surface record** and **exactly one comment**, through `../core/AUI-agent-ui-surfaces.md` AUI-FR-QRCX. It writes no file of the project and changes no repository state.

## Non-functional requirements
- A call costs one validation, one record write, and one comment append.
- Nothing in this tool requires network access.
- The compact catalog grows by one line for each component or function added to the catalog. The parameter schema does not grow.
