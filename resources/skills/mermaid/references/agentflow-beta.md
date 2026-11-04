# Agent flow diagram

Declaration: `agentflow-beta`

Use an agent flow to show how an AI agent works: its tasks, the tools it calls, its decisions, its inputs, and the reference documents it reads. The syntax is a strict subset of the flowchart syntax with a fixed set of node kinds and three edge kinds.

## Syntax

### Header and direction

The first line is `agentflow-beta`, then an optional direction on the same line: `TB` or `TD` (top to bottom, the default), `BT`, `LR`, or `RL`. Example: `agentflow-beta LR`.

A `direction LR` line inside a `flow` block sets the direction of that block only.

### Nodes and node kinds

A node has an id and an optional label. Write the label in double quotes when it has spaces or special characters. The `@{ shape: … }` block sets the kind. Each kind has its own shape and its own colour.

| Kind | Write it as | Shape | Meaning |
|---|---|---|---|
| task | `id["Label"]@{ shape: task }` | rounded rectangle | A unit of work that the agent does. This is the default kind. |
| tool | `id["Label"]@{ shape: tool }` | framed rectangle | A tool that the agent calls. |
| decision | `id["Label?"]@{ shape: decision }` | diamond | A branch point. |
| input | `id["Label"]@{ shape: input }` | parallelogram | A value that the user or caller gives. |
| refdoc | `id["Label"]@{ shape: refdoc }` | lined document | A reference document that a step reads. |
| action | `id["Label"]@{ shape: action }` | hexagon | An action with an effect outside the agent. |

Shape names must be lowercase. The canonical names `roundedRect`, `subroutine`, `subprocess`, `diamond`, `lean-right`, `lin-doc`, `hexagon`, and `hex` also work.

These bracket forms are short forms of the same kinds:

| Bracket form | Kind |
|---|---|
| `id[Label]`, `id(Label)`, `id` | task |
| `id[[Label]]` | tool |
| `id{Label}` | decision |
| `id{{Label}}` | action |

There is no bracket form for input and refdoc. Use `@{ shape: input }` and `@{ shape: refdoc }`.

The label can also go in the metadata block: `id@{ shape: tool, label: "web_search" }`.

You can declare a node first and use only its id later. You can also declare a node inside an edge: `a --> b["Check"]@{ shape: decision }`.

### Edges

Only three edge operators have a meaning.

| Operator | Kind | Meaning |
|---|---|---|
| `a --> b` | sequence | Control or data goes from `a` to `b`. |
| `a -.- b` | reference | `a` reads `b`. No control goes there. The line is dotted and has no arrowhead. |
| `a --x b` | failure | The failure path out of `a`. |

Labels go on sequence and failure edges:

```
check -- yes --> ship
check -->|no| fix
call -- timeout --x retry
```

Do not put a label on a `-.-` edge. A reference edge has no label meaning.

Extra dashes make an edge longer: `--->`, `---->`, `-..-`. The kind does not change.

Chains and groups work as in a flowchart: `a --> b --> c`, `a & b --> c`, `a --> b & c`.

An edge id goes before the operator: `a e1@--> b`. Then `e1@{ animate: true }` animates the edge, and `e1@{ curve: linear }` sets its curve.

### Metadata blocks

The `@{ … }` block after a node id is YAML. Put a single-line block in flow style, with commas between the keys. Use a multi-line block for nested values.

```
search["web_search"]@{ shape: tool, description: "Searches the web" }
fetch["fetch_page"]@{
  shape: tool
  description: "Gets one page"
  params:
    url: string
}
```

Only `shape`, `label`, and `labelType` change the drawing of a node. Only `view` changes a `flow`. Only `animate`, `animation`, and `curve` change an edge. All other keys, such as `description`, `instruction`, or `params`, are stored as data for tools and do not show in the drawing.

You can attach a block later with the bare id: `search@{ description: "Searches the web" }`.

### Containers: `flow`

A `flow` block groups steps. Close it with `end`. Blocks can be nested.

```
flow research["Research"]
  plan --> search
end
```

Always give a flow an id and a title in square brackets. `flow My flow` makes an id `My flow` with no title.

A node belongs to the first `flow` block that mentions it, also through an edge. So write the edges between flows after the flow blocks. To show a flow as one compact node, add `research@{ view: collapsed }`. Edges to its hidden nodes then attach to the collapsed node.

### Global nodes: `global`

A `global … end` block marks nodes that belong to no flow, for example `global` / `memory["Shared memory"]` / `end` on three lines. Use it for a shared node that edges inside several flows use.

### Connectors: `connector`

A `connector` line declares an external system, for example an MCP server or an API. It draws as a node with its own colour.

```
connector github["GitHub"]
connector slack["Slack"]@{ protocol: mcp }
```

