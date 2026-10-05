# AI integrations

**Spec code:** `AII`

## Intent
AI integrations is where the author tells Synthesis which AI it may use on their behalf, together with the project-scoped controls that let one project depart from those choices. Synthesis reaches for AI in two fundamentally different ways, and each is its own section of the Global settings window: **AI API** holds the API endpoints the application calls itself — a base URL, a key, a model, and the reasoning that model can be asked for — and **Agentic AI** holds the agent backends it hands work to, whether a coding-agent CLI installed on this machine or an agent-execution endpoint reached over HTTP. They are two sections rather than two halves of one because they present the same shape and would otherwise read as a single undifferentiated list of fields; the author navigates to the kind of AI they mean, and sees only that. Each level configures every integration it supports independently under its own tab, so an author can keep several ready and move between them without re-entering anything; exactly one integration is active within each level, and a project may override either choice with any integration of that level already configured and verified. The two levels are what let the application call a model over HTTP for one feature while driving an agent for another, without either choice disturbing the other. The AI API level's endpoints and keys carry every call the application makes for itself, but the model and the reasoning chosen in it are the graduation loop's alone (`../ai/GRL-graduation-loop.md` GRL-FR-EKXT). A conversational agent carries its own model and reasoning as part of the persona it is, and those are described in the Global settings **Agents** section (`AGT-agents.md`); the agent is served by the endpoint and key of the AI API provider the open project resolves to — so a model chosen here never speaks for an agent, an agent's model is never chosen here, and activating a different provider moves every agent to that provider's endpoint. Out of scope: the model and the reasoning a conversational agent runs on, which belong to that agent's own description (`AGT-agents.md` AGT-FR-01), and the wire format of any completion call, which belongs to the framework adapter (`../ai/CVL-conversation-loop.md` CVL-FR-ZPGW); modalities other than conversation — no voice, embedding, or image endpoint is configurable here, and the API level's providers are conversational endpoints and nothing else; a reasoning budget expressed as a count of tokens, which no selector here offers, because reasoning is chosen as a level the selected model itself declares; and the agent adapters of `../core/ADP-adapters.md`, which transform Synthesis artifacts into an external agent's workspace layout and are installed and configured elsewhere (`GLS-global-settings.md` GLS-FR-09, `SET-project-settings.md` SET-FR-05).

## User stories
- As an author who pays for more than one AI provider, I want each one configured once so that switching the application between them is a single click rather than re-entering a URL, a key, and a model.
- As an author, I want to point the application at the company LLM Gateway with one host and one secret, and to see the models the gateway serves, so that I can use them without learning its routes.
- As an author who keeps more than one coding agent installed, I want each configured once, and I want an agent I reach over HTTP to sit beside the ones I have installed locally rather than in some separate place.
- As an author, I want to know an endpoint or a binary actually works before I depend on it, rather than discovering it is wrong in the middle of a task.
- As an author, I want my API keys to be as safe here as the tokens I have already given the application, and never to see one displayed back to me.
- As an author who runs Claude Code inside an isolated container with no session of its own, I want to hand the application the token it will need once, and to be told before I press Verify if what I pasted is not a token at all.
- As an author whose work project has to use a different agent, or a different endpoint, from my personal default, I want to override that one project and leave every other project alone.
- As an author whose provider offers hundreds of models, I want to reach the one I mean by typing part of its name, rather than scrolling a list to find it.
- As an author, I want to say how hard the model should think, and to be offered only the depths the model I actually picked can honour, so that a setting I choose here is never one the model quietly ignores.
- As an author, I want the section to tell me what the model and the reasoning I choose in it are used for, so that I do not set them expecting my agents to follow them.
- As an author who wants one kind of work in a run to use a different model or a different depth from the rest, I want that kind of work named once with both of its selections beside it, so that I read and set what a turn will run on in one place rather than matching a task to itself in two lists.

## Wireframes

The AI API section of the Global settings window:

```
┌─ Global settings ────────────────────────────────────────────────────────┐
│ Appearance          │  AI API                                            │
│ Recent projects     │  The endpoints Synthesis calls itself.             │
│ Installed plugins   │  The model and reasoning here are used only by the │
│ Installed adapters  │  graduation loop. An agent uses its own model and  │
│ GitHub              │  reasoning, set in the Agents section.             │
│ AI API            ◂ │                                                    │
│ Agentic AI          │  ┌─ OpenRouter ─┬─ Anthropic ─┬─ OpenAI ─┬ Custom ┐│
│                     │  │  ● Active                                      ││
│                     │  │  Base URL [https://openrouter.ai/api/v1      ] ││
│                     │  │  API key  [ ••••3f9a                ] [Verify] ││
│                     │  │           verified · 2 minutes ago             ││
│                     │  │  Model    [ Claude Opus 5                   ▾] ││
│                     │  │           from the endpoint                    ││
│                     │  │  Reasoning[ high                            ▾] ││
│                     │  │  Timeout  [ 300                            ] s ││
│                     │  │           bounds each agent turn here          ││
│                     │  │                                     [ Clear ]  ││
│                     │  └────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────────────────┘
```

