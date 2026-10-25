/**
 * EDT-FR-QJJP: more hint sets of the other chart types, mostly of the beta
 * chart types of the bundled Mermaid version (11.17.2).
 */
import type { Hint, HintSet } from "./types";

/** One hint: a snippet and its short explanation. */
function hint(code: string, text: string): Hint {
  return { code, text };
}

const SWIMLANE: HintSet = {
  key: "swimlane",
  name: "Swimlane diagram",
  mermaidIds: ["swimlane"],
  detect: /^swimlane-beta\b/,
  declaration: hint("swimlane-beta LR", "Declares a swimlane diagram. It uses the flowchart syntax."),
  groups: [
    {
      title: "Lanes",
      hints: [
        hint("subgraph shop [Shop]\n  pack[Pack the items]\nend", "Each top-level subgraph is a lane."),
        hint("swimlane-beta TB", "TB and LR set the direction of the flow."),
      ],
    },
    {
      title: "Nodes and edges",
      hints: [
        hint("pack[Pack the items] --> ship[Ship the box]", "Nodes and edges use the flowchart forms."),
        hint("check{In stock?} -->|Yes| pack", "An edge with a label."),
      ],
    },
  ],
  example: [
    "swimlane-beta LR",
    "  subgraph customer [Customer]",
    "    order[Place the order]",
    "  end",
    "  subgraph shop [Shop]",
    "    pack[Pack the items] --> ship[Ship the box]",
    "  end",
    "  order --> pack",
  ].join("\n"),
};

const INFO: HintSet = {
  key: "info",
  name: "Info",
  mermaidIds: ["info"],
  detect: /^info\b/,
  declaration: hint("info", "Shows the version of the bundled Mermaid library."),
  groups: [
    {
      title: "Options",
      hints: [
        hint("info showInfo", "Shows more information about the library."),
        hint("title About", "Sets a title."),
      ],
    },
  ],
  example: ["info", "  showInfo"].join("\n"),
};

const RADAR: HintSet = {
  key: "radar",
  name: "Radar chart",
  mermaidIds: ["radar"],
  detect: /^radar-beta\b/,
  declaration: hint("radar-beta", "Declares a radar chart."),
  groups: [
    {
      title: "Axes and curves",
      hints: [
        hint('axis speed["Speed"], power["Power"], range["Range"]', "The axes: an id and a label for each."),
        hint('curve car["Car"]{4, 3, 5}', "A curve with one value for each axis, in order."),
        hint("curve bike{speed: 2, power: 1, range: 3}", "A curve with a value for each named axis."),
        hint("title Vehicles", "Sets the title of the chart."),
      ],
    },
    {
      title: "Options",
      hints: [
        hint("max 5\nmin 0", "Sets the range of the axes."),
        hint("ticks 5", "Sets the number of circles in the grid."),
        hint("graticule polygon", "The grid shape is circle or polygon."),
        hint("showLegend false", "Hides the legend."),
      ],
    },
  ],
  example: [
    "radar-beta",
    "  title Vehicles",
    '  axis speed["Speed"], power["Power"], range["Range"]',
    '  curve car["Car"]{4, 3, 5}',
    '  curve bike["Bike"]{2, 1, 3}',
    "  max 5",
  ].join("\n"),
};

const TREE_VIEW: HintSet = {
  key: "treeView",
  name: "Tree view",
  mermaidIds: ["treeView"],
  detect: /^treeView-beta\b/,
  declaration: hint("treeView-beta", "Declares a tree view, such as a file tree."),
  groups: [
    {
      title: "Nodes",
      hints: [
        hint("src\n  main.ts\n  utils", "Each line is a node. The indent sets the parent."),
        hint('"My files"', "Quotes keep the spaces of a name."),
        hint("main.ts ## The entry point", "The ## form adds a description."),
        hint("docs icon(mdi:folder)", "The icon form sets the icon of a node."),
        hint("main.ts :::important", "The ::: form gives a CSS class to a node."),
      ],
    },
  ],
  example: [
    "treeView-beta",
    "  project",
    "    src",
    "      main.ts ## The entry point",
    "      utils.ts",
    "    README.md",
  ].join("\n"),
};

