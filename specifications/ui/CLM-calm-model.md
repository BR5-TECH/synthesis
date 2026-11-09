# CALM model

**Spec code:** `CLM`

## Intent
The CALM model is how the application reads a FINOS CALM (Common Architecture Language Model) architecture document. A **read** takes the JSON source and makes one of two results: a laid-out diagram graph, or an error that names the step that failed. Every CALM surface of `CAD-calm-architecture-diagrams.md` shows the result of a read, so a document reads the same way in a fence, in an overlay, and in an Architecture tab. The read runs in the frontend over text that a surface already holds, and it needs no backend operation. Out of scope: editing an architecture through its diagram, CALM patterns and standards, and validation against a CALM schema version other than 1.3.

## Functional requirements
1. **CLM-FR-KDTA** The application bundles the CALM 1.3 meta schema files of `@finos/calm-schema` version 1.3.0, and a read validates against them. A read gets nothing from the network.
2. **CLM-FR-ODQZ** A read runs its steps in this order: parse, schema, references, graph, layout. The first step that fails ends the read with an error that names that step. A read whose steps all pass gives a diagram graph.
3. **CLM-FR-ATLN** The parse step fails when the source is not JSON, or when its top-level value is not an object. Its error statement is "The CALM source is not valid JSON." The reason gives the parser message, with the line and the column of the error when the parser supplies them.
4. **CLM-FR-BWGJ** The schema step validates the document against the `calm.json` meta schema of CALM 1.3, whatever `$schema` value the document declares. Its error statement is "The CALM source does not match the CALM 1.3 schema."
5. **CLM-FR-OIGM** The reason of a schema error lists at most five problems, one on each line. Each line gives the JSON pointer of the value and the message of the problem. When there are more than five problems, a last line states how many more there are.
6. **CLM-FR-DFFG** The references step fails when a relationship, a decision of an `options` relationship, or a flow transition names a node, a relationship, or an interface that the document does not hold. Its error statement is "The architecture refers to an item that does not exist." The reason names each missing `unique-id` and the JSON pointer that names it.
7. **CLM-FR-DHXO** The references step also fails when two nodes, two relationships, or two flows have the same `unique-id`. Its error statement is "The architecture uses one unique-id more than once." The reason names each repeated `unique-id`.
8. **CLM-FR-DLUV** The graph step makes one diagram node for each node of the document. A diagram node shows the `name` of its node and its `node-type`, and it holds the `unique-id`, the description, the interfaces, the controls, and the metadata of its node for the details of `CAD-calm-architecture-diagrams.md` CAD-FR-KUBU.
9. **CLM-FR-DTYX** A diagram node has one of these shapes, from its `node-type`: actor, system (for `system` and `ecosystem`), service, database, web client, network (for `network` and `ldap`), and data asset. Any other `node-type` value gives the generic shape, which shows the value as text.
10. **CLM-FR-DWLY** Each `deployed-in` and each `composed-of` relationship makes its `container` node a **group** that holds the nodes it lists. A group can hold a group, so containment nests as deep as the document nests it. A `composed-of` group has a solid border, and a `deployed-in` group has a dashed border.
11. **CLM-FR-DYAW** A node that two or more groups list shows inside the first of them in document order. It shows a badge that names each other group that lists it.
12. **CLM-FR-EKSX** The graph step fails when containment makes a loop, so that a group holds itself directly or through other groups. Its error statement is "The architecture holds a containment loop." The reason names the nodes of the loop.
13. **CLM-FR-ERXV** A `connects` relationship makes one edge from the node of its `source` to the node of its `destination`. The edge label is the description of the relationship, when it has one. Its `protocol`, when present, shows as a chip on the edge. The interfaces that each end names show as text at that end.
14. **CLM-FR-ESCS** An `interacts` relationship makes one edge from its `actor` node to each node that it lists. Each of these edges has a dashed line, and its label is the description of the relationship, when it has one.
15. **CLM-FR-ESMQ** An `options` relationship makes no edge. Each node and each relationship that one of its decisions names shows a decision marker, and the details of that item list the description of each such decision.
16. **CLM-FR-ETEA** A diagram node with interfaces shows the `unique-id` of its first three interfaces under its name, and then "+N more" when it has more than three. A node or an edge that has controls shows a control marker with the number of its controls.
17. **CLM-FR-FDGM** The graph step makes one **flow** for each flow of the document. A flow holds the steps of its transitions, ordered by `sequence-number` from the lowest. Steps with the same `sequence-number` keep their document order.
18. **CLM-FR-FKHM** A flow step goes from the source to the destination of its relationship when its `direction` is `source-to-destination` or is absent, and from the destination to the source when its `direction` is `destination-to-source`. The source of an `interacts` relationship is its actor.
19. **CLM-FR-FSCM** A step whose relationship makes no edge (`deployed-in`, `composed-of`, or `options`) highlights the nodes that its relationship names, in place of an edge.
20. **CLM-FR-GDML** The layout step places the graph with a layered layout, from left to right. Each group is large enough to hold its members, and no member extends past the border of its group. The same graph always gets the same positions.
21. **CLM-FR-GFZK** The layout step fails when the layout engine reports an error. Its error statement is "The application cannot compute the layout." The reason gives the message of the engine.
22. **CLM-FR-HPJT** When the libraries of a read cannot be loaded, the read ends with the error statement "The diagram library cannot be loaded." A later read of any source tries the load again.
23. **CLM-FR-HSUH** A read of a document with no nodes gives a diagram graph with no nodes. A surface shows it as the statement "The architecture has no nodes." in place of a diagram.
24. **CLM-FR-IAHL** The results of reads are kept by source text, at most 64 of them. A read of a source whose result is kept returns that result with no new read. A result that a surface shows now is not removed from what is kept.
25. **CLM-FR-IRGG** CALM text is untrusted. A read does not run, load, or follow anything that a document holds. Every string of a document reaches a surface as plain text, and no surface renders it as HTML or as a link.
26. **CLM-FR-JIWR** Each read that is not a kept result logs one `DEBUG` record under the `frontend` domain through `src/logging.ts`. The record names the step that ended the read, the number of nodes, and the duration. A failed library load logs one `WARN` record. No record holds text from the document.

