# Search

**Spec code:** `SCC`

## Intent
The engine behind the universal search bar (`../ui/SCH-search.md`). It answers a query by matching the text of the project's files rather than by consulting an index: one asynchronous producer enumerates the files of the active content root and a fixed pool of asynchronous consumers matches them, streaming each hit to the UI the moment it is found so the overlay fills progressively instead of waiting for a whole-project sweep. The file list it enumerates is the one the Project panel's scan already maintains, so a search costs no second walk of the tree and honours one set of ignore rules. Out of scope: any persistent or incremental index — every search reads the files afresh; relevance ranking — results carry the enumeration's order and no score; search over historical revisions, over the content of non-text files, or over any content outside the active worktree.

## Contract surface
Tauri commands — names match `../ui/SCH-search.md` byte-for-byte:

- `"start search (query, mode, scope)"` → `start_search(query, mode, scope)` → `search_id`. Returns as soon as the search is registered, before any file has been matched. `mode ∈ { literal_insensitive, smart_case, regex }`; `scope ∈ { capped, full }`.
- `"cancel search (search id)"` → `cancel_search(search_id)` — stops a running search. A `search_id` that is already ended is a no-op, not an error.

### Events (Tauri event bus)
- `"search results"` — emitted repeatedly while a search runs, carrying the hits found since the previous emission. Payload: `{ search_id, hits: [SearchHit] }`.
- `"search ended"` — emitted exactly once per search. Payload: `{ search_id, reason }`, where `reason ∈ { completed, capped, cancelled, superseded, failed }`.

### Payload shapes
```
SearchHit {
  id,            // the ASC node id of the file (ASC-FR-13) — the identity ../ui/TAB-tabs.md TAB-FR-04 compares
  name,          // basename
  path,          // project-relative path
  ordinal,       // the file's position in the enumeration; the ordering key (SCC-FR-10)
  group,         // "artifact" | "playbook" | "workstream" | "role" | "run" | "history" | "file"
  match_kind,    // "content" | "name"
  subtype?,      // artifact group only: one of the eight types of ASC-artifact-scanning.md ASC-FR-02
  edit_context?, // artifact group only: "standalone" | "flow"
  line?,         // content matches only: 1-based line number of the first match
  snippet?       // content matches only: the text of that line (SCC-FR-07)
}
```

Typed errors returned by `start_search`: `"invalid query"` (SCC-FR-06).

