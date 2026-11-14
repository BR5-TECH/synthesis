# Flow graph validation

**Spec code:** `FGV`

## Intent
The backend authority on what a Flow document is. A Flow is a **delivery plan**: a hierarchy of milestones, epics, features, tasks, and defects, the `blocks` and `relates` edges between them, and the drafts its tasks are worked in. This module owns the Flow document schema and the structural rules the plan obeys, and answers one question about a body of text: is this a Flow, and if not, exactly what is wrong with it. A Flow is a plain JSON file committed to the repository: an agent writes one, a merge resolves one, and a person edits one by hand. The same schema also carries a plan that is held on GitHub, which `GDP-github-delivery-plan.md` builds in memory. The canvas that enforces these rules while an author draws is present for none of that, so a document is checked on the way in and refused on the way out. The document holds the plan alone: where its items sit on a canvas is a per-machine preference (`DMS-delivery-machine-state.md`), so a Flow's Git history shows only changes to the plan. Out of scope: what a plan's state is, which `FDL-flow-delivery.md` derives; whether a reference, a draft link, or a footprint path points at something that still exists, which the editor shows rather than refuses (per `../ui/FLO-flow.md` FLO-FR-11); and which plan is the project's delivery, which `DSR-delivery-source.md` decides.

## Functional requirements
1. **FGV-FR-01** This module is the single definition of the Flow document schema. The shape in the contract surface is that definition. Every module that reads or writes a Flow document conforms to it: `../ui/FLO-flow.md` (FLO-FR-03), `../ui/DFV-diff-viewer.md` (DFV-FR-33), `FDL-flow-delivery.md`, `FOA-flow-overlap-analysis.md`, `DSR-delivery-source.md`, and `GDP-github-delivery-plan.md`.
2. **FGV-FR-02** `validate_flow_document(body)` is pure: it reads no file, requires no open project, emits no event, and changes nothing anywhere. The same body always yields the same report, so a caller may validate a document that exists only in memory.
3. **FGV-FR-03** A validation reports **every** violation the document carries, not the first one found. An author handed one error at a time fixes a document one relaunch at a time, and a generating agent handed one error at a time cannot correct its output in a single pass.
4. **FGV-FR-04** The violations are ordered so that the ones about the document as a whole come first, then those about items, then those about edges; within each group they follow the order the offending entries appear in the document. The order is stable for a given body.
5. **FGV-FR-05** A body that is empty or contains only whitespace is **valid** and describes the empty plan, so a Flow created by the New Artifact window (per `../ui/NTA-new-typed-artifact.md` NTA-FR-11), by New File, or by an external `touch` validates rather than failing (per `../ui/FLO-flow.md` FLO-FR-04).
6. **FGV-FR-06** A body that is not parseable JSON yields exactly one violation, code `not_json`, naming the parse failure and where in the text it occurred. No structural rule is evaluated against a document that could not be read.
7. **FGV-FR-07** A document whose `version` is absent or is any value other than `3` yields one violation, code `unknown_version`, reported alone. A version `1` or a version `2` document is an unknown version.
8. **FGV-FR-08** A required field that is absent, or any field whose value is of the wrong type or outside its enumeration, yields a violation of code `shape` naming the field's location. A field the schema does not define yields a violation of code `unknown_field`.
   - *Why:* Only tools that know the schema write these documents, so an unknown field is more often a misspelling of one that matters than an extension.
9. **FGV-FR-09** Two items carrying the same `id` yield a violation of code `duplicate_id`, whatever their kinds and containers. Two edges carrying the same `id` yield the same code.
10. **FGV-FR-10** An edge whose `from` or `to` names no item in the document yields a violation of code `dangling_edge_endpoint`.
11. **FGV-FR-11** An edge whose `from` and `to` are the same item yields a violation of code `self_edge`. Two items joined by more than one edge, whatever the edges' directions and types, yield a violation of code `duplicate_edge` on the second and each later edge.
12. **FGV-FR-13** A `parentId` naming no item in the document, or naming a `task` or a `defect`, yields a violation of code `dangling_parent`. An item that contains itself through any chain of `parentId` values yields a violation of code `containment_cycle` on each item in the chain.
13. **FGV-FR-YHFX** An item whose container is not one the hierarchy permits for its kind yields a violation of code `invalid_parent_kind`. The kinds rank `milestone`, `epic`, `feature`, and then `task` and `defect` together. A `milestone` sits at the top level. Every other kind sits at the top level or in a container of a higher rank.
14. **FGV-FR-KBLT** An edge between an item and one of its ancestors or descendants yields a violation of code `edge_within_lineage`.
15. **FGV-FR-PMTY** `blocks` edges that make a leaf wait, directly or through other leaves, for itself yield a violation of code `dependency_cycle` on each of those edges. A leaf waits for every leaf at or under an edge's `from` when the edge's `to` is that leaf or one of its ancestors. A container that holds no leaf counts as a leaf.
    - *Why:* A container is done only when its leaves are done, and an empty one never is (`FDL-flow-delivery.md` FDL-FR-JYKC), so such a cycle is a deadlock.
