# Components

Use the components where no card fits. Every property in `!` is required. A text property is a literal string, a binding `{"@path": "/pointer"}`, or a call `{"@call": "<function>", "args": {…}}`.

## Layout

| Component | Properties | Use |
| --- | --- | --- |
| `Column` | `children!`, `justify`, `align` | Children one below the other. |
| `Row` | `children!`, `justify`, `align` | Children side by side. A direct child can have `weight`. |
| `List` | `children!`, `direction` | A list, often a template over a data-model list. |
| `Card` | `child!` | A bordered block around one child. |
| `Tabs` | `tabs!`: `[{ title, child }]` | One child for each tab. |
| `Divider` | `axis` | A line between parts. |

A template repeats one component for each item of a list: `"children": { "componentId": "row", "path": "/items" }`. Inside the template, a pointer with no leading `/` reads from the item, for example `{"@path": "name"}`, and `{"@call": "@index", "args": {}}` gives the index of the item.

## Display

| Component | Properties | Use |
| --- | --- | --- |
| `Text` | `text!`, `variant` (`body`, `caption`) | Plain text. |
| `Icon` | `name!` | A small icon, for example `info`, `warning`, `check`. |
| `Table` | `columns!`: `[{ key, title }]`, `rows!`, `caption` | A table. `rows` is a list of objects, literal or bound. A cell shows `row[key]`. |

## Inputs and the button

| Component | Properties | Value |
| --- | --- | --- |
| `TextField` | `label!`, `value!`, `placeholder`, `variant` (`shortText`, `longText`, `number`) | a string |
| `CheckBox` | `label!`, `value!` | a boolean |
| `ChoicePicker` | `options!`: `[{ label, value }]`, `value!`, `label`, `variant` (`mutuallyExclusive`, `multipleSelection`), `displayStyle` (`checkbox`, `chips`) | a list of option values |
| `Slider` | `max!`, `value!`, `min`, `steps`, `label` | a number |
| `DateTimeInput` | `value!`, `label`, `enableDate`, `enableTime`, `min`, `max` | an ISO 8601 string; start it with `""` |
| `Button` | `child!` (a `Text` with the label), `action!`, `variant` (`default`, `primary`, `borderless`) | sends its `context` |

Bind the `value` of an input to the data model: `"value": {"@path": "/note"}`. The input writes the value there, and every component bound to the same pointer shows it at once.

A Button sends `action.event.name` and `action.event.context`. Give each value you need in the context, usually as a binding:

```
"action": { "event": { "name": "save_settings", "context": { "note": { "@path": "/note" } } } }
```

A context key is letters, digits, and `_`, and starts with a letter.

## Checks

An input or a Button can have `@checks`: a list of `{ condition, message }`. A Button whose condition is false is disabled, and the message shows under it. The usual check is `required`:

```
"@checks": [{ "condition": { "@call": "required", "args": { "value": { "@path": "/choice" } } }, "message": "Pick one." }]
```

## Functions

| Function | Arguments | Gives |
| --- | --- | --- |
| `formatString` | `value!` | the text, with each `${/pointer}` replaced by its value |
| `formatNumber` | `value!`, `decimals`, `grouping` | a number as text |
| `formatDate` | `value!`, `format!` | a date as text, for example `yyyy-MM-dd` |
| `pluralize` | `value!`, `other!`, `zero`, `one` | the text for the count |
| `required` | `value!` | true when the value is not empty |
| `length` | `value!`, `min`, `max` | true when the text length is in the bounds; give `min`, `max`, or both |
| `numeric` | `value!`, `min`, `max` | true when the number is in the bounds; give `min`, `max`, or both |
| `and`, `or` | `values!` (two or more) | the logic of the values |
| `not` | `value!` | the opposite |
| `@index` | `offset` | the index of the template item |
