# Greenfield workflow

You are creating a new spec (or a paired UI + core spec). The boundary classification from Phase 3 has already told you which layer(s) the feature lives on.

## Step 1 — Pick the number(s) and path(s)

List `specifications/ui/` and/or `specifications/core/`. Pick the smallest unused two-digit number in the right range:

- UI: 00–02 are foundational. New surfaces use 10+.
- Core: no convention established yet (the folder is mostly empty as of this writing). Use 10+ for surfaces/modules; reserve 00–02 for foundational specs (overview, IPC contract, persistence) if/when they are written.

If the feature is contract-spanning, the UI spec and the core spec do not need matching numbers. They reference each other by filename.

State to the user: "I'll create `specifications/ui/CODE-name.md`" (and the core counterpart if contract-spanning). If the user disagrees with the slug or number, take their preference.

## Step 1.5 — Assign the spec code

Every spec needs a globally-unique 3-letter code — it is the namespace that keeps each `FR`/`TS` ID unique across the whole corpus (see the spec-code hard rules in `SKILL.md`). Mint it now, before drafting, so the IDs you write are already prefixed:

1. Derive a candidate from the feature's first letters (Project picker → `PPK`, Filesystem access → `FSA`, Shell & navigation → `SNV`).
2. List the codes already taken — `grep -r "Spec code:" specifications/` — and confirm your candidate collides with none of them. `ui/` and `core/` share one namespace.
3. For a contract-spanning pair whose two names would yield the same code, give the UI surface the natural code and the core anchor a variant (e.g. `GIT`/`GTC`, `SCH`/`SCC`, `HVW`/`HIS`). The pairing is expressed through the matching operation names in their contract sections; nothing else needs to record it.
4. State the chosen code(s) to the user alongside the filename. The code goes on a `**Spec code:** \`XXX\`` line directly under the H1, per `specifications/TEMPLATE.md`.

## Step 2 — Draft the spec(s) in memory

Follow `specifications/TEMPLATE.md` exactly. Section order is rigid. Use the existing UI specs (`PPK-project-picker.md`, `DSH-dashboard.md`, etc.) as style references for the level of detail and tone.

For each spec, draft sections in this order:

1. **Source link** (optional). If the user mentions a Notion page or other source, capture it in the HTML comment.
2. **Title** — slugify the filename. Immediately under the H1, add the `**Spec code:** \`XXX\`` line using the code minted in Step 1.5.
3. **Intent** — pull from Phase 2 (intent capture). 2–4 sentences. Include the persona and the problem.
4. **User stories** — only if persona-driven motivation is the clearest framing. UI specs often have these; core specs usually don't.
5. **Wireframes** — UI specs only, only if layout is non-trivial. ASCII art is the convention. Follow with "Layout notes" bullets capturing the must-have constraints.
6. **UI contract boundary** (UI spec) or **Contract surface** (core spec). For UI:
   - **Owned by the UI**: enumerate every UI-owned concern from the Phase 3 table.
   - **Delegated to backend (abstract)**: list every operation the UI invokes. Names must match the core spec exactly.
7. **Functional requirements** — derive them from the intent and the boundary table. Each FR is one testable claim of at most 60 words in Simplified Technical English, and each takes your spec's code plus four random capital letters (`XXX-FR-QJZM`), drawn again if the ID is already taken. Where the reason is genuinely not obvious, add one `*Why:*` sub-bullet of at most 40 words; most requirements need none.

   These requirements ARE the acceptance criteria. Write no test scenarios: the `engineer` derives the test cases from the FRs and tags each test with the requirement ids it verifies. An FR you cannot imagine a test for is an FR that is too vague, and that is the signal to sharpen it here.
8. **Non-functional requirements** — pull from Phase 2 (constraints). A statement belongs here only when no test names it; anything testable and tested is an FR.

Non-goals from Phase 2 belong in the Intent as a short "Out of scope" sentence. Closed-off alternatives are not written down — the spec describes the target state you chose, not the options you rejected.

## Step 3 — For contract-spanning features, verify pairing

Before moving to validation (Phase 6), eyeball the two drafts side by side:

- Every entry in the UI spec's "Delegated to backend" list appears as an operation in the core spec's "Contract surface" with the **same name**.
- The core spec defines input/output/error shapes for each operation. The UI spec does not need to redefine them but should not contradict them.
- The two specs are connected purely through the shared operation names in their contract sections — the UI spec's "Delegated to backend" entries and the core spec's "Contract surface" operations line up byte-for-byte. No prose cross-note about the pairing is needed.

If anything is missing, fix the draft now rather than waiting for the consistency rules to catch it. Phase 6 is the safety net, not the first line.

## Step 4 — Hand off to Phase 6 (validation)

Drafts are now ready for `consistency-rules.md`. Do not write to disk yet.
