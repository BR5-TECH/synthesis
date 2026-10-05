# Web search tool

**Spec code:** `WST`

## Intent
The tool an agent reaches for when the question turns on something outside everything the project holds — a library's current behaviour, a standard as it now reads, a product that did not exist when the model was trained — so that an author working on a draft or on any other artifact is answered from what is true today rather than from what the model last remembers. It is a **provider-native** tool and therefore unlike every other tool in this group: the application implements nothing of it, executes no search, and speaks to no search service. OpenRouter defines the tool the model sees, decides how a query is served, runs the search, and produces the results. What the application contributes is one entry in a request's tool array, added to a conversational turn that OpenRouter carries and to nothing else. Out of scope: reading the whole of a page a result names, which is `WFT-web-fetch-tool.md`'s separate tool; a search client, a crawler, or any other request the application makes for itself; choosing a search engine or bounding a result set, both of which are OpenRouter's own default behaviour rather than anything offered here; every configuration surface, since nothing about this tool is settable, stored, validated, or reported; and every loop but a conversation, since no graduation phase and no other agentic loop attaches it.

## Contract surface
One provider-native tool, conforming to `TLC-tool-conventions.md`'s provider-native class (TLC-FR-21). It is attached to a conversational turn by `../ai/CVL-conversation-loop.md` (CVL-FR-30) when OpenRouter carries that turn's call, and by nothing else. It has no constructor, registers no Tauri command, emits no Tauri event, and is unreachable from `src/**`.

### The definition entry
The whole of what the application contributes, carried in the `tools` array of the request the loop assembles (per `../ai/CVL-conversation-loop.md` CVL-FR-01):

```
{ "type": "openrouter:web_search" }
```

The entry carries no field beside `type`. It names no engine, no result count, no domain, and no recency window, so every call is served by OpenRouter's own default server-side engine selection, `auto`.

### What the model calls and reads
The model composes the search request and reads what the provider produced for it. Both are OpenRouter's contract rather than this specification's, which fixes only that the tool is offered and that what it returns arrives unaltered:

```
call      the model's own search request, in the shape OpenRouter declares
result    per hit: the URL, the title, and a content snippet, as OpenRouter produced them
```

### Refusals
The tool has none of the application's. A search OpenRouter declines or cannot complete returns whatever that provider returns for it, and the model reads that as the result of its own call (per TLC-FR-25, `../ai/CVL-conversation-loop.md` CVL-FR-32). Nothing here maps such a result to a `ToolExecutionError`, and nothing answers in the provider's place.