An open model selector, which is how a model is found among the hundreds a
provider may offer:

```
┌─ OpenRouter ────────────────────────────────────────────────
│
│   Model    [ Claude Opus 5                            ▾]
│           ┌────────────────────────────────────────────┐
│           │ 🔍 opus                                    │
│           ├────────────────────────────────────────────┤
│           │   Claude Opus 5                          ● │
│           │   Claude Opus 5 (fast)                     │
│           │   Claude Opus 4.6                          │
│           └────────────────────────────────────────────┘
│             from the endpoint · 3 of 365 shown
│
│   Reasoning[ model default                            ▾]
│             this model reasons at medium unless asked
└─────────────────────────────────────────────────────────────
```

A model that reasons but offers no levels of its own, and one that cannot be
asked to stop reasoning at all:

```
│   Reasoning[ on                                       ▾]     ← off · on
│
│   Reasoning[ high                                     ▾]     ← high · medium · low
│             this model always reasons
```

The Agentic AI section, reached from the same navigation. Only one of the two
sections is on screen at a time:

```
┌─ Global settings ────────────────────────────────────────────────────────┐
│ Appearance          │  Agentic AI                                        │
│ Recent projects     │  The agent backends Synthesis hands work to.       │
│ Installed plugins   │                                                    │
│ Installed adapters  │  ┌─ Claude Code ─┬ Codex ─┬ OpenCode ─┬ Claude ───┐│
│ GitHub              │  │                              Agent API ─┬ Custom││
│ AI API              │  │  ● Active                                      ││
│ Agentic AI        ◂ │  │  Binary   [/opt/homebrew/bin/claude ]          ││
│                     │  │  OAuth    [ ••••ygAA                ] [Verify] ││
│                     │  │           empty re-verifies with the stored    ││
│                     │  │           token                                ││
│                     │  │           detected · verified · 2.1.4          ││
│                     │  │  Model    [ Opus 5                          ▾] ││
│                     │  │           from the installed CLI               ││
│                     │  │  Effort   [ high                            ▾] ││
│                     │  │  ▸ per task · 5 follow the defaults            ││
│                     │  │                                     [ Clear ]  ││
│                     │  └────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────────────────┘
```

The per-task section, open. It holds one row per kind of work a run hands to
this backend, and each row carries that kind's model and that kind's effort
side by side, each following the default selector above until it is given a
value of its own:

```
│   Model    [ Opus 5                                   ▾]
│             from the installed CLI
│   Effort   [ high                                     ▾]
│
│  ▾ per task · 2 of 3 differ from the defaults
│                       Model                  Effort
│    Work            [ Sonnet 5         ▾]  [ same as default  ▾]
│    Review          [ same as default  ▾]  [ medium           ▾]
│    Reconciliation  [ same as default  ▾]  [ same as default  ▾]
└─────────────────────────────────────────────────────────────
```

The same section in a tab whose vendor declares no reasoning-effort levels.
The Effort default selector and the effort column are both absent, and the
rows close up rather than carrying a control that can hold nothing:

```
│   Model    [ Opus 5                                   ▾]
│             from the installed CLI
│
│  ▾ per task · 3 follow the defaults
│                       Model
│    Work            [ same as default  ▾]
│    Review          [ same as default  ▾]
│    Reconciliation  [ same as default  ▾]
└─────────────────────────────────────────────────────────────
```

Layout notes:

- The per-task section is one disclosure holding both dimensions, and it sits
  below both default selectors rather than under either one of them. The two
  defaults read as the pair of values the backend runs on, and the section
  beneath them is where a single kind of work departs from that pair.
- The section is closed until the author opens it, and its summary line counts
  the rows that differ from the defaults rather than naming them. A row counts
  once whether it overrides its model, its effort, or both, so the count is a
  count of the kinds of work configured apart and never of the selectors
  changed.
- A row is one task label followed by that task's two selectors on one line,
  in the order the defaults above them are in: model first, effort second. The
  columns are aligned across the three rows and headed by the name of the
  default each follows, so a column reads down as one dimension and a row
  reads across as one kind of work.
- Each selector's first entry is "same as default", which is what a selector
  with no value of its own shows. The entries after it are the same list the
  default selector at the head of that column offers — the record's models
  under Model and the vendor's own levels under Effort — and a model selector
  opens the same filterable panel the Model default selector opens.
- A vendor that declares no effort levels renders neither the Effort default
  selector nor the effort column, and no disabled or empty control stands in
  their place. Every agentic tab renders the Model default selector and the
  per-task section's model column.
- The three rows are in the order a run reaches them and are labelled for the
  author rather than by the identifier each carries: Work, Review, and
  Reconciliation, the last of which is the semantic-rebase turn under the name
  the rest of the application gives it.

