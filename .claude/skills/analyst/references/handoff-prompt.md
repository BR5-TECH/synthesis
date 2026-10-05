# Handoff prompt

The handoff prompt is the contract between `analyst` and `engineer`, formatted exactly as below. It is intentionally minimal — the spec files are the source of truth, and the handoff just points at them.

Write it the same way every time, whoever is going to read it: the same session a minute from now, `engineer` mid-implementation, or the user pasting it back in three days. A handoff that only works while it stays inside one session has stopped being a contract.

## Format

```
## Engineer handoff

Mode: <greenfield | brownfield>

Specs (source of truth):
- specifications/ui/CODE-name.md
- specifications/core/CODE-name.md      <-- omit if no core spec was touched

Changed/new requirements (use each spec's prefixed IDs):
- specifications/ui/CODE-name.md: XXX-FR-QJZM, XXX-FR-BKPT, XXX-FR-03
- specifications/core/CODE-name.md: YYY-FR-MTWL, YYY-FR-07

Related specs to respect (read but do not modify):
- specifications/ui/SNV-shell-navigation.md
- specifications/ui/OVW-overview.md

Notes from analyst: <one or two sentences, only if there's something the engineer genuinely needs that isn't already in the specs; otherwise omit the section>
```

## Rules

- **Name every spec file touched this turn.** Both UI and core, if both were touched.
- **List the new and changed prefixed IDs per file** (`XXX-FR-NN` or `XXX-FR-QJZM`, using each spec's own code). For greenfield, list every FR. For brownfield, list only the ones that changed in this turn. There are no scenario identifiers to list: the requirements are the acceptance criteria, and the engineer writes the tests that verify them.
- **List related specs the engineer must respect.** Any spec that cross-references the modified specs and whose claims still apply. The engineer reads these but does not modify them.
- **Notes are optional and short.** Only include them when there is genuinely something the spec doesn't say but the engineer needs to know — usually rare, because if the engineer needs to know it, it should be in a spec. Lean toward omitting.
- **No implementation hints.** No "use library X", no "the React component should be called Y", no "consider caching this". The spec is the source of truth. If you find yourself wanting to write an implementation hint, that's a signal the spec is missing something — go back and add it to the spec.

## Where the handoff appears

Above the block, give the user a short summary of what changed: which specs you wrote, what conflicts were resolved, anything noteworthy. Then the block itself.

That is the whole of the analyst's deliverable. It closes the role, not the turn — see "Finishing the handoff is not stopping" in `SKILL.md` for what that distinction means in practice. Do not append a question asking whether to proceed; whoever entered this skill already knows why they did, and the block gives them everything they need to act on it.

## If the session goes on to implement

When the same session carries the work into `engineer`, two habits keep it honest. Both come down to the difference between a conversation and an artifact:

**The spec files get read from disk, even though they were just written.** What `engineer` implements is the file, not the discussion that produced it. Drafts get amended in Phase 6, conflict resolutions get folded into prose, wording shifts. The written spec is what the test architect and the code reviewer will read months from now, so if the file and anyone's memory of the conversation disagree, the file wins.

**Nothing already in context gets read twice.** The related specs enumerated in the blast-radius pass were read this turn; opening them again buys nothing. This is the one place a continuous run is genuinely cheaper than two sessions — take the saving.

The TODO list carries across the boundary rather than restarting. From the user's side this is one piece of work moving from planning into building, and a list that resets at the handoff hides how much is left.

## When `engineer` sends work back

`engineer` re-enters this skill mid-implementation when it finds a spec gap or contradiction it can name — a behavior the code needs that no requirement covers, or two specs that cannot both hold. Treat that as a normal brownfield turn scoped to the named gap: run the phases against that gap alone, surface any conflict to the user as always, write the amendment, and give back the block.

`engineer` is already holding the implementation, so the block is all it needs; starting a second implementation on top of the one it is running is how the two skills end up in a loop.

Two things not to do, because the corpus is only trustworthy while they hold:

- **Do not widen an amendment into a re-spec of the feature.** The gap that was named is the scope.
- **Do not accept "the code turned out differently" as a gap.** An amendment corrects a spec that is wrong; it never ratifies an implementation that drifted. If that is what is being asked, say so and send it back.

## Example

```
## Engineer handoff

Mode: brownfield

Specs (source of truth):
- specifications/ui/PPK-project-picker.md

Changed/new requirements:
- specifications/ui/PPK-project-picker.md: PPK-FR-05 (modified), PPK-FR-09 (new), PPK-FR-QJZM (new)

Related specs to respect (read but do not modify):
- specifications/ui/OVW-overview.md
- specifications/ui/SNV-shell-navigation.md
```
