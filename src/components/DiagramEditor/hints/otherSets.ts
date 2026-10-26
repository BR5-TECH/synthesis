/**
 * EDT-FR-QJJP: the hint sets of the other chart types. Each holds the
 * declaration, the main element forms, and a short example, as the bundled
 * Mermaid version (11.17.2) reads them.
 */
import { OTHER_SETS_2 } from "./otherSets2";
import type { Hint, HintSet } from "./types";

/** One hint: a snippet and its short explanation. */
function hint(code: string, text: string): Hint {
  return { code, text };
}

const C4: HintSet = {
  key: "c4",
  name: "C4 diagram",
  mermaidIds: ["c4"],
  detect: /^C4(?:Context|Container|Component|Dynamic|Deployment)\b/,
  declaration: hint("C4Context", "Declares a C4 system context diagram."),
  groups: [
    {
      title: "Diagram levels",
      hints: [
        hint("C4Container", "Declares a C4 container diagram."),
        hint("C4Component", "Declares a C4 component diagram."),
        hint("C4Dynamic", "Declares a C4 dynamic diagram. It numbers the relations."),
        hint("C4Deployment", "Declares a C4 deployment diagram."),
        hint("title Banking system", "Sets the title of the diagram."),
      ],
    },
    {
      title: "Elements",
      hints: [
        hint('Person(user, "Customer", "A bank customer.")', "A person: an id, a label, and a description."),
        hint('Person_Ext(guest, "Guest")', "The _Ext form shows an external element."),
        hint('System(bank, "Online banking", "Shows the accounts.")', "A software system."),
        hint('System_Ext(mail, "Mail system")', "An external software system."),
        hint('SystemDb(db, "Accounts")', "A system with a database shape."),
        hint('SystemQueue(queue, "Events")', "A system with a queue shape."),
        hint('Container(web, "Web app", "React", "Shows the UI.")', "A container: an id, a label, a technology, and a description."),
        hint('ContainerDb(store, "Database", "PostgreSQL")', "A container with a database shape."),
        hint('Component(auth, "Auth", "Rust", "Checks the users.")', "A component of a container."),
      ],
    },
    {
      title: "Boundaries",
      hints: [
        hint('Enterprise_Boundary(org, "Bank") {\n  System(core, "Core")\n}', "An enterprise boundary around elements."),
        hint('System_Boundary(sys, "Banking") {\n  Container(app, "App")\n}', "A system boundary around containers."),
        hint('Container_Boundary(api, "API") {\n  Component(auth, "Auth")\n}', "A container boundary around components."),
        hint('Boundary(team, "Team", "group") {\n  System(tool, "Tool")\n}', "A general boundary with a type."),
        hint('Deployment_Node(dc, "Data center", "Ubuntu") {\n  Container(app, "App")\n}', "A deployment node. Nodes can hold other nodes."),
        hint('Node(srv, "Server") {\n  Container(app, "App")\n}', "The short form of a deployment node."),
      ],
    },
    {
      title: "Relations",
      hints: [
        hint('Rel(user, bank, "Uses")', "A relation from one element to another."),
        hint('Rel(user, bank, "Uses", "HTTPS")', "A relation with a technology."),
        hint('BiRel(bank, mail, "Syncs")', "A relation in the two directions."),
        hint('Rel_D(bank, mail, "Sends")', "Rel_U, Rel_D, Rel_L, and Rel_R set the direction."),
        hint('Rel_Back(mail, bank, "Replies")', "A relation that points back."),
        hint('RelIndex(1, user, bank, "Logs in")', "A relation with a number, for a dynamic diagram."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint('UpdateElementStyle(user, $bgColor="grey", $borderColor="red")', "Changes the colors of an element."),
        hint('UpdateRelStyle(user, bank, $textColor="blue", $offsetY="-10")', "Changes the color and the label position of a relation."),
        hint('UpdateLayoutConfig($c4ShapeInRow="3", $c4BoundaryInRow="1")', "Sets the number of shapes and boundaries in a row."),
      ],
    },
  ],
  example: [
    "C4Context",
    "  title Banking system",
    '  Person(user, "Customer", "A bank customer.")',
    '  Enterprise_Boundary(org, "Bank") {',
    '    System(bank, "Online banking", "Shows the accounts.")',
    '    SystemDb(db, "Accounts")',
    "  }",
    '  System_Ext(mail, "Mail system")',
    '  Rel(user, bank, "Uses", "HTTPS")',
    '  Rel(bank, db, "Reads")',
    '  Rel(bank, mail, "Sends mail")',
  ].join("\n"),
};

const GIT_GRAPH: HintSet = {
  key: "gitGraph",
  name: "Git graph",
  mermaidIds: ["gitGraph"],
  detect: /^gitGraph\b/,
  declaration: hint("gitGraph", "Declares a Git graph. The graph starts on the main branch."),
  groups: [
    {
      title: "Direction",
      hints: [
        hint("gitGraph TB:", "LR, TB, and BT set the direction. The colon is necessary."),
      ],
    },
    {
      title: "Commits",
      hints: [
        hint("commit", "Adds a commit to the current branch."),
        hint('commit id: "Fix"', "A commit with an id. Other statements use the id."),
        hint('commit msg: "Add the login"', "A commit with a message."),
        hint('commit tag: "v1.0"', "A commit with a tag."),
        hint("commit type: HIGHLIGHT", "The type is NORMAL, REVERSE, or HIGHLIGHT."),
      ],
    },
    {
      title: "Branches",
      hints: [
        hint("branch develop", "Makes a branch and makes it the current branch."),
        hint("branch hotfix order: 1", "The order sets the position of the branch line."),
        hint("checkout main", "Makes a branch the current branch."),
        hint("switch main", "The same as checkout."),
      ],
    },
    {
      title: "Merges",
      hints: [
        hint("merge develop", "Merges a branch into the current branch."),
        hint('merge develop id: "M1" tag: "v2.0" type: REVERSE', "A merge commit with an id, a tag, and a type."),
        hint('cherry-pick id: "Fix"', "Copies a commit of another branch to the current branch."),
      ],
    },
  ],
  example: [
    "gitGraph",
    '  commit id: "Init"',
    "  branch develop",
    "  checkout develop",
    '  commit id: "Feature"',
    "  checkout main",
    '  commit id: "Fix" type: HIGHLIGHT',
    '  merge develop tag: "v1.0"',
  ].join("\n"),
};

const QUADRANT: HintSet = {
  key: "quadrantChart",
  name: "Quadrant chart",
  mermaidIds: ["quadrantChart"],
  detect: /^quadrantChart\b/,
  declaration: hint("quadrantChart", "Declares a quadrant chart."),
  groups: [
    {
      title: "Axes and title",
      hints: [
        hint("title Priorities", "Sets the title of the chart."),
        hint("x-axis Low effort --> High effort", "The labels at the two ends of the x axis."),
        hint("y-axis Low value --> High value", "The labels at the two ends of the y axis."),
        hint("x-axis Effort", "One label for the axis."),
      ],
    },
    {
      title: "Quadrants",
      hints: [
        hint("quadrant-1 Do first", "The top right quadrant."),
        hint("quadrant-2 Plan", "The top left quadrant."),
        hint("quadrant-3 Skip", "The bottom left quadrant."),
        hint("quadrant-4 Delegate", "The bottom right quadrant."),
      ],
    },
    {
      title: "Points",
      hints: [
        hint("Task A: [0.3, 0.6]", "A point. The x and y values are from 0 to 1."),
        hint("Task B: [0.8, 0.2] radius: 10, color: #ff3300", "A point with its own size and color."),
        hint("Task C: [0.5, 0.9] stroke-color: #333, stroke-width: 2px", "A point with its own border."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("classDef urgent color: #c00, radius: 8", "Defines a style class for points."),
        hint("Task D:::urgent: [0.9, 0.9]", "The ::: form gives a class to a point."),
      ],
    },
  ],
  example: [
    "quadrantChart",
    "  title Priorities",
    "  x-axis Low effort --> High effort",
    "  y-axis Low value --> High value",
    "  quadrant-1 Plan",
    "  quadrant-2 Do first",
    "  quadrant-3 Delegate",
    "  quadrant-4 Skip",
    "  Login: [0.2, 0.8]",
    "  Reports: [0.7, 0.6]",
  ].join("\n"),
};

const XY_CHART: HintSet = {
  key: "xychart",
  name: "XY chart",
  mermaidIds: ["xychart"],
  detect: /^xychart(?:-beta)?\b/,
  declaration: hint("xychart", "Declares an XY chart. The xychart-beta form also works."),
  groups: [
    {
      title: "Layout and title",
      hints: [
        hint("xychart horizontal", "Draws the bars from left to right."),
        hint('title "Monthly sales"', "Sets the title of the chart."),
      ],
    },
    {
      title: "Axes",
      hints: [
        hint("x-axis [Jan, Feb, Mar]", "An x axis with categories."),
        hint('x-axis "Month" [Jan, Feb, Mar]', "An x axis with a title and categories."),
        hint('x-axis "Score" 0 --> 100', "An x axis with a range of numbers."),
        hint('y-axis "Revenue" 0 --> 1000', "A y axis with a title and a range."),
      ],
    },
    {
      title: "Data",
      hints: [
        hint("bar [300, 500, 400]", "A bar series. One value for each category."),
        hint("line [300, 500, 400]", "A line series."),
      ],
    },
  ],
  example: [
    "xychart",
    '  title "Monthly sales"',
    "  x-axis [Jan, Feb, Mar, Apr]",
    '  y-axis "Revenue" 0 --> 1000',
    "  bar [300, 500, 400, 700]",
    "  line [300, 500, 400, 700]",
  ].join("\n"),
};

const REQUIREMENT: HintSet = {
  key: "requirement",
  name: "Requirement diagram",
  mermaidIds: ["requirement"],
  detect: /^requirement(?:Diagram)?\b/,
  declaration: hint("requirementDiagram", "Declares a requirement diagram."),
  groups: [
    {
      title: "Requirements",
      hints: [
        hint(
          "requirement login {\n  id: 1\n  text: The user can log in.\n  risk: high\n  verifymethod: test\n}",
          "A requirement with an id, a text, a risk, and a verify method.",
        ),
        hint("functionalRequirement export {\n  id: 2\n}", "Other types: interfaceRequirement, performanceRequirement, physicalRequirement, designConstraint."),
        hint("requirement speed {\n  risk: medium\n}", "The risk is low, medium, or high."),
        hint("requirement audit {\n  verifymethod: inspection\n}", "The verify method is analysis, inspection, test, or demonstration."),
      ],
    },
    {
      title: "Elements",
      hints: [
        hint("element login_test {\n  type: \"test suite\"\n  docref: tests/login\n}", "An element with a type and a document reference. Quotes keep a keyword, such as test, in a value."),
      ],
    },
    {
      title: "Relations",
      hints: [
        hint("login_test - verifies -> login", "A relation from the element to the requirement."),
        hint("login <- refines - mfa", "The arrow can point to the left."),
        hint("export - derives -> login", "The types: contains, copies, derives, satisfies, verifies, refines, traces."),
      ],
    },
    {
      title: "Layout and styling",
      hints: [
        hint("direction LR", "Sets the direction of the diagram."),
        hint("classDef important fill:#f96,stroke:#333", "Defines a style class."),
        hint("class login important", "Gives the class to a requirement or an element."),
        hint("style login fill:#ffd", "Styles one requirement or element."),
      ],
    },
  ],
  example: [
    "requirementDiagram",
    "  requirement login {",
    "    id: 1",
    "    text: The user can log in.",
    "    risk: high",
    "    verifymethod: test",
    "  }",
    "  element login_test {",
    '    type: "test suite"',
    "  }",
    "  login_test - verifies -> login",
  ].join("\n"),
};

const KANBAN: HintSet = {
  key: "kanban",
  name: "Kanban",
  mermaidIds: ["kanban"],
  detect: /^kanban\b/,
  declaration: hint("kanban", "Declares a Kanban board."),
  groups: [
    {
      title: "Columns and cards",
      hints: [
        hint("todo[To do]", "A column: an id and a title in brackets."),
        hint("todo[To do]\n  t1[Write the spec]", "An indented line under a column is a card."),
      ],
    },
    {
      title: "Card data",
      hints: [
        hint("t2[Fix the login]@{ assigned: 'alex' }", "The @{ } form adds data to a card."),
        hint("t3[Fix the build]@{ ticket: 'BUG-12' }", "A ticket number for the card."),
        hint("t4[Update the docs]@{ priority: 'High' }", "The priority is 'Very High', 'High', 'Low', or 'Very Low'."),
      ],
    },
    {
      title: "Ticket links",
      hints: [
        hint(
          "---\nconfig:\n  kanban:\n    ticketBaseUrl: 'https://example.com/browse/#TICKET#'\n---\nkanban",
          "The ticketBaseUrl option makes each ticket a link. #TICKET# gets the ticket number.",
        ),
      ],
    },
  ],
  example: [
    "kanban",
    "  todo[To do]",
    "    t1[Write the spec]",
    "    t2[Fix the login]@{ priority: 'High' }",
    "  doing[In progress]",
    "    t3[Review the code]@{ assigned: 'alex' }",
    "  done[Done]",
  ].join("\n"),
};

const BLOCK: HintSet = {
  key: "block",
  name: "Block diagram",
  mermaidIds: ["block"],
  detect: /^block(?:-beta)?(?![\w-])/,
  declaration: hint("block-beta", "Declares a block diagram."),
  groups: [
    {
      title: "Blocks",
      hints: [
        hint("columns 3", "Sets the number of columns. The blocks fill the rows in order."),
        hint("a b c", "Three blocks in a row."),
        hint('a["Label"]', "A block with a label."),
        hint('a["Wide"]:2', "A block that spans two columns."),
        hint("space", "An empty cell."),
        hint("space:2", "An empty space of two columns."),
      ],
    },
    {
      title: "Shapes",
      hints: [
        hint('a("Rounded")', "A block with round corners."),
        hint('a(("Circle"))', "A circle block."),
        hint('a[("Database")]', "A cylinder block, for a database."),
        hint('a{"Decision"}', "A rhombus block."),
        hint('a>"Flag"]', "An asymmetric block."),
        hint('next<["Next"]>(right)', "An arrow block. The direction is right, left, up, down, x, or y."),
      ],
    },
    {
      title: "Nested blocks",
      hints: [
        hint("block:group1:2\n  columns 2\n  x y\nend", "A block that holds other blocks and spans two columns."),
      ],
    },
    {
      title: "Edges",
      hints: [
        hint("a --> b", "An arrow from one block to another."),
        hint('a -- "calls" --> b', "An arrow with a label."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("style a fill:#f96,stroke:#333", "Styles one block."),
        hint("classDef blue fill:#66f,color:#fff", "Defines a style class."),
        hint("class a blue", "Gives the class to a block."),
      ],
    },
  ],
  example: [
    "block-beta",
    "  columns 3",
    '  front["Frontend"] call<["calls"]>(right) api["API"]',
    '  space:2 db[("Database")]',
    "  api --> db",
    "  style db fill:#ffd",
  ].join("\n"),
};

const ARCHITECTURE: HintSet = {
  key: "architecture",
  name: "Architecture diagram",
  mermaidIds: ["architecture"],
  detect: /^architecture(?:-beta)?\b/,
  declaration: hint("architecture-beta", "Declares an architecture diagram."),
  groups: [
    {
      title: "Groups and services",
      hints: [
        hint("group cloud(cloud)[Cloud]", "A group: an id, an icon, and a title."),
        hint("group data(database)[Data] in cloud", "The in form puts a group into another group."),
        hint("service api(server)[API]", "A service: an id, an icon, and a title."),
        hint("service store(database)[Database] in cloud", "A service in a group."),
        hint("service disk(disk)[Storage]", "The built-in icons: cloud, database, disk, internet, server."),
        hint("junction hub", "A junction joins more edges in one point."),
      ],
    },
    {
      title: "Edges",
      hints: [
        hint("api:R -- L:db", "An edge from the right side of api to the left side of db."),
        hint("api:B --> T:disk", "An edge with an arrow. The sides are L, R, T, and B."),
        hint("api:R <--> L:db", "An edge with an arrow at the two ends."),
        hint("api:R -[queries]- L:db", "An edge with a title."),
        hint("api{group}:R --> L:db", "The {group} form starts the edge at the group of the service."),
      ],
    },
  ],
  example: [
    "architecture-beta",
    "  group cloud(cloud)[Cloud]",
    "  service api(server)[API] in cloud",
    "  service db(database)[Database] in cloud",
    "  service web(internet)[Web]",
    "  web:R --> L:api",
    "  api:R -- L:db",
  ].join("\n"),
};

const PACKET: HintSet = {
  key: "packet",
  name: "Packet diagram",
  mermaidIds: ["packet"],
  detect: /^packet(?:-beta)?\b/,
  declaration: hint("packet", "Declares a packet diagram. The packet-beta form also works."),
  groups: [
    {
      title: "Fields",
      hints: [
        hint('0-15: "Source port"', "A field from bit 0 to bit 15."),
        hint('16: "Flag"', "A field of one bit."),
        hint('+8: "Length"', "A field of the next 8 bits."),
        hint("title TCP header", "Sets the title of the diagram."),
      ],
    },
  ],
  example: [
    "packet",
    "  title UDP header",
    '  0-15: "Source port"',
    '  16-31: "Destination port"',
    '  +16: "Length"',
    '  +16: "Checksum"',
  ].join("\n"),
};

const SANKEY: HintSet = {
  key: "sankey",
  name: "Sankey diagram",
  mermaidIds: ["sankey"],
  detect: /^sankey(?:-beta)?\b/,
  declaration: hint("sankey", "Declares a Sankey diagram. The sankey-beta form also works."),
  groups: [
    {
      title: "Flows",
      hints: [
        hint("Solar,Grid,40", "A flow: the source, the target, and the value, as CSV."),
        hint('"Coal, raw",Plant,25', "Quotes keep a comma in a name."),
        hint("Grid,Homes,30\nGrid,Industry,10", "A node can be the target of one flow and the source of another."),
      ],
    },
  ],
  example: ["sankey", "Solar,Grid,40", "Wind,Grid,20", "Grid,Homes,35", "Grid,Industry,25"].join("\n"),
};

export const OTHER_SETS: readonly HintSet[] = [
  C4,
  GIT_GRAPH,
  QUADRANT,
  XY_CHART,
  REQUIREMENT,
  KANBAN,
  BLOCK,
  ARCHITECTURE,
  PACKET,
  SANKEY,
  ...OTHER_SETS_2,
];
