# Template rules

The canonical template lives at `specifications/TEMPLATE.md`. Read it first. This file does not duplicate the template — it captures the rules that surround it.

## File placement

- Specs live at `specifications/{ui,core,server,ai,tools,infra}/CODE-kebab-case-name.md`.
- `CODE` is the spec's own 3-letter spec code, in the case it carries on the `**Spec code:**` line. The filename holds no ordering number: ordering and grouping live inside the specs.
- Before naming a new spec, run `grep -r "Spec code:" specifications/` and pick a code that collides with nothing.

## Naming convention

- Kebab-case, no underscores.
- The filename's slug should match the H1 closely (slugified): `# Project picker` → `PPK-project-picker.md`.

## Spec code

- Every spec carries a globally-unique 3-letter code, derived from the first letters of the feature name (Project picker → `PPK`, Filesystem access → `FSA`). It is the namespace that makes each ID unique across the whole corpus.
- Declare it on a `**Spec code:** \`XXX\`` line directly under the H1.
- Codes are unique across the entire corpus — `ui/` and `core/` share one namespace. Before assigning one, list the codes already taken (`grep -r "Spec code:" specifications/`) and avoid collisions.
- For a contract-spanning pair whose names collide, the UI surface keeps the natural code and the core anchor takes a variant (`GIT`/`GTC`, `SCH`/`SCC`, `HVW`/`HIS`). The pairing shows up in the contract sections — the UI spec's "Delegated to backend" names match the core spec's "Contract surface" operations byte-for-byte — so no separate note is needed.

## Cross-references

- Always backtick the filename, no path: `` `PPK-project-picker.md` ``.
- When referring to a specific requirement, use the target spec's prefixed ID: `` `SNV-shell-navigation.md` SNV-FR-09 ``. Never a bare `FR-9`.
- Every cross-reference must resolve — the file must exist, its declared code must match the prefix, and the ID must exist within it exactly as that spec declares it (`XXX-FR-NN` or `XXX-FR-QJZM`).

## ID rules

- **Every new requirement ID ends in four random capital letters**: `XXX-FR-QJZM`. Draw the four letters at random from A-Z. Then search the spec for that ID. If it is taken, draw again. Draw at random rather than pick something meaningful: a random draw is what stops two parallel edits of the same spec from choosing the same ID.
- **An ID says which requirement, never which position.** New IDs hold no number for exactly that reason. A sequence forces a renumber each time a requirement is added or removed in the middle, and two graduation runs that renumber the same spec always conflict.
- **Never renumber, never re-letter, never reuse.** A deleted requirement retires its ID; the requirements around it do not move. There is no gap to close, because there is no sequence.
- **Existing numeric IDs keep their numbers for good.** `GRD-FR-58` and every other `XXX-FR-NN` in the corpus stays exactly as it is. Both forms are valid and one spec may hold a mix. Never convert one form to the other: a conversion is a renumber, and it breaks every cross-reference, test, and code comment that cites the old ID.
- A bare `FR-N` / `FR-ABCD` with no code prefix is never valid, in a definition or a reference.
- **There is no `-TS-` kind.** A specification defines requirements alone. The requirements are the acceptance criteria; the test that verifies one names it, and `tools/spec-check/spec_check.py --coverage` reports which requirements no test names.
- The ordered-list number in front of an ID is markdown numbering alone. It is not part of the ID, and nothing may cite it.

## Section ordering (required for every spec)

1. Source link comment (optional, HTML comment at top — `<!-- Source: ... -->`).
2. `# <Title>` — H1, single line.
3. `**Spec code:** \`XXX\`` — directly under the H1.
4. `## Intent` — 2–4 sentences.
5. `## Functional requirements` — `XXX-FR-NN` list.
6. `## User stories` — OPTIONAL.
7. `## Wireframes` — OPTIONAL, UI specs only.
8. `## UI contract boundary` (UI specs) or `## Contract surface` (core specs).
9. `## Non-functional requirements` — bulleted, free-form.

Requirements stand near the top because a reader who samples a long spec must reach what it requires before they reach how it is shaped.

Sections must appear in this order. Optional sections, if present, must appear at the indicated position.

## Style

- **Write in ASD-STE100 Simplified Technical English.** One idea per sentence, the active voice, a short sentence in preference to a long one, one term for one thing.
- **An FR is one testable assertion, at most 60 words.** One claim per FR. A statement that needs a subordinate clause to defend itself is a requirement plus a reason, not one requirement.
- **The cap governs what you write; it never licenses a split.** A requirement you add or rewrite meets it. An existing one that does not is brought down by moving its reasoning out, never by dividing it into two — a split leaves every citation of it pointing at less than it cited, and the corpus holds tens of thousands. `python3 tools/spec-check/spec_check.py --concision` lists what is still over.
- **Cut the reason; do not relocate it.** A justification clause defending a requirement is deleted outright. This is the single largest thing that made the corpus unreadable, and moving it one line down saves nothing.
- **Keep a `*Why:*` sub-bullet only where the reason is load-bearing** — where a reader would otherwise implement the requirement wrongly, or read it as arbitrary and quietly "fix" it. One sentence, at most 40 words, and expect to keep one for roughly one requirement in five.
- **A statement in Non-functional requirements is one no test names.** Anything testable and tested is an FR, so that a test can cite it.
- Every section describes the target state. Don't narrate history, alternatives ruled out, or what changed in this edit — the spec says what the feature IS, not how it got here. The *why* belongs in Intent (motivation), not in a log of decisions.

## Contract surface (core specs)

A core spec replaces "UI contract boundary" with a "Contract surface" section that enumerates every operation the backend exposes, including:

- Operation name (must match the UI spec's "Delegated to backend" entry byte-for-byte).
- Input shape (TypeScript-flavored or Rust-flavored type signature is fine; pick the convention used elsewhere in the corpus).
- Output shape, including the error shape.
- Side effects, if any.

If no UI spec yet uses the operation, the core spec can still define it; mark it inline in the Contract surface as unreferenced (e.g. `list_recent_projects (no UI consumer yet)`) so it's clear it may be removed if none materializes.
