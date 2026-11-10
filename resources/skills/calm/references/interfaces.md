# Interfaces

An interface is a point where a node accepts or makes a connection: a port, a URL, a host, a queue name, or an API. A node lists its interfaces in `interfaces`. A `connects` relationship can name the interfaces that it uses at each end.

The diagram shows the `unique-id` of the first three interfaces of a node under its name, and "+N more" when the node has more. The details panel shows each key and value of each interface.

## Shape

`interfaces` is an array of objects on a node. Each object has one of two forms.

The free form. It needs `unique-id`, and it can have any other keys:

```jsonc
{ "unique-id": "payment-api-https", "host": "pay.example.com", "port": 443, "url": "https://pay.example.com/v1" }
```

The definition form. It refers to an outside schema for the interface. It needs exactly these three keys and no other key:

```jsonc
{
  "unique-id": "payment-api-grpc",
  "definition-url": "https://example.com/interfaces/grpc-service.json",
  "config": { "service": "payments.v1.Payments", "port": 9090 }
}
```

- `unique-id`: a string, unique in the whole document.
- `definition-url`: a string, the URL or path of the schema that `config` follows. The application does not fetch it.
- `config`: an object.

Usual keys of the free form are `host`, `port`, `url`, `hostname`, `path`, and `name`. Use the keys that describe the interface clearly.

## Use interfaces in a relationship

A `connects` end can name interfaces of its own node:

```jsonc
"connects": {
  "source": { "node": "web-shop", "interfaces": ["web-shop-client"] },
  "destination": { "node": "payment-api", "interfaces": ["payment-api-https"] }
}
```

Each id in `interfaces` must be the `unique-id` of an interface of the node of that end. The diagram shows the interfaces as text at each end of the edge.

## Example

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    {
      "unique-id": "web-shop",
      "node-type": "webclient",
      "name": "Web shop",
      "description": "Browser application of the shop.",
      "interfaces": [
        { "unique-id": "web-shop-client", "name": "Payment client" }
      ]
    },
    {
      "unique-id": "payment-api",
      "node-type": "service",
      "name": "Payment API",
      "description": "Takes card payments.",
      "interfaces": [
        { "unique-id": "payment-api-https", "host": "pay.example.com", "port": 443, "path": "/v1/payments" },
        {
          "unique-id": "payment-api-grpc",
          "definition-url": "https://example.com/interfaces/grpc-service.json",
          "config": { "service": "payments.v1.Payments", "port": 9090 }
        },
        { "unique-id": "payment-api-metrics", "port": 9100, "path": "/metrics" }
      ]
    },
    {
      "unique-id": "fraud-check",
      "node-type": "service",
      "name": "Fraud check",
      "description": "Scores each payment for fraud.",
      "interfaces": [
        { "unique-id": "fraud-check-grpc", "port": 7000 }
      ]
    }
  ],
  "relationships": [
    {
      "unique-id": "web-shop-to-payment-api",
      "description": "Submits a payment",
      "protocol": "HTTPS",
      "relationship-type": {
        "connects": {
          "source": { "node": "web-shop", "interfaces": ["web-shop-client"] },
          "destination": { "node": "payment-api", "interfaces": ["payment-api-https"] }
        }
      }
    },
    {
      "unique-id": "payment-api-to-fraud-check",
      "description": "Asks for a fraud score",
      "relationship-type": {
        "connects": {
          "source": { "node": "payment-api" },
          "destination": { "node": "fraud-check", "interfaces": ["fraud-check-grpc"] }
        }
      }
    }
  ]
}
```

## Common mistakes

- An interface with no `unique-id`. Each interface needs one.
- `interfaces` as an array of strings, for example `["https"]`, or as an object. It is an array of objects.
- The definition form with an extra key, for example `definition-url`, `config`, and `port` together, or with no `config`. The schema then reads the object as the free form, and CALM tools do not apply the definition. Put `port` inside `config`, and write `config` even as `{}`.
- A `connects` end that names an interface of another node. The interface must belong to the node of the same end.
- A `connects` end that names an interface by its host or port. Name it by its `unique-id`.
- Two interfaces with the same `unique-id`, also on two different nodes. Put the node id in the interface id, for example `payment-api-https`.