16. **FGV-FR-MZIA** A field present on an item whose kind does not carry it yields a violation of code `field_not_allowed_for_kind`: `draftId` or `footprint` on a container kind, and `due` on any kind but `milestone`.
17. **FGV-FR-CTFX** Two items whose `draftId` values are equal yield a violation of code `duplicate_draft_link` on the second and each later item.
18. **FGV-FR-RQEM** A `footprint` entry that is empty, absolute, contains a `..` segment, or repeats an earlier entry of the same item yields a violation of code `invalid_footprint_path`.
19. **FGV-FR-RRND** A `due` that is not a calendar date in the form `YYYY-MM-DD` yields a violation of code `invalid_due_date`.
20. **FGV-FR-JDGO** An `artifactIds` list or the document's `references` list that holds the same id twice yields a violation of code `duplicate_reference`.
22. **FGV-FR-15** These are not violations: a reference that resolves to no file; a `draftId` that names no draft; a `footprint` path that names nothing on disk; an empty Flow or item name; an empty container; and `relates` edges that form a cycle.

## Contract surface
### Tauri commands
- `"validate flow document"` → `validate_flow_document(body)` — takes the document's text and returns `{ valid, violations }`. It reads no file, opens no project, and mutates nothing, so it can be called on a body that has never been on disk and on a body about to replace one. Invoked by `../ui/FLO-flow.md` FLO-FR-46 when a Flow tab loads, by `PST-project-storage.md::save_artifact_contents` before any `flow`-typed artifact is written (PST-FR-28), and in-process by `FDL-flow-delivery.md`, `FOA-flow-overlap-analysis.md`, `DSR-delivery-source.md`, and `GDP-github-delivery-plan.md` before they read or write a document.

  The report shape:

  ```json
  {
    "valid": false,
    "violations": [
      { "code": "dependency_cycle", "message": "…", "edge_id": "e4" },
      { "code": "invalid_parent_kind", "message": "…", "element_id": "t7" }
    ]
  }
  ```

  `element_id` names the item a violation sits on, `edge_id` names the edge, and a violation about the document as a whole carries none of them. `code` is one of the fixed set that FGV-FR-06 to FGV-FR-JDGO define; `message` states the violation in the terms an author reads it in.

### The Flow document schema
- The Flow document schema, version `3`:

  ```json
  {
    "version": 3,
    "name": "Payments v2",
    "description": "Card payments for the EU launch.",
    "references": ["docs/payments-hld.md"],
    "items": [
      {
        "id": "m1",
        "kind": "milestone",
        "name": "Beta",
        "description": "Accept **Visa** and **Mastercard**.",
        "artifactIds": ["specifications/core/PAY-payments.md"],
        "due": "2027-03-31"
      },
      {
        "id": "f1",
        "kind": "feature",
        "name": "Card capture",
        "parentId": "m1"
      },
      {
        "id": "t1",
        "kind": "task",
        "name": "Tokenize card numbers",
        "parentId": "f1",
        "draftId": "dr_7f3a",
        "footprint": ["src-tauri/src/payments/", "specifications/core/PAY-payments.md"]
      },
      {
        "id": "t2",
        "kind": "task",
        "name": "Card form",
        "parentId": "f1"
      },
      {
        "id": "x1",
        "kind": "defect",
        "name": "Decline message is wrong"
      }
    ],
    "edges": [
      { "id": "e1", "from": "t1", "to": "t2", "type": "blocks" },
      { "id": "e2", "from": "t2", "to": "x1", "type": "relates" }
    ]
  }
  ```

  Every item carries `id`, `kind`, and `name`. `kind` is one of `milestone`, `epic`, `feature`, `task`, and `defect`; the first three are **container kinds** and `task` and `defect` are **leaf kinds**. `description`, `artifactIds`, `parentId`, `draftId`, `footprint`, and `due` are optional, and each is absent rather than empty when unset. An absent `parentId` means the item sits at the top level. `artifactIds` and `references` hold the stable, path-derived node keys of `ASC-artifact-scanning.md` ASC-FR-13 in the order the author added them. `draftId` holds the stable id of a draft (`DRS-draft-storage.md`). A `footprint` entry is a project-relative path, a trailing `/` naming a folder. `due` is a calendar date. No item carries a position, a size, or a collapsed state: `DMS-delivery-machine-state.md` keeps them per machine.

  An edge carries `id`, `from`, `to`, and `type`, which is `blocks` or `relates`. A `blocks` edge reads "`from` blocks `to`": `to` waits for `from`. A `relates` edge carries no order, and its `from` and `to` are only the ends it was drawn between.

  `references` is optional, and `edges` and `items` may be absent on a document holding none.

- This module invokes nothing. It performs no filesystem access, so it uses no primitive of `FSA-filesystem-access.md`, and it contacts no integration.

## Non-functional requirements
- A validation of a document holding several thousand items completes fast enough to sit inside a save without the author noticing it, so no caller has reason to skip it, to sample it, or to run it in the background.
- The module holds no state between calls. There is no cache of past reports, no notion of "the current Flow", and no ordering requirement among calls, so concurrent validations of unrelated documents never interact.
- Violation codes are a stable, closed set. A caller may branch on a code and render its own wording; the `message` is for the caller that does not.
- Validation contacts no model and reaches no network. A document is judged by its own structure alone.
- The rule set here and the rules the canvas enforces while an author draws (`../ui/FLO-flow.md` FLO-FR-16, FLO-FR-17, FLO-FR-ZEPI, FLO-FR-GZCH, FLO-FR-OAOZ) describe the same plan. The canvas answers instantly and locally so a refused connection costs no round trip; this module decides whether a document is one, and neither defers to the other.