The Claude Code tab with nothing stored and a value that is not a token, which
is the one CLI tab carrying a credential:

```
┌─ Claude Code ───────────────────────────────────────────────
│
│   Binary   [ /opt/homebrew/bin/claude       ] [ Browse… ]
│   OAuth    [ ••••••••••••••••••••           ] [ Verify ]
│    token   ⚠ a token starts with sk-ant-oat01- followed by
│              letters, digits, or hyphens
│              required — Claude Code runs where it cannot
│              sign in for itself
│
│   Model    [ backend default                          ▾]
│
│   [ Use this integration ]   (disabled until verified)
└─────────────────────────────────────────────────────────────
```

An API-kind tab in the Agentic level, carrying the fields its kind needs in place of a binary path:

```
┌─ Claude Agent API ──────────────────────────────────────────
│
│   Base URL [ https://api.anthropic.com/v1              ]
│   API key  [ ••••a71c                        ] [ Verify ]
│             verified · 2026-07-30 14:02
│
│   Model    [ Opus 5                                   ▾]
│             from the endpoint
│   Effort   [ backend default                          ▾]
│  ▸ per task · 4 follow the defaults
│
│   [ Use this integration ]                    [ Clear ]
└─────────────────────────────────────────────────────────────
```

The Custom tab of the AI API level, which is how the company LLM Gateway is configured:

```
┌─ Custom ────────────────────────────────────────────────────
│
│   Gateway URL [ https://llm-gateway.example.com        ]
│                host or base URL, without /v1
│   Secret      [ ••••3f9a                   ] [ Verify ]
│                required — sent as a bearer token
│                verified · 2 minutes ago
│
│   Model    [ gpt-example                              ▾]
│             from the Custom gateway · chat + responses
│
│   [ Use this integration ]   (disabled until verified)
└─────────────────────────────────────────────────────────────
```

The AI integration controls in the Project section of Project settings:

```
┌─ Project settings ──────────────────────────────────────────────┐
│ Project           ◂ │  Agentic integration                      │
│ Target repo bindings│  Codex · overrides the global choice      │
│ Agent adapters      │                              [ Change ]   │
│ MCP servers         │                                           │
│ Plugins             │  AI API integration                       │
│                     │  OpenRouter · inherited from the global   │
│                     │  choice                     [ Change ]    │
└─────────────────────────────────────────────────────────────────┘
```

Layout notes:
- Each level is a whole Global settings section, named in the tab's own section navigation — AI API and Agentic AI — and only the selected one is on screen. The two levels present the same shape (a tab strip, a status line, a model selector, an activation control), so side by side they read as one continuous list of fields whose second half happens to be about agents; the navigation is what tells them apart before the author reads a word of either.
- A level renders no heading of its own: the section heading above it names it, and a second heading would name the same thing twice. Below that heading each level carries a short explanation of what it configures, then its tab strip. The Agentic AI level's explanation is one sentence naming the kind of AI it configures; the AI API level's also says who uses the model and the reasoning it holds (AII-FR-54), because that is the one thing about this section an author cannot work out from the fields in front of them.
- Within a level, every tab renders the same rows in the same order, so moving between tabs does not move the controls. Where the two kinds of agentic integration differ, the differing row occupies the same position: a CLI tab's binary field and an API tab's base URL and key fields both sit directly above the verification status line.
- The Claude Code tab carries one row the other CLI tabs do not: its OAuth token field, which sits between the binary field and the verification status line and takes the Verify action, so the credential and the path are submitted as one act. Codex and OpenCode render nothing in that position and their binary field takes Verify as it otherwise would.
- The verification status line sits directly beneath the field or fields it describes and is where every configuration error attaches; no such error is rendered as a modal or a transient notification.
- A token that does not look like a token is reported by the token field itself rather than by the verification status line, because nothing has been submitted yet; the two never render at once for the same tab.
- The turn timeout row sits beneath the reasoning row, or beneath the model's status line where no reasoning row renders. A unit label `s` follows its field, and its explanation line sits beneath it.
- The activation control is the last thing in a tab, and the active tab of a level renders an active marker in place of it.
- A model selector opens into a panel carrying a filter box at its top and the matching options beneath it. The box takes focus on open, and a count beneath the panel says how many of the provider's models are showing. It is the same control in both levels: a list of five and a list of hundreds are the same problem once one of them grows.
- The reasoning row sits directly beneath the model selector and its status line, so the depth being asked for reads as a property of the model named above it. A model or a vendor that declares no reasoning renders no row at all, and the rows beneath close the gap rather than leaving one. In an agentic tab the per-task section follows both of those rows, because it departs from the two of them together.
- An API key field renders a masked hint and never a key. It is a password-style field, and the value the author types is never echoed anywhere in the section.
- The Project settings controls are two single lines, each naming the resolved integration of its level and how it was resolved, plus one action; neither renders a URL, a key, a path, a model list, or a verification state.

