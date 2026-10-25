/**
 * EDT-FR-DUSC: the rich hint sets. Each holds the element forms, the
 * connection forms, the labels, the grouping, the styling, and the comments
 * of its chart type, as the bundled Mermaid version (11.17.2) reads them.
 */
import type { Hint, HintGroup, HintSet } from "./types";

/** One hint: a snippet and its short explanation. */
function hint(code: string, text: string): Hint {
  return { code, text };
}

/** The comment group, which every rich set holds. */
const COMMENTS: HintGroup = {
  title: "Comments",
  hints: [hint("%% A comment", "A line that starts with %% is a comment.")],
};

const FLOWCHART: HintSet = {
  key: "flowchart",
  name: "Flowchart",
  mermaidIds: ["flowchart", "flowchart-v2", "flowchart-elk"],
  detect: /^(?:flowchart(?:-elk)?|graph)\b/,
  declaration: hint(
    "flowchart LR",
    "Declares a flowchart. TD, TB, BT, RL, and LR set the direction.",
  ),
  groups: [
    {
      title: "Nodes",
      hints: [
        hint("A", "A node with its id as the text."),
        hint("A[Rectangle]", "A rectangle node."),
        hint("A(Rounded)", "A node with round corners."),
        hint("A([Stadium])", "A stadium node."),
        hint("A[[Subroutine]]", "A subroutine node."),
        hint("A[(Database)]", "A cylinder node, for a database."),
        hint("A((Circle))", "A circle node."),
        hint("A(((Double circle)))", "A double circle node."),
        hint("A{Decision}", "A rhombus node, for a decision."),
        hint("A{{Hexagon}}", "A hexagon node."),
        hint("A>Flag]", "An asymmetric node."),
        hint("A[/Parallelogram/]", "A parallelogram node."),
        hint("A[/Trapezoid\\]", "A trapezoid node."),
        hint('A@{ shape: doc, label: "Report" }', "The @{ } form sets one of the new shapes."),
        hint("A@{ shape: circle }", "Other shapes: rect, rounded, diam, cyl, hex, stadium, fork."),
      ],
    },
    {
      title: "Edges",
      hints: [
        hint("A --> B", "An arrow."),
        hint("A --- B", "A line with no arrow head."),
        hint("A -.-> B", "A dotted arrow."),
        hint("A ==> B", "A thick arrow."),
        hint("A --o B", "An edge with a circle at the end."),
        hint("A --x B", "An edge with a cross at the end."),
        hint("A <--> B", "An arrow at the two ends."),
        hint("A ~~~ B", "An invisible link. It changes only the layout."),
        hint("A ----> B", "More dashes make a longer edge."),
      ],
    },
    {
      title: "Chaining",
      hints: [
        hint("A --> B --> C", "Edges can follow each other on one line."),
        hint("A --> B & C", "The & joins one node to two nodes."),
        hint("A & B --> C & D", "The & works at the two sides of an edge."),
      ],
    },
    {
      title: "Labels",
      hints: [
        hint("A -->|Yes| B", "A label on an edge."),
        hint("A -- Yes --> B", "The same label in the other form."),
        hint("A -. Maybe .-> B", "A label on a dotted edge."),
        hint('A["Text (with) [brackets]"]', "Quotes keep special characters in the text."),
        hint("A[Line one<br>Line two]", "The <br> tag breaks the text."),
        hint('A["`**Bold** and *italic*`"]', "Backticks in quotes make Markdown text."),
      ],
    },
    {
      title: "Grouping",
      hints: [
        hint("subgraph one [Title]\n  A --> B\nend", "A subgraph with an id and a title."),
        hint("subgraph two\n  direction LR\n  C --> D\nend", "A subgraph with its own direction."),
        hint("subgraph one\n  A\nend\none --> B", "An edge can start or end at a subgraph."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("classDef warn fill:#fdd,stroke:#c00", "Defines a style class."),
        hint("class A,B warn", "Gives the class to the nodes."),
        hint("A:::warn --> B", "The ::: form gives a class in an edge."),
        hint("classDef default fill:#eef", "The default class applies to all nodes."),
        hint("style A fill:#ffd,stroke-width:3px", "Styles one node."),
        hint("linkStyle 0 stroke:#c00,stroke-width:2px", "Styles an edge by its number."),
        hint("linkStyle default stroke:#888", "Styles all edges."),
      ],
    },
    COMMENTS,
  ],
  example: [
    "flowchart LR",
    "  %% A small order flow",
    "  start([Start]) --> check{In stock?}",
    "  check -->|Yes| pack[Pack the order]",
    "  check -- No --> wait[(Back orders)]",
    "  subgraph ship [Shipping]",
    "    pack ==> send>Send]",
    "  end",
    "  classDef alert fill:#fdd,stroke:#c00",
    "  class wait alert",
  ].join("\n"),
};

const SEQUENCE: HintSet = {
  key: "sequence",
  name: "Sequence diagram",
  mermaidIds: ["sequence"],
  detect: /^sequenceDiagram\b/,
  declaration: hint("sequenceDiagram", "Declares a sequence diagram."),
  groups: [
    {
      title: "Participants",
      hints: [
        hint("participant A", "A participant box. A message can also add it."),
        hint("participant A as Alice", "A participant with an id and a label."),
        hint("actor U as User", "A participant with a person symbol."),
        hint('participant D@{ "type": "database" }', "The @{ } form sets a symbol, such as database."),
        hint("create participant B\nA->>B: Hello", "Adds a participant at the next message."),
        hint("A->>B: Bye\ndestroy B", "Removes a participant after a message."),
        hint("autonumber", "Puts a number on each message."),
      ],
    },
    {
      title: "Messages",
      hints: [
        hint("A->>B: Request", "A solid line with an arrow."),
        hint("B-->>A: Reply", "A dotted line with an arrow."),
        hint("A->B: Text", "A solid line with no arrow."),
        hint("A-->B: Text", "A dotted line with no arrow."),
        hint("A-xB: Fail", "A solid line with a cross at the end."),
        hint("A-)B: Event", "A solid line with an open arrow, for an async message."),
        hint("A<<->>B: Sync", "A solid line with an arrow at the two ends."),
        hint("A->>A: Think", "A message to the same participant."),
      ],
    },
    {
      title: "Activation",
      hints: [
        hint("A->>+B: Start\nB-->>-A: Done", "The + and - turn the activation bar on and off."),
        hint("activate B\ndeactivate B", "The same activation in long form."),
      ],
    },
    {
      title: "Notes and labels",
      hints: [
        hint("Note right of A: Text", "A note at the right of a participant."),
        hint("Note left of A: Text", "A note at the left of a participant."),
        hint("Note over A,B: Text", "A note across two participants."),
        hint("A->>B: Line one<br/>Line two", "The <br/> tag breaks the text."),
      ],
    },
    {
      title: "Control flow",
      hints: [
        hint("loop Every minute\n  A->>B: Ping\nend", "A loop block."),
        hint(
          "alt Success\n  B->>A: OK\nelse Failure\n  B->>A: Error\nend",
          "Alternative paths.",
        ),
        hint("opt Has cache\n  A->>B: Read\nend", "An optional path."),
        hint("par To B\n  A->>B: Hi\nand To C\n  A->>C: Hi\nend", "Paths that run at the same time."),
        hint("critical Connect\n  A->>B: Open\noption Timeout\n  A->>A: Retry\nend", "A critical region."),
        hint("break On error\n  A->>B: Stop\nend", "A break out of the flow."),
      ],
    },
    {
      title: "Grouping and styling",
      hints: [
        hint("box Aqua Backend\n  participant B\n  participant C\nend", "A colored box around participants."),
        hint("rect rgb(220, 235, 255)\n  A->>B: Hi\nend", "A colored background behind messages."),
      ],
    },
    COMMENTS,
  ],
  example: [
    "sequenceDiagram",
    "  autonumber",
    "  actor U as User",
    "  participant S as Server",
    "  U->>S: Request",
    "  alt Valid",
    "    S-->>U: Data",
    "  else Invalid",
    "    S--xU: Error",
    "  end",
    "  Note over U,S: Done",
  ].join("\n"),
};

const CLASS: HintSet = {
  key: "class",
  name: "Class diagram",
  mermaidIds: ["class", "classDiagram"],
  detect: /^classDiagram(?:-v2)?\b/,
  declaration: hint("classDiagram", "Declares a class diagram."),
  groups: [
    {
      title: "Classes",
      hints: [
        hint("class Animal", "A class."),
        hint('class Animal["Living thing"]', "A class with a label."),
        hint("class Animal {\n  +String name\n  +eat(food) bool\n}", "A class with its members."),
        hint("Animal : +int age", "Adds an attribute to a class."),
        hint("Animal : +move(speed) void", "Adds a method to a class."),
        hint("class Box~T~", "A generic class."),
        hint("class Shape {\n  <<interface>>\n}", "An annotation, such as interface or abstract."),
        hint("class Color\n<<enumeration>> Color", "An annotation on its own line, after the class."),
      ],
    },
    {
      title: "Members",
      hints: [
        hint("class A {\n  +pub\n  -priv\n  #prot\n  ~pkg\n}", "Visibility: + public, - private, # protected, ~ package."),
        hint("class A {\n  +count()$ int\n  +draw()*\n}", "The $ marks a static member, the * an abstract one."),
        hint("class A {\n  +List~int~ ids\n}", "The ~ form gives a generic type."),
      ],
    },
    {
      title: "Relations",
      hints: [
        hint("Animal <|-- Dog", "Inheritance."),
        hint("Car *-- Wheel", "Composition."),
        hint("Team o-- Player", "Aggregation."),
        hint("A --> B", "Association."),
        hint("A -- B", "A solid link."),
        hint("A ..> B", "Dependency."),
        hint("Shape <|.. Square", "Realization."),
        hint("A .. B", "A dashed link."),
        hint("A <|--|> B", "Arrow heads can be at the two ends."),
      ],
    },
    {
      title: "Labels and cardinality",
      hints: [
        hint("A --> B : uses", "A label on a relation."),
        hint('Customer "1" --> "*" Order : places', "Cardinality at the two ends."),
        hint('A "0..1" -- "1..*" B', "Other cardinality values: 1, 0..1, 1..*, *, n."),
      ],
    },
    {
      title: "Grouping and notes",
      hints: [
        hint("namespace Shapes {\n  class Circle\n  class Square\n}", "A namespace around classes."),
        hint('note "A general note"', "A note for the diagram."),
        hint('note for Dog "Barks a lot"', "A note for one class."),
        hint("direction RL", "Sets the direction of the diagram."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("classDef hot fill:#f96,stroke:#333", "Defines a style class."),
        hint("class Animal:::hot", "The ::: form gives a class."),
        hint("cssClass \"Animal,Dog\" hot", "Gives the class to more classes."),
        hint("style Animal fill:#ffd,stroke-width:2px", "Styles one class."),
      ],
    },
    COMMENTS,
  ],
  example: [
    "classDiagram",
    "  class Animal {",
    "    <<abstract>>",
    "    +String name",
    "    +move() void",
    "  }",
    "  Animal <|-- Dog",
    '  Dog "1" o-- "*" Toy : plays with',
    '  note for Dog "A good friend"',
  ].join("\n"),
};

const STATE: HintSet = {
  key: "state",
  name: "State diagram",
  mermaidIds: ["state", "stateDiagram"],
  detect: /^stateDiagram(?:-v2)?\b/,
  declaration: hint("stateDiagram-v2", "Declares a state diagram."),
  groups: [
    {
      title: "States",
      hints: [
        hint("Idle", "A state with its id as the text."),
        hint('state "Waits for input" as Idle', "A state with an id and a description."),
        hint("Idle : Waits for input", "Another form of a description."),
        hint("[*] --> Idle", "The start state."),
        hint("Done --> [*]", "The end state."),
        hint("state check <<choice>>", "A choice point."),
        hint("state split <<fork>>\nstate merge <<join>>", "A fork and a join."),
      ],
    },
    {
      title: "Transitions",
      hints: [
        hint("Idle --> Active", "A transition."),
        hint("Idle --> Active : start", "A transition with a label."),
        hint("check --> Ok : valid\ncheck --> Bad : invalid", "Transitions out of a choice point."),
      ],
    },
    {
      title: "Composite states",
      hints: [
        hint(
          "state Active {\n  [*] --> Running\n  Running --> Paused\n}",
          "A state that holds other states.",
        ),
        hint(
          "state Active {\n  A1 --> A2\n  --\n  B1 --> B2\n}",
          "The -- line separates regions that run at the same time.",
        ),
        hint("state Active {\n  direction LR\n  A --> B\n}", "A composite state with its own direction."),
        hint("direction LR", "Sets the direction of the diagram."),
      ],
    },
    {
      title: "Notes",
      hints: [
        hint("note right of Idle : Text", "A note at the right of a state."),
        hint("note left of Idle\n  Line one\n  Line two\nend note", "A note with more lines."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("classDef bad fill:#f66,color:#fff", "Defines a style class."),
        hint("class Error bad", "Gives the class to a state."),
        hint("Error:::bad --> Idle", "The ::: form gives a class in a transition."),
        hint("style Idle fill:#ffd,stroke:#333", "Styles one state."),
      ],
    },
    COMMENTS,
  ],
  example: [
    "stateDiagram-v2",
    "  [*] --> Idle",
    "  Idle --> Active : start",
    "  state Active {",
    "    [*] --> Running",
    "    Running --> Paused : pause",
    "    Paused --> Running : resume",
    "  }",
    "  Active --> [*] : stop",
    "  note right of Idle : Waits for input",
  ].join("\n"),
};

const ER: HintSet = {
  key: "er",
  name: "Entity relationship diagram",
  mermaidIds: ["er"],
  detect: /^erDiagram\b/,
  declaration: hint("erDiagram", "Declares an entity relationship diagram."),
  groups: [
    {
      title: "Entities",
      hints: [
        hint("CUSTOMER", "An entity."),
        hint('p["Person"]', "An entity with an id and a label."),
        hint("CUSTOMER {\n  int id PK\n  string name\n}", "An entity with its attributes."),
      ],
    },
    {
      title: "Attributes",
      hints: [
        hint("ORDER {\n  int id PK\n  int customer_id FK\n}", "PK marks the primary key, FK a foreign key."),
        hint("USER {\n  string email UK\n}", "UK marks a unique key."),
        hint('USER {\n  string name "Full name"\n}', "A comment in quotes after an attribute."),
        hint("USER {\n  int id PK, FK\n}", "A comma separates more keys."),
      ],
    },
    {
      title: "Relationships",
      hints: [
        hint("CUSTOMER ||--o{ ORDER : places", "A solid line: an identifying relationship."),
        hint("CAR ||..o{ PERSON : drives", "A dotted line: a non-identifying relationship."),
        hint("PERSON ||--|| PASSPORT : has", "One to exactly one."),
        hint("POST }o--o{ TAG : has", "Zero or more to zero or more."),
      ],
    },
    {
      title: "Cardinality",
      hints: [
        hint("A ||--|| B : x", "|| means exactly one."),
        hint("A |o--o| B : x", "|o and o| mean zero or one."),
        hint("A }|--|{ B : x", "}| and |{ mean one or more."),
        hint("A }o--o{ B : x", "}o and o{ mean zero or more."),
      ],
    },
    {
      title: "Labels and layout",
      hints: [
        hint('ORDER ||--|{ ITEM : "is made of"', "Quotes keep the spaces in a label."),
        hint("direction LR", "Sets the direction of the diagram."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("classDef hot fill:#f96,stroke:#333", "Defines a style class."),
        hint("class CUSTOMER,ORDER hot", "Gives the class to entities."),
        hint("CUSTOMER:::hot ||--o{ ORDER : places", "The ::: form gives a class."),
        hint("style CUSTOMER fill:#ffd", "Styles one entity."),
      ],
    },
    COMMENTS,
  ],
  example: [
    "erDiagram",
    "  CUSTOMER ||--o{ ORDER : places",
    "  ORDER ||--|{ LINE_ITEM : contains",
    "  CUSTOMER {",
    "    int id PK",
    "    string name",
    "  }",
    "  ORDER {",
    "    int id PK",
    "    int customer_id FK",
    "  }",
  ].join("\n"),
};

const GANTT: HintSet = {
  key: "gantt",
  name: "Gantt chart",
  mermaidIds: ["gantt"],
  detect: /^gantt\b/,
  declaration: hint("gantt", "Declares a gantt chart."),
  groups: [
    {
      title: "Settings",
      hints: [
        hint("title Release plan", "The title of the chart."),
        hint("dateFormat YYYY-MM-DD", "The format of the dates in the tasks."),
        hint("axisFormat %d %b", "The format of the dates on the axis."),
        hint("tickInterval 1week", "The distance between the ticks of the axis."),
        hint("excludes weekends", "Days that the tasks skip."),
        hint("excludes 2024-12-25, 2024-12-26", "Dates that the tasks skip."),
        hint("weekday monday", "The first day of the week on the axis."),
        hint("todayMarker off", "Hides the line of the current day."),
        hint("inclusiveEndDates", "Makes an end date part of the task."),
      ],
    },
    {
      title: "Tasks",
      hints: [
        hint("Design :des, 2024-01-01, 5d", "A task with an id, a start date, and a duration."),
        hint("Build :2024-01-08, 2024-01-20", "A task with a start date and an end date."),
        hint("Write docs :3d", "A task that starts after the task above it."),
        hint("Deploy :12h", "Durations can be in ms, s, m, h, d, w, M, and y."),
      ],
    },
    {
      title: "Dependencies",
      hints: [
        hint("Code :code, after des, 10d", "A task that starts after another task."),
        hint("Test :after des code, 3d", "A task that starts after all the listed tasks."),
        hint("Fix bugs :2024-02-01, until rel", "A task that runs until another task starts."),
      ],
    },
    {
      title: "Markers",
      hints: [
        hint("Review :done, rev, 2024-01-01, 2d", "A task that is complete."),
        hint("Code :active, 2024-01-03, 5d", "A task in progress."),
        hint("Audit :crit, 2024-01-05, 2d", "A critical task."),
        hint("Fix :crit, active, 2024-01-06, 1d", "Markers can be combined."),
        hint("Release :milestone, rel, 2024-02-01, 0d", "A milestone."),
        hint("Freeze :vert, 2024-01-20, 0d", "A vertical line across the chart."),
      ],
    },
    {
      title: "Sections",
      hints: [
        hint("section Build\n  Code :c1, 2024-01-01, 5d", "A section groups the tasks under it."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("---\ndisplayMode: compact\n---", "Puts more tasks on one row, when they do not overlap."),
        hint("---\nconfig:\n  gantt:\n    barHeight: 30\n---", "Sets the height of the bars."),
      ],
    },
    COMMENTS,
  ],
  example: [
    "gantt",
    "  title Release plan",
    "  dateFormat YYYY-MM-DD",
    "  excludes weekends",
    "  section Build",
    "    Design :done, des, 2024-01-01, 5d",
    "    Code :active, code, after des, 10d",
    "  section Ship",
    "    Test :crit, test, after code, 4d",
    "    Release :milestone, after test, 0d",
  ].join("\n"),
};

const PIE: HintSet = {
  key: "pie",
  name: "Pie chart",
  mermaidIds: ["pie"],
  detect: /^pie\b/,
  declaration: hint("pie", "Declares a pie chart."),
  groups: [
    {
      title: "Declaration",
      hints: [
        hint("pie title Pets", "Declares a pie chart with a title."),
        hint("pie showData", "Shows the value of each slice next to its label."),
      ],
    },
    {
      title: "Slices",
      hints: [
        hint('"Dogs" : 42', "A slice with a label and a value."),
        hint('"Cats" : 17.5', "A value can have decimals. It cannot be negative."),
        hint('"Dogs" : 3\n"Cats" : 1', "The size of a slice is its part of the sum."),
      ],
    },
    {
      title: "Labels",
      hints: [
        hint("title Pets at home", "The title of the chart, on its own line."),
        hint('"Big dogs (over 20 kg)" : 7', "The quotes hold the label, also with special characters."),
        hint("accTitle: Pets", "A title for screen readers."),
        hint("accDescr: Dogs are the most common pet.", "A description for screen readers."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("---\nconfig:\n  theme: forest\n---", "Sets the theme, and with it the slice colors."),
        hint(
          '---\nconfig:\n  themeVariables:\n    pie1: "#4a90d9"\n    pie2: "#f5a623"\n---',
          "Sets the colors of the slices, from pie1 to pie12.",
        ),
        hint("---\nconfig:\n  pie:\n    textPosition: 0.8\n---", "Moves the labels: 0 is the center, 1 the edge."),
      ],
    },
    COMMENTS,
  ],
  example: ['pie title Pets', '  "Dogs" : 42', '  "Cats" : 31', '  "Birds" : 12'].join("\n"),
};

const MINDMAP: HintSet = {
  key: "mindmap",
  name: "Mindmap",
  mermaidIds: ["mindmap"],
  detect: /^mindmap\b/,
  declaration: hint("mindmap", "Declares a mindmap."),
  groups: [
    {
      title: "Structure",
      hints: [
        hint("root((Topic))", "The first node is the root. A mindmap has one root."),
        hint("Topic\n  Child A\n  Child B", "More indent makes a child of the node above."),
        hint("Topic\n  Child\n    Grandchild", "Each indent level is one level of the tree."),
      ],
    },
    {
      title: "Node shapes",
      hints: [
        hint("Plain text", "A node with no border."),
        hint("id[Square]", "A square node."),
        hint("id(Rounded)", "A node with round corners."),
        hint("id((Circle))", "A circle node."),
        hint("id))Bang((", "A bang node."),
        hint("id)Cloud(", "A cloud node."),
        hint("id{{Hexagon}}", "A hexagon node."),
      ],
    },
    {
      title: "Labels",
      hints: [
        hint('id["`**Bold** and *italic*`"]', "Backticks in quotes make Markdown text."),
        hint('id["`Line one\nLine two`"]', "A Markdown text can break across lines."),
      ],
    },
    {
      title: "Icons and classes",
      hints: [
        hint("Book\n  ::icon(fa fa-book)", "Adds an icon to the node above."),
        hint("Alert\n  :::urgent large", "Gives CSS classes to the node above."),
      ],
    },
    {
      title: "Styling and layout",
      hints: [
        hint("---\nconfig:\n  theme: forest\n---", "Sets the theme, and with it the branch colors."),
        hint("---\nconfig:\n  mindmap:\n    maxNodeWidth: 300\n---", "Sets the largest width of a node."),
      ],
    },
    COMMENTS,
  ],
  example: [
    "mindmap",
    "  root((Project))",
    "    Goals",
    "      Ship v1",
    "      Grow users",
    "    Risks",
    "      ::icon(fa fa-bolt)",
    "      id)Delays(",
    "    Team[Team]",
  ].join("\n"),
};

const TIMELINE: HintSet = {
  key: "timeline",
  name: "Timeline",
  mermaidIds: ["timeline"],
  detect: /^timeline\b/,
  declaration: hint("timeline", "Declares a timeline."),
  groups: [
    {
      title: "Title",
      hints: [
        hint("title History of the web", "The title of the timeline."),
        hint("accTitle: Web history", "A title for screen readers."),
        hint("accDescr: Main events of the web.", "A description for screen readers."),
      ],
    },
    {
      title: "Periods and events",
      hints: [
        hint("2004 : Facebook", "A period and one event."),
        hint("2005 : YouTube : Reddit", "A period with more events."),
        hint("2006 : Twitter\n     : Spotify", "An event on its own line continues the period above."),
        hint("Early days : First website", "A period can be any text."),
      ],
    },
    {
      title: "Sections",
      hints: [
        hint("section 1990s\n  1991 : First website\n  1998 : Google", "A section groups the periods under it."),
      ],
    },
    {
      title: "Labels",
      hints: [
        hint("2010 : Instagram<br>Pinterest", "The <br> tag breaks the text."),
        hint("Q1<br>2024 : Launch", "A period text can also break."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("---\nconfig:\n  theme: forest\n---", "Sets the theme, and with it the section colors."),
        hint(
          "---\nconfig:\n  timeline:\n    disableMulticolor: true\n---",
          "Gives all periods the same color.",
        ),
        hint(
          '---\nconfig:\n  themeVariables:\n    cScale0: "#ff0000"\n---',
          "Sets the color of a section, from cScale0 to cScale11.",
        ),
      ],
    },
    COMMENTS,
  ],
  example: [
    "timeline",
    "  title Web history",
    "  section 1990s",
    "    1991 : First website",
    "    1998 : Google",
    "  section 2000s",
    "    2004 : Facebook : Gmail",
    "    2005 : YouTube",
  ].join("\n"),
};

const JOURNEY: HintSet = {
  key: "journey",
  name: "User journey",
  mermaidIds: ["journey"],
  detect: /^journey\b/,
  declaration: hint("journey", "Declares a user journey."),
  groups: [
    {
      title: "Title",
      hints: [
        hint("title My work day", "The title of the journey."),
        hint("accTitle: Work day", "A title for screen readers."),
        hint("accDescr: The steps of a work day.", "A description for screen readers."),
      ],
    },
    {
      title: "Tasks",
      hints: [
        hint("Make tea: 5: Me", "A task with a score and an actor."),
        hint("Commute: 2: Me, Cat", "A comma separates more actors."),
        hint("Read mail: 3: Me", "Each actor gets a color in the legend."),
      ],
    },
    {
      title: "Scores",
      hints: [
        hint("Lunch: 5: Me", "A score of 5 is the best experience."),
        hint("Meeting: 1: Me", "A score of 1 is the worst experience."),
        hint("Code: 3: Me", "A score of 3 is a neutral experience."),
      ],
    },
    {
      title: "Sections",
      hints: [
        hint("section Morning\n  Make tea: 5: Me\n  Commute: 2: Me", "A section groups the tasks under it."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("---\nconfig:\n  theme: forest\n---", "Sets the theme, and with it the colors."),
        hint(
          '---\nconfig:\n  themeVariables:\n    fillType0: "#ffcc00"\n---',
          "Sets the color of a section, from fillType0 to fillType7.",
        ),
      ],
    },
    COMMENTS,
  ],
  example: [
    "journey",
    "  title A work day",
    "  section Morning",
    "    Make tea: 5: Me",
    "    Commute: 2: Me, Cat",
    "  section Work",
    "    Do work: 3: Me",
  ].join("\n"),
};

/** The rich sets, in the order that detection tries them. */
export const RICH_SETS: readonly HintSet[] = [
  FLOWCHART,
  SEQUENCE,
  CLASS,
  STATE,
  ER,
  GANTT,
  PIE,
  MINDMAP,
  TIMELINE,
  JOURNEY,
];