const EVENT_MODELING: HintSet = {
  key: "eventmodeling",
  name: "Event modeling",
  mermaidIds: ["eventmodeling"],
  detect: /^eventmodeling\b/,
  declaration: hint("eventmodeling", "Declares an event model."),
  groups: [
    {
      title: "Frames",
      hints: [
        hint("tf 01 ui LoginForm", "A time frame: a number, a type, and a name. The number sets the order."),
        hint("tf 02 cmd LogIn ->> 01", "The ->> form links the frame to a source frame."),
        hint("tf 03 evt UserLoggedIn ->> 02", "The types: ui, cmd, evt, rmo, and pcr."),
        hint("rf 04 rmo Sessions ->> 03", "A reset frame."),
        hint('tf 05 evt LoginFailed ->> 02 { "user": "alex" }', "Inline data for the frame."),
      ],
    },
    {
      title: "Data and notes",
      hints: [
        hint("data Credentials {\n  user: string\n}", "A data block with a name."),
        hint("tf 06 cmd Reset [[Credentials]]", "The [[ ]] form links a frame to a data block."),
        hint("note 02 {\n  The user enters a password.\n}", "A note on a frame."),
      ],
    },
  ],
  example: [
    "eventmodeling",
    "  tf 01 ui LoginForm",
    "  tf 02 cmd LogIn ->> 01",
    "  tf 03 evt UserLoggedIn ->> 02",
    "  tf 04 rmo ActiveSessions ->> 03",
  ].join("\n"),
};

const ISHIKAWA: HintSet = {
  key: "ishikawa",
  name: "Ishikawa diagram",
  mermaidIds: ["ishikawa"],
  detect: /^ishikawa(?:-beta)?\b/i,
  declaration: hint("ishikawa-beta", "Declares an Ishikawa (fishbone) diagram."),
  groups: [
    {
      title: "Problem and causes",
      hints: [
        hint("Late delivery", "The first line is the problem at the head."),
        hint("Late delivery\n  People", "An indented line under the problem is a category."),
        hint("Late delivery\n  People\n    Not enough staff", "A line under a category is a cause."),
        hint("Late delivery\n  Tools\n    Old servers\n      No budget", "More indent adds a cause of a cause."),
      ],
    },
  ],
  example: [
    "ishikawa-beta",
    "  Late delivery",
    "    People",
    "      Not enough staff",
    "    Process",
    "      Slow approval",
    "    Tools",
    "      Old servers",
  ].join("\n"),
};

const VENN: HintSet = {
  key: "venn",
  name: "Venn diagram",
  mermaidIds: ["venn"],
  detect: /^venn-beta\b/,
  declaration: hint("venn-beta", "Declares a Venn diagram."),
  groups: [
    {
      title: "Sets",
      hints: [
        hint("set A", "A set with its id as the label."),
        hint('set A["Apples"]', "A set with a label."),
        hint('set A["Apples"]: 20', "A set with a size."),
        hint('union A,B["Both"]', "The overlap of two or more sets, with a label."),
        hint("title Fruit", "Sets the title of the diagram."),
      ],
    },
    {
      title: "Text and styling",
      hints: [
        hint('text A,B both["Apples and pears"]', "A text item in the named sets: an id and a label. Write it with no indent."),
        hint('set A\n  text pie["Apple pie"]', "An indented text line goes into the set above it."),
        hint("style A fill:#f96, color:#333", "Styles a set or an overlap."),
      ],
    },
  ],
  example: [
    "venn-beta",
    "  title Fruit",
    '  set A["Apples"]',
    '  set B["Pears"]',
    '  union A,B["Both"]',
    "  style A fill:#f96",
  ].join("\n"),
};

const TREEMAP: HintSet = {
  key: "treemap",
  name: "Treemap",
  mermaidIds: ["treemap"],
  detect: /^treemap(?:-beta)?\b/,
  declaration: hint("treemap-beta", "Declares a treemap. The treemap form also works."),
  groups: [
    {
      title: "Sections and leaves",
      hints: [
        hint('"Sales"', "A section: a name in quotes."),
        hint('"Sales"\n  "Online": 40', "An indented leaf with its value."),
        hint('"Sales"\n  "Europe"\n    "France": 12', "Sections can hold other sections."),
      ],
    },
    {
      title: "Styling",
      hints: [
        hint("classDef hot fill:#f96,color:#000", "Defines a style class."),
        hint('"Online": 40:::hot', "The ::: form gives a class to a section or a leaf."),
      ],
    },
  ],
  example: [
    "treemap-beta",
    '  "Sales"',
    '    "Online": 40',
    '    "Shops": 25',
    '  "Costs"',
    '    "Staff": 30',
    '    "Rent": 10',
  ].join("\n"),
};

