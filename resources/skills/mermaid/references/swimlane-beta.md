# Swimlane diagram

Declaration: `swimlane-beta`

Use a swimlane diagram to show a process where each step belongs to one role, team, or system. The syntax is the flowchart syntax. Each top-level `subgraph` becomes a lane, and a special layout keeps every node inside its lane.

## Syntax

### Header and direction

```
swimlane-beta LR
```

| Direction | Flow of the steps | Lanes |
|---|---|---|
| `TB` or `TD` (default) | top to bottom | columns, side by side |
| `BT` | bottom to top | columns |
| `LR` | left to right | rows, one above the other |
| `RL` | right to left | rows |

The layout uses one direction for all lanes. A `direction` line inside a lane has no effect on the layout.

### Lanes

A lane is a top-level `subgraph` with an id and a title. Close it with `end`.

```
subgraph sales [Sales]
  quote[Send a quote]
end
subgraph legal["Legal team"]
  review[Review the contract]
end
```

- The title can be `[Title]` or `["Title"]`. Use quotes when the title has special characters.
- Lanes keep their source order.
- A `subgraph` inside a lane is drawn as a box inside that lane.
- A node that is in no lane goes into an extra lane with no title.

A node belongs to the first lane that mentions it. This includes an edge inside a lane. So declare each node inside its own lane, and write the edges that cross lanes after all lanes, at the top level.

### Nodes

| Form | Shape |
|---|---|
| `id[Text]` | rectangle |
| `id(Text)` | rounded rectangle |
| `id([Text])` | stadium |
| `id[[Text]]` | subroutine |
| `id[(Text)]` | cylinder |
| `id((Text))` | circle |
| `id(((Text)))` | double circle |
| `id>Text]` | flag |
| `id{Text}` | diamond |
| `id{{Text}}` | hexagon |
| `id[/Text/]`, `id[\Text\]` | parallelogram |
| `id[/Text\]`, `id[\Text/]` | trapezoid |

The `@{ … }` form selects any flowchart shape by name: `id@{ shape: doc, label: "Invoice" }`. Some names: `rect`, `rounded`, `stadium`, `circle`, `diamond`, `hex`, `cyl`, `doc`, `lean-r`, `trap-b`, `fork`, `cloud`, `bolt`. Shape names are lowercase.

Use `"…"` around text with special characters: `id["Pay (card)"]`. Use `` "`…`" `` for Markdown text: ``id["`**Bold** text`"]``.

### Edges

| Edge | Meaning |
|---|---|
| `a --> b` | arrow |
| `a --- b` | line |
| `a -.-> b` | dotted arrow |
| `a ==> b` | thick arrow |
| `a ~~~ b` | invisible link |
| `a --o b`, `a --x b` | circle end, cross end |
| `a <--> b` | arrows at the two ends |

Labels: `a -->|Yes| b` or `a -- Yes --> b`. Other forms: `a -. maybe .-> b`, `a == must ==> b`.

Chains and groups: `a --> b --> c`, `a & b --> c`. An edge id goes before the operator: `a e1@--> b`, then `e1@{ animate: true }`.

### Other statements

| Statement | Effect |
|---|---|
| `%% text` | A comment line. |
| `accTitle: text`, `accDescr: text` | Accessible title and description. |
| `click id href "https://…" _blank` | A link on a node. |
| `;` | Ends a statement. |

## Styling and configuration

`classDef`, `class`, `:::`, `style`, and `linkStyle` work as in a flowchart.

```
classDef urgent fill:#fdd,stroke:#c00
class review urgent
style quote stroke-width:2px
linkStyle 0 stroke:#888
```

Each lane gets its own colour from the theme. The defaults are `theme: redux-color` and `look: neo`. A title comes only from frontmatter.

Frontmatter `config:` keys under `swimlane:`:

| Key | Default | Effect |
|---|---|---|
| `lineHops` | `arc` | `arc` or `gap` at edge crossings. `false` turns it off. |
| `automaticLaneOrdering` | `false` | `true` reorders lanes to make edges shorter. |
| `ignoreCrossLaneEdges` | `true` | Ignores cross-lane edges when it sets the ranks. |
| `optimizeRanksByCrossings` | `true` | Runs a pass that reduces crossings. |

The `flowchart:` keys `nodeSpacing`, `rankSpacing`, `curve`, and `htmlLabels` also apply.

## Examples

```mermaid
---
title: Online order
---
swimlane-beta LR
  subgraph customer [Customer]
    order([Place the order])
    receive([Receive the box])
  end
  subgraph shop [Shop]
    check{In stock?}
    pack[Pack the items]
    refund[Refund the payment]
  end
  subgraph courier [Courier]
    ship[Ship the box]
  end
  order --> check
  check -->|Yes| pack
  check -->|No| refund
  pack --> ship --> receive
  refund -.-> receive
```

```mermaid
swimlane-beta TB
  subgraph dev ["Developer"]
    code[Write the change]
    fix[Fix the findings]
  end
  subgraph ci ["CI system"]
    test[[Run the tests]]
    db[(Test database)]
  end
  subgraph review ["Reviewer"]
    approve{Approve?}
    merge(((Merged)))
  end
  code --> test
  test --- db
  test --> approve
  approve -- No --> fix
  fix --> test
  approve == Yes ==> merge
  classDef gate fill:#ffe,stroke:#aa0
  class approve gate
```

## Common mistakes

- Do not add `graph LR` or `flowchart LR` after the header. Put the direction on the header line.
- A `title …` line does not parse. Use frontmatter.
- An edge inside a lane pulls a new node into that lane. Write cross-lane edges at the top level.
- `end` is a keyword. Do not use it as a node id. Ids such as `ending` are correct.
- A node id that starts with `o` or `x` right after `---` or `-->` makes a different edge: `a---oB` is a circle edge. Add a space: `a --- oB`.
- Text with `(`, `)`, `[`, `]`, `{`, `}`, or `|` must be in double quotes.
- `linkStyle` numbers count edges from 0. A number that is too high makes the parse fail.
