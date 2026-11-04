---
name: mermaid
description: Write, fix, or explain a Mermaid diagram (Mermaid 12) in a Markdown document. Use it for any diagram or chart in text form - flowchart, sequence, class, state, entity relationship (ER), Gantt, timeline, mindmap, pie, XY chart, sankey, quadrant, radar, treemap, packet, Venn, Git graph, kanban, user journey, requirement, C4 architecture, block, architecture, agent flow (agentflow), swimlane, event modeling, use case, tree view, Ishikawa (fishbone), Wardley map, Cynefin, and railroad (grammar) diagrams.
---

# Mermaid diagrams

This skill tells you how to write a Mermaid diagram that the application renders. The application uses Mermaid 12.2.0. Syntax from older Mermaid versions can fail, and several diagram types exist only in Mermaid 12. Do not write a diagram from memory: read the reference file of its type first.

## Procedure

1. Find the diagram type that fits the request in the table below. When the author names a type, use that type. When the author names no type, select the type that shows the information most clearly.
2. Read the reference file of that type. It is a file of this skill: load it with the path in the table, for example `references/agentflow-beta.md`. Read only the files you need.
3. Write the diagram with the syntax of the reference file. Use only the statements, shapes, and options that it shows.
4. Put the diagram in a fenced code block with the info string `mermaid`. The first line in the block is the declaration keyword of the type, or a frontmatter block (`---` ... `---`) and then the keyword.
5. Check the diagram against the "Common mistakes" section of the reference file before you give it to the author.

## Diagram types

| Type | Declaration | Use it for | Reference file |
| --- | --- | --- | --- |
| Flowchart | `flowchart` / `graph` | A process, a decision path, or a pipeline as boxes and arrows | `references/flowchart.md` |
| Sequence | `sequenceDiagram` | Messages between participants in time order | `references/sequenceDiagram.md` |
| Class | `classDiagram` | Types with fields and methods, and their relations | `references/classDiagram.md` |
| State | `stateDiagram-v2` | The states of one thing and the events that move it | `references/stateDiagram.md` |
| Entity relationship | `erDiagram` | A data model with entities, keys, and cardinality | `references/erDiagram.md` |
| Block | `block` | Boxes that you place in a grid yourself | `references/block.md` |
| Architecture | `architecture-beta` | Services, the groups that hold them, and their connections | `references/architecture-beta.md` |
| C4 | `C4Context`, `C4Container`, `C4Component`, `C4Dynamic`, `C4Deployment` | Software architecture at one C4 level | `references/c4.md` |
| Agent flow | `agentflow-beta` | How an AI agent works: tasks, tools, decisions, inputs, and documents | `references/agentflow-beta.md` |
| Swimlane | `swimlane-beta` | A process where each step belongs to one role, team, or system | `references/swimlane-beta.md` |
| Event modeling | `eventmodeling` | Screens, commands, events, and read models on a timeline | `references/eventmodeling.md` |
| Use case | `usecase-beta` | Actors, use cases, and system boundaries | `references/usecase-beta.md` |
| Requirement | `requirementDiagram` | SysML requirements and the elements that satisfy or verify them | `references/requirementDiagram.md` |
| Gantt | `gantt` | A project plan of tasks on a time axis | `references/gantt.md` |
| Timeline | `timeline` | Events in time order, grouped by period | `references/timeline.md` |
| User journey | `journey` | The steps a user takes toward a goal, with satisfaction scores | `references/journey.md` |
| Git graph | `gitGraph` | Commits, branches, and merges | `references/gitGraph.md` |
| Kanban | `kanban` | Work items in status columns | `references/kanban.md` |
| Mindmap | `mindmap` | Ideas as a tree around one central topic | `references/mindmap.md` |
| Tree view | `treeView-beta` | A hierarchy as an indented list, such as a file tree | `references/treeView-beta.md` |
| Ishikawa | `ishikawa` | The causes of one effect (fishbone) | `references/ishikawa.md` |
| Wardley map | `wardley-beta` | A value chain by visibility and evolution | `references/wardley-beta.md` |
| Cynefin | `cynefin-beta` | Situations sorted into the five Cynefin domains | `references/cynefin-beta.md` |
| Pie | `pie` | How a whole divides into a few parts | `references/pie.md` |
| XY chart | `xychart` | Bar and line series over categories or a numeric range | `references/xychart.md` |
| Quadrant chart | `quadrantChart` | Items on two axes in four groups | `references/quadrantChart.md` |
| Sankey | `sankey` | Quantities that flow from sources to targets | `references/sankey.md` |
| Radar | `radar-beta` | Several items compared over the same measures | `references/radar-beta.md` |
| Treemap | `treemap` | A hierarchy of rectangles sized by value | `references/treemap.md` |
| Packet | `packet` | The bit layout of a packet header or a register | `references/packet.md` |
| Venn | `venn-beta` | How sets overlap | `references/venn-beta.md` |
| Railroad | `railroad-beta`, `railroad-ebnf-beta`, `railroad-abnf-beta`, `railroad-peg-beta` | A grammar drawn as railroad tracks | `references/railroad-beta.md` |

## Rules for every diagram

- Write one diagram in one code block. Do not put two declarations in one block.
- Use the declaration keyword exactly as the table shows. Keywords are case-sensitive, except where a reference file says otherwise.
- Put a label that holds spaces or punctuation in double quotes, as the reference file shows. Do not use a raw double quote inside a label; use `#quot;`.
- Keep node ids short and plain: letters, digits, and underscores. Do not use a reserved word, such as `end`, as a node id.
- A line that starts with `%%` is a comment. Do not put `{}` in a comment.
- To set a title or a theme, use a frontmatter block at the start of the code block when the reference file says the type reads it:

```text
---
title: Order flow
config:
  theme: neutral
---
```

- Do not add `click` callbacks, scripts, or HTML that runs code. The application renders diagrams with strict security, and it ignores them.
- When you change an existing diagram, keep its type, its ids, and its style unless the author asks for a change.

## When the diagram does not render

Read the reference file again and compare each line with its syntax. The most frequent causes are: a keyword of an older Mermaid version, a missing quote around a label with special characters, a reserved word used as an id, and wrong indentation in the types that use it (mindmap, kanban, tree view, timeline).
