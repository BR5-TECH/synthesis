# Nodes

A node is one part of the architecture: a person, a system, a service, a database, a network, a client, or a data set. The application shows each node as a box with its `name` and its `node-type`.

## Shape

```jsonc
{
  "unique-id": "payment-api",          // required, unique in the document
  "node-type": "service",              // required, see the table below
  "name": "Payment API",               // required, shows in the box
  "description": "Takes payments.",    // required, shows in the details
  "details": {                         // optional, only these two keys
    "detailed-architecture": "architecture/payment-api.calm.json",
    "required-pattern": "patterns/api-service.json"
  },
  "interfaces": [],                    // optional (references/interfaces.md)
  "controls": {},                      // optional (references/controls.md)
  "metadata": {}                       // optional, object or array of objects
}
```

The four keys `unique-id`, `node-type`, `name`, and `description` are required. A node can have more keys of its own, for example `"version": "2.1"`, but the application does not show them. Put such data in `metadata`.

## Node types

| `node-type` | Use it for | Shape in the diagram |
| --- | --- | --- |
| `actor` | A person, a role, or an outside party | Actor |
| `ecosystem` | A large group of systems, for example a company or a cloud | System |
| `system` | A software system that holds services | System |
| `service` | One deployable service or application | Service |
| `database` | A database or another data store | Database |
| `network` | A network, a zone, a cluster, or a VPC | Network |
| `ldap` | A directory service | Network |
| `webclient` | A browser application or a mobile application | Web client |
| `data-asset` | A data set, a file, a topic, or a document | Data asset |

`node-type` can also be any other string, for example `message-queue` or `cache`. The application shows such a node with a generic shape and the text of its type. Use one of the nine values above when one fits.

## `details`

`details` is an optional object with only two optional keys:

- `detailed-architecture`: a string, the path or URL of a CALM document that shows the inside of this node.
- `required-pattern`: a string, the path or URL of a CALM pattern that this node must follow.

Any other key in `details` is a schema error.

## Examples

Each node type:

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    { "unique-id": "clerk", "node-type": "actor", "name": "Bank clerk", "description": "Opens accounts for customers." },
    { "unique-id": "bank-group", "node-type": "ecosystem", "name": "Bank group", "description": "All systems of the bank." },
    { "unique-id": "core-banking", "node-type": "system", "name": "Core banking", "description": "Keeps accounts and balances." },
    { "unique-id": "account-service", "node-type": "service", "name": "Account service", "description": "Opens and closes accounts." },
    { "unique-id": "account-db", "node-type": "database", "name": "Account database", "description": "Stores account records." },
    { "unique-id": "internal-network", "node-type": "network", "name": "Internal network", "description": "The private network of the bank." },
    { "unique-id": "staff-directory", "node-type": "ldap", "name": "Staff directory", "description": "Holds staff accounts and groups." },
    { "unique-id": "clerk-portal", "node-type": "webclient", "name": "Clerk portal", "description": "Browser application for clerks." },
    { "unique-id": "account-records", "node-type": "data-asset", "name": "Account records", "description": "Personal data of account holders." },
    { "unique-id": "event-bus", "node-type": "message-queue", "name": "Event bus", "description": "Carries account events." }
  ],
  "relationships": []
}
```

A node with `details`, interfaces, controls, and metadata:

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    {
      "unique-id": "payment-api",
      "node-type": "service",
      "name": "Payment API",
      "description": "Takes card payments from the web shop.",
      "details": {
        "detailed-architecture": "architecture/payment-api.calm.json"
      },
      "interfaces": [
        { "unique-id": "payment-api-https", "host": "pay.example.com", "port": 443 }
      ],
      "controls": {
        "data-encryption": {
          "description": "Card data is encrypted at rest.",
          "requirements": [
            {
              "requirement-url": "https://example.com/controls/encryption-at-rest.json",
              "config": { "algorithm": "AES-256" }
            }
          ]
        }
      },
      "metadata": {
        "owner": "payments-team",
        "language": "Rust"
      }
    }
  ],
  "relationships": []
}
```

## Common mistakes

- A missing `description`. Each node needs `unique-id`, `node-type`, `name`, and `description`, also when the description is short.
- `type` or `nodeType` in place of `node-type`, or `id` in place of `unique-id`.
- Two nodes with the same `unique-id`. Each id must be unique in the whole document.
- `node-type` with a different case or form, for example `Service`, `web-client`, or `data_asset`. These are other strings and show the generic shape. Write `service`, `webclient`, and `data-asset`.
- A container node with a list of children, for example `"nodes": [...]` inside a node. A node never holds other nodes. Use a `deployed-in` or `composed-of` relationship (`references/relationships.md`).
- A key in `details` other than `detailed-architecture` and `required-pattern`.
- `interfaces` as an object or as an array of strings. It is an array of objects, and each object has a `unique-id`.
