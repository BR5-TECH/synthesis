# Specification map

**Spec code:** `SMP`

## Intent
The Map tab shows the project's specification corpus as one nested map, so the architect sees at a glance which functional areas exist, what each one is for, and how complete each one is. It is also a planning surface: from the map the architect reorganizes the hierarchy (`SMO-specification-map-organization.md`) and starts new work as drafts (`SMD-specification-map-drafts.md`). This spec owns the tab frame: its identity, its entry point, the load of the index, the toolbar, and the lifetime of the map session. Canvas navigation, node rendering, dependency edges, and the inspector are specified in `SMZ-specification-map-zoom.md`, `SMN-specification-map-nodes.md`, `SME-specification-map-edges.md`, and `SMI-specification-map-inspector.md`. Out of scope: indexing the corpus, deriving completeness states, and persisting the map beyond the open project.

## Functional requirements
1. **SMP-FR-KQTD** The Map tab is a main-viewport tab. Its label is `Map — specifications` and the tab strip shows the layers icon in front of the label.
2. **SMP-FR-WBNL** At most one Map tab is open per project. A request to open the map while a Map tab is open activates that tab and creates no second tab (per `TAB-tabs.md` TAB-FR-KXMW).
3. **SMP-FR-HZRA** The **View map** affordance of the Project panel opens the Map tab and makes it the active tab (per `LIB-library.md` LIB-FR-VMAQ).
4. **SMP-FR-PMCX** When the Map tab renders and the session holds no index, the tab invokes `"load specification map"` once and shows a loading state in the canvas area until the result arrives.
5. **SMP-FR-FEJV** The index is a tree of index nodes with spec nodes as leaves, all spec nodes at one depth. The tab renders indexes of 3 to 5 levels. An index node at depth 0 is a **root node**, one whose children are spec nodes is a **leaf group**, and any other is a **branch node**.
6. **SMP-FR-QMRE** The index supplies a plural name and a singular name for each level, for example `domains` and `domain`. The map uses these names in the level control, the legend, the inspector, and the action labels, and uses no fixed level name of its own.
7. **SMP-FR-TGYU** A spec node carries exactly one completeness state: verified, built, drafted, or gap. The map shows the state the index supplies and derives no state itself.
8. **SMP-FR-HWIC** `"load specification map"` returns a fixed demo index that is built into the application. Every spec node in the demo index names a spec file that exists under `specifications/`, and every dependency names two spec nodes of the demo index.
   - *Why:* The demo index stands in for the indexing backend, but **Open in editor** must still reach a real file.
9. **SMP-FR-ONSD** When `"load specification map"` fails, the canvas area shows an error state that says the map could not be loaded, with a **Retry** button that invokes the load again. The toolbar controls are disabled, and no other tab or panel changes.
10. **SMP-FR-LRAX** When the loaded index has no root nodes, the canvas area shows an empty state that says no specifications are indexed.
11. **SMP-FR-CVIB** A toolbar of 38px height sits above the canvas. From left to right it holds: the level control (per `SMZ-specification-map-zoom.md` SMZ-FR-VYOS), a divider, the **Completeness** label, the project completeness bar, the counts line, a flexible spacer, the **dependencies** toggle (per `SME-specification-map-edges.md` SME-FR-OKUH), and the **gaps only** toggle (per `SMN-specification-map-nodes.md` SMN-FR-HLDQ).
12. **SMP-FR-YDKM** The project completeness bar is 170px wide and 8px high. It holds four segments in the order verified, built, drafted, gap. The width of each segment is the share of all spec nodes in that state. The counts line reads `N verified · N built · N drafted · N gaps`.
13. **SMP-FR-GJEW** Below a Map tab width of 1180px, the **Completeness** label is hidden, the bar is 92px wide, and the counts line reads `P% verified · N gaps`. Below 1000px the counts line is hidden, and the two toggles show only their icon, with their name as the tooltip.
14. **SMP-FR-QNUH** The map session holds the loaded index, the organization edits, the draft placements, the selection, the view, the focused subtree, the two toggles, and the inspector state. The session is in memory for the open project. Deactivating and then reactivating the Map tab keeps the whole session.
15. **SMP-FR-BXAP** Closing the Map tab resets the view and the focused subtree, so the next open frames the map as on first paint (per `SMZ-specification-map-zoom.md` SMZ-FR-NUOB). The index, the organization edits, the draft placements, the selection, the toggles, and the inspector state stay in the session.
16. **SMP-FR-MEZK** A project switch or an active-worktree change discards the whole map session (per `OVW-overview.md` OVW-FR-11, OVW-FR-12). The next open of the Map tab loads the index again.
17. **SMP-FR-SWUL** The Map tab never shows a dirty indicator, and Save is unavailable while the Map tab is the active tab.

