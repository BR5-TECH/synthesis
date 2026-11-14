# Specification delivery links

**Spec code:** `SDL`

## Intent
The two lenses of the Map tab (`SMP-specification-map.md`) show two sides of one project: the Delivery lens (`MDL-map-delivery-lens.md`) shows the work that is planned, and the Specifications lens shows what the specifications hold and how complete each part is. This spec ties the two together. The architect moves from a delivery item to the specifications it delivers, and from a specification to the plan items that deliver it. The architect also sees which specified work no plan item covers yet. The links come from what the plan already holds, which is each item's references and footprint, so there is no second set of links to keep up to date. New work that starts from a specification goes into the delivery as a task, so it is kept, and it is synced wherever the delivery lives. Out of scope: an edit of a specification from the map, and links for a project that has no delivery.

## Functional requirements
1. **SDL-FR-TWOV** A delivery item's **spec links** are the spec nodes of the loaded specification index whose file is one of the item's references, or is covered by one of its footprint entries on the terms of `../core/FOA-flow-overlap-analysis.md` FOA-FR-XRBA.
2. **SDL-FR-OCPB** A spec node's file is compared as a project-relative path: `specifications/<path>` in the demo index and `<path>` in the `openspec` index (per `SMI-specification-map-inspector.md` SMI-FR-CEVL). A reference is compared as the path of its artifact id.
3. **SDL-FR-QUUA** A container item's spec links are its own spec links and those of every item under it.
4. **SDL-FR-KNOP** The selected-item panel of a delivery item (per `FLO-flow.md` FLO-FR-38) shows a **Specs** list. Each row is one linked spec node with its code badge, its label, and its completeness dot (per `SMN-specification-map-nodes.md` SMN-FR-CMLN). The list is absent where the item has no spec link.
5. **SDL-FR-BVNS** Activating a row of the **Specs** list switches the Map tab to the Specifications lens, selects that spec node, shows the last level, and centres the node on the terms of `SMI-specification-map-inspector.md` SMI-FR-PRSL.
6. **SDL-FR-VAGU** The inspector of the Specifications lens shows a **Delivered by** list for a subject that is a spec node, a leaf group, a branch node, or a root node. It lists each delivery item that links the subject or a spec node under it, in plan order, with its kind glyph, its name, and its status chip (per `FLO-flow.md` FLO-FR-KQCE).
7. **SDL-FR-MCGI** Activating a row of the **Delivered by** list switches the Map tab to the Delivery lens, selects that item, expands each collapsed container that holds it, and centres it.
8. **SDL-FR-CFHH** For a spec node subject and a leaf group subject, the inspector offers **Plan work**. It asks the Delivery lens to add a `task` named with the subject's label and referencing the file of each spec node of the subject, once each (per `MDL-map-delivery-lens.md` MDL-FR-EEDO). The Map tab then shows the Delivery lens with the new task selected.
9. **SDL-FR-SGQM** A spec chip that at least one delivery item links carries a **planned mark**: a 2px `--accent` bar along its bottom edge. The meta line of its hover card (per `SMN-specification-map-nodes.md` SMN-FR-FXRL) ends with `· N plan items`.
10. **SDL-FR-HZLB** The toolbar of the Specifications lens carries an **unplanned** toggle after the **gaps only** toggle (per `SMP-specification-map.md` SMP-FR-CVIB). It is off by default and uses the token treatment of the **gaps only** toggle (per `SMN-specification-map-nodes.md` SMN-FR-HLDQ).
11. **SDL-FR-KXCY** While **unplanned** is on, a spec chip in the gap or the drafted state that no delivery item links renders at full opacity, and every other spec chip renders at opacity 0.25. An index node with no such spec chip beneath it renders at opacity 0.3.
12. **SDL-FR-VUQH** Turning **unplanned** on turns **gaps only** off, and turning **gaps only** on turns **unplanned** off.
13. **SDL-FR-CKAA** While the delivery source is `none` (per `../core/DSR-delivery-source.md` DSR-FR-ZKGG), the Specifications lens shows no **Delivered by** list, no planned mark, no **unplanned** toggle, and no **Plan work**.
14. **SDL-FR-VYIF** The Specifications lens loads the delivery plan on the terms of `MDL-map-delivery-lens.md` MDL-FR-LZIE when the map session holds none, and the Delivery lens loads the specification index on the terms of SMP-FR-PMCX when the session holds none. Until an answer arrives, each list that needs it shows a loading line.
15. **SDL-FR-NXXQ** The links are derived again each time the delivery plan, an item's references or footprint, or the specification index changes. A new derivation changes no selection and no view.

## Wireframes
```
Delivery lens, selected item                 Specifications lens, inspector
┌──────────────────────────────────┐        ┌ INSPECTOR ─────────────── spec  × ┐
│ ▣ Card capture  In progress #42  │        │ workspace → panels                │
│ SPECS                            │        │ [LIB] Project panel               │
│ [LIB] Project panel        ●     │        │ …                                 │
│ [PST] Project storage      ●     │        │ DELIVERED BY                      │
└──────────────────────────────────┘        │ ▣ Card capture     In progress    │
                                            │ ☐ Card form        Not started    │
toolbar of the Specifications lens          │ [Open in editor] [Zoom to]        │
[Delivery|Specifications] │ … ⑂ ⚠gaps only ◌unplanned   │ [Plan work]                       │
                                            └───────────────────────────────────┘
```
- Layout notes: the **Specs** list follows the references of the selected-item panel and uses its row treatment. The **Delivered by** list follows the **Depends on** list of the inspector (per `SMI-specification-map-inspector.md` SMI-FR-YHNT), and **Plan work** sits in the inspector's action row after the primary and secondary actions.

## UI contract boundary
- **Owned by the UI**: the derivation of spec links from the plan and the index, the **Specs** and **Delivered by** lists, the switch between lenses with its selection and centring, the planned mark, the **unplanned** toggle and its filter, and the **Plan work** request.
- **Delegated to backend (abstract)**: none of its own. The plan comes from `MDL-map-delivery-lens.md`, the index from `SMP-specification-map.md`, and the delivery source from `"get delivery source"` (owned by `../core/DSR-delivery-source.md`).

## Non-functional requirements
- The derivation runs in memory over the loaded plan and index, and a plan of several hundred items against an index of 150 spec nodes derives without a visible delay.
- Spec links are never stored. They cannot go stale against the plan or the index they come from.
