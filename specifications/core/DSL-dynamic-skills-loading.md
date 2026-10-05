# Dynamic skills loading

**Spec code:** `DSL`

## Intent
The registry of skills an AI agent may reach for on its own. It walks the four folders the agent ecosystems keep their project skills in — `.claude/skills/`, `.codex/skills/`, `.github/skills/`, and `.opencode/skills/` — reads each skill's declared name and description out of its frontmatter, and publishes them both as a queryable list and as the eleventh index of `BMI-bm25-indexing.md`. It exists because an agent choosing a skill reads descriptions, not bodies: given "which skill helps me review a specification?", the answer has to come from the one or two sentences a skill wrote about itself, ranked against every other skill's, rather than from a text sweep over skill bodies that would rank a long skill above an apt one. Loading is dynamic in that the set is derived from disk on every index pass and never declared anywhere: a skill added to a folder is reachable without registration, and one that opts out of model invocation disappears from the set the moment it says so. Out of scope: running a skill, or deciding when to, which belongs to whatever consumes this; the full text of a skill, which `BMI-bm25-indexing.md`'s `skill` artifact index already holds chunked; and any user-facing control — there is no setting, no toggle, and nothing rendered, because the set is a fact about the project rather than a preference.

## Contract surface

### Internal Rust API
The whole surface of this module. Exposed by `synthesis-core` alongside `BMI-bm25-indexing.md`'s `search`, and imported directly by the backend modules that resolve skills. This module registers no Tauri command and emits no Tauri event, so no `invoke()` in `src/**` can reach it.

- **`list_skills()`** → `[SkillDescriptor]` *(consumed by `../tools/SLT-skill-list-tool.md` and `../tools/LSK-load-skill-tool.md`)* — every eligible skill in the active worktree, in path order. Returns a list and never an error (DSL-FR-15). Reads nothing from disk at call time and mutates nothing.
- **`search_skills(query, limit)`** → `[RankedSkill]` *(consumed by `../tools/SST-skill-search-tool.md`)* — the `limit` best-matching skills for `query`, ordered by descending `score`. Returns a list and never an error (DSL-FR-16). Reads nothing from disk at call time and mutates nothing.

### Payload shapes
```
Ecosystem = "claude" | "codex" | "github" | "opencode"   // which of the four
                                                          // folders it was found in

SkillDescriptor {
  path,           // project-relative path of the SKILL.md, e.g.
                  // ".claude/skills/analyst/SKILL.md"
  ecosystem,      // the folder family above
  folder_name,    // the <skill-name> folder segment, e.g. "analyst"
  name,           // the declared frontmatter name, or folder_name (DSL-FR-10)
  description     // the declared frontmatter description (DSL-FR-09)
}

RankedSkill {
  skill,          // the SkillDescriptor
  score           // BM25 score against the skills index (BMI-FR-11)
}
```

A descriptor carries no node id: a skill is eligible whether or not the project scan surfaced its file (DSL-FR-06, DSL-FR-11), so `path` is the only identity it always has.