## UI contract boundary
- **Owned by the UI**: the section's division into two levels and their navigation; each level's tabs and their layout; the base URL, API key, and binary path fields with their prefilling, masking, and file picker; the Verify action; the rendering of verification status and of every typed verification failure; the model selector including its filter box, the client-side matching that narrows it, its count of shown options, and its keyboard handling; the reasoning selector of the API level and the reasoning-effort selector of the agentic level, each including its default entry and the shape it takes from what the selected model or the vendor declares; the per-task section of an agentic tab, with its open state, its summary count, the two-selector shape of each of its rows, the withdrawal of its effort column for a vendor that declares no effort levels, and the "same as default" entry of each of its selectors; the turn timeout field, its bounding, and its explanation line; the activation control and its enablement; the Clear action; the section's empty and unconfigured states; the immediate-apply behaviour that keeps the section free of dirty state; and — in the Project section of Project settings — the two lines naming the project's resolved integrations, how each was resolved, and the actions that change them.
- **Delegated to backend (abstract)**: "list ai api integrations", "verify ai api integration", "set ai api model", "set ai api reasoning", "set ai api turn timeout", "set active ai api integration", "clear ai api integration", "get project ai api integration", "set project ai api integration", every one owned by `../core/AAP-ai-api-integrations.md`; and "list agentic integrations", "detect agentic cli binary", "verify agentic integration", "set agentic integration model", "set agentic integration effort", "set active agentic integration", "clear agentic integration", "get project agentic integration", "set project agentic integration", every one owned by `../core/AIC-agentic-integrations.md`.

