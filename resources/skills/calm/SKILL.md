---
name: calm
description: Write, fix, or explain a FINOS CALM 1.3 (Common Architecture Language Model) architecture as code in JSON. Use it for a software or system architecture model with nodes, relationships, interfaces, controls, business processes, metadata, and ADR links, in a calm fenced block of a Markdown document or in a whole .calm.json Architecture file.
---

# FINOS CALM architectures

This skill tells you how to write a FINOS CALM architecture that the application renders. CALM (Common Architecture Language Model) keeps an architecture as code: one JSON document holds the nodes of the architecture, the relationships between them, and optional interfaces, business flows, controls, and metadata.

The application validates each document against the CALM 1.3 schema. It shows no diagram when the document has a schema error, a reference to a node, relationship, or interface that does not exist, a `unique-id` that two items use, or a containment loop. Do not write CALM from memory: read the reference file of each concept that you use.

## Where CALM goes

- **In a Markdown document**: put one CALM document in a fenced code block with the info string `calm`. The block holds the JSON and nothing else. Put one document in one block.
- **As a file**: write the whole document to a file whose name ends with `.calm.json`, for example `architecture/payments.calm.json`. The application opens this file as an Architecture artifact. The file holds the JSON and nothing else.

## Procedure

1. Find the concepts that the request needs in the table below. Each architecture needs the document frame and nodes. Most architectures also need relationships.
2. Read the reference file of each concept. It is a file of this skill: load it with the path in the table, for example `references/nodes.md`. Read only the files you need. For a full model, read `references/complete-example.md` too.
3. Write the document with the shapes of the reference files. Use only the keys and values that they show.
4. Set `"$schema": "https://calm.finos.org/release/1.3/meta/calm.json"` as the first key of the document.
5. Give each node, relationship, flow, and interface a `unique-id` that no other item of the document uses. Use lowercase letters, digits, and hyphens, for example `payment-api`.
6. Make sure that each id you refer to exists: each node of a relationship, each interface of a `connects` end, each relationship of a flow transition, and each node and relationship of a decision.
7. Check the document against the "Common mistakes" section of each reference file that you read before you give it to the author.

## Concepts

| Concept | Use it when | Reference file |
| --- | --- | --- |
| Document | Always. The top-level object, `$schema`, top-level `metadata`, and `adrs` links | `references/document.md` |
| Nodes | Always. The people, systems, services, databases, networks, clients, and data of the architecture | `references/nodes.md` |
| Relationships | Two or more nodes interact, connect, or contain each other, or the architecture has alternatives | `references/relationships.md` |
| Interfaces | A node exposes a port, an endpoint, a host, or another point of connection | `references/interfaces.md` |
| Flows | A business process goes through the architecture in a sequence of steps | `references/flows.md` |
| Controls | The architecture must meet security, compliance, or operational requirements | `references/controls.md` |
| Complete example | You write a full architecture with most of the concepts together | `references/complete-example.md` |

## Rules for every document

- Write valid JSON: double quotes around keys and strings, no comments, and no comma after the last item of an object or an array.
- Write each key exactly as the reference files show it. CALM keys use hyphens, for example `unique-id`, `node-type`, and `relationship-type`.
- Each string shows as plain text. Do not put HTML or Markdown in a name or a description.
- The application gets nothing from the network. A URL in a document is text that the author can read.
- When you change an existing document, keep its ids, its order, and its style unless the author asks for a change. A change of a `unique-id` must also change each reference to it.

## When the diagram does not show

The application shows an error that names the step that failed. Read the reference file again and compare the document with it.

- "The CALM source is not valid JSON.": find the JSON syntax error at the line and column that the error gives.
- "The CALM source does not match the CALM 1.3 schema.": each line of the reason gives a JSON pointer and a problem. Compare that value with the shape in the reference file.
- "The architecture refers to an item that does not exist.": add the missing item, or correct the id that refers to it.
- "The architecture uses one unique-id more than once.": give each item its own id, and correct the references.
- "The architecture holds a containment loop.": a `deployed-in` or `composed-of` group holds itself through other groups. Remove one of the relationships of the loop.
