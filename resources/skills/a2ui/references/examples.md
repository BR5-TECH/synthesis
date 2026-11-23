# Examples

Each example is a complete `show_surface` panel: `components` and `data_model`. Start from the one nearest to the case, and change it.

## A short form with a check

```json
{
  "components": [
    { "id": "root", "component": "Column", "children": ["intro", "name", "kind", "notify", "save"] },
    { "id": "intro", "component": "Text", "text": "Name the new environment." },
    { "id": "name", "component": "TextField", "label": "Name", "value": { "@path": "/name" } },
    {
      "id": "kind",
      "component": "ChoicePicker",
      "label": "Kind",
      "options": [
        { "label": "Staging", "value": "staging" },
        { "label": "Production", "value": "production" }
      ],
      "value": { "@path": "/kind" }
    },
    { "id": "notify", "component": "CheckBox", "label": "Tell the team", "value": { "@path": "/notify" } },
    {
      "id": "save",
      "component": "Button",
      "variant": "primary",
      "child": "save_label",
      "action": {
        "event": {
          "name": "create_environment",
          "context": { "name": { "@path": "/name" }, "kind": { "@path": "/kind" }, "notify": { "@path": "/notify" } }
        }
      },
      "checks": [
        { "condition": { "@call": "required", "args": { "value": { "@path": "/name" } } }, "message": "Give a name." },
        { "condition": { "@call": "required", "args": { "value": { "@path": "/kind" } } }, "message": "Pick a kind." }
      ]
    },
    { "id": "save_label", "component": "Text", "text": "Create" }
  ],
  "data_model": { "name": "", "kind": [], "notify": false }
}
```

## A table with a summary line

```json
{
  "components": [
    { "id": "root", "component": "Column", "children": ["table", "total"] },
    {
      "id": "table",
      "component": "Table",
      "caption": "Test runs this week",
      "columns": [
        { "key": "suite", "title": "Suite" },
        { "key": "runs", "title": "Runs" },
        { "key": "failed", "title": "Failed" }
      ],
      "rows": { "@path": "/rows" }
    },
    {
      "id": "total",
      "component": "Text",
      "variant": "caption",
      "text": { "@call": "formatString", "args": { "value": "${/failed} of ${/runs} runs failed." } }
    }
  ],
  "data_model": {
    "rows": [
      { "suite": "backend", "runs": 40, "failed": 1 },
      { "suite": "frontend", "runs": 52, "failed": 0 }
    ],
    "runs": 92,
    "failed": 1
  }
}
```

## Tabs with a list template

```json
{
  "components": [
    { "id": "root", "component": "Tabs", "tabs": [{ "title": "Open", "child": "open" }, { "title": "Done", "child": "done" }] },
    { "id": "open", "component": "List", "children": { "componentId": "open_item", "path": "/open" } },
    { "id": "open_item", "component": "Text", "text": { "@path": "title" } },
    { "id": "done", "component": "List", "children": { "componentId": "done_item", "path": "/done" } },
    { "id": "done_item", "component": "Text", "variant": "caption", "text": { "@path": "title" } }
  ],
  "data_model": { "open": [{ "title": "Write the spec" }], "done": [{ "title": "Read the issue" }] }
}
```