## Functional requirements
1. **AII-FR-01** The AI is configured across exactly two levels, each presented as its own section of the Global settings window (per `GLS-global-settings.md` GLS-FR-03): **AI API**, the level of API endpoints the application calls itself, populated from "list ai api integrations"; and **Agentic AI**, the agentic level, populated from "list agentic integrations". A level issues its own list operation when its section is opened, and neither level's operations are issued on account of the other.
2. **AII-FR-02** The two levels share no control: no field, selector, status line, or activation marker in one level renders or reflects anything belonging to the other. Only the selected section is presented, so the two are never on screen together, and neither renders a heading of its own — the section heading names it.
3. **AII-FR-03** Each level presents one tab per integration it supports — OpenRouter, Anthropic, OpenAI, and Custom in that order in the AI API level, OpenRouter first and selected when the section opens; Claude Code, Codex, OpenCode, Claude Agent API, and Custom agent API in the Agentic level — each carrying that integration's complete configuration. A level orders its own tabs, independently of the order its list operation returns records in. No field is shared between tabs, and moving between tabs neither discards nor applies anything.
4. **AII-FR-04** Exactly one integration is active within each level, and activating one in either level leaves the other level's active choice untouched, so the application can hold a conversation through one provider while driving an agent through an unrelated backend.
5. **AII-FR-05** Every tab in the AI API level renders the same controls in the same order: the base URL field, the API key field, the Verify action, the verification status line, the model selector, the reasoning selector where one is rendered (AII-FR-41), the turn timeout field (AII-FR-QSOR), and the activation control.
6. **AII-FR-06** An API-level tab's base URL field is prefilled with that provider's default and remains editable, so a provider reached through a gateway is configurable; the Custom tab renders it empty, labels it as the gateway URL, and states that it is a host or base URL without `/v1`. It is the tab through which the company LLM Gateway is configured, and the only Custom tab: the section has no second gateway tab and no authentication-mode selector.
7. **AII-FR-07** An API key field is a password-style field that is never populated with a stored key. A tab whose key is stored renders the record's masked hint beside the field, says that leaving the field empty re-verifies with the key already held, and does exactly that (per `../core/AAP-ai-api-integrations.md` AAP-FR-32) — so a configured provider is checked again without the author holding their key a second time. A key the author types replaces the stored one only on the next successful verification.
8. **AII-FR-08** Every API-level tab states that its key is required, and the Custom tab labels the field as the gateway secret and states that it is sent as a bearer token. Verifying with the field empty while no key is stored renders the `key_missing` failure in the status line, and no request is sent.
9. **AII-FR-09** Verification is explicit: activating Verify in an API-level tab invokes "verify ai api integration" with the tab's provider, the base URL currently in the field, and the key currently in the field. The status line renders the outcome — verified with the reported time on success, or the typed failure the operation returned, named in terms the author can act on.
10. **AII-FR-10** A successful verification is what commits an API-level configuration; the fields hold candidates until then, and a base URL or key that has never verified successfully is not stored. Reopening the section shows the last configuration that verified, not an abandoned candidate.
11. **AII-FR-11** Editing an API-level tab's base URL or key away from what last verified returns the tab to an unverified presentation, without invoking any operation.
12. **AII-FR-12** An API-level tab's model selector is the filterable selector of AII-FR-36. It offers the options carried by the provider's record, preceded by an entry meaning the provider's own default, which is what an unset selection renders as. The status line beneath it states whether the options came from the endpoint or from the application's bundled list, and choosing one invokes "set ai api model" at once. On the Custom tab the status line states that the list came from the Custom gateway, and each option may name the route or routes its model supports: "chat", "responses", or both where the model's `mode` is null or absent (per `../core/AAP-ai-api-integrations.md` AAP-FR-RTMZ). Filtering and keyboard behaviour are those of AII-FR-36 through AII-FR-40.
13. **AII-FR-13** An API-level tab's activation control is enabled only while that integration is verified. Activating it invokes "set active ai api integration" for that provider; on success that tab renders the active marker and every other tab in the level renders its activation control again. The marker follows the `active` flag of each record rather than the author's last click, so the sole verified provider on a machine renders as active without ever having been activated (per `../core/AAP-ai-api-integrations.md` AAP-FR-14).
14. **AII-FR-14** Clear in an API-level tab invokes "clear ai api integration" for that provider, returning it to its unconfigured presentation: the provider's default base URL, no stored key, no masked hint, no model selection, no discovered model list, and no activation.
15. **AII-FR-15** An API integration whose stored key can no longer be read renders as such in its status line and keeps its stored base URL and model selection; it is neither cleared nor silently re-verified, and it cannot be activated until it verifies again.
16. **AII-FR-16** The Agentic level's tab strip holds CLI-kind and API-kind integrations together in one strip. A tab renders the configuration fields its own integration has and no others — a binary path field with detection and a file picker for a CLI, that same field plus an OAuth token field for Claude Code (AII-FR-49), and a base URL field and an API key field for an API agent — while the Verify action, the verification status line, the model selector, the reasoning-effort selector, the per-task section, and the activation control are common to both kinds and occupy the same positions in every tab.
17. **AII-FR-17** When a CLI-kind agentic tab renders with no stored path, the section invokes "detect agentic cli binary" for that vendor and fills the field with what it returns, marking the value as detected. A detection that finds nothing leaves the field empty and says so in the status line.
18. **AII-FR-18** The author may replace a CLI-kind tab's path with one of their own, typed into the field or chosen through a file picker. A path the author supplied is marked as such and is never overwritten by a later detection.
19. **AII-FR-19** An API-kind agentic tab's base URL field is prefilled with that vendor's default and remains editable; the Custom agent API tab renders it empty, and it is the tab through which a self-hosted or relocated agent-execution deployment is configured. Its API key field behaves exactly as an API-level one does (AII-FR-07).
20. **AII-FR-20** Activating Verify in an agentic tab invokes "verify agentic integration" with the tab's vendor and the configuration that integration carries — the path for Codex and OpenCode, the path together with a newly entered token for Claude Code (AII-FR-51), the base URL and key for an API agent. The status line renders the outcome: the reported version on success, or the typed failure the operation returned, named in terms the author can act on.
21. **AII-FR-21** A successful verification is what commits an agentic configuration; the fields hold candidates until then, and a configuration that has never verified successfully is not stored. Reopening the section shows the last configuration that verified, not an abandoned candidate.
22. **AII-FR-22** Editing an agentic tab's configuration away from what last verified returns the tab to an unverified presentation and clears the displayed version, without invoking any operation.
23. **AII-FR-23** An agentic tab's model selector is the same filterable selector as an API-level tab's (AII-FR-36). It offers the options carried by the vendor's record, preceded by an entry meaning the backend's own default. The status line beneath it states whether the options came from the installed CLI or the endpoint, or from the application's bundled list, and choosing one invokes "set agentic integration model" at once with no task named, which is what sets the backend's default model.
    The **model selector of a per-task row** (AII-FR-QDLW) selects the model that row's kind of work uses. It offers a "same as default" entry first, followed by the same options the default selector offers, in the same filterable panel; choosing an option invokes "set agentic integration model" naming that row's canonical task kind, and choosing "same as default" invokes it naming that same kind with no model, which returns the row to the default. Every agentic tab renders the default selector and the per-task section's model column, whichever vendor the tab is for.
24. **AII-FR-24** An agentic tab's reasoning-effort selector offers the vendor's declared effort levels, preceded by the same backend-default entry, and choosing one invokes "set agentic integration effort" at once with no task named, which is what sets the backend's default effort. A vendor that declares no effort levels renders no effort selector at all.
    The **effort selector of a per-task row** (AII-FR-QDLW) selects the effort that row's kind of work uses. It offers a "same as default" entry first, followed by the same levels the default selector offers; choosing a level invokes "set agentic integration effort" naming that row's canonical task kind, and choosing "same as default" invokes it naming that same kind with no effort, which returns the row to the default. A vendor that renders no effort selector renders no effort column in the per-task section either, and no disabled control stands in its place.
