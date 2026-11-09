/**
 * CAD-FR-JWXC, CAD-FR-KSAQ: the CALM templates of the Insert CALM
 * architecture menu. Each one is a complete CALM 1.3 document whose read gives
 * a diagram graph.
 */

/** CLM-FR-BWGJ: the `$schema` that each template declares. */
export const CALM_SCHEMA_URL = "https://calm.finos.org/release/1.3/meta/calm.json";

export interface CalmTemplate {
  key: string;
  /** The name that the menu shows. */
  name: string;
  /** The document, as the fence holds it. */
  source: string;
}

const doc = (body: Record<string, unknown>) =>
  JSON.stringify({ $schema: CALM_SCHEMA_URL, ...body }, null, 2);

const node = (id: string, type: string, name: string, description: string, extra = {}) => ({
  "unique-id": id,
  "node-type": type,
  name,
  description,
  ...extra,
});

const connects = (
  id: string,
  source: string,
  destination: string,
  description: string,
  protocol: string,
) => ({
  "unique-id": id,
  description,
  protocol,
  "relationship-type": { connects: { source: { node: source }, destination: { node: destination } } },
});

const MINIMAL = doc({
  nodes: [
    node("user", "actor", "User", "A person who uses the service."),
    node("service", "service", "Service", "The service that the user calls."),
  ],
  relationships: [
    {
      "unique-id": "user-uses-service",
      description: "Uses",
      "relationship-type": { interacts: { actor: "user", nodes: ["service"] } },
    },
  ],
});

const THREE_TIER = doc({
  nodes: [
    node("customer", "actor", "Customer", "A person who buys from the shop."),
    node("web-client", "webclient", "Web client", "The single-page application."),
    node("shop-api", "service", "Shop API", "The service that holds the shop logic.", {
      interfaces: [{ "unique-id": "shop-api-http", port: 8080 }],
    }),
    node("shop-db", "database", "Shop database", "The orders and the products."),
  ],
  relationships: [
    {
      "unique-id": "customer-uses-web-client",
      description: "Browses",
      "relationship-type": { interacts: { actor: "customer", nodes: ["web-client"] } },
    },
    {
      ...connects("web-client-to-api", "web-client", "shop-api", "Calls the API", "HTTPS"),
      "relationship-type": {
        connects: {
          source: { node: "web-client" },
          destination: { node: "shop-api", interfaces: ["shop-api-http"] },
        },
      },
    },
    connects("api-to-db", "shop-api", "shop-db", "Reads and writes", "JDBC"),
  ],
});

const DEPLOYED = doc({
  nodes: [
    node("cluster", "network", "Kubernetes cluster", "The cluster that runs the services."),
    node("orders", "service", "Orders service", "Takes the orders."),
    node("payments", "service", "Payments service", "Takes the payments."),
    node("ledger", "database", "Ledger", "The record of each payment."),
  ],
  relationships: [
    {
      "unique-id": "services-in-cluster",
      description: "Runs in",
      "relationship-type": {
        "deployed-in": { container: "cluster", nodes: ["orders", "payments"] },
      },
    },
    connects("orders-to-payments", "orders", "payments", "Requests a payment", "HTTPS"),
    connects("payments-to-ledger", "payments", "ledger", "Records the payment", "JDBC"),
  ],
});

const FLOW = doc({
  nodes: [
    node("buyer", "actor", "Buyer", "A person who pays for an order."),
    node("checkout", "service", "Checkout", "Takes the payment of an order."),
    node("gateway", "system", "Payment gateway", "The external card processor."),
    node("orders-db", "database", "Orders database", "The state of each order."),
  ],
  relationships: [
    {
      "unique-id": "buyer-pays",
      description: "Pays",
      "relationship-type": { interacts: { actor: "buyer", nodes: ["checkout"] } },
    },
    connects("checkout-to-gateway", "checkout", "gateway", "Charges the card", "HTTPS"),
    connects("checkout-to-db", "checkout", "orders-db", "Marks the order paid", "JDBC"),
  ],
  flows: [
    {
      "unique-id": "pay-for-order",
      name: "Pay for an order",
      description: "The buyer pays, and the order becomes paid.",
      transitions: [
        {
          "relationship-unique-id": "buyer-pays",
          "sequence-number": 1,
          description: "The buyer submits the payment.",
        },
        {
          "relationship-unique-id": "checkout-to-gateway",
          "sequence-number": 2,
          description: "Checkout charges the card.",
        },
        {
          "relationship-unique-id": "checkout-to-gateway",
          "sequence-number": 3,
          description: "The gateway confirms the charge.",
          direction: "destination-to-source",
        },
        {
          "relationship-unique-id": "checkout-to-db",
          "sequence-number": 4,
          description: "Checkout marks the order paid.",
        },
      ],
    },
  ],
});

/** CAD-FR-JWXC: the templates, in the order of the menu. */
export const CALM_TEMPLATES: readonly CalmTemplate[] = [
  { key: "minimal", name: "Minimal", source: MINIMAL },
  { key: "three-tier", name: "Three-tier web application", source: THREE_TIER },
  { key: "deployed", name: "Deployed services", source: DEPLOYED },
  { key: "flow", name: "Business flow", source: FLOW },
];
