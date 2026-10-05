# Brownfield workflow

You are modifying one or more existing specs. The biggest risk in brownfield work is changing one spec while leaving stale, contradictory claims in its neighbors. The blast radius pass is how you avoid that.

## Step 1 — Identify the directly-targeted spec(s)

From the user's request, name the spec file(s) they're modifying. If they didn't name a file, find it by searching `specifications/` for the surface or feature they mentioned. Confirm with the user before proceeding if there's any ambiguity.

## Step 2 — Blast radius, Pass 1: the spec graph

For each directly-targeted spec, run a grep (or equivalent) for its filename across `specifications/` to find every other spec that cross-references it. Then read on a budget, because this corpus is far too large to read whole — a single spec runs to 100 KB and more, and reading ten of them costs more context than the rest of the turn has.

Read in this order, and stop at the first level that settles the question:

1. **The citing lines.** `grep -n` the referencing spec for the targeted spec's filename and for each impacted ID. Read those lines with a few lines of context.
2. **The requirement blocks those lines sit in.** A requirement is one numbered item; read the whole item, not the whole file.
3. **The whole file** — only when this turn actually rewrites that spec, or when the citing requirements genuinely do not settle whether the change breaks them.

A reference in one FR sometimes implies state assumed in another FR you have not read, so where step 2 leaves you unsure, go to step 3 for *that* spec rather than pre-emptively for all of them. Reading a spec you were never going to modify, in full, on the chance that it matters, is the single most expensive thing this workflow can do.

For every referencing spec, ask:

- Does any FR or TS in this spec depend on a claim that the directly-targeted spec is about to change?
- Does this spec's UI contract boundary mention an operation that is about to be renamed, removed, or have its semantics changed?

List the impacted prefixed IDs (`CODE-FR-NN`) per referencing spec. Keep this list in your scratch space; you'll need it for the consistency phase and the handoff.

## Step 3 — Blast radius, Pass 2: layer crossing

Re-run the boundary classification (from `boundary-mapping.md`) on the requested *change*, not on the original feature. Ask:

- Does the change keep every piece on the layer it currently lives on, or does any piece migrate (e.g., a previously-UI-only concern now needs persistence)?
- Does the change introduce a new contract-spanning piece — a UI behavior that requires a backend operation that doesn't exist yet?
- Does the change remove a contract-spanning piece — leaving a now-orphaned backend operation that may need to be removed?

If the change crosses a boundary, the directly-targeted spec is not the only one being modified — the *other* layer's spec must change in the same analyst turn. If no spec exists on the other layer yet (very likely for core specs in this codebase as of this writing), you must either:

- Create the missing spec in the same turn (treat as a greenfield half — load `greenfield-workflow.md` for the new spec), **or**
- Surface a conflict to the user: "This change requires backend behavior that has no spec yet. Should I draft `specifications/core/NN-name.md` as part of this turn, or should we keep the operation marked as a stub in the UI spec for now?"

## Step 3.5 — Re-present the expanded file scope to the user

The blast radius (Steps 2 and 3) often discovers specs the user did not name in their original request: a referencing spec whose claims become stale, a missing `core/` spec the change implies, a `OVW-overview.md` whose surface inventory needs to update. Before drafting, present the full updated file list to the user — the same format as in `boundary-mapping.md`, now expanded to include the blast-radius additions — and confirm.

This is a checkpoint, not a question to answer at length. If the user is happy, proceed to drafting. If they push back ("don't touch `OVW-overview.md` for this"), update the plan and re-confirm. The point is that nobody is surprised at Phase 7 by which files were written.

## Step 4 — Draft the modifications

Modify (in memory) every directly-targeted spec and every spec impacted by the blast radius. Rules:

- Keep the spec's existing code; never change a code on a brownfield edit (it would break every reference to its IDs). New FRs reuse the spec's own code prefix.
- Put a new FR where it reads best, next to the requirements it belongs with. Its ID is independent of its position, so inserting one in the middle costs nothing and every existing reference stays valid.
- Never renumber. A new requirement takes a fresh random-letter ID (`XXX-FR-QJZM`); the requirements around it keep theirs.
- **Edit for target state, not for diff.** The modified spec must read as if it were written fresh against the new behavior. Fold each change into the existing prose: rewrite the affected FRs, Intent, and contract sections so the result is one coherent description of what the feature now IS. Do not annotate what changed, what it used to be, or why — no "(was X)", no "now also supports", no dated entries, no log of this turn's edits. A reader six months from now should not be able to tell this spec was edited rather than authored in one sitting.
- For a removed FR: delete the requirement and leave every other ID alone. Its ID is retired, not reused, and the gap it leaves is expected — IDs carry no order. Do not leave a "(removed)" placeholder: the spec describes the current target state, which no longer includes that requirement. Cross-references to the deleted ID elsewhere in the corpus still have to go, since they no longer resolve.

## Step 5 — Hand off to Phase 6 (validation)

Drafts are now ready for `consistency-rules.md`. The validation phase will re-check cross-references and contract-surface alignment with the rest of the corpus.

## Output expected from this workflow

When this workflow is done, you should be able to state to yourself, in one short paragraph:

> The user wants change X to `<spec-A>`. The blast radius is `<spec-B>` (impacted FRs ...), `<spec-C>` (impacted FRs ...). The change introduces a new contract-spanning operation `op_name` requiring updates to `core/<spec-D>`. Drafts are ready for consistency validation.

If you can't state that paragraph cleanly, the blast radius is incomplete. Go back to Step 2 — **at most twice**. A third failure means the radius is not converging by reading more: state what you have established, name the specs you could not settle and what is unresolved about each, and carry that forward as a stated assumption rather than looping again.