## Functional requirements
1. **SCC-FR-01** `start_search` and `cancel_search` exist as Tauri commands and `"search results"` and `"search ended"` as events, all with the documented payload shapes. In the walking-skeleton build the implementation may stream canned hits; the overlay and the full-results tab must both be exercisable against them.
2. **SCC-FR-02** A search is a producer/consumer pipeline: one asynchronous producer enumerates the candidate files and a fixed pool of asynchronous consumers takes files from that enumeration and matches them. Both run off the thread serving the UI, so `start_search` returns its `search_id` immediately and no command in this module blocks on matching.
3. **SCC-FR-03** The candidate files are exactly the file nodes of the project's current scan (`ASC-artifact-scanning.md` ASC-FR-17) — already excluding `.git/`, gitignored paths, `.synthesis/cache/`, and the module-owned `.synthesis/` subtrees, `.synthesis/drafts/` among them, per ASC-FR-09. This module walks no tree of its own and applies no second ignore rule, so a file is searchable exactly when the Project panel can surface it.
4. **SCC-FR-04** The candidate list is mounted with the project's content root and lives as long as it: it is torn down when the project closes (`PST-project-storage.md` PST-FR-14) and when the active worktree changes (`WTC-worktree-context.md` WTC-FR-08), and the scan's watcher keeps it current (ASC-FR-10, ASC-FR-15), so no search rebuilds it. A candidate whose file has been removed between enumeration and matching is skipped rather than fatal, exactly as ASC-FR-11 skips it mid-walk.
5. **SCC-FR-05** `mode` decides what the query means. `literal_insensitive` matches the query as a literal substring ignoring case. `smart_case` matches it as a literal substring, ignoring case while the query contains no uppercase character and respecting case as soon as it contains one. `regex` compiles the query as a regular expression and matches against it. No mode assigns meaning to any character the others treat literally.
6. **SCC-FR-06** A `regex` query that does not compile returns a typed `"invalid query"` error from `start_search`. No `search_id` is issued, no producer or consumer starts, and neither event is emitted for it.
7. **SCC-FR-07** A file is a hit when its project-relative path matches the query, or when at least one of its lines matches. A file produces at most one hit. When any line matches, the hit carries `match_kind = "content"` with `line` and `snippet` taken from the **first** matching line, `snippet` being that line's text bounded to a maximum length that always retains the matched text. A file matching only by path carries `match_kind = "name"` and neither `line` nor `snippet`.
8. **SCC-FR-08** A hit's `group` is decided by where the file sits and how the scan classified it, highest precedence first: a file under `.synthesis/playbooks/`, `.synthesis/workstreams/`, or `.synthesis/roles/` takes the `playbook`, `workstream`, or `role` group respectively; a file with a resolved artifact type (per ASC-FR-06) takes the `artifact` group, carrying that type as its `subtype` and its `edit_context`; every other file takes the `file` group. The `.synthesis/` entity directories outrank artifact classification so a playbook that also classifies as an artifact is grouped as a playbook.
9. **SCC-FR-09** The `run` and `history` groups are part of the payload shape and are never populated by this engine, which matches only files under the active content root; agent runs and historical revisions are not such files. A consumer therefore renders them from the same contract without either ever being present in v1.
10. **SCC-FR-10** Every hit carries the `ordinal` of the file it was found in — that file's position in the producer's enumeration. Consumers finish files in whatever order they finish them, so hits are emitted unordered; ordering by `ordinal` yields enumeration order, which is what `../ui/SCH-search.md` SCH-FR-16 renders. Enumeration order is deterministic for an unchanged tree, so the same query run twice orders its results identically.
11. **SCC-FR-11** `scope` bounds the sweep. `capped` ends the search as soon as the total number of hits across all groups reaches a fixed cap, with `reason = "capped"`; the producer and every consumer stop there rather than finishing the tree. `full` runs the enumeration to exhaustion and ends with `reason = "completed"`. The cap is a single total, not a per-group allowance.
12. **SCC-FR-12** At most one search runs at a time. `start_search` ends any search still running before registering the new one, and the ended search emits `"search ended"` with `reason = "superseded"`. A caller therefore never has to cancel before starting.
13. **SCC-FR-13** `"search ended"` is emitted exactly once for every `search_id` that was issued, whatever the outcome. After it, no `"search results"` event carries that `search_id`, so a consumer keying by id can discard its accumulator on the terminal event.
14. **SCC-FR-14** `cancel_search(search_id)` stops the producer and the consumers at their next file boundary and ends the search with `reason = "cancelled"`. No hit found after the cancellation is emitted, and no `"search results"` for that id is emitted after its `"search ended"`.
15. **SCC-FR-15** A candidate whose content is not valid UTF-8, or whose size exceeds a fixed ceiling, is not read for content matching. Such a file can still produce a `match_kind = "name"` hit, so a binary asset remains findable by its path while never being scanned line by line.
16. **SCC-FR-16** Every operation in this module is read-only: no search creates, modifies, or deletes any file, writes any `.synthesis/` content, or mutates any store.
17. **SCC-FR-17** When no project is open, `start_search` issues a `search_id`, emits no `"search results"`, and immediately emits `"search ended"` with `reason = "completed"`. It returns no error; v1 search is scoped to the open project's content root.
18. **SCC-FR-18** Every search registers an operation with `PRG-progress-reporting.md` under `kind = "search"` (per PRG-FR-11) and terminates it when the search ends, whatever the `reason`. Both `capped` and `full` searches attribute; the operation becomes determinate once the candidate count is known, and PRG's coalescing (PRG-FR-07) absorbs the churn of searches dispatched in quick succession.
19. **SCC-FR-19** Hits are streamed, not accumulated: a `"search results"` event is emitted as soon as there are hits to report rather than being withheld until the search ends. A search that finds its first hit early therefore delivers it while the rest of the tree is still being matched.

## Non-functional requirements
- The consumer pool is a fixed size chosen from the machine's available parallelism and bounded so a search never starves the rest of the application; it does not grow with the size of the tree.
- The capped total of SCC-FR-11 is an implementation choice of the order of a few tens of hits — enough to fill an overlay that shows a prefix per group, small enough that a common query stops almost immediately.
- The content-size ceiling of SCC-FR-15 is an implementation choice; the contract is only that an oversized or non-text file is never read line by line.
- Batching of `"search results"` is an implementation choice; the contract is that hits are not withheld until the end and that the event bus is not flooded per hit.
- No search requires network access, and none reads outside the active content root.
- There is no persistent index: the cost of a search is the cost of reading the candidate files, and nothing about a search survives it.
