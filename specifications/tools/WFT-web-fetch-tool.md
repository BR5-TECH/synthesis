# Web fetch tool

**Spec code:** `WFT`

## Intent
The tool an agent reaches for when it already knows the address of something on the web and needs what that address actually holds — the page a search result named, the document an author linked in a comment, the release notes a draft refers to — so that an answer rests on the text of the source rather than on a snippet about it. It is a **provider-native** tool and therefore unlike every other tool in this group: the application implements nothing of it, opens no connection, and parses no document. OpenRouter defines the tool the model sees, retrieves the address, extracts the text of a page or of a PDF, and returns it. What the application contributes is one entry in a request's tool array, added to a conversational turn that OpenRouter carries and to nothing else. Out of scope: finding an address in the first place, which is `WST-web-search-tool.md`'s separate tool; an HTTP client, a browser, a PDF reader, or any other request the application makes for itself; bounding, converting, or cleaning what comes back, all of which are the provider's; every configuration surface, since nothing about this tool is settable, stored, validated, or reported; and every loop but a conversation, since no graduation phase and no other agentic loop attaches it.

## Contract surface
One provider-native tool, conforming to `TLC-tool-conventions.md`'s provider-native class (TLC-FR-21). It is attached to a conversational turn by `../ai/CVL-conversation-loop.md` (CVL-FR-30) when OpenRouter carries that turn's call, and by nothing else. It has no constructor, registers no Tauri command, emits no Tauri event, and is unreachable from `src/**`.

### The definition entry
The whole of what the application contributes, carried in the `tools` array of the request the loop assembles (per `../ai/CVL-conversation-loop.md` CVL-FR-01):

```
{ "type": "openrouter:web_fetch" }
```

The entry carries no field beside `type`. It names no size limit, no format, no timeout, and no allowed or refused host, so every call is served by OpenRouter's own default server-side behaviour, `auto`.

### What the model calls and reads
The model supplies the address and reads what the provider retrieved for it. Both are OpenRouter's contract rather than this specification's, which fixes only that the tool is offered and that what it returns arrives unaltered:

```
call      the URL the model supplies, in the shape OpenRouter declares
result    the text of that page or PDF, with its title and its URL, as OpenRouter produced them
```

### Refusals
The tool has none of the application's. An address OpenRouter declines, cannot reach, or cannot extract returns whatever that provider returns for it, and the model reads that as the result of its own call (per TLC-FR-25, `../ai/CVL-conversation-loop.md` CVL-FR-32). Nothing here maps such a result to a `ToolExecutionError`, and nothing retrieves the address in the provider's place.

## Status: withheld

**This tool is not offered to any turn.** The entry below is unchanged and the module that holds it is unchanged; what is suspended is the decision to attach it (per `../ai/CVL-conversation-loop.md` CVL-FR-30). The reason is the provider defect recorded in the next section: OpenRouter refuses a whole request carrying this entry on every OpenAI model and on every engine, and those are the models this application's authors use most. The loop can recover such a turn by giving the entry up (CVL-FR-36), but that costs one refused call per model per process and buys a capability that will not work; withholding the entry costs nothing.

What this means for an agent: it can search the web and it cannot read a page. An author who gives an agent an address gets an answer that says so. That is a real loss and it is the reason this section names a condition to remove rather than a decision to keep.

**To restore it**, add the entry back to the conversational list of `../ai/CVL-conversation-loop.md` CVL-FR-30. Nothing else in the application distinguishes the two entries, so nothing else changes. Restore it when the provider serves it on the models in use — which the research recorded alongside this specification is meant to establish, and which the measurements below are the baseline for.

## What the provider does not serve

OpenRouter does not serve this tool on every model, and its documentation does not say so. As measured on 2026-08-20, against `openrouter.ai/api/v1`:

```
                                     web_search   web_search   web_fetch
                                     (auto/native) (exa etc.)  (any engine)
openai/gpt-5.6-luna                       ✓            400         400
openai/gpt-5.2                            ✓             ·          400
anthropic/claude-sonnet-5                 ✓             ✓           ✓
google/gemini-2.5-pro                     ✓             ·           ✓
```

