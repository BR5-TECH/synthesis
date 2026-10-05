# Editor find and replace

**Spec code:** `EFR`

## Intent
The Editor's in-tab search. A Find panel and a Find & Replace panel take over the band directly above the editing surface for as long as they are open, so an author revising a long artifact renames a term or corrects a wording without leaving the tab and without a modal standing over the text they are changing. The panels match the **in-memory buffer** the author is actually editing rather than the bytes on disk, so an edit made a moment ago is searchable at once — which is what separates this from the universal project search of `SCH-search.md`, which walks the project on disk and cannot see an unsaved edit. Each artifact carries its own panel state, so a query entered against one file is never visible against another. Out of scope: capture-group substitution, so a replacement inserts its text literally in every match mode; searching any artifact other than the open one, which is the universal search's; and any persistence of the panel's state, which lives in memory with the rest of the artifact's editing session and is discarded with it.

## Wireframes

The Find panel (⌘F) occupying the band:
```
┌──────────────────────────────────────────────────────────────┐
│ Skill · onboarding.md                            [ ❝ ] [ ◧ ] │
├──────────────────────────────────────────────────────────────┤
│ [Aa][aA][.*] session                   3/17    ‹   ›     ✕   │  ← Find panel
├──────────────────────────────────────────────────────────────┤
│ # Onboarding                                                 │
│                                                              │
│ Steps to run before the first ▒session▒…                     │
└──────────────────────────────────────────────────────────────┘
```

The Find & Replace panel (⌘R) occupying the same band:
```
┌──────────────────────────────────────────────────────────────┐
│ Skill · onboarding.md                            [ ❝ ] [ ◧ ] │
├──────────────────────────────────────────────────────────────┤
│ [Aa][aA][.*] session                   3/17    ‹   ›     ✕   │  ← Find & Replace panel
│              run                    [ Replace ] [ Replace All ]│
├──────────────────────────────────────────────────────────────┤
│ # Onboarding                                                 │
│                                                              │
│ Steps to run before the first ▒session▒…                     │
└──────────────────────────────────────────────────────────────┘
```
- Layout notes: the band sits between the tab's action cluster (per `EDT-editor.md` EDT-FR-16) and the editing surface, and holds at most one of three surfaces — the formatting toolbar being a Markdown file's alone. The three query-mode toggles sit inside the query input at its leading edge, in the same order and with the same meanings as the universal search input's (per `SCH-search.md` SCH-FR-12), with exactly one rendered active. The match counter, the previous/next controls, and the close control are trailing, in that order, on the query row. The Find & Replace panel's replacement input aligns under the query input on a second row with its two actions trailing; the editing surface reflows beneath the taller band and the current match stays in view. Matches are highlighted in place in the editing surface, with the current match rendered distinctly from the rest.

## UI contract boundary
- **Owned by the UI**: the whole of this surface. The band's occupancy, both panels, their accelerators and the Edit-menu items behind them (per `SNV-shell-navigation.md` SNV-FR-43), the query-mode toggles, the matching, the highlighting, the match navigation, and the replacements. All of it is client-side over the in-memory buffer the Editor already holds.
- **Delegated to backend (abstract)**: none. In-editor search deliberately does not go through `"start search (query, mode, scope)"` (owned by `../core/SCC-search.md`), which walks the project on disk and therefore cannot see an unsaved edit. The panels' query mode is the artifact's own value, so it neither reads nor writes the user-global mode the universal search input persists through `"load app preferences"` / `"save app preferences"` (per `SCH-search.md` SCH-FR-13). The panel state is part of the Editor's in-memory retained edit state (per `EDT-editor.md` EDT-FR-28) and so needs no storage contract, and the bytes a replacement produces reach disk only through the `"save artifact contents"` the Editor already invokes (per `EDT-editor.md` EDT-FR-70).