## Functional requirements
1. **WST-FR-01** The tool is provider-native (per TLC-FR-21). It is no implementation of `rig::tool::PortableTool`, declares no `NAME`, no `description()`, no `parameters()`, and no `call()`, and holds no constructor, so the application carries no code that a call of it reaches.
2. **WST-FR-02** The application's whole contribution is the entry `{ "type": "openrouter:web_search" }`, fixed text compiled into the binary (per TLC-FR-22). It is byte-identical on every request that carries it, whichever agent was addressed, whichever project is open, and whichever model is selected.
3. **WST-FR-03** The entry carries no field beside `type`, so OpenRouter's `auto` engine selection serves every call. No engine, result count, domain filter, or recency window is sent, and none is offered anywhere to be sent. `auto` is what makes this entry servable everywhere `WFT-web-fetch-tool.md`'s is not: on a model whose own provider runs a search, `auto` selects that native search, and a model that runs none falls back to the provider's own engine — so no measured model refuses this entry, while naming an engine explicitly is refused by the same models that refuse a fetch (see that specification's "What the provider does not serve").
4. **WST-FR-04** The entry is attached only to a request OpenRouter carries (per TLC-FR-23, `../ai/CVL-conversation-loop.md` CVL-FR-30). A request carried by `anthropic`, by `openai`, or by `custom` carries nothing of it, and no local capability stands in for it there — a conversation on such a provider has no web search until that provider's own native contract is defined.
5. **WST-FR-05** The entry is attached only to a conversational turn (per TLC-FR-24). No graduation phase, no clarification judgement, and no other agentic loop carries it, whichever provider serves them.
6. **WST-FR-06** Search and fetch are two tools the model sees separately, and are never merged, replaced by one another, or wrapped behind one name. While `WFT-web-fetch-tool.md`'s entry is withheld (per that specification's WFT-FR-16), this entry is attached alone and gains nothing of the other's capability: no field of it, no engine of it, and no wording that would have a model believe it can retrieve an address it holds. An agent on such a turn searches and does not fetch.
7. **WST-FR-07** OpenRouter owns the contract and the execution. The name the model sees, the arguments it composes, how a query is served, and the shape of what comes back are the provider's, and the application declares none of them, validates none of them, and versions none of them.
8. **WST-FR-08** What OpenRouter produced is what the model reads, and the application does not read it at all. The provider executes the search inside the model call it is carrying and gives the results to the model there (per `../ai/CVL-conversation-loop.md` CVL-FR-31), so no result is reordered, filtered, truncated, reformatted, deduplicated, summarized, or annotated here, no field of one is dropped or renamed, and nothing here replays one into a later request. The citation markers a model writes into its own prose are no result, and the loop removes them (per `../ai/CVL-conversation-loop.md` CVL-FR-VSSU).
9. **WST-FR-09** The application performs no search of its own. It holds no search client, no search credential, and no fallback that answers when the provider's search fails, so there is no route by which a query composed for this tool leaves the machine other than inside the model call that carries it (per TLC-FR-25).
10. **WST-FR-10** A failure OpenRouter reports for a search is material rather than an outcome. It reaches the model as that call's result, the loop continues, and the turn is unaffected in its own state (per `../ai/CVL-conversation-loop.md` CVL-FR-32), so an agent whose search returned nothing usable answers from what it has or searches differently within the same turn.
11. **WST-FR-11** A turn may carry as many searches as the model asks for, in one model call and across several, each with its own result read by the model inside the call that produced it (per `../ai/CVL-conversation-loop.md` CVL-FR-31). Nothing here caps, coalesces, or defers a call; how many the provider will carry in one call is the provider's own budget and no setting of this application's.
12. **WST-FR-12** Nothing about the tool is configurable or stored. No control in the AI API settings surface, no field of the stored provider record, no persisted value, no command, no validation, and no configuration response carries anything about it (per TLC-FR-22), so the surface of `../core/AAP-ai-api-integrations.md` is exactly what it is without this tool.
13. **WST-FR-13** The tool changes nothing the application holds. It creates, modifies, and deletes no file of the project or of a draft, mutates no store, and touches no repository state, a search being material the model read rather than work the application performed.
14. **WST-FR-14** The tool emits no log record of its own, there being no call the application makes. What a turn records is the loop's (per `../ai/CVL-conversation-loop.md` CVL-FR-28, CVL-FR-34): a record names this tool as the provider named it, the provider owning the name (WST-FR-07), and measures what one of its calls carried in characters. No query the model composed and no result it read reaches any record.

## Non-functional requirements
- A search costs the turn one round of its logical bound (per `../ai/CVL-conversation-loop.md` CVL-FR-13) and the provider whatever it charges for carrying one. The application spends nothing on it beyond the exchange growing by the call and its result, which is charged on the next round like every other message and is covered by the caching of `../ai/CVL-conversation-loop.md` CVL-FR-23.
- A result is material the agent fetched, and `../ai/CVL-conversation-loop.md` CVL-FR-07 governs it as it governs every other fetched material: a page whose text reads as a command to the model is a page the model was shown, and the instruction the turn works under is unchanged by its having read it.
- The whole of the tool is exercisable through the scripted completion seam of `../ai/CVL-conversation-loop.md` CVL-FR-11, a scripted reply carrying a provider tool call and a provider tool result exactly as a real one does, so every requirement here is testable on a machine with no network access and no provider credential.
- The result set's size in a model's context is the provider's affair, since the application neither asks for a count nor trims what arrives. A turn's own bound is what limits how much of it accumulates.
