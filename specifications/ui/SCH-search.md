# Search

**Spec code:** `SCH`

## Intent
A single, universal search box available from every screen at all times. Two presentation levels: a lightweight overlay below the search input for fast jump-to-result, and a full results tab in the main viewport for the complete result set. One input, no separate command palette. Results stream in as the backend finds them, so the overlay starts filling before the project has been swept, and a small cluster of toggles inside the input decides how the query text is interpreted. Out of scope: filters over the streamed result set in the overlay — those belong to the full results page — and any relevance ordering; results appear in the order the project is walked and never rearrange under the cursor.

## User stories
- As a user, I want to find any entity from any screen by typing into one input so that I never need to learn multiple discovery surfaces.
- As a user with a long result set, I want to escalate from the overlay to a full results page so that I can scroll, filter, and keep navigation context.
- As a user searching for a symbol rather than a word, I want to switch the query into regular-expression mode without leaving the input so that one box serves both kinds of search.

## Wireframes
```
┌──────────────────────────────────────────────────────────────────────┐
│ [Aa][aA][.*]  find me                                                │  ← universal search input
└──────────────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────────────┐
│ ▾ Artifacts                                                          │
│    Skill                                                             │
│      onboarding.md            .claude/skills/onboarding.md           │
│        12│ …steps to find me before the first run…                   │
│    Spec                                                              │
│      SCH-search.md            specifications/ui/SCH-search.md        │
│         3│ …a single universal box to find me from anywhere…         │
│ ▾ Files                                                              │
│      main.rs                  src/main.rs                            │
│        87│     // find me: the walker starts here                    │
│                                                        [View all results] │
└──────────────────────────────────────────────────────────────────────┘
```
- Layout notes: the three query-mode toggles sit inside the input, at its leading edge, ahead of the text caret — `Aa` case-insensitive literal, `aA` smart-case literal, `.*` regular expression — with exactly one rendered active. The overlay hangs directly below the input and is at most as wide as it. Each result is one line for the file — name and project-relative path — with the snippet line indented beneath it, prefixed by its line number; a result that matched only on its path has no snippet line. Group headers carry the collapse control. "View all results" sits at the bottom right.

## UI contract boundary
- **Owned by the UI**: search input rendering and focus management, the query-mode toggle cluster and which mode is active, the typing debounce that decides when a search is dispatched, overlay rendering and dismissal, accumulation of streamed hits and their ordering, full results tab rendering, group collapse state, snippet rendering, click-through routing, and the filter UI on the full results page.
- **Delegated to backend (abstract)**: `"start search (query, mode, scope)"` and `"cancel search (search id)"`, together with the events `"search results"` and `"search ended"` that stream hits and announce termination — all owned by `../core/SCC-search.md`. The active query mode is persisted through `"load app preferences"` and `"save app preferences"` (owned by `../core/GSS-global-settings-storage.md`). Stubs may stream canned hits during the walking skeleton.

## Functional requirements
1. **SCH-FR-01** A universal search input is rendered in the top chrome of the main window and is always visible (see `SNV-shell-navigation.md` SNV-FR-01). It cannot be hidden or gated behind a hotkey.
2. **SCH-FR-02** As the user types, an overlay appears below the input showing results grouped by entity type, populated by the streamed hits of the dispatched search (SCH-FR-15, SCH-FR-16).
3. **SCH-FR-03** Result groups in v1 are: Artifacts (split by subtype — Skill, Agent, Prompt, Spec, Flow, Instructions, Scenario, Scratchpad), Playbooks, Workstreams, Roles, Runs, History matches, and Files. The Artifacts subtypes are exactly the built-in artifact types of `../core/ASC-artifact-scanning.md` ASC-FR-02; the Files group holds matches in files carrying no artifact type.
4. **SCH-FR-04** Empty result groups are hidden from the overlay and from the full results page. A group appears when its first hit arrives and is never rendered ahead of one.
5. **SCH-FR-05** Each group is collapsible (overlay and full page).
6. **SCH-FR-06** At the bottom right corner of overlay, is a "View all results" button that opens a full results page.
7. **SCH-FR-07** The overlay is dismissed by any of: selecting a result; pressing Escape (focus remains in the search input); clicking the "View all results" button; a pointer-down anywhere outside both the search input and the overlay (anywhere else in the top chrome or the main viewport); or the search input losing keyboard focus. A pointer interaction inside the overlay — collapsing a group, scrolling the result list — keeps the overlay open; the overlay closes only on the dismissal triggers listed here.
8. **SCH-FR-08** Submitting the query (Enter) or activating an "open full results" affordance opens a Search results tab in the main viewport with the same grouping, and that tab dispatches its own search over the same query and mode with `scope = full` (SCH-FR-18) rather than inheriting the overlay's capped result set.
9. **SCH-FR-09** Clicking a result routes to where its target entity lives. Every row it shares with Dashboard widget click-through routes identically to that surface's (see `DSH-dashboard.md` DSH-FR-06); the workstream row is this surface's own, Dashboard listing drafts rather than workstream entities:
   - Artifact in standalone edit context → Editor.
   - Artifact in flow context → Flow.
   - Flow artifact → Flow.
   - Workstream → Project panel filtered to that workstream.
   - Run → Runs (bottom panel).
   - History match → History viewer scoped to that entity.
   - File → Editor, opened as a plain text file (per `ESH-editor-source-files.md` ESH-FR-ATDS).