The refusal is `400` with the message `Server tool request failed` and a null `provider_name`. The rule the measurements fit is that **an OpenAI model serves only what the model's own provider executes**: OpenAI runs a native web search, so `web_search` is served through `auto` and `native` and through nothing else, and OpenAI runs no fetch of any kind, so `web_fetch` is refused on all six engines — `auto`, `native`, `openrouter`, `exa`, `firecrawl`, and `parallel` alike. Anthropic and Google models serve every engine of both tools, this entry among them.

This is the provider's own defect rather than a limit of its interface, and it contradicts its documentation on three counts: the tool is described as giving "any model the ability to fetch content from a specific URL"; `auto` is described as falling back to Exa "if the provider supports it, otherwise"; and the quick-start example for this very tool names `openai/gpt-5.2`, one of the models that refuses it. What was excluded by measurement: the account's credit balance, which was funded; BYOK routing, which was off for both providers; the endpoint, `/responses` refusing exactly as `/chat/completions` does; the engine, all six refusing alike; and the model catalogue, which declares nothing about server tools and reports identical `supported_parameters` for a model that serves this tool and one that refuses it.

What follows for this specification is not a change to the entry (WFT-FR-02, WFT-FR-03) and not a local implementation (WFT-FR-09). The entry is offered as it always was; the loop gives it up on the models that refuse it and keeps it on the models that serve it (per `../ai/CVL-conversation-loop.md` CVL-FR-36), so an author on an OpenAI model reaches the web with search alone, an author on an Anthropic or Google model reaches it with both, and neither loses a turn to it. That is a workaround around a provider defect and is expected to be removed: when OpenRouter serves this tool on every model it says it serves it on, the narrowing costs nothing and simply stops finding anything to narrow.