const WARDLEY: HintSet = {
  key: "wardley",
  name: "Wardley map",
  mermaidIds: ["wardley"],
  detect: /^wardley-beta\b/i,
  declaration: hint("wardley-beta", "Declares a Wardley map."),
  groups: [
    {
      title: "Map",
      hints: [
        hint("title Tea shop", "Sets the title of the map."),
        hint("size [1100, 800]", "Sets the width and the height."),
        hint("evolution Genesis -> Custom -> Product -> Commodity", "Sets the names of the evolution stages."),
      ],
    },
    {
      title: "Components",
      hints: [
        hint("anchor Customer [0.95, 0.63]", "An anchor: the visibility and the evolution, from 0 to 1."),
        hint("component Cup of tea [0.79, 0.61]", "A component at a visibility and an evolution."),
        hint("component Kettle [0.43, 0.35] label [-20, 10]", "The label form moves the label."),
        hint("component Power [0.1, 0.7] (buy)", "The strategy is build, buy, outsource, or market."),
        hint("component Water [0.38, 0.82] inertia", "Marks the inertia of a component."),
      ],
    },
    {
      title: "Links and changes",
      hints: [
        hint("Customer -> Cup of tea", "A link from one component to another."),
        hint("Cup of tea -> Kettle; needs hot water", "A link with a label."),
        hint("evolve Kettle 0.62", "An arrow to the future evolution of a component."),
        hint('note "Main need" [0.9, 0.2]', "A note at a position."),
      ],
    },
  ],
  example: [
    "wardley-beta",
    "  title Tea shop",
    "  anchor Customer [0.95, 0.63]",
    "  component Cup of tea [0.79, 0.61]",
    "  component Kettle [0.43, 0.35]",
    "  Customer -> Cup of tea",
    "  Cup of tea -> Kettle",
    "  evolve Kettle 0.62",
  ].join("\n"),
};

const CYNEFIN: HintSet = {
  key: "cynefin",
  name: "Cynefin framework",
  mermaidIds: ["cynefin"],
  detect: /^cynefin-beta(?:[\s:]|$)/,
  declaration: hint("cynefin-beta", "Declares a Cynefin framework diagram."),
  groups: [
    {
      title: "Domains",
      hints: [
        hint('complex\n  "Probe, sense, respond"', "A domain with its items in quotes."),
        hint("clear", "The domains: clear, complicated, complex, chaotic, and confusion."),
        hint("title Team work", "Sets the title of the diagram."),
      ],
    },
    {
      title: "Transitions",
      hints: [
        hint("chaotic --> complex", "A transition from one domain to another."),
        hint('complex --> complicated : "Learn"', "A transition with a label."),
      ],
    },
  ],
  example: [
    "cynefin-beta",
    "  complex",
    '    "New product"',
    "  complicated",
    '    "Database tuning"',
    "  clear",
    '    "Password reset"',
    '  complex --> complicated : "Learn"',
  ].join("\n"),
};

const RAILROAD: HintSet = {
  key: "railroad",
  name: "Railroad diagram",
  mermaidIds: ["railroad"],
  detect: /^railroad-beta\b/i,
  declaration: hint("railroad-beta", "Declares a railroad diagram with function forms."),
  groups: [
    {
      title: "Rules",
      hints: [
        hint('digit = terminal("0");', "A rule: a name, =, an expression, and a semicolon."),
        hint('item = nonterminal("digit");', "A reference to another rule."),
        hint('list = sequence(terminal("["), nonterminal("item"), terminal("]"));', "Items one after the other."),
        hint('sign = choice(terminal("+"), terminal("-"));', "One of the items."),
      ],
    },
    {
      title: "Repetition",
      hints: [
        hint('sign = optional(terminal("-"));', "An item that can be missing."),
        hint('digits = oneOrMore(nonterminal("digit"));', "An item one or more times."),
        hint('spaces = zeroOrMore(terminal(" "));', "An item zero or more times."),
        hint('any = special("any character");', "A special item with a free text."),
      ],
    },
  ],
  example: [
    "railroad-beta",
    '  number = sequence(optional(terminal("-")), oneOrMore(nonterminal("digit")));',
    '  digit = choice(terminal("0"), terminal("1"), terminal("2"));',
  ].join("\n"),
};

