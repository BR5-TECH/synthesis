# Specification consistency

**Spec code:** `SPC`

## Intent
The check that keeps the specification corpus referable. Every requirement in this project is named by identifier from Rust and TypeScript comments, from test names, and from other specifications, and that web of references is the only thing tying the code to the reasons it exists. A reference stops resolving the moment a spec is renamed, an identifier is retired, or a requirement moves to another spec — and because nothing reads those references, the break is silent and permanent. This check reads them. It is what makes the structural rules the `analyst` skill states — one well-formed spec code per spec, codes unique across the corpus, every identifier carrying its own spec's code, every citation resolving — enforceable rather than advisory. Out of scope: judging whether a requirement is well written, whether a test that names a requirement actually verifies it, or whether a citation names the *right* requirement; the check answers only whether the identifier exists. It edits nothing, and it proposes no correction for what it reports.

## Contract surface
The check owns one script, its tests, its command line, the violations it defines, and its exit status.

### The script
```text
tools/spec-check/spec_check.py          the whole implementation, one file
tools/spec-check/test_spec_check.py     its tests, standard library alone
```
- **Language** — Python 3, standard library alone. It imports no third-party package, has no lock file, and needs no install step, so it runs on a checkout before `pnpm install` and without a Rust toolchain.
- **Invocation** — `python3 tools/spec-check/spec_check.py [root] [--layers] [--coverage] [--concision]`, where `root` is the repository root and defaults to the working directory. Each flag adds a candidate list that the exit status ignores.

### The violation classes
```text
spec-code        a spec declares no Spec code line, or more than one
code-collision   two specs declare the same spec code
reserved-code    a spec declares a code reserved for test fixtures
foreign-prefix   a definition carries a code other than its spec's own
bad-suffix       a definition's suffix is neither digits nor four capitals
duplicate-id     one spec defines one identifier twice
unknown-code     a citation carries a code no spec declares
unknown-id       a citation's code resolves, but that spec defines no such id
ts-section       a spec carries a Test scenarios heading
unclosed-fence   a spec opens a fenced block and never closes it
```

### The candidate lists
```text
unmatched-op     --layers: a UI spec delegates an operation no core spec exposes
orphaned-op      --layers: a core spec exposes an operation no UI spec delegates to
fr-uncited       --coverage: no file outside the corpus cites this requirement
fr-too-long      --concision: a requirement's statement is over 60 words
why-too-long     --concision: a reason is over 40 words or is more than one sentence
why-orphaned     --concision: a reason stands under no requirement
```

### The reserved codes
```text
ABC   THG
```

