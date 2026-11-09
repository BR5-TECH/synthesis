# CALM architecture diagrams

**Spec code:** `CAD`

## Intent
How the application shows a FINOS CALM architecture as a diagram. An author keeps an architecture as code, in a ```` ```calm ```` fence of a Markdown file or in a whole `*.calm.json` Architecture artifact, and reads it as a diagram next to the prose that explains it. The diagram is a read-only canvas: the JSON source is the only thing that an author edits, and the canvas shows the result of a read of `CLM-calm-model.md`. A CALM fence is a diagram fence of `EDT-editor.md` EDT-FR-MOYS, so it has the same view toggle, Edit button, View button, and overlays as a Mermaid fence, and this spec states only what is different for CALM. Out of scope: editing on the canvas, and a CALM diagram in a PDF or DOCX export, which keeps the fence as a code block (per `../core/DXP-document-export.md` DXP-FR-WGLT).

## Functional requirements

### CALM fence
1. **CAD-FR-PFNM** A **CALM fence** is a fenced code block whose language is `calm`. Its language is compared as the language of a Mermaid fence is. It is a diagram fence (per `EDT-editor.md` EDT-FR-MOYS), and its source is one CALM document as JSON.
2. **CAD-FR-IPRR** On the WYSIWYG surface, a CALM fence whose read gives a diagram graph (per `CLM-calm-model.md` CLM-FR-ODQZ) shows the **inline canvas**. The canvas is as wide as the text column. Its height follows the fitted diagram, and it is at least 160 px and at most 480 px.
3. **CAD-FR-IRIT** The inline canvas is a static picture. It does not pan, zoom, or select, and nothing on it can be dragged. The mouse wheel and a trackpad scroll over it scroll the document.
4. **CAD-FR-IRPP** A click on the inline canvas, or Enter or Space while it has focus, shows the source of the fence in place and puts the caret in that source (per `EDT-editor.md` EDT-FR-KQXZ). The inline canvas can take keyboard focus, and its accessible name is "CALM architecture diagram".
5. **CAD-FR-ITUS** When the read of a CALM fence ends with an error, the fence shows its source with the error message under it, on the terms of `EDT-editor.md` EDT-FR-XTEB and EDT-FR-VIOP. The message is "Diagram error:" and then the error statement of the step that failed (per `CLM-calm-model.md` CLM-FR-ODQZ), with its reason under it.
6. **CAD-FR-IWGT** A schema error, a reference error, and a containment loop each block the diagram, as a Mermaid syntax error does. A CALM fence never shows a diagram and an error at the same time.
7. **CAD-FR-IXAC** A CALM diagram uses the colours of the active light or dark theme. When the theme changes, it shows the new colours with no new read of its source.
8. **CAD-FR-JBUT** A CALM fence whose read gives a graph with no nodes shows the statement "The architecture has no nodes." in the inline canvas (per `CLM-calm-model.md` CLM-FR-HSUH). This fence has the View button (per `EDT-editor.md` EDT-FR-YVAI).

### Insert CALM architecture
9. **CAD-FR-HMXS** The formatting toolbar has an **Insert CALM architecture** button immediately after the Insert diagram button (per `EDT-editor.md` EDT-FR-VUGQ). It has the size of the other buttons and shows an architecture icon: three boxes in two levels that lines join. Its accessible name and tooltip are "Insert CALM architecture". It is disabled while the surface accepts no input.
10. **CAD-FR-JNWR** The button opens and closes the **CALM template menu**. The menu is the filterable selector of `AII-ai-integrations.md` AII-FR-36 to AII-FR-39, and it opens, filters, and closes as the diagram type menu does (per `EDT-editor.md` EDT-FR-OWNT). The filter matches the name of a template.
11. **CAD-FR-JWXC** The CALM template menu lists at least these templates, in this order: "Minimal", "Three-tier web application", "Deployed services", and "Business flow". Each template is a complete CALM document that declares `"$schema": "https://calm.finos.org/release/1.3/meta/calm.json"` and whose read gives a diagram graph.
12. **CAD-FR-KSAQ** The "Three-tier web application" template holds `connects` relationships with protocols. The "Deployed services" template holds a `deployed-in` group. The "Business flow" template holds a flow of at least three transitions.
13. **CAD-FR-KTWD** Selecting a template inserts a CALM fence that holds that template, at the place that `EDT-editor.md` EDT-FR-IPMX states. The insert is one undo step on the terms of EDT-FR-HAGB. The new fence shows its source with the caret in it.

### Diagram editor overlay
14. **CAD-FR-KUYJ** For a CALM fence, the diagram editor overlay has the accessible name "Edit CALM architecture". Its diagram pane shows the CALM diagram of the source, fitted and static, as the inline canvas is (CAD-FR-IRIT). It reads the source again while the author types, on the terms of `EDT-editor.md` EDT-FR-QSOQ, EDT-FR-STFK, and EDT-FR-UDVE.
15. **CAD-FR-KWIY** For a CALM fence, the hints area has the accessible name "CALM syntax hints" and the title "CALM 1.3". It shows one hint set with the forms of the document frame with its `$schema`, a node, each relationship type, an interface, a flow with its transitions, a control, and metadata, and then a short complete example.
16. **CAD-FR-LEII** The CALM hints are English text bundled in the application, on the terms of `EDT-editor.md` EDT-FR-NVGA. The complete example of the CALM hint set is a document whose read gives a diagram graph.

### Diagram viewer overlay
17. **CAD-FR-LLNQ** For a CALM fence, the diagram viewer overlay has the accessible name "View CALM architecture". Its diagram area has the accessible name "CALM architecture diagram", and it shows the **interactive canvas**: the CALM diagram of the fence, with the flow selector (CAD-FR-LQLI) and the details panel (CAD-FR-KUBU).
18. **CAD-FR-MBML** The interactive canvas fits, zooms, and pans by the rules of `EDT-editor.md` EDT-FR-LJAP, EDT-FR-MZIF, EDT-FR-PDBC, and EDT-FR-JJDP. The title bar of the viewer has the zoom readout and the Fit control of EDT-FR-CCZU. Nothing on the canvas can be dragged, connected, or deleted.

### Flows and details
19. **CAD-FR-LQLI** When the document has flows, the interactive canvas has a **flow selector**, with the accessible name "Flow". Its first option is "No flow", which is the default, and then each flow by its `name` in document order. When the document has no flows, the canvas has no flow selector.
20. **CAD-FR-KSEJ** When the author chooses a flow, the canvas shows each step of it (per `CLM-calm-model.md` CLM-FR-FDGM) on its edge, with its `sequence-number` in a circle and its arrow in the direction of CLM-FR-FKHM. It dims each node and each edge that no step uses. "No flow" removes the highlight.
21. **CAD-FR-KUBU** The interactive canvas has a **details panel** on its right side. A click on a node or on an edge selects it, and a click on an empty part of the canvas clears the selection. The details panel shows the item that is selected, and only one item is selected at a time.
22. **CAD-FR-LQZM** For a selected node, the details panel shows its name, `unique-id`, `node-type`, and description. Then it shows its interfaces, its controls, its metadata, and the decisions that name it (per `CLM-calm-model.md` CLM-FR-ESMQ). A section with no content does not show.
23. **CAD-FR-MDTX** For a selected edge, the details panel shows the `unique-id`, the relationship type, the description, and the protocol of its relationship. Then it shows the interfaces that each end names, its controls, its metadata, and the decisions that name it. A section with no content does not show.
24. **CAD-FR-MISQ** Each control shows its name, its description, its requirement URL, and each property of its configuration as a key and a value. Each item of metadata and of an interface shows as a key and a value. A value that is an object or an array shows as indented JSON text.
25. **CAD-FR-MJWJ** When nothing is selected and a flow is chosen, the details panel shows the name and the description of the flow and the list of its steps in order. Each step shows its `sequence-number`, its description, and the names of the two nodes that it joins.
26. **CAD-FR-MUJL** When nothing is selected and no flow is chosen, the details panel shows the number of nodes, relationships, and flows, the top-level metadata, controls, and `adrs` of the document, and a **Nodes** list of each node by name.
27. **CAD-FR-MXHO** Each entry of the Nodes list and each step of the flow list is a control that the keyboard can reach and operate. Activating an entry selects its node, or the edge of its step, and the canvas pans to show it. This is how the keyboard selects an item.
28. **CAD-FR-MYHZ** The details panel shows all CALM text as plain text, and a URL as text that the author can select (per `CLM-calm-model.md` CLM-FR-IRGG). Its content can be selected and copied, and it scrolls when it is longer than the panel.
29. **CAD-FR-MZSV** Choosing a flow, selecting an item, and the scroll of the details panel are view state (per `EDT-editor.md` EDT-FR-EAOC). The viewer starts each opening with "No flow" and nothing selected.
30. **CAD-FR-NNHM** When the source changes while the interactive canvas shows, the canvas shows the new read. It keeps the chosen flow and the selected item when the new read still holds a flow or an item with the same `unique-id`. Otherwise it goes back to "No flow" or clears the selection.

### Architecture artifact
31. **CAD-FR-RBYK** An Editor tab on a source file (per `ESH-editor-source-files.md` ESH-FR-SSDV) whose artifact type is `architecture` (per `../core/ASC-artifact-scanning.md` ASC-FR-02) has two views. A Markdown file with that type keeps the surfaces of `EDT-editor.md`. The **Source view** is the source surface of `ESH-editor-source-files.md` ESH-FR-SSDV. The **Diagram view** is the interactive canvas of the buffer, which fills the document column.
32. **CAD-FR-OJTD** The action cluster of an Architecture tab has the **Diagram / Source toggle** in the place of the editing-mode toggle (per `EDT-editor.md` EDT-FR-16). It is an icon button that the keyboard can reach and operate. Its accessible name and tooltip state the view it switches to: "Show architecture source" or "Show architecture diagram".
33. **CAD-FR-NQXR** An Architecture tab opens on the Diagram view. A tab whose buffer is empty or holds only whitespace opens on the Source view. A tab whose artifact type becomes `architecture` after it opened gets the toggle and stays on the Source view.
34. **CAD-FR-OPTU** The Diagram view reads the buffer again after each change of the buffer: an edit, an undo, a redo, a reload after an external change, or a reset after a rollback. It keeps its zoom, its pan, its flow, and its selection on the terms of CAD-FR-NNHM.
35. **CAD-FR-OZST** When the read of the buffer ends with an error, the Diagram view shows the error statement and its reason (per `CLM-calm-model.md` CLM-FR-ODQZ) and a **Show source** button. The button switches the tab to the Source view and puts the caret at the line of the error when the reason gives one.
36. **CAD-FR-PDVE** When the buffer is empty or holds only whitespace, the Diagram view shows the statement "No diagram to show" and the Show source button.
37. **CAD-FR-PTBB** The view of an Architecture tab is view state. A switch of the view leaves the buffer unchanged, does not mark the artifact dirty, adds no undo step, and is not saved. The write schedule, the save, and the external-change rules of `EDT-editor.md` apply in both views.
38. **CAD-FR-PVCT** Opening Find or Find & Replace while the Diagram view shows switches the tab to the Source view first (per `EFR-editor-find-replace.md`). Activating a comment thread while the Diagram view shows switches the tab to the Source view and shows the fragment of that thread.
39. **CAD-FR-QADV** On an Architecture tab that accepts no input, the Diagram / Source toggle stays enabled, because a switch of the view changes no byte. The Diagram view is read-only in each state of the tab.
40. **CAD-FR-QLQZ** Each switch of the view of an Architecture tab logs one `DEBUG` record under the `frontend` domain through `src/logging.ts`, with the name of the new view. No record holds text from the document.

## User stories
- As an architect, I want to keep a system architecture as CALM JSON in the project, so that agents and people read the same model.
- As an author, I want to put a CALM architecture in a Markdown document and see it as a diagram, so that the prose and the model stay together.
- As a reviewer, I want to choose a business flow and see the steps it takes through the architecture, so that I can check the flow against the design.

## Wireframes
A CALM fence in the WYSIWYG surface (CAD-FR-IPRR):
```
  ## Payment platform
  ┌─ CALM architecture diagram ───────────────────────── [⤢] [✎] [</>] ┐
  │   ┌───────┐        ┌─ deployed-in: cluster ─ ─ ─ ─ ─ ─ ─ ─ ┐        │
  │   │ Buyer │ ─ ─ ─▶ │  ┌─────────┐   HTTPS   ┌──────────┐   │        │
  │   └───────┘        │  │ Pay API │ ────────▶ │ Ledger db│            │
  │                    │  └─────────┘           └──────────┘   │        │
  │                    └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┘        │
  └────────────────────────────────────────────────────────────────────┘
