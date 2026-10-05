# Flow graph validation

**Spec code:** `FGV`

## Intent
The backend authority on what a Flow document is. This module owns the Flow document schema and the structural rules its graph obeys, and answers one question about a body of text: is this a Flow, and if not, exactly what is wrong with it. It exists because a Flow is a plain JSON file committed to the repository — an agent writes one, a merge resolves one, a person edits one by hand, a tool generates one — and the canvas that enforces these rules while an author draws is present for none of that. Checking a document on the way in and refusing it on the way out is what makes "a Flow on disk opens in the Flow tab" true rather than hoped for. Out of scope: what a Flow *means* — running it, evaluating it, or fusing it into a single prompt, none of which any module performs in v1; whether a node's artifact reference points at a file that still exists, since a reference to a deleted artifact is a Flow to be repaired in the editor and not a document to be refused (per `../ui/FLO-flow.md` FLO-FR-11); and anything about where elements sit on the canvas, geometry being the canvas's business and never this module's.

## Contract surface
- `"validate flow document"` → `validate_flow_document(body)` — takes the document's text and returns `{ valid, violations }`. It reads no file, opens no project, and mutates nothing, so it can be called on a body that has never been on disk and on a body about to replace one. Invoked by `../ui/FLO-flow.md` FLO-FR-46 when a Flow tab loads, and by `PST-project-storage.md::save_artifact_contents` before any `flow`-typed artifact is written (PST-FR-28).

  The report shape:

  ```json
  {
    "valid": false,
    "violations": [
      { "code": "cross_container_edge", "message": "…", "edge_id": "e4" },
      { "code": "dangling_parent", "message": "…", "element_id": "n7" }
    ]
  }
  ```

  `element_id` names the node or loop a violation sits on, `edge_id` names the edge, and a violation about the document as a whole carries neither. `code` is one of the fixed set FGV-FR-06 to FGV-FR-14 define; `message` states the violation in the terms an author reads it in.

- The Flow document schema this module validates against, version `1`:

  ```json
  {
    "version": 1,
    "name": "Onboarding review",
    "description": "Reviews a new joiner's first PR.",
    "loops": [
      {
        "id": "l1",
        "name": "Review cycle",
        "artifactIds": ["prompts/stop-when-clean.md"],
        "maxPasses": 5,
        "parentId": "l0",
        "position": { "x": 320, "y": 60 },
        "size": { "width": 480, "height": 260 }
      }
    ],
    "nodes": [
      {
        "id": "n1",
        "name": "Collect context",
        "artifactIds": ["prompts/gather-repo.md", ".claude/skills/code-review/SKILL.md"],
        "prompt": "Read the repo and list every changed file.",
        "parentId": "l1",
        "position": { "x": 120, "y": 80 }
      }
    ],
    "edges": [
      { "id": "e1", "from": "n1", "to": "l1", "label": "ok" }
    ]
  }
  ```

  `description`, `artifactIds`, `prompt`, `label`, `maxPasses`, and `parentId` are optional; every other field is required, and `loops` may be absent on a document holding none. An absent `parentId` means the element sits at the top level of the graph rather than inside a loop. `artifactIds` is carried by loops and nodes alike — it holds the stable, path-derived node keys of `ASC-artifact-scanning.md` ASC-FR-13 in the order the author added them, and is absent rather than empty on an element referencing nothing. `prompt` is a node's alone: a loop says what it is run under through the artifacts it references (`../ui/FLO-flow.md` FLO-FR-37) and carries no inline body. A `position` is measured from the origin of whatever holds the element — the canvas for a top-level element, the container for a child — so moving a loop moves everything inside it without rewriting a single child's position.

- This module invokes nothing. It performs no filesystem access, so it uses no primitive of `FSA-filesystem-access.md`, and it contacts no integration.

