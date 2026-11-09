# Complete example

This file shows one full architecture that uses most of the concepts together. Use it as a model for a full architecture. Read the reference file of each concept for its rules.

The architecture is a payment platform:

- A buyer uses a web shop in a browser.
- The web shop calls a payment API. The payment API calls a fraud check and writes to a ledger database.
- The payment API, the fraud check, and the ledger database run in a Kubernetes cluster. The cluster and the web shop are parts of the payment platform system.
- An `options` relationship records a decision that is still open: a fraud check in the cluster, or an outside fraud provider.
- A flow shows the checkout. Controls state the security and audit requirements.

## Example

```json
{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    {
      "unique-id": "buyer",
      "node-type": "actor",
      "name": "Buyer",
      "description": "A person who pays for an order."
    },
    {
      "unique-id": "payment-platform",
      "node-type": "system",
      "name": "Payment platform",
      "description": "Takes payments for the shop."
    },
    {
      "unique-id": "web-shop",
      "node-type": "webclient",
      "name": "Web shop",
      "description": "Browser application where the buyer pays.",
      "interfaces": [
        { "unique-id": "web-shop-url", "url": "https://shop.example.com" }
      ]
    },
    {
      "unique-id": "k8s-cluster",
      "node-type": "network",
      "name": "Kubernetes cluster",
      "description": "Runs the payment services.",
      "metadata": { "region": "eu-west-1" }
    },
    {
      "unique-id": "payment-api",
      "node-type": "service",
      "name": "Payment API",
      "description": "Takes card payments and records them.",
      "interfaces": [
        { "unique-id": "payment-api-https", "host": "pay.example.com", "port": 443 },
        { "unique-id": "payment-api-metrics", "port": 9100, "path": "/metrics" }
      ],
      "controls": {
        "card-data-protection": {
          "description": "The service does not store full card numbers.",
          "requirements": [
            {
              "requirement-url": "https://example.com/controls/card-data.json",
              "config": { "store-full-card-number": false }
            }
          ]
        }
      },
      "metadata": { "owner": "payments-team", "language": "Rust" }
    },
    {
      "unique-id": "fraud-check",
      "node-type": "service",
      "name": "Fraud check",
      "description": "Scores each payment for fraud in the cluster.",
      "interfaces": [
        { "unique-id": "fraud-check-grpc", "port": 7000 }
      ]
    },
    {
      "unique-id": "fraud-provider",
      "node-type": "system",
      "name": "Fraud provider",
      "description": "An outside service that scores payments for fraud."
    },
    {
      "unique-id": "ledger-db",
      "node-type": "database",
      "name": "Ledger database",
      "description": "Stores each payment as a ledger entry."
    }
  ],
  "relationships": [
    {
      "unique-id": "buyer-uses-web-shop",
      "description": "Pays for an order",
      "relationship-type": {
        "interacts": { "actor": "buyer", "nodes": ["web-shop"] }
      }
    },
    {
      "unique-id": "web-shop-to-payment-api",
      "description": "Submits the payment",
      "protocol": "HTTPS",
      "relationship-type": {
        "connects": {
          "source": { "node": "web-shop", "interfaces": ["web-shop-url"] },
          "destination": { "node": "payment-api", "interfaces": ["payment-api-https"] }
        }
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
    },
    {
      "unique-id": "payment-api-to-fraud-check",
      "description": "Asks for a fraud score",
      "protocol": "mTLS",
      "relationship-type": {
        "connects": {
          "source": { "node": "payment-api" },
          "destination": { "node": "fraud-check", "interfaces": ["fraud-check-grpc"] }
        }
      }
    },
    {
      "unique-id": "payment-api-to-fraud-provider",
      "description": "Asks the outside provider for a fraud score",
      "protocol": "HTTPS",
      "relationship-type": {
        "connects": {
          "source": { "node": "payment-api" },
          "destination": { "node": "fraud-provider" }
        }
      }
    },
    {
      "unique-id": "payment-api-to-ledger-db",
      "description": "Writes the ledger entry",
      "protocol": "JDBC",
      "relationship-type": {
        "connects": {
          "source": { "node": "payment-api" },
          "destination": { "node": "ledger-db" }
        }
      }
    },
    {
      "unique-id": "cluster-runs-services",
      "relationship-type": {
        "deployed-in": {
          "container": "k8s-cluster",
          "nodes": ["payment-api", "fraud-check", "ledger-db"]
        }
      }
    },
    {
      "unique-id": "platform-parts",
      "relationship-type": {
        "composed-of": {
          "container": "payment-platform",
          "nodes": ["web-shop", "k8s-cluster"]
        }
      }
    },
    {
      "unique-id": "fraud-check-choice",
      "description": "Where the fraud score comes from",
      "relationship-type": {
        "options": [
          {
            "description": "Run the fraud check in the cluster",
            "nodes": ["fraud-check"],
            "relationships": ["payment-api-to-fraud-check"]
          },
          {
            "description": "Use the outside fraud provider",
            "nodes": ["fraud-provider"],
            "relationships": ["payment-api-to-fraud-provider"],
            "controls": ["vendor-review"]
          }
        ]
      }
    }
  ],
  "flows": [
    {
      "unique-id": "checkout-flow",
      "name": "Checkout",
      "description": "The buyer pays for an order, and the platform records the payment.",
      "transitions": [
        {
          "relationship-unique-id": "buyer-uses-web-shop",
          "sequence-number": 1,
          "description": "Buyer submits the card details"
        },
        {
          "relationship-unique-id": "web-shop-to-payment-api",
          "sequence-number": 2,
          "description": "Web shop submits the payment"
        },
        {
          "relationship-unique-id": "payment-api-to-fraud-check",
          "sequence-number": 3,
          "description": "Payment API gets a fraud score"
        },
        {
          "relationship-unique-id": "payment-api-to-ledger-db",
          "sequence-number": 4,
          "description": "Payment API writes the ledger entry"
        },
        {
          "relationship-unique-id": "web-shop-to-payment-api",
          "sequence-number": 5,
          "description": "Payment API confirms the payment",
          "direction": "destination-to-source"
        }
      ],
      "controls": {
        "checkout-latency": {
          "description": "The checkout completes in 2 seconds or less.",
          "requirements": [
            {
              "requirement-url": "https://example.com/controls/latency.json",
              "config": { "maximum": { "unit": "seconds", "value": 2 } }
            }
          ]
        }
      }
    }
  ],
  "controls": {
    "audit-logging": {
      "description": "Each payment is logged for audit.",
      "requirements": [
        {
          "requirement-url": "https://example.com/controls/audit-logging.json",
          "config-url": "https://example.com/configs/payment-audit-logging.json"
        }
      ]
    },
    "vendor-review": {
      "description": "Each outside provider passes a security review before use.",
      "requirements": [
        {
          "requirement-url": "https://example.com/controls/vendor-review.json",
          "config": { "review-interval": { "unit": "years", "value": 1 } }
        }
      ]
    }
  },
  "metadata": {
    "owner": "payments-team",
    "version": "1.2.0"
  },
  "adrs": [
    "docs/adr/0003-payment-provider.md",
    "docs/adr/0004-fraud-scoring.md"
  ]
}
```

## How to read it

- `platform-parts` (`composed-of`) makes `payment-platform` a group with a solid border. It holds `web-shop` and the `k8s-cluster` group.
- `cluster-runs-services` (`deployed-in`) makes `k8s-cluster` a group with a dashed border. It holds the three services.
- Each `connects` relationship is one edge with its protocol as a chip. `buyer-uses-web-shop` (`interacts`) is a dashed edge.
- `fraud-check-choice` (`options`) makes no edge. It puts a decision marker on the two fraud nodes and on their two relationships. Its second decision names the `vendor-review` control of the top level.
- The `checkout-flow` flow has five steps. Step 5 uses `web-shop-to-payment-api` again in the other direction, for the reply.

## Common mistakes

- An id in a relationship, a decision, or a transition that does not match a `unique-id` exactly. Ids are case-sensitive.
- A node that is in no relationship. This is valid, but the node shows alone. Make sure that the author wants it.
- A node that two groups list. This is valid. The node shows in the first group and has a badge for the other group. Do not use it to show a dependency: use `connects`.
- A group that holds its own container, for example `payment-platform` in `k8s-cluster` and `k8s-cluster` in `payment-platform`. This is a containment loop, and the application shows no diagram.
