# Specification map zoom

**Spec code:** `SMZ`

## Intent
The architect moves over the specification map by panning and by semantic zoom: each zoom step reveals one level deeper in the index, and no level ever renders below readable type. This spec owns the canvas of the Map tab (`SMP-specification-map.md`), the detail level and scale, the first-paint framing, the zoom controls, the legend, and the focused-subtree view with its breadcrumb. Out of scope: a "fit the whole graph" zoom that shrinks type below the scale window.

## Functional requirements
1. **SMZ-FR-DPWC** The canvas fills the Map tab below the toolbar and beside the inspector. Its background is `--bg-canvas` with a dot grid of 1px `--border-2` dots at a 16px pitch. All nodes and edges draw on one stage that the canvas transforms by one translation and one scale.
2. **SMZ-FR-KAHU** A pointer drag that starts on empty canvas pans the stage by the pointer movement. A pointer drag that starts on a node does not pan. The canvas shows the grab cursor.
3. **SMZ-FR-RNIT** The detail level is explicit state, from level 0 to the last level of the index. At level L, index nodes of depth below L render expanded and index nodes of depth L render collapsed. Deeper index nodes do not render. At the last level, leaf groups render expanded with their spec chips.
4. **SMZ-FR-VYOS** The level control is a segmented control with one button per level, labelled with the plural level names of the index (per `SMP-specification-map.md` SMP-FR-QMRE). The button of the current level has an `--accent-soft` background and `--accent` text. The other buttons are transparent with `--fg-3` text.
5. **SMZ-FR-GMEB** At every level the scale stays between 0.66 and 1.30, including both ends.
   - *Why:* The scale window keeps every level legible; a smaller scale is reached by a shallower level, not by smaller type.
6. **SMZ-FR-ZQLP** A wheel step multiplies the scale by 1.06 toward the user and by 0.94 away. A result above 1.30 steps one level deeper with scale 0.66, and a result below 0.66 steps one level shallower with scale 1.30. At the last level and at level 0 the scale clamps instead.
7. **SMZ-FR-HCTF** Every scale change multiplies the translation by the ratio of the new scale to the old scale.
8. **SMZ-FR-JWXA** The zoom controls sit at the bottom right of the canvas. They hold a minus button that zooms by a factor of 1/1.18, a plus button that zooms by 1.18, and a percentage button that shows the scale rounded to a whole percent. Both zoom buttons follow SMZ-FR-ZQLP. The percentage button applies SMZ-FR-NUOB.
9. **SMZ-FR-NUOB** Until the author pans or zooms, the map shows level 0 at scale 0.8 with horizontal translation 0 and vertical translation `max(0, contentHeight / 2 × scale − canvasHeight / 2 + 26)`. The canvas height is measured, and the framing is computed again each time the measured height changes.
   - *Why:* The canvas has no height in its first frame; framing from that height paints the map centred with the first card above the fold.
10. **SMZ-FR-EPTR** Choosing a level other than 0 in the level control sets that level with scale 0.85 at the last level and 0.8 at other levels. It then translates the stage so that the rendered node holding the selection is centred horizontally, with its vertical centre at most 240px below its top. Choosing level 0 applies SMZ-FR-NUOB.
11. **SMZ-FR-QOTA** When no node is selected, or no rendered node holds the selection, choosing a level other than 0 frames that level top-aligned: horizontal translation 0, and vertical translation `max(0, contentHeight / 2 × scale − canvasHeight / 2 + 26)` at the scale of SMZ-FR-EPTR.
12. **SMZ-FR-AILK** The legend sits at the bottom left of the canvas. It shows the plural name of the current level, a divider, and four 6px dots labelled verified, built, drafted, and gap, in the state colours of `SMN-specification-map-nodes.md` SMN-FR-CMLN.
12. **SMZ-FR-FOXU** Focusing an index node makes the map render only the subtree of that node. The focused node takes the place of the only root node. The level control and the legend then list the levels from the depth of the focused node to the last level.
13. **SMZ-FR-BRCT** While a node is focused, a breadcrumb sits at the top left of the canvas. It reads `specifications`, then each ancestor of the focused node, then the focused node, separated by `→`. Clicking an ancestor focuses that ancestor. Clicking `specifications` ends the focus and shows the whole index.
14. **SMZ-FR-LWEQ** Focusing a node or ending a focus shows level 0 of the new view with the framing of SMZ-FR-NUOB, and keeps the selection.
15. **SMZ-FR-CUTY** The wheel and the zoom controls change nothing while a drag of `SMO-specification-map-organization.md` is in progress.

## Wireframes
```
┌──────────────────────────────────────────────────────────────┐
│ specifications → workspace → shell & navigation              │
│ · · · · · · · · · · · · · · · · · · · · · · · · · · · · · ·  │
│ · · · ┌────────────────────────────┐ · · · · · · · · · · · ·  │
│ · · · │ shell & navigation  17 sp  │ · · · · · · · · · · · ·  │
│ · · · │ ┌ zones ───────────────┐   │ · · · · · · · · · · · ·  │
│ · · · │ └──────────────────────┘   │ · · · · · · · · · · · ·  │
│ · · · └────────────────────────────┘ · · · · · · · · · · · ·  │
│ ┌──────────────────────────────────────────┐   ┌──────────┐   │
│ │ groups │ ● verified ● built ● drafted ● gap│   │ −  + 80% │   │
│ └──────────────────────────────────────────┘   └──────────┘   │
└──────────────────────────────────────────────────────────────┘
```
- Layout notes: the legend pill is 26px high on `--bg-elevated` with a 1px `--border-2` border and `--r-sm` radius. The zoom controls sit on `--bg-elevated` with a 1px `--border-2` border, `--r-md` radius, and 2px padding. The breadcrumb never overlaps the inspector open button of `SMI-specification-map-inspector.md` SMI-FR-BNHI.

## UI contract boundary
- **Owned by the UI**: the stage transform, the pointer and wheel handling, the level and scale state, the canvas measurement and framing, the zoom controls, the legend, and the focused-subtree view with its breadcrumb.
- **Delegated to backend (abstract)**: none.

## Non-functional requirements
- A level change animates node position and height over 200ms and opacity over 140ms with the `--ease-out` curve. A hover or border change uses `--dur-fast`. No motion bounces.
- The wheel listener is not passive, so a wheel step over the canvas never scrolls any other surface.
