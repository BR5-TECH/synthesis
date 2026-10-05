# Consistency rules

These are the rules every draft must pass before it can be written to disk.

**Run the checker first, and read the corpus only for what it cannot answer.** The corpus is 120-odd specs and well over a million tokens; it does not fit in context and reading it to check a cross-reference is the most expensive mistake this phase can make. `tools/spec-check/spec_check.py` already resolves every citation in every spec and in every `.rs`/`.ts`/`.tsx`/`.md`/`.yaml`/`.json` file of the repository, and `--layers` additionally lists the layer-alignment candidates:

```
python3 tools/spec-check/spec_check.py --layers
```

Write your drafts to disk first when you can, so the checker reads them too; where you cannot, run it against the corpus as it stands and reason about your drafts on top of what it reports. Rules 8, 10 and 12 below are the checker's output, not a reading of the corpus. The remaining rules are about the drafts in front of you, which you have already read.

For every violation: do **not** auto-fix. Surface it to the user via `AskUserQuestion` with the realistic resolution options you see, plus an "Other" path for them to write in their own resolution. Apply the chosen resolution directly to the relevant spec — fold it into the target-state prose; do not leave a note about what was decided or why.

**When no user is there to ask** — a graduation run, or any other unattended turn — do not answer the question on their behalf and do not loop trying to make it go away. Pick the resolution the corpus most supports, apply it, and record it as a stated assumption in what you hand on: the violation, the resolution you took, and why. An assumption the author can overturn in one reading costs them a moment; a question you answered as though you were them costs them a specification written on it.

## Structural rules

1. **Template sections present and in order.** Every spec has the sections from `specifications/TEMPLATE.md` in the required order: Intent, Functional requirements, then User stories, Wireframes, the contract surface, and the non-functional requirements. A `## Test scenarios` section is a violation — `spec_check.py` reports it as `ts-section`. Requirements come near the top so that a partial read of a long spec reaches what it requires rather than only its headings.
2. **H1 matches filename slug.** `# Project picker` and `PPK-project-picker.md` must agree. An intentional divergence is fine; otherwise fix it.
3. **Spec code present and well-formed.** Every spec declares a `**Spec code:** \`XXX\`` line directly under the H1, where `XXX` is a 3-letter code derived from the feature name. A missing or malformed line is a violation.
4. **IDs well-formed, prefixed, and unique.** Every ID carries this spec's code and is a requirement — there is no `-TS-` kind. A **new** ID ends in four random capital letters (`XXX-FR-QJZM`); an ID the spec already holds keeps whatever form it has, letters or number, unchanged. No two IDs in one spec share a suffix. A bare `FR-1` / `FR-ABCD`, a suffix that is neither two digits nor four capital letters, and a duplicate suffix are each a violation. Gaps in the numeric IDs are **not** a violation: IDs carry no order.
5. **Every FR is one testable claim, at most 60 words, in Simplified Technical English.** `spec_check.py --concision` reports a longer one as `fr-too-long`. The cap binds every requirement written or rewritten in this turn. An existing requirement over it is brought down by moving its reasoning into `*Why:*`, never by splitting it: a split leaves every citation of that requirement pointing at less than it cited.

   5a. **The reason is cut, not relocated.** A justification clause defending a requirement is deleted. A `*Why:*` sub-bullet is kept only where the reason is load-bearing — where a reader would otherwise implement the requirement wrongly or read it as arbitrary and "fix" it — and is then one sentence of at most 40 words, reported as `why-too-long` otherwise. Expect roughly one requirement in five to keep one.

## Spec-code uniqueness rules (the namespace that keeps every ID unique)

6. **Codes are globally unique across the corpus.** `ui/` and `core/` share one code namespace. Build the set of in-use codes (`grep -r "Spec code:" specifications/` plus your drafts); no two specs may share a code. A collision is a violation — resolve it before writing.
7. **Paired specs disambiguate.** When a feature has paired `ui/` + `core/` specs whose names yield the same code, the UI surface keeps the natural code and the core anchor takes a variant (`GIT`/`GTC`, `SCH`/`SCC`, `HVW`/`HIS`). The pairing is visible through the matching operation names in their contract sections — no separate note required.

## Cross-reference rules