25. **AII-FR-QDLW** An agentic tab holds **one per-task section**, a single disclosure sitting below both default selectors and holding one row for each kind of work a run hands to this backend — Work, Review, and Reconciliation, in that order. A row is that kind's label followed by its own model selector and its own effort selector on one line, aligned into a model column and an effort column headed by the name of the default each follows. Each selector's accessible name carries both the row's task label and the dimension it selects, so the two are told apart by anybody who cannot see which column they stand in. The two selectors of a row are independent of each other and of every other row: a kind of work may take a model of its own and the default effort, an effort of its own and the default model, both, or neither, and returning one selector to "same as default" returns that kind to one default and never to two. The section holds one open state, so the author opens the three kinds of work once and sees every selection a run resolves in a single reading.
    **A row is labelled for the author and identified by its canonical task kind**, and the two are fixed against each other: the Work row is `work`, the Review row is `review`, and the **Reconciliation row is `semantic_rebase`** — the three kinds of turn the execution boundary accepts (per `../tools/EAC-execute-agent-cli.md` EAC-FR-IRRD), the label being what the run surfaces of this application already call that work, and the canonical kind being the one the backend holds its selections under (per `../core/AIC-agentic-integrations.md` AIC-FR-10). The section sends the canonical kind and never the label: every "set agentic integration model" and "set agentic integration effort" a row invokes names that row's canonical kind, and each of a row's two selectors reads its own value from the record's entry under that same kind. What a row shows is therefore what a turn of that kind resolves when the run reaches it (per `../core/AIC-agentic-integrations.md` AIC-FR-19), and the Reconciliation row is what a semantic-rebase turn runs on.
26. **AII-FR-25** An agentic tab's activation control is enabled only while that integration is verified. Activating it invokes "set active agentic integration" for that vendor; on success that tab renders the active marker and every other tab in the level renders its activation control again. The marker follows the `active` flag of each record, so the sole verified backend on a machine renders as active without ever having been activated (per `../core/AIC-agentic-integrations.md` AIC-FR-13).
27. **AII-FR-PMZK** The per-task section is closed until the author opens it, and its summary counts the rows that differ from the defaults rather than naming them. A row counts once whether it overrides its model, its effort, or both, so the summary is a count of the kinds of work configured apart from the defaults and never of the selectors changed, and it reads out of three whichever vendor the tab is for. A tab whose backend is configured one way for everything therefore reads as the two default selectors and one closed line.
    **The summary counts what the section renders**, so it and the rows beneath it always agree. A record can hold an override in a dimension the tab has no column for — the backend keys its two maps on the turn kind alone and keeps a stored selection through a degradation (per `../core/AIC-agentic-integrations.md` AIC-FR-10, and AII-FR-27) — and an override the author is shown no selector for is counted by neither the summary nor the row. It is held, not lost: the section neither clears it nor writes on its account, and it is the value the row reads again if the vendor comes to declare that dimension. What the summary therefore reports is the number of kinds of work the author can see configured apart, which is the number they can act on.
