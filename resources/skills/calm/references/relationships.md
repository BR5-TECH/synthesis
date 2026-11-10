# Relationships

A relationship joins nodes. Each relationship has exactly one type in its `relationship-type` object: `interacts`, `connects`, `deployed-in`, `composed-of`, or `options`.

## Shape

```jsonc
{
  "unique-id": "web-to-api",            // required, unique in the document
  "description": "Sends payment requests", // optional, shows as the edge label
  "relationship-type": {                // required, exactly ONE of the five keys
    "connects": { "source": { "node": "web-shop" }, "destination": { "node": "payment-api" } }
  },
  "protocol": "HTTPS",                  // optional, see the list below
  "controls": {},                       // optional (references/controls.md)
  "metadata": {}                        // optional, object or array of objects
}
```

`unique-id` and `relationship-type` are required.

## The five types

| Type | Shape | Use it for | Diagram |
| --- | --- | --- | --- |
| `interacts` | `{ "actor": "<node id>", "nodes": ["<node id>", ...] }` | A person or an outside party uses one or more nodes | One dashed edge from the actor to each node |
| `connects` | `{ "source": { "node": "<node id>", "interfaces": ["<interface id>"] }, "destination": { "node": "<node id>", "interfaces": [...] } }` | One node calls or sends data to another node | One edge from source to destination |
| `deployed-in` | `{ "container": "<node id>", "nodes": ["<node id>", ...] }` | Nodes run inside a runtime, for example a cluster or a network | A group with a dashed border |
| `composed-of` | `{ "container": "<node id>", "nodes": ["<node id>", ...] }` | A system is made of its parts | A group with a solid border |
| `options` | `[ { "description": "...", "nodes": [...], "relationships": [...], "controls": [...] } ]` | The architecture has alternatives, and one of them is chosen later | No edge; a decision marker on each item a decision names |

Rules of each type:

- `interacts`: `actor` and `nodes` are required. `nodes` has at least one id. The actor is usually a node of type `actor`.
- `connects`: `source` and `destination` are required, and each needs `node`. `interfaces` is optional. When present, it lists the `unique-id` of interfaces of that same node (`references/interfaces.md`).
- `deployed-in` and `composed-of`: `container` and `nodes` are required. `nodes` has at least one id. The container node becomes a group that holds the listed nodes. A group can hold another group. A group must not hold itself, directly or through other groups.
- `options`: the value is an array of decisions. Each decision needs `description`, `nodes`, and `relationships`, which can be empty arrays. `controls` is optional and lists keys of controls that apply when the author takes this decision. Each id in a decision must exist in the document.

When a node is in two groups, the diagram shows it inside the first group and shows a badge for the other group.

## Protocol

`protocol` is optional. When present, it must be one of these values, with this exact case:

`HTTP`, `HTTPS`, `FTP`, `SFTP`, `JDBC`, `WebSocket`, `SocketIO`, `LDAP`, `AMQP`, `TLS`, `mTLS`, `TCP`

A `connects` edge shows its protocol as a chip. For another protocol, for example gRPC or Kafka, do not set `protocol`. Put the name in the description or in `metadata`.

## Examples