## Functional requirements
1. **SPC-FR-NUAB** The check is one Python 3 script at `tools/spec-check/spec_check.py` that imports only the standard library.
2. **SPC-FR-DAIC** The check takes the repository root as its only argument and uses the working directory when none is given.
3. **SPC-FR-OSJF** The check reads every file ending `.md` under `specifications/`, at any depth, as the corpus.
4. **SPC-FR-IFPN** `specifications/TEMPLATE.md` is no part of the corpus: it declares the placeholder code and defines the placeholder identifiers of the template itself.
5. **SPC-FR-EQZH** A file whose name begins with a dot is no part of the corpus, so scaffolding left beside the specs is neither parsed nor reported.
6. **SPC-FR-MPSM** Each spec of the corpus declares exactly one `**Spec code:** \`XXX\`` line, where `XXX` is three capital letters.
7. **SPC-FR-EFND** A spec declaring no such line, or more than one, is a `spec-code` violation naming the file and the number of lines found.
8. **SPC-FR-GVNV** Spec codes are unique across the whole corpus, `ui/`, `core/`, `ai/`, `tools/`, `infra/` and `server/` sharing one namespace.
9. **SPC-FR-LQEK** Two specs declaring one code is a `code-collision` violation naming both files and the code.
10. **SPC-FR-CALX** A short set of three-letter codes is **reserved for test fixtures** — identifiers that deliberately name a specification that does not exist, written into the graduation hand-off tests so that a test can assert on a hand-off without naming a real requirement. The reserved set is `ABC` and `THG`.
11. **SPC-FR-GKZK** A spec that declares a reserved code is a `reserved-code` violation, so a fixture code can never be taken by a real spec.
12. **SPC-FR-NZLK** A **definition** is a numbered list item whose text begins `N. **XXX-FR-<suffix>**`, anchored at the start of a line and allowing leading whitespace. A requirement is the only thing a specification defines.
13. **SPC-FR-RICP** A definition whose code is not the declaring spec's own code is a `foreign-prefix` violation naming the file, the identifier, and the spec's code.
14. **SPC-FR-JHRC** A definition's suffix is either one or more digits or exactly four capital letters, and any other shape is a `bad-suffix` violation.
15. **SPC-FR-UKYM** One spec defining one identifier twice is a `duplicate-id` violation. The identifier is the code and the suffix together.
16. **SPC-FR-ORAH** The check then reads every file in the repository ending `.rs`, `.ts`, `.tsx`, `.md`, `.yaml`, `.yml` or `.json`, and collects every citation in it.
17. **SPC-FR-YRIP** The check descends into no directory named `node_modules`, `dist` or `target`, and into no directory whose name begins with a dot, and it reads no file whose name begins with a dot.
18. **SPC-FR-KUSH** A **citation** is any `XXX-FR-<suffix>` or `XXX-TS-<suffix>` token standing in ordinary text, matched on word boundaries and compared by its whole suffix. The retired `-TS-` kind is still read as a citation and no spec defines one, so every surviving test-scenario reference is reported as an `unknown-id` rather than passing unnoticed, so `GRD-FR-1` never matches inside `GIP-FR-NOWM` and `GRD-FR-01` is a different citation from `GRD-FR-1`.
19. **SPC-FR-TQVN** An identifier written inside inline backticks or inside a fenced code block is **mentioned rather than cited**, and the check reads no citation from it. A specification states a cross-reference as a backticked filename followed by a bare identifier — `` `SNV-shell-navigation.md` SNV-FR-09 `` — so an identifier that is itself backticked is being shown rather than pointed at, which is how a specification writes an example of an identifier that is meant to resolve to nothing.
20. **SPC-FR-XKBW** `specifications/TEMPLATE.md` contributes no citation either, its identifiers being the placeholders of the template rather than references to a requirement.
21. **SPC-FR-WDKM** A citation resolves when some spec of the corpus declares its code and defines that identifier.
22. **SPC-FR-MUPX** A citation whose code no spec declares is an `unknown-code` violation, and one whose code resolves to a spec that defines no such identifier is an `unknown-id` violation. Each names the file, the line number, and the identifier.
23. **SPC-FR-EYLW** A citation carrying a reserved code is no violation, whichever file it stands in.
24. **SPC-FR-RSIV** The check prints one line per violation, then a summary counting the specs read, the identifiers defined, the distinct identifiers cited, the citations found, the requirements no file outside the corpus cites, and the violations reported.
25. **SPC-FR-HBDE** The check exits with a non-zero status when it reported one violation or more, and with zero when it reported none.
26. **SPC-FR-MDLI** The check is read-only. It creates, changes and deletes no file, and it reads nothing outside the repository root it was given.
27. **SPC-FR-LQTV** Given `--layers`, the check additionally reports the two **layer-alignment** candidate lists, and reports them as candidates rather than as violations: every operation a UI spec delegates to the backend that no `core/` spec exposes, as an `unmatched-op` line, and every operation a `core/` spec exposes that no UI spec delegates to, as an `orphaned-op` line. A UI spec's delegated operation is a quoted name under its `**Delegated to backend` list, and one marked inline as a stub is excluded; a `core/` spec's exposed operations are the quoted names of its `### Tauri commands` and `### Events` entries. Two names match once any trailing parenthesised parameter list is removed from each, because the corpus states one operation both with and without its parameters. Neither list is a violation, neither reaches the violation count, and neither changes the exit status: what this reports is a list to read rather than a set of failures.
28. **SPC-FR-VJQT** A spec carrying a test-scenarios heading is a `ts-section` violation naming the file and the line. The heading is matched at any level and in any case, a demoted or title-cased one being the same section under another spelling.
   - *Why:* The requirements are the acceptance criteria and the tests verify them, so a written scenario section states the same thing a third time.
29. **SPC-FR-QXBM** Given `--coverage`, the check additionally reports every requirement that no file outside `specifications/` cites, as candidates rather than as violations, and the exit status ignores them.
   - *Why:* A requirement may be stated before it is built, so an uncited one is a list to read rather than a failure.
30. **SPC-FR-WNZP** Given `--concision`, the check reports every requirement whose statement is over 60 words as an `fr-too-long` candidate, naming the definition's own line. The statement is the whole list item: a definition that wraps onto indented continuation lines is counted whole, and its `*Why:*` sub-bullet is no part of it.
   - *Why:* A requirement past the cap is two requirements, or one requirement with its argument still attached.
31. **SPC-FR-BQLT** Given `--concision`, the check reports every `*Why:*` sub-bullet that is over 40 words or holds more than one sentence as a `why-too-long` candidate, and every one standing under no requirement as a `why-orphaned` candidate. A full stop inside inline backticks, inside an abbreviation, or inside an ellipsis ends no sentence.
   - *Why:* A reason dropped in silence is the one outcome a drift check may not have.
32. **SPC-FR-XTRD** The check carries tests at `tools/spec-check/test_spec_check.py` that import only the standard library and run under `python3 -m unittest discover -s tools/spec-check`. The `specs` lane runs them before the check itself.
33. **SPC-FR-ZMVK** A spec that opens a fenced block and never closes it is an `unclosed-fence` violation.
   - *Why:* Everything after the unclosed fence reads as code, so it defines nothing and is checked by nothing, and the file passes by saying nothing at all.
   - *Why:* The check is the only thing reading the corpus's reference web, so a defect in it is silent everywhere.

## Non-functional requirements
- The check needs neither Node.js nor a Rust toolchain nor an install step, so it runs on a bare checkout with Python 3 alone.
- Violations are reported in a stable order for the same corpus, so two runs over one checkout produce identical output.
- The check reads the whole repository in a single pass over the filesystem and completes in a few seconds on a checkout of this size.
- The output names a file and a line for every violation that has one, so each is reachable from the terminal without a second search.