const RAILROAD_EBNF: HintSet = {
  key: "railroadEbnf",
  name: "Railroad diagram (EBNF)",
  mermaidIds: ["railroadEbnf"],
  detect: /^railroad-ebnf-beta\b/i,
  declaration: hint("railroad-ebnf-beta", "Declares a railroad diagram with EBNF rules."),
  groups: [
    {
      title: "Rules",
      hints: [
        hint('digit = "0" | "1" | "2" ;', "A rule. The | separates the choices."),
        hint("number ::= digit , digit ;", "The ::= form also works. The comma is optional."),
        hint('sign = [ "-" ] ;', "Square brackets make an optional item."),
        hint("digits = { digit } ;", "Braces repeat an item zero or more times."),
        hint('group = ( "a" | "b" ) ;', "Parentheses group items."),
      ],
    },
    {
      title: "Postfix forms",
      hints: [
        hint("list = item+ ;", "The + repeats an item one or more times. The * and ? also work."),
        hint('char = letter - "x" ;', "The - removes an item from a choice."),
        hint("any = ? any character ? ;", "A special sequence between question marks."),
      ],
    },
  ],
  example: [
    "railroad-ebnf-beta",
    '  number = [ "-" ] , digit , { digit } ;',
    '  digit = "0" | "1" | "2" ;',
  ].join("\n"),
};

const RAILROAD_ABNF: HintSet = {
  key: "railroadAbnf",
  name: "Railroad diagram (ABNF)",
  mermaidIds: ["railroadAbnf"],
  detect: /^railroad-abnf-beta\b/i,
  declaration: hint("railroad-abnf-beta", "Declares a railroad diagram with ABNF rules."),
  groups: [
    {
      title: "Rules",
      hints: [
        hint('bit = "0" / "1";', "A rule. The / separates the choices. A semicolon ends the rule."),
        hint("byte = 8bit;", "A number before an item repeats it that number of times."),
        hint("word = 1*ALPHA;", "The n*m form repeats an item from n to m times."),
        hint('sign = ["-"];', "Square brackets make an optional item."),
        hint('pair = (bit bit) / "x";', "Parentheses group items."),
        hint("digit = %x30-39;", "A range of character codes."),
      ],
    },
  ],
  example: [
    "railroad-abnf-beta",
    '  number = ["-"] 1*digit;',
    "  digit = %x30-39;",
  ].join("\n"),
};

const RAILROAD_PEG: HintSet = {
  key: "railroadPeg",
  name: "Railroad diagram (PEG)",
  mermaidIds: ["railroadPeg"],
  detect: /^railroad-peg-beta\b/i,
  declaration: hint("railroad-peg-beta", "Declares a railroad diagram with PEG rules."),
  groups: [
    {
      title: "Rules",
      hints: [
        hint('Sign <- "+" / "-" ;', "A rule. The / separates the ordered choices."),
        hint("Number <- Sign? Digit+ ;", "The ? makes an item optional. The + and * repeat it."),
        hint('Group <- ("a" / "b") "c" ;', "Parentheses group items."),
        hint('Word <- !"end" . ;', "The ! and & check the next text. The dot is any character."),
      ],
    },
  ],
  example: [
    "railroad-peg-beta",
    "  Number <- Sign? Digit+ ;",
    '  Sign <- "+" / "-" ;',
    '  Digit <- "0" / "1" / "2" ;',
  ].join("\n"),
};

export const OTHER_SETS_2: readonly HintSet[] = [
  SWIMLANE,
  INFO,
  RADAR,
  TREE_VIEW,
  EVENT_MODELING,
  ISHIKAWA,
  VENN,
  TREEMAP,
  WARDLEY,
  CYNEFIN,
  RAILROAD,
  RAILROAD_EBNF,
  RAILROAD_ABNF,
  RAILROAD_PEG,
];
