# Cards

A card is a whole panel in one component. Put it as the only component, with the id `root`. Every text of a card is a plain string. The values of the options of one card are unique.

## ChoiceCard

A question with 2 to 8 options and a Send button.

| Property | Required | Meaning |
| --- | --- | --- |
| `question` | yes | The question. |
| `options` | yes | 2 to 8 entries `{ value, label, description? }`. |
| `multiple` | no | `true` lets the author choose more than one option. |
| `allowOther` | no | `true` adds a field for the author's own answer. |
| `sendLabel` | no | The label of the button. The default is `Send`. |
| `action` | yes | `{ name }`: the action name you get back. |

It sends `{ "selected": [values], "other": "text" }`. `other` is present only with `allowOther`.

```json
{
  "components": [
    {
      "id": "root",
      "component": "ChoiceCard",
      "question": "Which storage do you want for the cache?",
      "options": [
        { "value": "sqlite", "label": "SQLite", "description": "One file, transactions, small footprint." },
        { "value": "files", "label": "Plain files", "description": "No dependency, no queries." }
      ],
      "allowOther": true,
      "action": { "name": "choose_storage" }
    }
  ]
}
```

## ConfirmCard

A yes or no decision. Each button sends its own action name, with no values.

| Property | Required | Meaning |
| --- | --- | --- |
| `title` | yes | What the author decides. |
| `body` | no | The detail of the decision. |
| `confirm` | yes | `{ label, name }`: the main button. |
| `decline` | no | `{ label, name }`: the other button. |
| `tone` | no | `danger` for a step that removes or overwrites something. |

```json
{
  "components": [
    {
      "id": "root",
      "component": "ConfirmCard",
      "title": "Delete the three stale changes?",
      "body": "Their discussions are deleted with them.",
      "confirm": { "label": "Delete", "name": "delete_changes" },
      "decline": { "label": "Keep them", "name": "keep_changes" },
      "tone": "danger"
    }
  ]
}
```

## ComparisonCard

Options down the side, criteria across the top. With `pick`, the author picks one option.

| Property | Required | Meaning |
| --- | --- | --- |
| `title` | no | What is compared. |
| `criteria` | yes | 1 to 8 entries `{ key, title }`. |
| `options` | yes | 2 to 8 entries `{ value, label, values, recommended? }`. `values` maps a criterion key to a string, a number, or a boolean. |
| `pick` | no | `{ name, label? }`: the action of the Pick button. The default label is `Pick`. |

It sends `{ "picked": value }`.

```json
{
  "components": [
    {
      "id": "root",
      "component": "ComparisonCard",
      "title": "Queue backends",
      "criteria": [
        { "key": "ops", "title": "Operations" },
        { "key": "latency", "title": "Latency (ms)" }
      ],
      "options": [
        { "value": "redis", "label": "Redis", "values": { "ops": "One more service", "latency": 1 }, "recommended": true },
        { "value": "postgres", "label": "Postgres", "values": { "ops": "Already deployed", "latency": 5 } }
      ],
      "pick": { "name": "pick_queue" }
    }
  ]
}
```

## ChecklistCard

Items with checkboxes and a Send button. It sends the checked values, also when none is checked.

| Property | Required | Meaning |
| --- | --- | --- |
| `title` | no | What the list is for. |
| `items` | yes | 1 to 30 entries `{ value, label, description?, checked? }`. |
| `sendLabel` | no | The label of the button. The default is `Send`. |
| `action` | yes | `{ name }`: the action name you get back. |

It sends `{ "checked": [values] }`.

```json
{
  "components": [
    {
      "id": "root",
      "component": "ChecklistCard",
      "title": "Files to include in the change",
      "items": [
        { "value": "api", "label": "src/api.ts", "checked": true },
        { "value": "types", "label": "src/types.ts", "checked": true },
        { "value": "readme", "label": "README.md", "description": "Only the usage section changes." }
      ],
      "sendLabel": "Use these files",
      "action": { "name": "pick_files" }
    }
  ]
}
```
