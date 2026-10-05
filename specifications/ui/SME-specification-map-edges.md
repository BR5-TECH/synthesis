# Specification map edges

**Spec code:** `SME`

## Intent
Dependency edges show how the areas of the specification map depend on each other, so the architect sees coupling and broken citations before they plan a change. This spec owns the aggregation of spec-to-spec dependencies to the level the map shows, the limits and weight classes of the edges, the active and unresolved styles, and the **dependencies** toggle. Node geometry is owned by `SMN-specification-map-nodes.md`, and the levels by `SMZ-specification-map-zoom.md`.

## Functional requirements
1. **SME-FR-NQAB** Each dependency of the index is a directed pair of spec codes with a citation count and an unresolved flag (per `SMP-specification-map.md`).
2. **SME-FR-FKIW** At level 0 the map draws links between root nodes, and at the other levels below the last level between index nodes of depth 1. A link sums the citations of the resolved dependencies whose specs lie under its two nodes. An unresolved dependency, and one whose specs share a node, add to no link.
3. **SME-FR-LBVO** At level 0 the map draws the 10 links with the most citations. At the other levels below the last level it draws the 14 links with the most citations. Links with equal citations keep the order of the index.
4. **SME-FR-SGUC** A link is heavy at 60 or more citations at level 0 and 30 or more elsewhere, medium at 25 or more at level 0 and 14 or more elsewhere, and light otherwise. Light draws 1.4px in `--border-2`, medium 2.2px between `--border-2` and `--border-3`, heavy 3.2px in `--border-3`, all at opacity 0.7.
5. **SME-FR-TJMY** A link whose source or target node contains the active node (per `SMN-specification-map-nodes.md` SMN-FR-KTZB) draws 2.4px in `--accent` at opacity 0.95 with an `--accent` arrowhead.
6. **SME-FR-DZHE** A link is a quadratic curve from its source node to its target node with an arrowhead at the target. Each end sits where the line between the two node centres crosses a margin 6px outside that node. The control point sits at the midpoint, offset perpendicular to the line by 0.11 of its length.
7. **SME-FR-WRPX** At the last level the map draws only the resolved dependencies of the active spec, in both directions, from chip centre to chip centre in the active style. While a spec is active, a chip with no dependency to or from it renders at opacity 0.5.
8. **SME-FR-VAEC** An unresolved dependency draws at every level, outside the limits of SME-FR-LBVO: a 1.6px dashed curve with a 6px dash and a 5px gap in `--danger`, opacity 0.9, and a `--danger` arrowhead. It joins the rendered nodes that hold its two specs, and does not draw when one node holds both.
9. **SME-FR-OKUH** The **dependencies** toggle is on by default. While it is off, no resolved dependency draws. It uses `--bg-active` and `--accent` when on, and `--bg-sunken` and `--fg-3` when off.
10. **SME-FR-JRCY** Aggregation reads the current tree, so after an organization edit of `SMO-specification-map-organization.md` every link joins the nodes that now hold its specs.

## UI contract boundary
- **Owned by the UI**: link aggregation, limits, weight classes, edge geometry, edge styles, and the **dependencies** toggle.
- **Delegated to backend (abstract)**: none. The dependencies come from `"load specification map"` of `SMP-specification-map.md`.

## Non-functional requirements
- Edge colours are CSS custom properties applied through style rules, so a theme change recolours every edge without a new render of the map.
- Edges never capture the pointer, so a click through an edge reaches the node or the canvas below it.
