# Controls

A control is a requirement that the architecture must meet, for example "Data is encrypted in transit" or "Access needs multi-factor authentication". Each control points to a requirement and states how the architecture meets it. The diagram shows a control marker with the number of controls on each node and edge that has controls. The details panel shows each control.

## Where controls go

`controls` is an object. It can be on these items:

- the top level of the document, for the whole architecture
- a node
- a relationship
- a flow

## Shape

```jsonc
"controls": {
  "data-in-transit": {                     // the key: letters, digits, and hyphens only
    "description": "Data is encrypted in transit.", // required
    "requirements": [                      // required, an array (can be empty)
      {
        "requirement-url": "https://example.com/controls/encryption-in-transit.json", // required
        "config": { "minimum-tls-version": "1.2" }  // exactly ONE of config or config-url
      },
      {
        "requirement-url": "https://example.com/controls/certificate-rotation.json",
        "config-url": "https://example.com/configs/certificate-rotation.json"
      }
    ]
  }
}
```

- The key of a control is its id. Use only letters, digits, and hyphens, for example `data-in-transit`. A decision of an `options` relationship refers to a control by this key.
- `description`: a string. It is required.
- `requirements`: an array of requirement details. It is required.
- Each requirement detail needs `requirement-url`, a string. It also needs exactly one of these two keys:
  - `config`: an object that states how the architecture meets the requirement.
  - `config-url`: a string, the URL or path of such a configuration.

`requirement-url` usually points to a control requirement document. That document has a `control-id`, a `name`, and a `description`, for example `{ "control-id": "CR-001", "name": "Access control", "description": "Restrict access to sensitive data." }`. Do not put that document inside the architecture: write only its URL. The application does not fetch it.

`config` is free. When a value is a duration or a rate, a clear form is the CALM unit form: `{ "unit": "seconds", "value": 30 }` for a duration, and `{ "rate": 100, "per": "second" }` for a rate.

## Example

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    {
      "unique-id": "trading-api",
      "node-type": "service",
      "name": "Trading API",
      "description": "Takes trade orders.",
      "controls": {
        "rate-limit": {
          "description": "Each client sends at most 100 orders each second.",
          "requirements": [
            {
              "requirement-url": "https://example.com/controls/rate-limit.json",
              "config": { "limit": { "rate": 100, "per": "second" } }
            }
          ]
        }
      }
    },
    {
      "unique-id": "trade-db",
      "node-type": "database",
      "name": "Trade database",
      "description": "Stores trades."
    }
  ],
  "relationships": [
    {
      "unique-id": "trading-api-to-trade-db",
      "description": "Writes trades",
      "protocol": "JDBC",
      "relationship-type": {
        "connects": { "source": { "node": "trading-api" }, "destination": { "node": "trade-db" } }
      },
      "controls": {
        "data-in-transit": {
          "description": "The connection uses TLS 1.2 or later.",
          "requirements": [
            {
              "requirement-url": "https://example.com/controls/encryption-in-transit.json",
              "config": { "minimum-tls-version": "1.2" }
            }
          ]
        }
      }
    }
  ],
  "flows": [
    {
      "unique-id": "place-trade",
      "name": "Place a trade",
      "description": "A trade order is stored.",
      "transitions": [
        {
          "relationship-unique-id": "trading-api-to-trade-db",
          "sequence-number": 1,
          "description": "Trading API stores the trade"
        }
      ],
      "controls": {
        "trade-latency": {
          "description": "A trade is stored in 200 milliseconds or less.",
          "requirements": [
            {
              "requirement-url": "https://example.com/controls/latency.json",
              "config": { "maximum": { "unit": "milliseconds", "value": 200 } }
            }
          ]
        }
      }
    }
  ],
  "controls": {
    "audit-logging": {
      "description": "Each change of a trade is logged for audit.",
      "requirements": [
        {
          "requirement-url": "https://example.com/controls/audit-logging.json",
          "config-url": "https://example.com/configs/trading-audit-logging.json"
        }
      ]
    }
  }
}
```

## Common mistakes

- `controls` as an array. It is an object, and each key is the id of a control.
- A control key with a space, an underscore, a dot, or another character that is not a letter, a digit, or a hyphen, for example `data_in_transit`. The schema does not check such a key, so its errors do not show. Use `data-in-transit`.
- A control with no `description` or no `requirements`.
- A requirement detail with no `requirement-url`.
- A requirement detail with both `config` and `config-url`, or with neither. Write exactly one of them.
- `config` as a string. It is an object. A URL goes in `config-url`.
- The control requirement document (`control-id`, `name`, `description`) inside the architecture. Write only its URL in `requirement-url`.