## Functional requirements
1. **FGV-FR-01** This module is the single definition of the Flow document schema. The shape in the contract surface above is that definition, and every other module that reads or writes a Flow document — the Flow tab that serializes one (`../ui/FLO-flow.md` FLO-FR-03), the Diff tab that compares two (`../ui/DFV-diff-viewer.md` DFV-FR-33) — conforms to it rather than restating it.
2. **FGV-FR-02** `validate_flow_document(body)` is pure: it reads no file, requires no open project, emits no event, and changes nothing anywhere. The same body always yields the same report, so a caller may validate a document that exists only in memory.
3. **FGV-FR-03** A validation reports **every** violation the document carries, not the first one found. An author handed one error at a time fixes a document one relaunch at a time, and a generating agent handed one error at a time cannot correct its output in a single pass.
4. **FGV-FR-04** The violations are ordered so that the ones about the document as a whole come first, then those about elements, then those about edges; within each group they follow the order the offending items appear in the document. The order is stable for a given body, so two validations of the same document produce the same report.
5. **FGV-FR-05** A body that is empty or contains only whitespace is **valid** and describes the empty graph, so a Flow created by the New Artifact window (per `../ui/NTA-new-typed-artifact.md` NTA-FR-11), by New File, or by an external `touch` validates rather than failing (per `../ui/FLO-flow.md` FLO-FR-04).
6. **FGV-FR-06** A body that is not parseable JSON yields exactly one violation, code `not_json`, naming the parse failure and where in the text it occurred. No structural rule is evaluated against a document that could not be read.
7. **FGV-FR-07** A document whose `version` is absent or is a value this module does not know yields one violation, code `unknown_version`. Like `not_json`, it is reported alone: rules written for version 1 say nothing useful about a document that does not claim to be one.
8. **FGV-FR-08** A required field that is absent, or any field whose value is of the wrong type, yields a violation of code `shape` naming the field's location in the document. A field the schema does not define yields a violation of code `unknown_field`. An unrecognised field is refused rather than ignored, because the only thing that writes these documents is a tool that knows the schema, and a field nothing reads is more often a misspelling of one that matters than an extension of it.
9. **FGV-FR-09** Nodes and loops share one identifier space. Two elements carrying the same `id`, whatever their kinds and whatever their containers, yield a violation of code `duplicate_id`. Edge ids share that requirement among themselves.
10. **FGV-FR-10** An edge whose `from` or `to` names no element in the document yields a violation of code `dangling_edge_endpoint`. An endpoint may name a node or a loop; both are elements the graph connects.
11. **FGV-FR-11** An edge whose `from` and `to` are the same element yields a violation of code `self_edge`, and two edges sharing the same ordered `(from, to)` pair yield a violation of code `duplicate_edge` on the second and each subsequent one. `B→A` is a different ordered pair from `A→B` and is not a duplicate of it.
12. **FGV-FR-12** An edge whose two endpoints do not share a container — one at the top level and one inside a loop, or two inside different loops — yields a violation of code `cross_container_edge`. Two elements share a container when their `parentId` values are equal, an absent `parentId` on both counting as the top level. This is the whole of the containment rule on edges: a loop's members connect among themselves, and a loop reaches the rest of the graph as an element of whatever holds it.
13. **FGV-FR-13** A `parentId` naming no loop in the document, or naming an element that is a node rather than a loop, yields a violation of code `dangling_parent`. Containment must also be acyclic: a loop that contains itself through any chain of `parentId` values yields a violation of code `containment_cycle` on each loop in the chain.
14. **FGV-FR-14** A `maxPasses` that is present and is not an integer greater than zero yields a violation of code `invalid_max_passes`. Its absence is not a violation — a loop bounded by nothing but the prompt it references is an ordinary loop (per `../ui/FLO-flow.md` FLO-FR-37).
15. **FGV-FR-15** These are not violations, and a document carrying any of them validates: an `artifactIds` entry that resolves to no file in the project, which is a reference the editor renders as unresolved rather than a document defect (per `../ui/FLO-flow.md` FLO-FR-11); an empty or absent Flow `name`, or an empty element `name`; a node carrying neither an artifact reference nor a prompt; a loop referencing no artifact; a loop holding no element; a cycle among edges, which is the whole point of the graph being what it is (per `../ui/FLO-flow.md` FLO-FR-18); and every fact about where anything sits — a child positioned outside its container's bounds, two elements overlapping, a container smaller than what it holds. Geometry is settled by the canvas as the author edits and is carried here only so a Flow reopens where it was left.

## Non-functional requirements
- A validation of a document holding several thousand elements completes fast enough to sit inside a save without the author noticing it, so no caller has reason to skip it, to sample it, or to run it in the background.
- The module holds no state between calls. There is no cache of past reports, no notion of "the current Flow", and no ordering requirement among calls, so concurrent validations of unrelated documents never interact.
- Violation codes are a stable, closed set. A caller may branch on a code and render its own wording; the `message` is for the caller that does not.
- Validation contacts no model and reaches no network. A document is judged by its own structure alone.
- The rule set here and the rules the canvas enforces while an author draws (`../ui/FLO-flow.md` FLO-FR-16, FLO-FR-17, FLO-FR-40, FLO-FR-41) describe the same graph. The canvas answers instantly and locally so a refused connection costs no round trip; this module is what decides whether a document is one, and neither defers to the other.
