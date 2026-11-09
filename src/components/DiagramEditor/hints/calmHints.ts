/**
 * CAD-FR-KWIY, CAD-FR-LEII: the one hint set of a CALM fence. It gives the
 * forms of CALM 1.3 that an author writes most, and a short complete example.
 * The text is bundled English, and nothing comes from the network.
 */
import type { HintGroup } from "./types";

/** CAD-FR-KWIY: the title of the hints area for a CALM fence. */
export const CALM_HINTS_TITLE = "CALM 1.3";

/** CAD-FR-LEII: a short complete example whose read gives a diagram graph. */
export const CALM_HINT_EXAMPLE = `{
  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",
  "nodes": [
    { "unique-id": "user", "node-type": "actor", "name": "User", "description": "Uses the app." },
    { "unique-id": "api", "node-type": "service", "name": "API", "description": "Serves the app." },
    { "unique-id": "db", "node-type": "database", "name": "Database", "description": "Keeps the data." }
  ],
  "relationships": [
    { "unique-id": "user-api", "relationship-type": { "interacts": { "actor": "user", "nodes": ["api"] } } },
    { "unique-id": "api-db", "protocol": "JDBC",
      "relationship-type": { "connects": { "source": { "node": "api" }, "destination": { "node": "db" } } } }
  ]
}`;

export const CALM_HINT_GROUPS: readonly HintGroup[] = [
  {
    title: "Document",
    hints: [
      {
        code: '{\n  "$schema": "https://calm.finos.org/release/1.3/meta/calm.json",\n  "nodes": [],\n  "relationships": []\n}',
        text: "The frame of an architecture. Declare the CALM 1.3 schema. Each unique-id is used once.",
      },
      {
        code: '"metadata": { "owner": "payments-team" }',
        text: "Free key and value notes, at the top level or on a node or relationship.",
      },
      { code: '"adrs": ["https://example.com/adr/0001"]', text: "Links to decision records." },
    ],
  },
  {
    title: "Nodes",
    hints: [
      {
        code: '{ "unique-id": "api", "node-type": "service",\n  "name": "API", "description": "Serves the app." }',
        text: "A node. All four keys are required.",
      },
      {
        code: "actor  system  ecosystem  service  database\nwebclient  network  ldap  data-asset",
        text: "The node types that have a shape. Any other text gives the generic shape.",
      },
    ],
  },
  {
    title: "Relationships",
    hints: [
      {
        code: '"relationship-type": { "interacts":\n  { "actor": "user", "nodes": ["api"] } }',
        text: "An actor uses nodes. Draws a dashed edge to each node.",
      },
      {
        code: '"relationship-type": { "connects": {\n  "source": { "node": "api" },\n  "destination": { "node": "db", "interfaces": ["db-port"] } } }',
        text: "A connection from a source to a destination, with optional interface ids.",
      },
      { code: '"protocol": "HTTPS"', text: "The protocol of a connection. It shows as a chip on the edge." },
      {
        code: '"relationship-type": { "deployed-in":\n  { "container": "cluster", "nodes": ["api"] } }',
        text: "The container becomes a group with a dashed border.",
      },
      {
        code: '"relationship-type": { "composed-of":\n  { "container": "shop", "nodes": ["api", "db"] } }',
        text: "The container becomes a group with a solid border.",
      },
      {
        code: '"relationship-type": { "options": [ { "description": "Use a cache",\n  "nodes": ["cache"], "relationships": ["api-cache"] } ] }',
        text: "Decisions. They draw no edge, and they mark what they name.",
      },
    ],
  },
  {
    title: "Interfaces",
    hints: [
      {
        code: '"interfaces": [ { "unique-id": "api-http", "port": 8080 } ]',
        text: "Interfaces of a node. A connects end can name them.",
      },
    ],
  },
  {
    title: "Flows",
    hints: [
      {
        code: '"flows": [ { "unique-id": "checkout", "name": "Checkout",\n  "description": "The buyer pays.", "transitions": [] } ]',
        text: "A business flow over the relationships.",
      },
      {
        code: '{ "relationship-unique-id": "api-db", "sequence-number": 1,\n  "description": "Store the order.",\n  "direction": "destination-to-source" }',
        text: "One step. The direction is optional, and source-to-destination is the default.",
      },
    ],
  },
  {
    title: "Controls",
    hints: [
      {
        code: '"controls": { "security": { "description": "Encrypt data in transit.",\n  "requirements": [ { "requirement-url": "https://example.com/req",\n    "config": { "tls": "1.3" } } ] } }',
        text: "Controls of the architecture, a node, a relationship, or a flow.",
      },
    ],
  },
  {
    title: "Example",
    hints: [{ code: CALM_HINT_EXAMPLE, text: "A short complete architecture." }],
  },
];