## Functional requirements
1. **DSL-FR-01** `list_skills` and `search_skills` exist as internal Rust APIs with the documented shapes. This module registers no Tauri command, emits no Tauri event, and exposes nothing the frontend can invoke, so its only observable effect on the user is the records it logs (DSL-FR-24).
2. **DSL-FR-02** A skill is a file at `<special folder>/<skill-name>/SKILL.md`, where `<special folder>` is one of exactly four paths relative to the project root: `.claude/skills`, `.codex/skills`, `.github/skills`, and `.opencode/skills`. The four are matched at the project root alone, so a package's own `.claude/skills/` deeper in a monorepo yields no skills and cannot collide with the root's.
3. **DSL-FR-03** That shape is exact in all three of its parts: exactly one folder segment sits between the special folder and the file, so `.claude/skills/a/b/SKILL.md` is not a skill; the file sits in that folder rather than directly in the special folder, so `.claude/skills/foo.md` is not a skill; and the basename is `SKILL.md` compared case-sensitively against the name on disk, so `skill.md` is not a skill even on a filesystem that would open it under either name. A path failing any part is ignored rather than reported as malformed.
4. **DSL-FR-04** A skill's entry is its `SKILL.md` and nothing else. The supporting files a skill keeps beside it — a `references/` folder, a script, a second Markdown file — are never entries of their own and contribute no text to the index, however they are classified elsewhere.
5. **DSL-FR-05** Symlinks are refused rather than followed. If any component of the path — the special folder, the `<skill-name>` folder, or the `SKILL.md` itself — is a symbolic link, that skill is not eligible and nothing is read through it. A `<skill-name>` folder symlinked at a shared library of skills outside the project therefore contributes nothing, and a symlinked special folder contributes no skills at all.
6. **DSL-FR-06** This module enumerates the four folders itself and applies no ignore rule while doing so: a project that gitignores `.claude/` still yields its skills, because a skill an agent can invoke is present whether or not it is committed. This is the one enumeration in the application that does not inherit `ASC-artifact-scanning.md` ASC-FR-09's exclusions, and it reaches nothing beyond those four folders. A special folder that does not exist yields no skills and is not an error, and neither is one that cannot be read.
7. **DSL-FR-07** An eligible file must carry leading YAML frontmatter delimited by `---`. A `SKILL.md` with no frontmatter, or whose frontmatter does not parse, is excluded from the registry and the index.
8. **DSL-FR-08** A skill whose frontmatter sets `disable-model-invocation` to true is excluded. The key is read as a boolean, and the string `"true"` compared case-insensitively counts as one; every other value, and the key's absence, leaves the skill included.
9. **DSL-FR-09** A skill whose frontmatter declares no `description`, or whose `description` is blank once trimmed, is excluded. A skill that says nothing about itself cannot be ranked against one that does, and offering it unranked would put it ahead of skills that earned their place.
10. **DSL-FR-10** A skill whose frontmatter declares no `name`, or a blank one, takes its `<skill-name>` folder segment as its `name` rather than being excluded — the same rule by which a node with no declared name goes by its basename (`ASC-artifact-scanning.md` ASC-FR-19).
11. **DSL-FR-11** Eligibility does not depend on how `ASC-artifact-scanning.md` classified the file, in either direction: a `SKILL.md` in a special folder is eligible whether the scan typed it `skill`, typed it something else through an assignment (ASC-FR-05), or surfaced it with no type at all, and a file the scan typed `skill` outside the four folders is never eligible. The two systems answer different questions and are never consulted about each other's.
12. **DSL-FR-12** Every eligible skill has exactly one `SkillDescriptor` carrying its `path`, its `ecosystem`, its `folder_name`, its resolved `name`, and its `description`.
13. **DSL-FR-13** A skill's indexed document is its resolved `name` together with its `description`, and nothing else. One skill is one document; the body of the `SKILL.md` below the frontmatter contributes nothing. These documents are the entire content of the `skills` index of `BMI-bm25-indexing.md` (BMI-FR-02), which is why a skill is ranked on how it describes itself rather than on how much it wrote.
14. **DSL-FR-14** A `SKILL.md` that `ASC-artifact-scanning.md` also classifies `skill` is present in two indexes at once and they are independent: `BMI-bm25-indexing.md`'s `skill` artifact index holds its full text in chunks (BMI-FR-05), while the `skills` index holds its one descriptor document. Neither index's statistics reach the other (BMI-FR-02), and a query against one never returns the other's documents.
15. **DSL-FR-15** `list_skills()` returns every eligible skill ordered by `path`, and returns a list in every circumstance rather than an error: no project open, no special folder present, and every candidate excluded each yield an empty list. It answers from the registry as it stands and never blocks on a pass in flight.
16. **DSL-FR-16** `search_skills(query, limit)` returns the `limit` best-matching skills ordered by descending `score`, scoring against the `skills` index alone and never against another index. It returns a list in every circumstance rather than an error: an empty `query`, a `limit` of zero, no project open, and an empty registry each yield an empty list. It never blocks and never waits for indexing to settle, so a call made during the first build after an open returns what has been indexed so far (per BMI-FR-10).
17. **DSL-FR-17** A skill's name is not its identity. Two skills declaring the same `name` in different ecosystems — a `review` under `.claude/skills` and another under `.codex/skills` — are both present, both returned, and told apart by `path`, and neither displaces the other.
18. **DSL-FR-18** The registry and the `skills` index are derived together in one pass and are never observed disagreeing: a skill `list_skills` returns is one `search_skills` can rank, and a skill excluded from one is absent from the other.
19. **DSL-FR-19** A filesystem change under any of the four special folders reaches this module on the internal channel of `ASC-artifact-scanning.md` ASC-FR-20 — which reports paths the scan itself excludes, so a gitignored skill's edit arrives like any other — and requests an index pass. Every pass re-enumerates all four folders in full whatever triggered it, so the registry converges on what is on disk rather than on the sequence of events that reached it. This module mounts no watcher of its own.
20. **DSL-FR-20** A change to what a `SKILL.md` declares takes effect on the next pass in both directions: adding `disable-model-invocation: true` removes the skill from the registry and its document from the index; removing that key, or filling in a `description` that was missing, adds it back; and rewriting a `description` replaces the document rather than adding a second.
21. **DSL-FR-21** A draft's files are never eligible, because they live under `.synthesis/drafts/` rather than in a special folder (`DRS-draft-storage.md` DRS-FR-01). A graduation that publishes a `SKILL.md` into one of the four folders (`GRD-graduation.md` GRD-FR-ARLT) makes that skill eligible through the watcher's report of the published path like any other external creation, so a skill authored as a draft is reachable once the publication has landed in the active worktree and never before. A publication into a worktree that is not the active one makes nothing eligible here until that worktree is activated, this module being scoped to the content root like every other (per `WTC-worktree-context.md` WTC-FR-03).
22. **DSL-FR-22** Closing a project (per `PST-project-storage.md` PST-FR-14) and changing the project's active worktree (per `WTC-worktree-context.md` WTC-FR-08) discard the registry along with the indexes (BMI-FR-25). A new content root enumerates fresh; nothing from the outgoing root is carried over, and `list_skills()` on a closed project returns an empty list rather than the skills it last held.
23. **DSL-FR-23** Every read this module performs goes through `FSA-filesystem-access.md`'s `read_text` and is subject to its path-escape rejection (FSA-FR-10), and a file that does not decode as UTF-8 or exceeds the ceiling of `BMI-bm25-indexing.md` BMI-FR-09 is excluded rather than fatal. This module creates, modifies, and deletes no file anywhere, mutates no store, and touches no repository state.
24. **DSL-FR-24** A file that sits at an eligible path but is excluded under DSL-FR-05, DSL-FR-07, DSL-FR-08, or DSL-FR-09 produces one `WARN` record through `LGC-logging.md`'s internal API under the `backend` domain, naming its path and the reason it was excluded. Nothing that is merely not a skill — a supporting file, a folder outside the four — is logged, since a project has far more of those than of skills. A record names the path and the reason alone and never any part of the file's contents.
25. **DSL-FR-25** The registry is deterministic: the same four folders enumerated twice yield the same skills, the same resolved `name` for each, and the same order. A registry brought to a state by a series of passes is identical to one enumerated from scratch against the same tree, so an incremental history never drifts from what a fresh walk would produce (per BMI-FR-27).

## Non-functional requirements
- Enumeration is four shallow directory listings and one read per candidate `SKILL.md`, so a full re-enumeration on every pass (DSL-FR-19) costs a fraction of the pass that carries it even in a project with a hundred skills. This is what buys convergence: no event needs to be interpreted, only the folders re-read.
- A descriptor document is a name and a sentence or two, so the whole `skills` index is orders of magnitude smaller than any artifact index, and a `search_skills` call is a pure in-memory lookup cheap enough to sit inside a consumer's own loop.
- The frontmatter parse reads only `name`, `description`, and `disable-model-invocation`. Any other key a skill declares is ignored rather than rejected, so a skill carrying an ecosystem-specific key an agent CLI understands is still eligible here.
- Nothing this module holds is written anywhere, and nothing about it is persisted: the registry lives in memory for as long as the content root it was enumerated against is mounted (per BMI-FR-14).
- Nothing in this module requires network access.