28. **AII-FR-26** Clear in an agentic tab invokes "clear agentic integration" for that vendor, returning it to its unconfigured presentation: no path or base URL, no stored credential and no masked hint, no version, no selections, and no activation. Detection does not re-run for the rest of that visit to the section, because refilling the field the author just emptied would undo the action they took. Leaving the section ends that suppression: on a later visit the tab renders with no stored path like any other, so AII-FR-17 applies again and a detected path is offered afresh — an offer, never a stored value, and identical to what the tab shows after a relaunch.
28. **AII-FR-27** An agentic integration the backend reports as degraded renders as such in its status line and keeps its stored configuration and selections — a tab whose binary no longer exists on disk, and a tab whose stored credential can no longer be read, which the Claude Code tab can report as readily as an API-kind one. None is cleared nor silently re-detected, and none can be activated until it verifies again.
29. **AII-FR-28** Every action in this section applies immediately rather than through a section save, so the section holds no dirty state and contributes nothing to the Global settings window's save-before-close sweep (per `GLS-global-settings.md` GLS-FR-13 and GLS-FR-16). A selection the backend refuses changes nothing the section shows: the control keeps the value it held before the choice, every summary count keeps the number it held, and the typed error the operation returned is rendered inline beside that control rather than as a modal or a transient notification.
30. **AII-FR-29** A tab whose integration is unconfigured renders a first-class empty state naming what is needed rather than an error, and both levels render and are navigable with every integration in the section unconfigured.
31. **AII-FR-30** No credential is displayed anywhere in this section beyond its record's masked hint, in either level and whether it is an API key or Claude Code's OAuth token. A credential the author types is not echoed back into the field's rendered value, into a status line, into a validation message, into an error message, or into any URL the section displays.
32. **AII-FR-31** The Project section of Project settings names both the agentic integration and the AI API integration the open project resolves to and how each resolved, read from "get project agentic integration" and "get project ai api integration" when the section mounts (per `SET-project-settings.md` SET-FR-13 and SET-FR-14): as inherited from that level's global choice, as an override belonging to this project, or as nothing resolved at all.
33. **AII-FR-32** Each control's Change action offers the integrations of its own level that are configured and verified, plus an entry meaning "inherit the global choice". Confirming an integration invokes that level's "set project agentic integration" or "set project ai api integration" with it, and confirming the inherit entry invokes the same operation with none, clearing that level's override. Either applies at once, so both controls sit outside the Project section's dirty state (per `SET-project-settings.md` SET-FR-08).
34. **AII-FR-33** When a get operation reports that a recorded override no longer resolves — its integration was cleared or has stopped verifying — that level's control says so and names what is in effect instead, rather than presenting the dead override as the project's choice.
35. **AII-FR-34** When a level has no integration configured at all, its control says so and names that level's own Global settings section — AI API or Agentic AI — as where one is configured, exactly as the GitHub token control names its own section (per `SET-project-settings.md` SET-FR-12).
36. **AII-FR-35** Neither Project settings control renders a base URL, an API key, a masked hint, a binary path, a model list, or a verification state; each is a single line naming the resolved integration and how it resolved, plus one action.
37. **AII-FR-36** A model selector in either level is a selector the section renders itself rather than a native one, and it opens into a panel carrying a filter box above its options. The box takes focus when the panel opens, so a model is reached by typing rather than by scrolling a list that may run to hundreds of entries. What the author typed is discarded when the panel closes, so the selector always reopens showing everything the record offers.
38. **AII-FR-37** The filter narrows the panel to the options whose label or whose model identifier contains what the author typed, matched without regard to case and without requiring the match to begin either string. Matching applies to every entry in the panel, including the default entry, and an empty box offers all of them.
39. **AII-FR-38** The selector states how many options the filter is showing out of how many the record carries, so a filter narrowing hundreds of models to a handful reports what it did rather than leaving the author to guess whether the rest exist.
40. **AII-FR-39** A filter matching nothing renders a first-class row in the panel saying so rather than an empty panel, nothing in that state is selectable, and the current selection is left as it was.
41. **AII-FR-40** The selector is operable from the keyboard alone: the arrow keys move a highlight through the options the filter is showing, Enter commits the highlighted option exactly as clicking it would, and Escape closes the panel leaving the selection untouched. Closing by either route returns focus to the selector.
42. **AII-FR-41** The OpenRouter tab renders a reasoning selector, and renders it only while a model is selected whose record declares reasoning. The provider-default model entry carries no model whose capability is known, so no reasoning selector accompanies it, and no other tab in the AI API level renders one. In particular the Custom tab renders none: the gateway model list declares no reasoning, so Custom models use the model default.
43. **AII-FR-42** Where the selected model declares levels of its own, the reasoning selector offers exactly those levels, in the order the record carries them and no others, so the author is never offered a depth the selected model cannot honour.
44. **AII-FR-43** Where the selected model declares reasoning but no levels, the reasoning selector offers only off and on. A model whose reasoning is mandatory offers no off entry in either shape, and the selector says that the model always reasons rather than presenting a disabled control.
45. **AII-FR-44** The reasoning selector is preceded by an entry meaning the model's own default, which is what an unset choice renders as, and which states what the model does when nothing is asked of it.
46. **AII-FR-45** Choosing a reasoning entry invokes "set ai api reasoning" at once, and choosing the model-default entry invokes it with none.
47. **AII-FR-46** Changing the selected model re-renders the reasoning selector against what the newly selected model declares, and a choice the new model does not offer is not carried across — the selector returns to the model-default entry, matching the choice the record now holds (per `../core/AAP-ai-api-integrations.md` AAP-FR-29).
48. **AII-FR-47** A level the record carries that the application ships no knowledge of is offered like any other, labelled with the identifier the record gives it, because the record's levels are the whole of what the selector presents.
49. **AII-FR-48** When the OpenRouter tab holds a model list in which no model declares reasoning at all, the tab says the endpoint must be verified again before reasoning can be offered. A list that predates the application's knowledge of reasoning is indistinguishable, from the selector's side, from a list of models that simply cannot reason, and in both cases AII-FR-41 renders no reasoning row — so without this the author is left with a control that is absent for no stated reason.
50. **AII-FR-49** The Claude Code tab renders an OAuth token field, and no other tab in either level renders one — the only other credential fields in the section are the API key fields of AII-FR-07 and AII-FR-19. The Codex and OpenCode tabs render no token field and hold no token value, and nothing an author types into the Claude Code field is carried into any other tab of either level. The field is a password-style field that is never populated with a stored token, and it states that Claude Code requires one because that CLI is expected to run where it cannot sign in for itself.
51. **AII-FR-50** The section validates a token the author types against `^sk-ant-oat01-[A-Za-z0-9-]+$` and against nothing else, locally and on every keystroke, invoking no operation and reaching no service to do it. A value that is non-empty and does not match renders an inline validation message on the field naming the shape a token takes; that message quotes no part of what was entered, and an empty field renders no such message.
52. **AII-FR-51** Verify is available in the Claude Code tab only where the tab can produce a complete configuration: a token that satisfies AII-FR-50 is entered, or the field is empty and the record reports a stored token. An empty field with no stored token leaves Verify unavailable, and a non-matching value leaves it unavailable whatever the record holds, so the section never submits a configuration the backend would refuse as `token_missing` or `token_malformed`.
53. **AII-FR-52** A Claude Code tab whose token is stored renders the record's masked hint beside the empty field, says that leaving the field empty re-verifies with the token already held, and does exactly that — invoking "verify agentic integration" with the path alone, which the frontend can do without ever reading the token. A token the author types replaces the stored one only on the next successful verification, and the same masked hint is rendered afterwards whichever route verified.
54. **AII-FR-53** A token the author types exists in the section only as the value of the field it was typed into, and only until the submission carrying it resolves: it is not retained in any persisted UI state, not carried across a tab change or a visit to another section, and not restored when the tab is reopened. A tab reopened after a successful verification renders an empty field and a masked hint, which is indistinguishable from what it renders after a relaunch.
55. **AII-FR-54** The AI API level explains, above its tab strip and before any field, what it configures and who uses it: the endpoints the application calls for itself, whose model and reasoning selections serve the graduation loop alone, while a conversational agent runs on the model and reasoning its own description carries in the Agents section, served by the endpoint and key of the AI API provider the open project resolves to (`AGT-agents.md` AGT-FR-01, and `GLS-global-settings.md` GLS-FR-24 for where that section sits) and an agent turn a graduation hands out runs on the model and the reasoning effort the Agentic AI section holds for that kind of task. The explanation names the Agents section as where an agent's model is chosen, and no text in this level says or implies that a selection made here changes what an agent runs on. It also says that agents are served by the provider active for the project, so activating a provider here changes the endpoint every agent uses. The selections themselves are unaffected by it: the model selector and the reasoning selector behave exactly as AII-FR-12 and AII-FR-41 through AII-FR-46 state.
56. **AII-FR-QSOR** Every tab in the AI API level renders a **Turn timeout** field that holds whole seconds. It shows the provider's stored turn timeout, or is empty with the placeholder `300` where none is stored. A whole number from 30 to 3600 is committed at once (AII-FR-28), an empty field clears the stored value, and any other entry is not committed.
57. **AII-FR-QTZF** One line beneath the turn timeout field says that it bounds each agent conversation turn on this provider, and that an empty field uses the project's execution timeout or five minutes. On blur, an entry that was not committed is replaced by the stored value.