All five types in one document:

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    { "unique-id": "shopper", "node-type": "actor", "name": "Shopper", "description": "A person who buys products." },
    { "unique-id": "shop-system", "node-type": "system", "name": "Shop system", "description": "Everything that runs the shop." },
    { "unique-id": "k8s-cluster", "node-type": "network", "name": "Kubernetes cluster", "description": "Runs the shop services." },
    { "unique-id": "shop-api", "node-type": "service", "name": "Shop API", "description": "Serves the catalog and the cart." },
    { "unique-id": "shop-db", "node-type": "database", "name": "Shop database", "description": "Stores products and orders." },
    { "unique-id": "managed-db", "node-type": "database", "name": "Managed database", "description": "A cloud database service." }
  ],
  "relationships": [
    {
      "unique-id": "shopper-uses-shop",
      "description": "Browses and orders",
      "relationship-type": {
        "interacts": { "actor": "shopper", "nodes": ["shop-api"] }
      }
    },
    {
      "unique-id": "shop-api-to-db",
      "description": "Reads and writes orders",
      "protocol": "JDBC",
      "relationship-type": {
        "connects": {
          "source": { "node": "shop-api" },
          "destination": { "node": "shop-db" }
        }
      }
    },
    {
      "unique-id": "shop-api-to-managed-db",
      "description": "Reads and writes orders",
      "protocol": "TLS",
      "relationship-type": {
        "connects": {
          "source": { "node": "shop-api" },
          "destination": { "node": "managed-db" }
        }
      }
    },
    {
      "unique-id": "cluster-runs-shop",
      "relationship-type": {
        "deployed-in": { "container": "k8s-cluster", "nodes": ["shop-api", "shop-db"] }
      }
    },
    {
      "unique-id": "shop-parts",
      "relationship-type": {
        "composed-of": { "container": "shop-system", "nodes": ["k8s-cluster"] }
      }
    },
    {
      "unique-id": "database-choice",
      "description": "Where the shop keeps its data",
      "relationship-type": {
        "options": [
          {
            "description": "Run the database in the cluster",
            "nodes": ["shop-db"],
            "relationships": ["shop-api-to-db"]
          },
          {
            "description": "Use a managed cloud database",
            "nodes": ["managed-db"],
            "relationships": ["shop-api-to-managed-db"]
          }
        ]
      }
    }
  ]
}
```

A `connects` relationship with interfaces on both ends, and nested groups:

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    { "unique-id": "cloud", "node-type": "network", "name": "Cloud account", "description": "The cloud account of the team." },
    { "unique-id": "private-subnet", "node-type": "network", "name": "Private subnet", "description": "Holds the services." },
    {
      "unique-id": "order-service",
      "node-type": "service",
      "name": "Order service",
      "description": "Takes orders.",
      "interfaces": [{ "unique-id": "order-service-client", "port": 8080 }]
    },
    {
      "unique-id": "stock-service",
      "node-type": "service",
      "name": "Stock service",
      "description": "Reserves stock.",
      "interfaces": [{ "unique-id": "stock-service-http", "port": 8081 }]
    }
  ],
  "relationships": [
    {
      "unique-id": "order-to-stock",
      "description": "Reserves stock for an order",
      "protocol": "HTTP",
      "relationship-type": {
        "connects": {
          "source": { "node": "order-service", "interfaces": ["order-service-client"] },
          "destination": { "node": "stock-service", "interfaces": ["stock-service-http"] }
        }
      }
    },
    {
      "unique-id": "subnet-in-cloud",
      "relationship-type": {
        "deployed-in": { "container": "cloud", "nodes": ["private-subnet"] }
      }
    },
    {
      "unique-id": "services-in-subnet",
      "relationship-type": {
        "deployed-in": { "container": "private-subnet", "nodes": ["order-service", "stock-service"] }
      }
    }
  ]
}
```

## Common mistakes

- Two keys in one `relationship-type`, for example `connects` and `deployed-in` together. Write one relationship for each type.
- The type at the top level of the relationship, for example `"connects": {...}` next to `unique-id`. The type goes inside `relationship-type`.
- `source` or `destination` as a string, for example `"source": "web-shop"`. Each is an object: `"source": { "node": "web-shop" }`.
- `from` and `to`, `parent` and `children`, or `target` in place of the keys above.
- An empty `nodes` array in `interacts`, `deployed-in`, or `composed-of`. It needs at least one id.
- A decision with no `nodes` or no `relationships` key. Write `"nodes": []` or `"relationships": []` when it names none.
- `protocol` with a value that is not in the list, or in a different case, for example `https`, `gRPC`, or `Kafka`.
- An id that does not exist. Each node id, interface id, and relationship id that a relationship names must exist in the document. An interface id in a `connects` end must belong to the node of that end.
- A containment loop: group A holds group B and group B holds group A. Each group must be inside its container only.