## Functional requirements
1. **EFR-FR-ABHF** The band directly above the Editor's editing surface holds at most one of three surfaces at any moment: the formatting toolbar, the Find panel, or the Find & Replace panel.
2. **EFR-FR-ABVQ** Over a Markdown file (per `ESH-editor-source-files.md` ESH-FR-FKZB) the formatting toolbar occupies the band whenever neither panel is open, and returns the moment a panel closes.
3. **EFR-FR-ACMM** Over a source file (per `ESH-editor-source-files.md` ESH-FR-SSDV) the band holds the find panels alone and is otherwise empty: the toolbar's controls format a rich Markdown document, and a source file has none to format.
4. **EFR-FR-APFR** The band keeps its place whichever surface occupies it, so a panel opening or closing shifts nothing beneath it.
5. **EFR-FR-AVGS** The band sits on the field rather than on the page (per `EDT-editor.md` EDT-FR-63), so the controls over a document are never mistaken for part of it.
6. **EFR-FR-AVML** The band belongs to the tab's own chrome rather than being a floating overlay, so an open panel neither closes nor is closed by the mutually-exclusive overlays of the main window (per `STB-status-bar.md` STB-FR-12), and it is not dismissed by a pointer-down elsewhere in the tab.
7. **EFR-FR-AYNZ** ⌘F opens the Find panel in the band and places focus in its query input with any existing query selected, so typing replaces it.
8. **EFR-FR-BBBV** ⌘F pressed while the Find panel is open closes the panel, restores the formatting toolbar, and returns focus to the editing surface.
9. **EFR-FR-BBKS** ⌘F pressed while the Find & Replace panel is open collapses it to the Find panel, carrying the query, the active match mode, and the current match through unchanged, and retaining the replacement text so that expanding the panel again restores it.
10. **EFR-FR-BJUY** ⌘R opens the Find & Replace panel in the band and places focus in its query input with any existing query selected.
11. **EFR-FR-BOGW** ⌘R pressed while the Find & Replace panel is open closes the panel, restores the formatting toolbar, and returns focus to the editing surface.
12. **EFR-FR-BSST** ⌘R pressed while the Find panel is open expands it to the Find & Replace panel, carrying the query, the active match mode, and the current match through unchanged, and places focus in the replacement input.
13. **EFR-FR-BVHL** Both panels carry a close control at the trailing edge of their query row, and both close on Escape pressed while focus is anywhere within the panel.
14. **EFR-FR-CELO** Closing by either route closes the panel outright rather than collapsing Find & Replace to Find: the formatting toolbar returns and focus moves to the editing surface, placed at the current match so the author resumes editing where the search left them.
15. **EFR-FR-CLZF** Both panels carry three query-mode toggles at the leading edge of the query input — case-insensitive literal, smart-case literal, and regular expression — the same three modes the universal search input offers (per `SCH-search.md` SCH-FR-12).
16. **EFR-FR-COYI** Exactly one query mode is active at any moment, and there is no state in which none is active.
17. **EFR-FR-CWUR** The active query mode is the artifact's own value: activating one neither reads nor writes the user-global query mode the universal search input persists (per `SCH-search.md` SCH-FR-13), so the two surfaces' modes move independently.
18. **EFR-FR-DBOW** An artifact whose panel has not yet been opened in this session starts in case-insensitive literal.
19. **EFR-FR-DDUX** The query is matched against the content of the editing surface that is active (per `EDT-editor.md` EDT-FR-17) — the WYSIWYG rich body together with its frontmatter region, or the raw-text source.
20. **EFR-FR-DKQT** The query is matched against that content as it stands in the in-memory buffer and never against the bytes on disk, so an unsaved edit is searchable the moment it is made.
21. **EFR-FR-DOQP** Opening a panel, closing it, and switching between its two forms never switch the editing mode.
22. **EFR-FR-DOQR** Every match in the active surface is highlighted in place, and exactly one of them is the current match, rendered distinctly from the rest and scrolled into view.
23. **EFR-FR-DSKI** When the current match lies inside the WYSIWYG frontmatter region while that region is collapsed (per `EDT-editor.md` EDT-FR-20), the region expands as a peek so the match is visible, on the same principle as an undo that reaches into it (per `EDT-editor.md` EDT-FR-25).
24. **EFR-FR-DXTV** The panel displays the current match's ordinal within the total as `n/total`, and displays a zero state when the query is empty or nothing matches.
25. **EFR-FR-DYMA** The panel's previous-match and next-match controls move the current match backward and forward in document order and wrap at both ends: next from the last match becomes the first, and previous from the first becomes the last.
26. **EFR-FR-EGQB** Enter pressed in the query input is next-match and ⇧Enter is previous-match, so a query is walked without leaving the keyboard.
27. **EFR-FR-ENKY** In regular-expression mode, a pattern that does not compile yields an empty match set and an invalid-pattern indication in the panel, leaving the query and the active mode intact so the author corrects the pattern in place.
28. **EFR-FR-EPYP** While the pattern is invalid nothing is highlighted, and neither Replace nor Replace All does anything.
29. **EFR-FR-ESDZ** The match set, the current match, and the counter are recomputed whenever the query changes, the active match mode changes, the editing mode changes (per `EDT-editor.md` EDT-FR-17), or the buffer changes — including edits typed with a panel open, an undo or redo (per `EDT-editor.md` EDT-FR-22), and the Editor's own replacements.
30. **EFR-FR-EVHJ** The current match stays where it is when it survives a recomputation; otherwise it becomes the nearest match after the position it occupied, or the first match when none follows.
31. **EFR-FR-EWRP** The Find & Replace panel carries a replacement input and two actions, Replace and Replace All.
32. **EFR-FR-EXHA** Replace rewrites the current match with the replacement text and advances the current match to the next one, wrapping as the navigation controls do.
33. **EFR-FR-EYLX** Replace All rewrites every match in the active surface in one action.
34. **EFR-FR-FHFB** The replacement text is inserted literally in every match mode, regular expression included: no capture-group reference within it is substituted.
35. **EFR-FR-FHSQ** Both replacement actions change only the in-memory buffer and mark the artifact dirty (per `EDT-editor.md` EDT-FR-04), writing nothing to disk until a save.
36. **EFR-FR-FWOU** Both replacement actions do nothing when the match set is empty.
37. **EFR-FR-FYNZ** A single Replace is one user edit and occupies one position in the artifact's single undo history (per `EDT-editor.md` EDT-FR-22, EDT-FR-23).
38. **EFR-FR-FYRJ** A Replace All is likewise one step however many occurrences it rewrote, so one undo restores the document exactly as it stood before the sweep.
39. **EFR-FR-GBIV** Undoing or redoing a replacement leaves the panel open with its query, replacement text, and active match mode unchanged, and its match set recomputed against the restored content.
40. **EFR-FR-GBJT** Whether a panel is open and in which of its two forms, together with the query, the replacement text, and the active match mode, belongs to the artifact and forms part of its retained edit state (per `EDT-editor.md` EDT-FR-28).
41. **EFR-FR-GIPZ** Reopening an artifact whose panel was open in this session renders the same panel with the same query, replacement text, and mode, its matches recomputed against the restored buffer.
42. **EFR-FR-GMCE** Each artifact carries its own panel state, so a query entered in one Editor tab is never visible in another.
43. **EFR-FR-GNBZ** The panel state is held only in memory and is discarded with the rest of the artifact's retained edit state.
44. **EFR-FR-HLGY** While the external-change modal blocks the tab (per `EXC-editor-external-change.md` EXC-FR-VTUH), both panels are inert along with every other edit operation: the accelerators open nothing, and Replace and Replace All do not run.
45. **EFR-FR-HSET** Choosing "Keep my version" (per `EXC-editor-external-change.md` EXC-FR-WDEJ) leaves an open panel and its match state exactly as they were.
46. **EFR-FR-HVVO** Choosing "Load from filesystem" (per `EXC-editor-external-change.md` EXC-FR-WDAV) leaves the panel open with its query, replacement text, and mode intact, and recomputes its matches against the newly loaded content.

## Non-functional requirements
- Matching, highlighting, and recomputation run over the in-memory buffer alone and touch neither the filesystem nor the network.
- The whole surface is operable from the keyboard: opening either panel, switching modes, walking the match set, replacing, and closing all have accelerators or focusable controls, and focus never becomes trapped in the band.
- The panels hold no state that survives the application, so nothing here needs migration and nothing here can be corrupted by a stale stored value.
- A recomputation over a large buffer does not block the editing surface: typing with a panel open stays responsive.
