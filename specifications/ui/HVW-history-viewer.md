# History viewer

**Spec code:** `HVW`

## Intent
Per-artifact history with two surfaces: a version list in the bottom panel for the active tab's entity, and a read-only detail view as a main-viewport tab for a chosen revision. v1 supports read-only inspection, diff for Markdown artifacts, full read-only render at revision for Flow artifacts (project-time-travel model), and single-click restore-whole-version as a new forward commit.

## User stories
- As a user, I want to scan past versions of the active artifact without leaving its tab.
- As a user, I want to compare a past version against the current one as a multi-file diff.
- As a user reviewing a Flow, I want to see exactly what the graph looked like at a past point in time.
- As a user, I want to restore a past version with one click without rewriting Git history.

## UI contract boundary
- **Owned by the UI**: version list rendering in the bottom panel, detail view rendering as a tab, side-by-side / inline diff view for Markdown artifacts, read-only canvas render for Flow revisions, restore confirmation dialog, distinction between live and historical surfaces.
- **Owned by the UI**: the tab corner the action control occupies (HVW-FR-10). The control, the actions it offers, the composer it opens, and the floating discussion panel with its shared discussion surface are owned by `ACT-action-control.md`, which declares the operations behind them; this surface introduces none for them.
- **Delegated to backend (abstract)**: "list versions for entity (commit message, hash, timestamp, author)", "load version detail (Markdown artifact: multi-file diff payload; Flow: full revision render including resolved artifact references at the same revision)", "restore version (writes the historical state as a new forward commit on the current branch)". Archive-container transparency and binary metadata-only diffs are backend responsibilities and surface to the UI through the standard diff payload. Stubs may provide canned versions during the walking skeleton.

## Functional requirements
1. **HVW-FR-01** When a tab bound to an artifact (Editor or Flow) is active, the History viewer's version list is available in the bottom panel and lists historical versions of that artifact with commit message, short hash, timestamp, and author. The list is headed by the artifact it describes, named in the case it carries on disk (per `SNV-shell-navigation.md` SNV-FR-57). It is selected by its own toggle in the activity bar's bottom-panel cluster, which is enabled only while such a tab is active and greyed out otherwise (per `SNV-shell-navigation.md` SNV-FR-44 and SNV-FR-48); with no artifact-bound tab active the list renders empty.
2. **HVW-FR-02** Clicking a version in the list opens a History detail tab in the main viewport for that revision.
3. **HVW-FR-03** History detail tabs are exempt from the single-tab-per-artifact rule; multiple detail tabs for different revisions of the same artifact may coexist with the live editing tab (see `TAB-tabs.md` TAB-FR-06).
4. **HVW-FR-04** For Markdown artifacts (Editor-shaped), the detail view renders a PR-style multi-file diff. The diff can be against the current revision or against another chosen revision; the comparison target is selectable within the tab.
5. **HVW-FR-05** For Flow artifacts, the detail view renders the canvas read-only at the chosen revision. The graph's artifact references resolve to whatever those artifacts were at the same project commit (project-time-travel). The view does not allow editing.
6. **HVW-FR-06** The detail view exposes a "Restore this version" action. Activating it asks for confirmation; on confirmation, "restore version" is invoked and a new commit is written on the current branch (no history rewrite).
7. **HVW-FR-07** Partial restore (cherry-picking specific changes from a past version) is not available in v1; if the user wants pieces of a past version, they read them from the read-only detail view and copy them manually.
8. **HVW-FR-08** Entity scope in v1 is artifact-only. The History viewer does not surface workstream, role, or playbook history in v1.
9. **HVW-FR-09** Notes attached to a historical version are read and authored in the context of the History detail tab; their rendering is visually distinguished from notes on the current version (per `NTS-notes.md` NTS-FR-07).

10. **HVW-FR-10** A History detail tab carries the **action control** of `ACT-action-control.md` (ACT-FR-01), bound to the artifact **at its current identity in the project** rather than to the revision the tab renders (ACT-FR-02), so a question raised while reading an old version is asked about the artifact as it stands and lands in the same conversation its Editor tab shows. Being a read-only tab it offers **Discuss** alone whatever the artifact's type (ACT-FR-04), and its discussions are whole-target discussions of the artifact with no fragment (per `ACT-action-control.md` ACT-FR-LXKD), read in the floating panel as the shared discussion surface, this tab lending the comment rail no margin (ACT-FR-20). Opening one writes a comment log and never a byte of any revision: the detail view stays read-only (HVW-FR-04), nothing is restored, and no forward commit is made (HVW-FR-06). An artifact the project no longer holds carries a control whose Discuss is disabled (per `ACT-action-control.md` ACT-FR-07). Several detail tabs on different revisions of one artifact (HVW-FR-03) are bound to that one artifact and therefore to one set of discussions, a conversation about a file being about the file rather than about the version it was read at — which is why this tab opens no discussion against a historical revision.

## Non-functional requirements
- Restore must be irreversible only at the Git level (a new commit) — the UI itself does not delete the prior state.
- Diff payload size is bounded by the artifact; the UI does not assume small files.