10. **SCH-FR-10** The full results page exposes filters over the result set by owner, author, role, workstream, playbook, source, target, tags, lifecycle status, and custom fields. The overlay does not expose these filters.
11. **SCH-FR-11** Navigation from a result on the full results page preserves the tab so the user can return to the same query via Back / tab focus.
12. **SCH-FR-12** Three query-mode toggles sit inside the search input at its leading edge — case-insensitive literal, smart-case literal, and regular expression — of which exactly one is active at any moment. Activating one deactivates the other two; there is no state in which none is active. The active mode is what the dispatched search carries as its `mode` (SCH-FR-15).
13. **SCH-FR-13** The active query mode is user-global and survives relaunch: it is read from `"load app preferences"` when the search input mounts and written through `"save app preferences"` whenever the user changes it (per `../core/GSS-global-settings-storage.md` GSS-FR-21). A user who has never chosen one starts in case-insensitive literal.
14. **SCH-FR-14** Changing the active query mode while a query is present re-dispatches that query immediately in the new mode, without waiting for the typing pause of SCH-FR-15, because the change is a deliberate click rather than a keystroke in progress.
15. **SCH-FR-15** The overlay dispatches `"start search (query, mode, scope)"` only after the user has stopped typing for a short interval; every keystroke restarts that interval, so a burst of typing dispatches one search rather than one per character. A query edited down to empty dispatches nothing, cancels any search in flight (SCH-FR-20), and leaves the overlay empty.
16. **SCH-FR-16** Hits from `"search results"` render as their batches arrive, ordered by the `ordinal` each hit carries (per `../core/SCC-search.md` SCC-FR-10). A result already rendered never changes position as later batches arrive, so the list grows downward under a stable reading position.
17. **SCH-FR-17** A result is one entry per matching file, showing the file's name, its project-relative path, and — for a content match — the line number and text of its first match. A result that matched only on its path (`match_kind = "name"`) shows no snippet line. The overlay shows no match count.
18. **SCH-FR-18** The overlay dispatches its search with `scope = capped` so typing stays cheap, and the Search results tab dispatches with `scope = full`. Dismissing the overlay after the tab has opened does not affect the tab's search.
19. **SCH-FR-19** The overlay shows an empty state only after `"search ended"` has arrived for its search with no hits. While a search is still running and has produced nothing yet, it shows an in-progress state rather than claiming there are no results.
20. **SCH-FR-20** Dismissing the overlay (SCH-FR-07) cancels the overlay's in-flight search through `"cancel search (search id)"`. A Search results tab's search is cancelled only when that tab closes, so a full sweep started from the overlay survives the overlay closing.
21. **SCH-FR-21** When `"start search (query, mode, scope)"` returns the typed `"invalid query"` error — a regular expression that does not compile (per `../core/SCC-search.md` SCC-FR-06) — the overlay renders that error in place of the result groups and keeps the query and the active mode intact so the user can correct the pattern in place.

## Non-functional requirements
- The typing pause before dispatch is short enough to feel immediate on a deliberate query and long enough that ordinary typing speed dispatches once; it is an implementation choice of the order of a couple of hundred milliseconds.
- Rendering a streamed batch must not reflow the groups already on screen; the overlay appends within groups rather than rebuilding.
- Overlay results render as they stream, so the first result is visible well before the search ends; the overlay never waits for `"search ended"` to paint.
- The overlay's height is bounded and scrolls internally; scrolling it does not dismiss it (SCH-FR-07).
- The query-mode toggles are pointer targets in the same size class as the other top-chrome controls and are reachable by keyboard from the input.