## Wireframes
```
├──────────────────────────────────────────────────────────────────────────────────────┤
│ ⌂ │ Dashboard │ STB STB-status-bar.md ● │ ◈ Map — specifications × │                 │
├──────────────────────────────────────────────────────────────────────────────────────┤
│ [domains|features|groups|specs] │ COMPLETENESS ▮▮▮▮▮▯▯▯ 24 verified · 61 built …  ⑂ ⚠ │
├────────────────────────────────────────────────────────────┬─────────────────────────┤
│ · · · · · · · · · · · · · · · · · · · · · · · · · · · · ·  │ INSPECTOR   feature   × │
│ · ┌──────────────────┐ · ┌──────────────────┐ · · · · · ·  │ workspace → shell       │
│ · │■ authoring  26 sp│ · │■ graduation 20 sp│ · · · · · ·  │ 17 specs  shell & nav   │
│ · │ summary …        │ · │ summary …        │ · · · · · ·  │ ┌ SUMMARY ────────────┐ │
│ · │ ▮▮▮▮▮▯▯▯         │ · │ ▮▮▯▯▯▯▯▯         │ · · · · · ·  │ │ …                   │ │
│ · │ 3 ver 13 blt  2g │ · │ 0 ver 4 blt   3g │ · · · · · ·  │ └─────────────────────┘ │
│ · └──────────────────┘ · └──────────────────┘ · · · · · ·  │ Groups …                │
│ ┌──────────────────────────────────┐        ┌───────────┐  │ Depends on …            │
│ │ domains │ ● verified ● built … │        │ −  +  80% │  │ [Open domain] [Zoom to] │
│ └──────────────────────────────────┘        └───────────┘  │                         │
└────────────────────────────────────────────────────────────┴─────────────────────────┘
```
- Layout notes: the toolbar scrolls horizontally when it does not fit and never wraps. The canvas fills the width that the inspector leaves (per `SMI-specification-map-inspector.md` SMI-FR-QWOP). The loading, error, and empty states replace the canvas content and keep the toolbar. All colours, radii, shadows, and motion come from the design-system tokens in `src/styles/colors_and_type.css`.

## UI contract boundary
- **Owned by the UI**: the tab record and its singleton identity, the toolbar and its responsive rules, the loading, error, and empty states, the completeness roll-ups computed from spec nodes, and the in-memory map session with its reset rules.
- **Delegated to backend (abstract)**:
  - `"load specification map" (stub — no core spec yet)` — returns `{ levels: { plural: string; singular: string }[]; roots: IndexNode[]; dependencies: Dependency[] }`, where `IndexNode` is `{ id: string; label: string; summary: string; hue?: string; children: IndexNode[] | SpecNode[] }`, `SpecNode` is `{ code: string; path: string; label: string; summary: string; requirements: number; scenarios: number; state: "verified" | "built" | "drafted" | "gap" }` with `path` relative to `specifications/`, and `Dependency` is `{ from: string; to: string; citations: number; unresolved?: boolean }` naming spec codes. The stub serves the demo index of SMP-FR-HWIC.

## Non-functional requirements
- The map session is never written to disk and never survives a relaunch.
- The tab renders an index of 150 spec nodes without a visible delay on a pan or a zoom step.
- Diagnostic log records of the map carry counts and operation names only. They never carry a node label, a summary, or a draft name.
- The map needs no network connection.