## Wireframes
A diagram graph with one group, one actor, and one flow step chosen:
```
  ┌───────┐  uses          ┌─ deployed-in: k8s-cluster ─ ─ ─ ─ ─ ─ ─ ┐
  │ ☺     │ ─ ─ ─ ─ ─ ─ ─▶ │  ┌──────────────┐  ①  HTTPS  ┌────────┐  │
  │ Buyer │                │  │ Shop API     │ ─────────▶ │ ⛁ Shop │
  └───────┘                │  │ service      │            │ db     │  │
                           │  │ api-port     │            └────────┘
                           │  └──────────────┘ 🛡 2                   │
                           └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘
```
- Layout notes: the layout flows from left to right (CLM-FR-GDML). A `deployed-in` group has a dashed border, and a `composed-of` group a solid one (CLM-FR-DWLY). An `interacts` edge is dashed (CLM-FR-ESCS). The chip on a `connects` edge is its protocol (CLM-FR-ERXV). The circled number is the step of a chosen flow (`CAD-calm-architecture-diagrams.md` CAD-FR-KSEJ). The shield shows the number of controls (CLM-FR-ETEA).

## UI contract boundary
- **Owned by the UI**: the whole read. That is the bundled CALM 1.3 schema, the parse, the schema validation, the reference checks, the mapping to a diagram graph and its flows, the layout, the error statements and reasons, and the store of kept results. A read works on text that a surface already holds.
- **Delegated to backend (abstract)**: none. A read invokes no backend operation and reads nothing from disk.

## Non-functional requirements
- The schema validator, the layout engine, and the canvas library load only when the first CALM read starts. A file with no CALM costs nothing.
- The layout runs off the main thread, so a large document does not stop typing in the editor.
- A document of 200 nodes and 300 relationships reads and lays out in less than one second on a typical development machine.
- The bundled schema files are the exact files of `@finos/calm-schema` 1.3.0, and the application does not change them.
