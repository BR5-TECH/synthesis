# Flows

A flow is a business process that goes through the architecture, for example "Checkout" or "Open an account". It is a list of steps, and each step uses one relationship of the document. The application shows the flows in a flow selector. When the author chooses a flow, the diagram shows the number of each step on its edge and dims the items that no step uses.

## Shape

`flows` is an array at the top level of the document.

```jsonc
{
  "unique-id": "checkout-flow",                // required, unique in the document
  "name": "Checkout",                          // required, shows in the flow selector
  "description": "A shopper pays for a cart.", // required
  "requirement-url": "https://example.com/requirements/checkout", // optional
  "transitions": [                             // required, at least one
    {
      "relationship-unique-id": "shopper-uses-web-shop", // required, a relationship id
      "sequence-number": 1,                              // required, an integer
      "description": "Shopper submits the cart",         // required
      "direction": "source-to-destination"               // optional
    }
  ],
  "controls": {},                              // optional (references/controls.md)
  "metadata": {}                               // optional, object or array of objects
}
```

A flow allows only the keys above. A flow with any other key is a schema error.

## Transitions

- `relationship-unique-id`: the `unique-id` of a relationship of the document.
- `sequence-number`: an integer. The steps show in the order of this number, from the lowest. Steps with the same number keep their order in the array. Start at `1`.
- `description`: a short text of what happens in this step. It is required.
- `direction`: `source-to-destination` (the default) or `destination-to-source`. Use `destination-to-source` for a reply on the same relationship, for example the response of a request.

The direction of each relationship type:

| Type | Source | Destination |
| --- | --- | --- |
| `connects` | `source.node` | `destination.node` |
| `interacts` | `actor` | the nodes in `nodes` |
| `deployed-in`, `composed-of`, `options` | no edge | the step highlights the nodes that the relationship names |

Use `connects` and `interacts` relationships in a flow. A step on a relationship of another type has no edge.

## Example

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    { "unique-id": "shopper", "node-type": "actor", "name": "Shopper", "description": "A person who buys products." },
    { "unique-id": "web-shop", "node-type": "webclient", "name": "Web shop", "description": "Browser application of the shop." },
    { "unique-id": "order-service", "node-type": "service", "name": "Order service", "description": "Takes orders." },
    { "unique-id": "payment-service", "node-type": "service", "name": "Payment service", "description": "Takes card payments." },
    { "unique-id": "order-db", "node-type": "database", "name": "Order database", "description": "Stores orders." }
  ],
  "relationships": [
    {
      "unique-id": "shopper-uses-web-shop",
      "description": "Uses the shop",
      "relationship-type": { "interacts": { "actor": "shopper", "nodes": ["web-shop"] } }
    },
    {
      "unique-id": "web-shop-to-order-service",
      "description": "Places orders",
      "protocol": "HTTPS",
      "relationship-type": {
        "connects": { "source": { "node": "web-shop" }, "destination": { "node": "order-service" } }
      }
    },
    {
      "unique-id": "order-service-to-payment-service",
      "description": "Requests payment",
      "protocol": "HTTPS",
      "relationship-type": {
        "connects": { "source": { "node": "order-service" }, "destination": { "node": "payment-service" } }
      }
    },
    {
      "unique-id": "order-service-to-order-db",
      "description": "Stores orders",
      "protocol": "JDBC",
      "relationship-type": {
        "connects": { "source": { "node": "order-service" }, "destination": { "node": "order-db" } }
      }
    }
  ],
  "flows": [
    {
      "unique-id": "checkout-flow",
      "name": "Checkout",
      "description": "A shopper pays for the products in the cart.",
      "requirement-url": "https://example.com/requirements/checkout",
      "transitions": [
        {
          "relationship-unique-id": "shopper-uses-web-shop",
          "sequence-number": 1,
          "description": "Shopper submits the cart"
        },
        {
          "relationship-unique-id": "web-shop-to-order-service",
          "sequence-number": 2,
          "description": "Web shop places the order"
        },
        {
          "relationship-unique-id": "order-service-to-payment-service",
          "sequence-number": 3,
          "description": "Order service requests the payment"
        },
        {
          "relationship-unique-id": "order-service-to-payment-service",
          "sequence-number": 4,
          "description": "Payment service confirms the payment",
          "direction": "destination-to-source"
        },
        {
          "relationship-unique-id": "order-service-to-order-db",
          "sequence-number": 5,
          "description": "Order service stores the paid order"
        }
      ],
      "metadata": { "owner": "checkout-team" }
    }
  ]
}
```

## Common mistakes

- `steps` in place of `transitions`, or `relationship` or `relationship-id` in place of `relationship-unique-id`.
- A transition with no `description`. Each transition needs one.
- `sequence-number` as a string (`"1"`) or a decimal (`1.5`). It is an integer.
- A transition that names a node id in place of a relationship id. A transition always names a relationship.
- A transition that names a relationship that does not exist.
- A flow with an extra key, for example `steps` or `owner`. Put extra data in `metadata`.
- An empty `transitions` array. A flow has at least one transition.
- `direction` with another value, for example `reverse` or `response`. Use `destination-to-source`.
- `flows` inside a node or a relationship. It is a top-level key.
