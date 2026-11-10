# The CALM document

A CALM document is one JSON object. It is the frame that holds all other concepts. The application validates it against the `calm.json` meta schema of CALM 1.3.

## Shape

```jsonc
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json", // always this value
  "nodes": [],          // array of nodes (references/nodes.md)
  "relationships": [],  // array of relationships (references/relationships.md)
  "flows": [],          // optional: array of flows (references/flows.md)
  "controls": {},       // optional: controls of the whole architecture (references/controls.md)
  "metadata": {},       // optional: an object, or an array of objects
  "adrs": []            // optional: array of strings, links to decision records
}
```

| Key | Type | Required | Notes |
| --- | --- | --- | --- |
| `$schema` | string | Yes, by this skill | Always `https://calm.finos.org/release/1.3/meta/calm.json`. The schema does not require it, but the application and other CALM tools use it to identify the version. |
| `nodes` | array of node objects | No | An architecture with no nodes shows the statement "The architecture has no nodes." in place of a diagram. |
| `relationships` | array of relationship objects | No | Use `[]` when there are no relationships. |
| `flows` | array of flow objects | No | Business flows over the relationships. |
| `controls` | object | No | Requirements that apply to the whole architecture. |
| `metadata` | object, or array of objects | No | Free key-value data. The details panel shows each key and value. |
| `adrs` | array of strings | No | Links or paths to Architecture Decision Records. Each item is a string. |

All keys are optional for the schema, but write `$schema`, `nodes`, and `relationships` in each document. Put the keys in the order of the table.

## Metadata

`metadata` has two forms. Both forms are valid at the top level, on a node, on a relationship, and on a flow.

- An object: `"metadata": { "owner": "payments-team", "version": "2.1.0" }`.
- An array of objects: `"metadata": [ { "owner": "payments-team" }, { "version": "2.1.0" } ]`.

A value can be a string, a number, a boolean, an object, or an array. Use the object form unless the author uses the array form.

## Examples

The smallest useful document:

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    {
      "unique-id": "web-shop",
      "node-type": "system",
      "name": "Web shop",
      "description": "Sells products to customers online."
    }
  ],
  "relationships": []
}
```

A document with top-level metadata and ADR links:

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    {
      "unique-id": "customer",
      "node-type": "actor",
      "name": "Customer",
      "description": "A person who buys products."
    },
    {
      "unique-id": "web-shop",
      "node-type": "system",
      "name": "Web shop",
      "description": "Sells products to customers online."
    }
  ],
  "relationships": [
    {
      "unique-id": "customer-uses-web-shop",
      "description": "Browses and buys products",
      "relationship-type": {
        "interacts": {
          "actor": "customer",
          "nodes": ["web-shop"]
        }
      }
    }
  ],
  "metadata": {
    "owner": "commerce-team",
    "version": "1.0.0",
    "reviewed": true
  },
  "adrs": [
    "https://example.com/adr/0001-use-a-single-web-shop.md",
    "docs/adr/0002-hosting.md"
  ]
}
```

A document with metadata in the array form:

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    {
      "unique-id": "report-service",
      "node-type": "service",
      "name": "Report service",
      "description": "Makes monthly reports."
    }
  ],
  "relationships": [],
  "metadata": [
    { "owner": "finance-team" },
    { "environment": "production" }
  ]
}
```

## Not in an architecture document

CALM 1.3 also has timelines (`calm-timeline.json`, with `moments`), decorators, and evidence. These are separate documents with their own schemas. The application does not render them. Do not put `moments`, `decorators`, or `evidence` in an architecture document.

## Common mistakes

- A missing or wrong `$schema`. Always write `https://calm.finos.org/release/1.3/meta/calm.json`. Do not write the `1.0`, `1.1`, or `1.2` URL, and do not write `https://json-schema.org/...`.
- Two documents in one `calm` block, or text before or after the JSON. A block or a `.calm.json` file holds exactly one JSON object.
- Comments (`//` or `/* */`) or a comma after the last item. JSON does not allow them.
- `adrs` as one string. It is an array of strings: `"adrs": ["docs/adr/0001.md"]`.
- `metadata` as a string or a number. It is an object or an array of objects.
- A top-level key that is a JSON Schema keyword, such as `type`, `properties`, `required`, `$id`, or `$ref`. The application also checks the document as a JSON Schema, so these keys cause schema errors. A top-level `title` or `description` must be a string.
- `nodes` or `relationships` as an object keyed by id. Each is an array.