## Non-functional requirements
- Each section renders, navigates, and reports every integration's stored configuration without network access and without running any binary; nothing in either contacts a provider or an endpoint until the author activates Verify.
- Verification is the only action in either section that leaves the machine or runs another program, and the section stays interactive while it is in flight — the Verify action reports that it is working and every other control remains usable.
- The only credential-derived text either section ever renders is a masked hint. A key or a token is entered in its own field and nowhere else, and is not written to any log, error surface, or persisted UI state.
- Validating Claude Code's token is structural and entirely local: it decides whether a value is shaped like a token and nothing more, so the field answers on every keystroke on a machine with no network, and a token that is well-formed but revoked is indistinguishable here from one that works.
- Both sections are usable with no CLI installed, no endpoint reachable, and no key held: every control in both levels renders, and the only thing the author cannot do is activate.
- Both levels present the same shape — tabs, a verification status line, a model selector, an activation control — so that the difference between calling a model and driving an agent is a matter of which level the author is in, not of learning a second surface.
- Filtering a model list is entirely local: typing narrows options the record already carries and issues no operation and no request, so a provider offering hundreds of models stays responsive to every keystroke on a machine with no network.
- The reasoning the section offers for a model is only ever what that model's record declares. The section holds no list of level names of its own, so a provider adding, renaming, or withdrawing a level changes what the author sees without any change here.
- The model selector is reachable and operable from the keyboard alone, and it is the same control in both levels, so nothing about choosing a model has to be learned twice.

## Open questions
- Whether two selections made in quick succession should be ordered, refused, or left to settle as they arrive. Every action in this section applies at once (AII-FR-28) and none of them sequences against another: a model, a reasoning level, an activation, and a Clear each replace the level's copy of a record when the backend answers, so overlapping actions settle in the order the answers arrive rather than the order the author made them. The case that matters is a reasoning choice sent while a model change is still in flight, because the backend validates a reasoning choice against the model it has already stored (per `../core/AAP-ai-api-integrations.md` AAP-FR-27) rather than the one the author was looking at.
- Whether an action in flight should be visible. The verification status line reports that Verify is working (AII-FR-09), and no selector reports anything of the kind, so a selection against a slow backend looks like a selector that did not take.