8. **Every cross-reference resolves.** A reference to `` `SNV-shell-navigation.md` SNV-FR-09 `` requires that file to exist, its declared code to match the prefix (`SNV`), and `SNV-FR-09` to exist within it. **This is the checker's `unknown-code` and `unknown-id` output** — run it rather than reading specs to resolve references by hand. Its summary line also states how many identifiers the corpus defines and how many citations it holds; when a turn is only moving requirements about, those two numbers not changing is the proof nothing was lost.
9. **Reference format.** Backticked filename, no path: `` `CODE-name.md` ``. Every requirement reference — inside-spec or cross-spec — uses the fully prefixed ID exactly as the target spec declares it (`XXX-FR-NN` or `XXX-FR-QJZM`). A bare `FR-N` reference anywhere is a violation.

## Layer-alignment rules (the load-bearing ones)

10. **Operation names match byte-for-byte across layers.** Every entry in a UI spec's "Delegated to backend" section must appear, byte-for-byte, as an operation in a `core/` spec's "Contract surface" — or be marked inline as a stub in the "Delegated to backend" list (e.g. `open_project_at_path (stub — no core spec yet)`). **This is the checker's `unmatched-op` output under `--layers`.** It reports candidates rather than violations, because the corpus states some operations with their parameter list and some without; read the lines that name a spec this turn touches and ignore the rest.
11. **Operation shapes do not contradict.** A UI spec that says "open_project_at_path takes a string" must not co-exist with a core spec that says it takes an object. If shapes are defined on both sides, they must match.
12. **No orphaned core operations.** Every operation in a core spec's "Contract surface" must either appear in at least one UI spec's "Delegated to backend" list, or be marked inline in the Contract surface as awaiting a UI consumer (e.g. `list_recent_projects (no UI consumer yet)`). **This is the checker's `orphaned-op` output under `--layers`**, on the same terms as rule 10.
13. **Brownfield: paired updates.** If this turn adds, renames, removes, or changes the semantics of a contract-spanning operation on one layer, the corresponding spec on the other layer must change in the same turn — or the user must explicitly resolve the asymmetry (e.g., "leave UI as stub for now"), which is then reflected in the spec as a target-state stub, not as a note about the decision.

## Content-quality rules

14. **Tests cover FRs.** Every FR is named by at least one test. (One test may name several FRs; one FR may be named by several tests; both are fine.) `python3 tools/spec-check/spec_check.py --coverage` lists the requirements nothing outside the corpus cites. An uncited FR is a candidate rather than a failure — a requirement may be stated before it is built — but an FR that stays uncited after its feature ships is usually vague.
15. **Target state, no history.** The spec reads as a description of what the feature IS. No "previously X, now Y", no "changed from", no "we chose A over B", no changelog or dated entries. A brownfield edit must leave the spec indistinguishable from one written fresh against the new target state.
16. **Non-goals recorded.** Anything the user said was explicitly out of scope is captured in Intent as a short "Out of scope" sentence — phrased as a present-tense boundary of the feature, not as a decision that was made.

## Brownfield-specific rules

17. **Blast radius covered.** Every spec identified in the brownfield blast radius (Pass 1 + Pass 2) is either modified in this turn or has been explicitly determined to need no changes. If a referencing spec is impacted but not modified, that's a violation — either modify it or confirm to the user why it's safe (this confirmation lives in your message, not in the spec).
18. **No ID rewriting.** No turn changes an ID that already exists. If a turn changed one anyway, that is a violation: restore the original ID rather than propagate the new one. New requirements take fresh random-letter IDs and leave every existing ID alone, so no cross-reference in the corpus needs updating. The one exception is a **split** (`specifications/TEMPLATE.md`): a requirement moved into a new spec takes that spec's code and keeps its own suffix, every citation of it is rewritten in the same turn, and the checker's `ids defined` and `citations` counts are unchanged by the move.
19. **No new bare or mis-prefixed IDs.** A brownfield edit must not reintroduce a bare `FR-N`, a retired `TS` identifier, or an ID whose prefix is not this spec's code. The corpus-wide invariant is that every ID is `CODE-FR-NN` or `CODE-FR-QJZM` with the spec's own code.

## How to surface a violation to the user

Use `AskUserQuestion`. Phrase the violation concretely with file paths and IDs, and offer the realistic resolutions:

> The UI spec delegates `list_recent_projects` to the backend, but no core spec defines it.
>
> How should we resolve this?
> - Add `list_recent_projects` to a new `specifications/core/PST-project-storage.md` spec (drafted in this turn).
> - Mark it as a stub inline in the UI spec's "Delegated to backend" list; core spec to follow later.
> - Use a different operation name that already exists.

Wait for the answer, apply it, re-run the affected rules, and continue. Unattended, take the resolution the corpus most supports and record it as a stated assumption instead, as above.

## When all rules pass

Move to Phase 7 (Write). Write the spec files. Then Phase 8 (Handoff).