```
- Layout notes: the controls in the top-right corner are the View button, the Edit button, and the view toggle of `EDT-editor.md`, in that order. The canvas is as wide as the text column, and its height is between 160 px and 480 px (CAD-FR-IPRR).

The formatting toolbar insert group (CAD-FR-HMXS):
```
  … [ ❝ ] [ ⬚→⬚ ] [ ⬚⬚⬚ ]
          Insert   Insert CALM
          diagram  architecture
```

The diagram viewer overlay for a CALM fence, and the Diagram view of an Architecture tab (CAD-FR-LLNQ, CAD-FR-RBYK):
```
┌─ View CALM architecture ───────── Flow [ Checkout ▾ ]  100% [ Fit ] [ ✕ ] ┐
│                                                     │ CHECKOUT              │
│   ┌───────┐ ①      ┌─────────┐  ②  ┌──────────┐     │ Buyer pays for an     │
│   │ Buyer │ ─────▶ │ Pay API │ ──▶ │ Ledger db│     │ order.                │
│   └───────┘        └─────────┘     └──────────┘     │ 1  Buyer → Pay API    │
│                         ┊                           │    Submit payment     │
│                    ┌─────────┐   (dimmed)           │ 2  Pay API → Ledger   │
│                    │ Mailer  │                      │    Write entry        │
│                    └─────────┘                      │                       │
└─────────────────────────────────────────────────────┴───────────────────────┘
```
- Layout notes: the details panel takes the right side of the canvas, and it scrolls on its own (CAD-FR-KUBU, CAD-FR-MYHZ). In the viewer, the flow selector sits in the title bar before the zoom readout. In an Architecture tab, the flow selector, the zoom readout, and the Fit control sit in a bar above the canvas, and the Diagram / Source toggle is in the action cluster (CAD-FR-OJTD). Items that no step of the chosen flow uses show dimmed (CAD-FR-KSEJ).

## UI contract boundary
- **Owned by the UI**: the CALM fence and its inline canvas, the Insert CALM architecture button and its template menu with the bundled templates, the CALM labels and hints of the diagram editor overlay, the interactive canvas of the viewer and of the Architecture tab with its flow selector and details panel, the Diagram / Source toggle and the view state of an Architecture tab. The read of a document is the client-side read of `CLM-calm-model.md`.
- **Delegated to backend (abstract)**: `"open artifact by id"`, `"load artifact contents by id"`, and `"save artifact contents"`, owned by `../core/PST-project-storage.md`, serve an Architecture artifact as they serve every other file. The type `architecture` and its `.calm.json` rule are owned by `../core/ASC-artifact-scanning.md` (ASC-FR-02, ASC-FR-QVCN). This spec introduces no backend operation.

## Non-functional requirements
- A CALM diagram renders offline and fetches nothing from the network.
- The canvas library loads only when the first CALM diagram shows.
- The interactive canvas stays responsive with 200 nodes on a typical development machine.
- The inline canvas and the interactive canvas meet the contrast floor of the active theme for node text, edge labels, and the dimmed state of a chosen flow.