## Functional requirements
1. **WFT-FR-01** The tool is provider-native (per TLC-FR-21). It is no implementation of `rig::tool::PortableTool`, declares no `NAME`, no `description()`, no `parameters()`, and no `call()`, and holds no constructor, so the application carries no code that a call of it reaches.
2. **WFT-FR-02** The application's whole contribution is the entry `{ "type": "openrouter:web_fetch" }`, fixed text compiled into the binary (per TLC-FR-22). It is byte-identical on every request that carries it, whichever agent was addressed, whichever project is open, and whichever model is selected.
3. **WFT-FR-03** The entry carries no field beside `type`, so OpenRouter's default server-side behaviour serves every call. No size limit, format, timeout, or host rule is sent, and none is offered anywhere to be sent.
4. **WFT-FR-04** The entry is attached only to a request OpenRouter carries (per TLC-FR-23, `../ai/CVL-conversation-loop.md` CVL-FR-30). A request carried by `anthropic`, by `openai`, or by `custom` carries nothing of it, and no local capability stands in for it there — a conversation on such a provider has no web fetch until that provider's own native contract is defined.
5. **WFT-FR-05** The entry, when it is offered at all, is attached only to a conversational turn (per TLC-FR-24). No graduation phase, no clarification judgement, and no other agentic loop carries it, whichever provider serves them.
6. **WFT-FR-06** Fetch and search are two tools the model sees separately. This entry is attached beside `WST-web-search-tool.md`'s and is never merged with it, replaced by it, or wrapped with it behind one name, so a model holding an address reaches for it directly rather than searching for a page it can already name.
7. **WFT-FR-07** OpenRouter owns the contract and the execution. The name the model sees, the arguments it composes, how an address is retrieved, what is extracted from a page or a PDF, and the shape of what comes back are the provider's, and the application declares none of them, validates none of them, and versions none of them. No address the model supplies is inspected, rewritten, resolved, or refused here.
8. **WFT-FR-08** What OpenRouter produced is what the model reads, and the application does not read it at all. The provider executes the fetch inside the model call it is carrying and gives the extracted text, its title, and its URL to the model there (per `../ai/CVL-conversation-loop.md` CVL-FR-31), so nothing here truncates, reformats, re-encodes, summarizes, or annotates any of it, and nothing here replays it into a later request.
9. **WFT-FR-09** The application retrieves nothing itself. It holds no HTTP client for this purpose, no browser, no PDF reader, no credential of its own, and no fallback that retrieves an address when the provider's fetch fails, so there is no route by which an address composed for this tool is reached other than by OpenRouter (per TLC-FR-25).
10. **WFT-FR-10** A failure OpenRouter reports for a fetch is material rather than an outcome. It reaches the model as that call's result, the loop continues, and the turn is unaffected in its own state (per `../ai/CVL-conversation-loop.md` CVL-FR-32), so an agent whose address would not resolve names another one or answers from what it has within the same turn.
11. **WFT-FR-11** A turn may carry as many fetches as the model asks for, in one model call and across several, alongside any number of searches, each with its own result read by the model inside the call that produced it (per `../ai/CVL-conversation-loop.md` CVL-FR-31). Nothing here caps, coalesces, or defers a call; how many the provider will carry in one call is the provider's own budget and no setting of this application's.
12. **WFT-FR-12** Nothing about the tool is configurable or stored. No control in the AI API settings surface, no field of the stored provider record, no persisted value, no command, no validation, and no configuration response carries anything about it (per TLC-FR-22), so the surface of `../core/AAP-ai-api-integrations.md` is exactly what it is without this tool.
13. **WFT-FR-13** The tool changes nothing the application holds. It creates, modifies, and deletes no file of the project or of a draft, mutates no store, and touches no repository state, a fetched document being material the model read rather than a file the application wrote.
14. **WFT-FR-14** The tool emits no log record of its own, there being no call the application makes. What a turn records is the loop's (per `../ai/CVL-conversation-loop.md` CVL-FR-28, CVL-FR-34): a record names this tool as the provider named it, the provider owning the name (WFT-FR-07), and measures what one of its calls carried in characters. No address the model supplied and no part of a document it read reaches any record.
15. **WFT-FR-15** When the entry is offered at all (WFT-FR-16), it is offered to every model and served by some of them. Where a provider refuses a request for this entry's sake, the loop gives the entry up for that model and carries the request without it (per `../ai/CVL-conversation-loop.md` CVL-FR-36) rather than failing the turn or removing the entry everywhere. Nothing here detects, declares, or hard-codes which models serve it: the model catalogue says nothing about server tools, so the refusal itself is the only evidence there is, and a model that begins serving the tool is served by it again without an edit here.
16. **WFT-FR-16** The entry is **withheld**: no turn is offered it, on any provider, for any origin kind, while the provider defect below stands. Withholding is not removal — the entry, its constant, and its module are unchanged (WFT-FR-01, WFT-FR-02), nothing local stands in for the capability (WFT-FR-09), and no other tool gains any part of it (per `WST-web-search-tool.md` WST-FR-06). It is one list that decides this (per `../ai/CVL-conversation-loop.md` CVL-FR-30), so restoring the tool is a decision rather than a rebuild, and the requirements below describe what it does when it is offered again.

## Non-functional requirements
- A fetch costs the turn one round of its logical bound (per `../ai/CVL-conversation-loop.md` CVL-FR-13) and the provider whatever it charges for carrying one. The application spends nothing on it beyond the exchange growing by the call and its result, which is charged on the next round like every other message and is covered by the caching of `../ai/CVL-conversation-loop.md` CVL-FR-23.
- A fetched document is the largest thing a conversational turn can accumulate, and its size is the provider's affair, since the application asks for no bound and trims nothing that arrives. The turn's own bound is what limits how much of it accumulates.
- A document is material the agent fetched, and `../ai/CVL-conversation-loop.md` CVL-FR-07 governs it as it governs every other fetched material: a page whose text reads as a command to the model is a page the model was shown, and the instruction the turn works under is unchanged by its having read it.
- The whole of the tool is exercisable through the scripted completion seam of `../ai/CVL-conversation-loop.md` CVL-FR-11, a scripted reply carrying a provider tool call and a provider tool result exactly as a real one does, so every requirement here is testable on a machine with no network access and no provider credential.