Do not use `@{ shape: connector }`. It fails. Use the `connector` keyword.

### Other statements

| Statement | Effect |
|---|---|
| `%% text` | A comment. It can also go at the end of a line. |
| `accTitle: text` | The accessible title. |
| `accDescr: text` or `accDescr { … }` | The accessible description. |
| `;` | Ends a statement, like a new line. |

## Styling and configuration

The flowchart style statements work: `classDef`, `class`, `:::`, `style`, `linkStyle`, and `click`.

```
classDef risky stroke:#c00,stroke-width:2px
class deploy risky
deploy:::risky
style plan fill:#eef
linkStyle 0 stroke:#888
click docs "https://example.com" _blank
```

`linkStyle` numbers count the edges from 0, in source order. A number that is too high makes the parse fail.

A title comes only from frontmatter. Frontmatter `config:` reads these keys under `agentflow:`: `nodeSpacing`, `rankSpacing`, `diagramPadding`, `titleTopMargin`, `wrappingWidth`, `minNodeWidth`, and `useMaxWidth`. It also reads the global `look`, `theme`, and `layout`.

```
---
config:
  agentflow:
    nodeSpacing: 40
---
```

## Examples

```mermaid
---
title: Release notes agent
---
agentflow-beta TB
  changes["Merged pull requests"]@{ shape: input }
  style_guide["Style guide"]@{ shape: refdoc }
  draft["Draft the notes"]@{ shape: task }
  search["changelog_search"]@{ shape: tool, description: "Finds older entries" }
  ok["Accurate?"]@{ shape: decision }
  publish["Publish the notes"]@{ shape: action }
  changes --> draft
  draft -.- style_guide
  draft --> search --> ok
  ok -- yes --> publish
  ok -- no --> draft
  publish -- rejected --x draft
```

```mermaid
agentflow-beta LR
  connector github["GitHub"]@{ protocol: mcp }
  global
    memory["Shared memory"]@{ shape: refdoc }
  end
  flow triage["Triage"]
    issue["New issue"]@{ shape: input }
    label["label_issue"]@{ shape: tool }
    dup{"Duplicate?"}
    issue --> dup
    dup -- no --> label
    dup -.- memory
  end
  flow fix["Fix"]
    direction TB
    plan["Plan the change"]
    patch[["apply_patch"]]
    pr{{"Open pull request"}}
    plan --> patch --> pr
    plan -.- memory
  end
  label --> plan
  pr --> github
```

```mermaid
agentflow-beta TB
  flow support["Support agent"]
    ask["Customer question"]@{ shape: input }
    answer["Write the answer"]
    kb["kb_lookup"]@{
      shape: tool
      description: "Searches the knowledge base"
      params:
        query: string
    }
    ask --> answer
    answer e1@--> kb
  end
  flow billing["Billing checks"]
    rules["Check refund rules"]
    ledger[["ledger_query"]]
    rules --> ledger
  end
  escalate["Escalate to a person"]@{ shape: action }
  answer --> rules
  kb -- no result --x escalate
  e1@{ animate: true }
  billing@{ view: collapsed }
  classDef human stroke:#c60,stroke-width:2px
  class escalate human
```

## Common mistakes

- These operators do not parse: `---`, `==>`, `-.->`, `<-->`, `--o`, `-. text .-`. Use `-->`, `-.-`, or `--x`.
- `subgraph` is not a keyword. Use `flow … end`.
- Do not write `graph TD` or `flowchart LR`. Put the direction after `agentflow-beta`.
- A `title …` line does not parse. Put the title in frontmatter.
- `flow`, `global`, `connector`, `end`, `class`, and `style` are keywords. Do not use them as node ids. Ids that only start with them, such as `endpoint`, are correct. Do not use the id `connectors`. It is reserved.
- `@{ shape: Task }` fails because shape names must be lowercase. An unknown shape name, such as `shape: foo`, also fails.
- Old flowchart shapes are replaced by the task shape. This applies to `((…))`, `([…])`, `[(…)]`, `>…]`, `[/…/]`, `[\…\]`, and `@{ shape: stadium }`, `circle`, `cylinder`, `doc`. Use the six kinds.
- A single-line `@{ … }` block cannot contain `{` or `}`. Use a multi-line block for nested maps.
- Quote a metadata value that contains `: `, `,`, `#`, or brackets: `description: "Step 1: read"`. An unquoted `: ` fails. An unquoted `,` silently splits the value.
- A label with `(`, `)`, `[`, `]`, `{`, `}`, or `|` must be in double quotes: `a["Run (dry)"]`. Do not put a double quote inside a quoted label. Use `#quot;`.
- Edge text on a split edge cannot contain `--`: `a -- re--run --> b` fails. Use `a -->|re--run| b`.
